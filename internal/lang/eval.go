package lang

import (
	"encoding/json"
	"fmt"
	"io"
	"math"
	"os"
	"reflect"
	"regexp"
	"sort"
	"strconv"
	"strings"
	"unicode/utf8"
)

type Runtime struct {
	Globals         map[string]any
	MaxSteps, steps int
	Last            any
	Output          io.Writer
}

// arrayValue gives arrays reference identity, matching JavaScript array behavior.
type arrayValue struct{ items []any }

func NewRuntime(root any, max int) *Runtime {
	if max <= 0 {
		max = 1000000
	}
	return &Runtime{Globals: map[string]any{"$": importValue(root)}, MaxSteps: max, Output: io.Discard}
}
func (r *Runtime) Root() any { return exportValue(r.Globals["$"]) }
func (r *Runtime) Run(p *Program) error {
	sig, e := r.execList(p.List)
	if e != nil {
		return e
	}
	if sig != "" {
		return r.fail(Pos{1, 1}, sig+" outside loop")
	}
	return nil
}
func (r *Runtime) step(p Pos) error {
	r.steps++
	if r.steps > r.MaxSteps {
		return r.fail(p, "maximum execution steps exceeded")
	}
	return nil
}
func (r *Runtime) fail(p Pos, f string, a ...any) error {
	return &LangError{"RuntimeError", p, fmt.Sprintf(f, a...)}
}
func (r *Runtime) execList(xs []Stmt) (string, error) {
	for _, s := range xs {
		if e := r.step(s.stmtPos()); e != nil {
			return "", e
		}
		sig, e := r.exec(s)
		if e != nil || sig != "" {
			return sig, e
		}
	}
	return "", nil
}
func (r *Runtime) exec(s Stmt) (string, error) {
	switch x := s.(type) {
	case *ExprStmt:
		v, e := r.eval(x.X)
		if e == nil {
			r.Last = v
		}
		return "", e
	case *Block:
		return r.execList(x.List)
	case *IfStmt:
		v, e := r.eval(x.Cond)
		if e != nil {
			return "", e
		}
		if truth(v) {
			return r.exec(x.Then)
		}
		if x.Else != nil {
			return r.exec(x.Else)
		}
		return "", nil
	case *DeleteStmt:
		ref, e := r.reference(x.Target)
		if e != nil {
			return "", e
		}
		return "", ref.del()
	case *BreakStmt:
		return "break", nil
	case *ContinueStmt:
		return "continue", nil
	case *ForStmt:
		return r.execFor(x)
	}
	return "", r.fail(s.stmtPos(), "unknown statement")
}
func (r *Runtime) execFor(s *ForStmt) (string, error) {
	v, e := r.eval(s.Source)
	if e != nil {
		return "", e
	}
	if s.Of {
		return r.execForOf(s, v)
	}
	var keys []any
	switch x := v.(type) {
	case *arrayValue:
		for i := range x.items {
			keys = append(keys, float64(i))
		}
	case map[string]any:
		ss := make([]string, 0, len(x))
		for k := range x {
			ss = append(ss, k)
		}
		sort.Strings(ss)
		for _, k := range ss {
			keys = append(keys, k)
		}
	default:
		return "", r.fail(s.P, "for..in requires array or object")
	}
	for _, k := range keys {
		current, er := r.eval(s.Source)
		if er != nil {
			return "", er
		}
		if !exists(current, k) {
			continue
		}
		r.Globals[s.Name] = k
		sig, e := r.exec(s.Body)
		if e != nil {
			return "", e
		}
		if sig == "break" {
			break
		}
		if sig == "continue" {
			continue
		}
		if sig != "" {
			return sig, nil
		}
	}
	return "", nil
}

// execForOf follows JavaScript iterator behavior: the iterable expression is
// evaluated once, and an array's current length and current element are read on
// every iteration. Consequently, splice/delete and push affect later iterations.
func (r *Runtime) execForOf(s *ForStmt, iterable any) (string, error) {
	runBody := func(value any) (bool, error) {
		r.Globals[s.Name] = value
		sig, err := r.exec(s.Body)
		if err != nil {
			return false, err
		}
		switch sig {
		case "":
			return false, nil
		case "continue":
			return false, nil
		case "break":
			return true, nil
		default:
			return false, r.fail(s.P, "unexpected loop signal %q", sig)
		}
	}

	switch x := iterable.(type) {
	case *arrayValue:
		for i := 0; i < len(x.items); i++ {
			stop, err := runBody(x.items[i])
			if err != nil || stop {
				return "", err
			}
		}
		return "", nil
	case string:
		for _, value := range x {
			stop, err := runBody(string(value))
			if err != nil || stop {
				return "", err
			}
		}
		return "", nil
	default:
		return "", r.fail(s.P, "for..of requires array or string")
	}
}
func exists(v, key any) bool {
	switch x := v.(type) {
	case *arrayValue:
		i, ok := index(key)
		return ok && i < len(x.items)
	case map[string]any:
		k, ok := key.(string)
		if !ok {
			return false
		}
		_, ok = x[k]
		return ok
	}
	return false
}

func (r *Runtime) eval(e Expr) (any, error) {
	if er := r.step(e.exprPos()); er != nil {
		return nil, er
	}
	switch x := e.(type) {
	case *Literal:
		return x.Value, nil
	case *Variable:
		v, ok := r.Globals[x.Name]
		if !ok {
			return nil, r.fail(x.P, "undefined variable %q", x.Name)
		}
		return v, nil
	case *ArrayExpr:
		a := make([]any, len(x.Items))
		for i, q := range x.Items {
			v, e := r.eval(q)
			if e != nil {
				return nil, e
			}
			a[i] = v
		}
		return &arrayValue{a}, nil
	case *ObjectExpr:
		o := map[string]any{}
		for _, q := range x.Items {
			v, e := r.eval(q.Value)
			if e != nil {
				return nil, e
			}
			o[q.Key] = v
		}
		return o, nil
	case *Unary:
		v, e := r.eval(x.X)
		if e != nil {
			return nil, e
		}
		if x.Op == tBang {
			return !truth(v), nil
		}
		n, ok := v.(float64)
		if !ok {
			return nil, r.fail(x.P, "unary '-' requires number")
		}
		return -n, nil
	case *Binary:
		return r.binary(x)
	case *Assign:
		return r.assign(x)
	case *Member:
		obj, e := r.eval(x.Object)
		if e != nil {
			return nil, e
		}
		key, e := r.eval(x.Key)
		if e != nil {
			return nil, e
		}
		return r.memberValue(x.P, obj, key)
	case *Call:
		return r.call(x)
	case *MethodCall:
		return r.methodCall(x)
	}
	return nil, r.fail(e.exprPos(), "unknown expression")
}

func (r *Runtime) memberValue(p Pos, obj, key any) (any, error) {
	switch x := obj.(type) {
	case string:
		if key == "length" {
			return float64(utf8.RuneCountInString(x)), nil
		}
		return nil, r.fail(p, "string property %q does not exist", key)
	case map[string]any:
		k, ok := key.(string)
		if !ok {
			return nil, r.fail(p, "object key must be string")
		}
		v, ok := x[k]
		if !ok {
			return nil, nil
		}
		return v, nil
	case *arrayValue:
		if key == "length" {
			return float64(len(x.items)), nil
		}
		i, ok := index(key)
		if !ok {
			return nil, r.fail(p, "array index must be a non-negative integer")
		}
		if i >= len(x.items) {
			return nil, r.fail(p, "array index %d out of range", i)
		}
		return x.items[i], nil
	default:
		return nil, r.fail(p, "member access requires array or object")
	}
}
func (r *Runtime) binary(x *Binary) (any, error) {
	a, e := r.eval(x.Left)
	if e != nil {
		return nil, e
	}
	if x.Op == tAnd {
		if !truth(a) {
			return false, nil
		}
		b, e := r.eval(x.Right)
		return truth(b), e
	}
	if x.Op == tOr {
		if truth(a) {
			return true, nil
		}
		b, e := r.eval(x.Right)
		return truth(b), e
	}
	b, e := r.eval(x.Right)
	if e != nil {
		return nil, e
	}
	return r.apply(x.P, x.Op, a, b)
}
func (r *Runtime) apply(p Pos, op tokenKind, a, b any) (any, error) {
	switch op {
	case tEq:
		return reflect.DeepEqual(a, b), nil
	case tNe:
		return !reflect.DeepEqual(a, b), nil
	}
	if op == tPlus {
		if _, ok := a.(string); ok {
			return valueString(a) + valueString(b), nil
		}
		if _, ok := b.(string); ok {
			return valueString(a) + valueString(b), nil
		}
	}
	if x, ok := a.(float64); ok {
		y, ok := b.(float64)
		if !ok {
			return nil, r.fail(p, "numeric operator requires numbers")
		}
		switch op {
		case tPlus:
			return x + y, nil
		case tMinus:
			return x - y, nil
		case tStar:
			return x * y, nil
		case tSlash:
			if y == 0 {
				return nil, r.fail(p, "division by zero")
			}
			return x / y, nil
		case tGT:
			return x > y, nil
		case tGE:
			return x >= y, nil
		case tLT:
			return x < y, nil
		case tLE:
			return x <= y, nil
		}
	}
	if x, ok := a.(string); ok {
		y, ok := b.(string)
		if ok {
			switch op {
			case tGT:
				return x > y, nil
			case tGE:
				return x >= y, nil
			case tLT:
				return x < y, nil
			case tLE:
				return x <= y, nil
			}
		}
	}
	return nil, r.fail(p, "operator has incompatible operand types")
}
func (r *Runtime) assign(x *Assign) (any, error) {
	ref, e := r.reference(x.Target)
	if e != nil {
		return nil, e
	}
	v, e := r.eval(x.Value)
	if e != nil {
		return nil, e
	}
	if x.Op != tAssign {
		old, e := ref.get()
		if e != nil {
			return nil, e
		}
		ops := map[tokenKind]tokenKind{tPlusAssign: tPlus, tMinusAssign: tMinus, tStarAssign: tStar, tSlashAssign: tSlash}
		v, e = r.apply(x.P, ops[x.Op], old, v)
		if e != nil {
			return nil, e
		}
	}
	if e = ref.set(v); e != nil {
		return nil, e
	}
	return v, nil
}

type ref struct {
	get func() (any, error)
	set func(any) error
	del func() error
}

func (r *Runtime) reference(e Expr) (*ref, error) {
	switch x := e.(type) {
	case *Variable:
		name := x.Name
		return &ref{get: func() (any, error) {
			v, ok := r.Globals[name]
			if !ok {
				return nil, r.fail(x.P, "undefined variable %q", name)
			}
			return v, nil
		}, set: func(v any) error { r.Globals[name] = v; return nil }, del: func() error { return r.fail(x.P, "cannot delete variable") }}, nil
	case *Member:
		parent, e := r.reference(x.Object)
		if e != nil {
			return nil, e
		}
		obj, e := parent.get()
		if e != nil {
			return nil, e
		}
		key, e := r.eval(x.Key)
		if e != nil {
			return nil, e
		}
		return r.memberRef(x.P, parent, obj, key)
	}
	return nil, r.fail(e.exprPos(), "invalid assignment target")
}
func (r *Runtime) memberRef(p Pos, parent *ref, obj, key any) (*ref, error) {
	switch x := obj.(type) {
	case map[string]any:
		k, ok := key.(string)
		if !ok {
			return nil, r.fail(p, "object key must be string")
		}
		return &ref{get: func() (any, error) {
			v, ok := x[k]
			if !ok {
				return nil, nil
			}
			return v, nil
		}, set: func(v any) error { x[k] = v; return nil }, del: func() error {
			if _, ok := x[k]; !ok {
				return r.fail(p, "object property %q does not exist", k)
			}
			delete(x, k)
			return nil
		}}, nil
	case *arrayValue:
		i, ok := index(key)
		if !ok {
			return nil, r.fail(p, "array index must be a non-negative integer")
		}
		return &ref{get: func() (any, error) {
			if i >= len(x.items) {
				return nil, r.fail(p, "array index %d out of range", i)
			}
			return x.items[i], nil
		}, set: func(v any) error {
			if i >= len(x.items) {
				return r.fail(p, "array index %d out of range", i)
			}
			x.items[i] = v
			return nil
		}, del: func() error {
			if i >= len(x.items) {
				return r.fail(p, "array index %d out of range", i)
			}
			x.items = append(x.items[:i], x.items[i+1:]...)
			return nil
		}}, nil
	default:
		return nil, r.fail(p, "member access requires array or object")
	}
}
func index(v any) (int, bool) {
	n, ok := v.(float64)
	if !ok || n < 0 || math.Trunc(n) != n || n > float64(^uint(0)>>1) {
		return 0, false
	}
	return int(n), true
}

func (r *Runtime) call(c *Call) (any, error) {
	args := make([]any, len(c.Args))
	for i, e := range c.Args {
		v, er := r.eval(e)
		if er != nil {
			return nil, er
		}
		args[i] = v
	}
	switch c.Name {
	case "log":
		parts := make([]string, len(args))
		for i, arg := range args {
			parts[i] = valueString(arg)
		}
		if _, err := io.WriteString(r.Output, strings.Join(parts, " ")+"\n"); err != nil {
			return nil, r.fail(c.P, "write log output: %v", err)
		}
		return nil, nil
	case "env":
		if len(args) != 1 {
			return nil, r.fail(c.P, "env expects 1 argument")
		}
		name, ok := args[0].(string)
		if !ok {
			return nil, r.fail(c.P, "env requires a string argument")
		}
		value, ok := os.LookupEnv(name)
		if !ok {
			return nil, nil
		}
		return value, nil
	case "typeof":
		if len(args) != 1 {
			return nil, r.fail(c.P, "typeof expects 1 argument")
		}
		switch args[0].(type) {
		case string:
			return "string", nil
		case *arrayValue:
			return "array", nil
		case map[string]any, nil:
			return "object", nil
		case bool:
			return "boolean", nil
		case float64:
			return "number", nil
		default:
			return nil, r.fail(c.P, "typeof received unsupported value")
		}
	case "keys":
		if len(args) != 1 {
			return nil, r.fail(c.P, "keys expects 1 argument")
		}
		switch x := args[0].(type) {
		case *arrayValue:
			a := make([]any, len(x.items))
			for i := range x.items {
				a[i] = float64(i)
			}
			return &arrayValue{a}, nil
		case map[string]any:
			ss := make([]string, 0, len(x))
			for k := range x {
				ss = append(ss, k)
			}
			sort.Strings(ss)
			a := make([]any, len(ss))
			for i, k := range ss {
				a[i] = k
			}
			return &arrayValue{a}, nil
		}
		return nil, r.fail(c.P, "keys requires array or object")
	}
	return nil, r.fail(c.P, "unknown function %q", c.Name)
}

func (r *Runtime) methodCall(c *MethodCall) (any, error) {
	receiver, err := r.eval(c.Receiver)
	if err != nil {
		return nil, err
	}
	values := make([]any, len(c.Args))
	for i, arg := range c.Args {
		v, e := r.eval(arg)
		if e != nil {
			return nil, e
		}
		values[i] = v
	}
	if c.Name == "toString" {
		if len(values) != 0 {
			return nil, r.fail(c.P, "toString expects no arguments")
		}
		return valueString(receiver), nil
	}
	if array, ok := receiver.(*arrayValue); ok {
		return r.arrayMethod(c.P, array, c.Name, values)
	}
	if c.Name == "push" || c.Name == "splice" || c.Name == "join" {
		return nil, r.fail(c.P, "%s requires an array receiver", c.Name)
	}
	if s, ok := receiver.(string); ok {
		return r.stringMethod(c.P, s, c.Name, values)
	}
	return nil, r.fail(c.P, "unknown method %q", c.Name)
}

func (r *Runtime) stringMethod(p Pos, s, name string, args []any) (any, error) {
	runes := []rune(s)
	switch name {
	case "toLowerCase", "toUpperCase", "trim":
		if len(args) != 0 {
			return nil, r.fail(p, "%s expects no arguments", name)
		}
		if name == "toLowerCase" {
			return strings.ToLower(s), nil
		}
		if name == "toUpperCase" {
			return strings.ToUpper(s), nil
		}
		return strings.TrimSpace(s), nil
	case "substring":
		if len(args) < 1 || len(args) > 2 {
			return nil, r.fail(p, "substring expects 1 or 2 arguments")
		}
		start, ok := integerArg(args[0])
		if !ok {
			return nil, r.fail(p, "substring indexes must be integers")
		}
		end := len(runes)
		if len(args) == 2 {
			var ok bool
			end, ok = integerArg(args[1])
			if !ok {
				return nil, r.fail(p, "substring indexes must be integers")
			}
		}
		start = clamp(start, 0, len(runes))
		end = clamp(end, 0, len(runes))
		if start > end {
			start, end = end, start
		}
		return string(runes[start:end]), nil
	case "indexOf":
		if len(args) < 1 || len(args) > 2 {
			return nil, r.fail(p, "indexOf expects 1 or 2 arguments")
		}
		needle, ok := args[0].(string)
		if !ok {
			return nil, r.fail(p, "indexOf requires a string needle")
		}
		from := 0
		if len(args) == 2 {
			var ok bool
			from, ok = integerArg(args[1])
			if !ok {
				return nil, r.fail(p, "indexOf start must be an integer")
			}
		}
		from = clamp(from, 0, len(runes))
		i := strings.Index(string(runes[from:]), needle)
		if i < 0 {
			return float64(-1), nil
		}
		return float64(from + utf8.RuneCountInString(string(runes[from:])[:i])), nil
	case "lastIndexOf":
		if len(args) < 1 || len(args) > 2 {
			return nil, r.fail(p, "lastIndexOf expects 1 or 2 arguments")
		}
		needle, ok := args[0].(string)
		if !ok {
			return nil, r.fail(p, "lastIndexOf requires a string needle")
		}
		from := len(runes)
		if len(args) == 2 {
			var ok bool
			from, ok = integerArg(args[1])
			if !ok {
				return nil, r.fail(p, "lastIndexOf start must be an integer")
			}
		}
		needleRunes := []rune(needle)
		if len(needleRunes) == 0 {
			return float64(clamp(from, 0, len(runes))), nil
		}
		from = clamp(from, 0, len(runes)-len(needleRunes))
		for i := from; i >= 0; i-- {
			if string(runes[i:i+len(needleRunes)]) == needle {
				return float64(i), nil
			}
		}
		return float64(-1), nil
	case "localeCompare":
		if len(args) != 1 {
			return nil, r.fail(p, "localeCompare expects 1 argument")
		}
		other, ok := args[0].(string)
		if !ok {
			return nil, r.fail(p, "localeCompare requires a string argument")
		}
		return float64(strings.Compare(s, other)), nil
	case "split", "match", "matchAll", "replace", "replaceAll":
		return r.regexpStringMethod(p, s, name, args)
	default:
		return nil, r.fail(p, "unknown method %q", name)
	}
}

func (r *Runtime) regexpStringMethod(p Pos, s, name string, args []any) (any, error) {
	minArgs, maxArgs := 1, 1
	if name == "split" {
		maxArgs = 2
	}
	if name == "replace" || name == "replaceAll" {
		minArgs, maxArgs = 2, 2
	}
	if len(args) < minArgs || len(args) > maxArgs {
		if minArgs == maxArgs {
			return nil, r.fail(p, "%s expects %d argument(s)", name, minArgs)
		}
		return nil, r.fail(p, "%s expects %d or %d arguments", name, minArgs, maxArgs)
	}
	pattern, ok := args[0].(string)
	if !ok {
		return nil, r.fail(p, "%s pattern must be a string", name)
	}
	re, err := regexp.Compile(pattern)
	if err != nil {
		return nil, r.fail(p, "invalid regular expression: %v", err)
	}

	switch name {
	case "split":
		limit := -1
		if len(args) == 2 {
			limit, ok = integerArg(args[1])
			if !ok || limit < 0 {
				return nil, r.fail(p, "split limit must be a non-negative integer")
			}
		}
		parts := re.Split(s, -1)
		if limit >= 0 && len(parts) > limit {
			parts = parts[:limit]
		}
		items := make([]any, len(parts))
		for i, part := range parts {
			items[i] = part
		}
		return &arrayValue{items}, nil
	case "match":
		indexes := re.FindStringSubmatchIndex(s)
		if indexes == nil {
			return nil, nil
		}
		return regexpMatchValue(s, indexes), nil
	case "matchAll":
		matches := re.FindAllStringSubmatchIndex(s, -1)
		items := make([]any, len(matches))
		for i, indexes := range matches {
			items[i] = regexpMatchValue(s, indexes)
		}
		return &arrayValue{items}, nil
	case "replace", "replaceAll":
		replacement, ok := args[1].(string)
		if !ok {
			return nil, r.fail(p, "%s replacement must be a string", name)
		}
		if name == "replaceAll" {
			return re.ReplaceAllString(s, replacement), nil
		}
		indexes := re.FindStringSubmatchIndex(s)
		if indexes == nil {
			return s, nil
		}
		expanded := re.ExpandString(nil, replacement, s, indexes)
		return s[:indexes[0]] + string(expanded) + s[indexes[1]:], nil
	}
	return nil, r.fail(p, "unknown method %q", name)
}

func regexpMatchValue(s string, indexes []int) *arrayValue {
	items := make([]any, len(indexes)/2)
	for i := range items {
		start, end := indexes[i*2], indexes[i*2+1]
		if start >= 0 {
			items[i] = s[start:end]
		}
	}
	return &arrayValue{items}
}

func (r *Runtime) arrayMethod(p Pos, array *arrayValue, name string, args []any) (any, error) {
	switch name {
	case "push":
		if len(args) == 0 {
			return nil, r.fail(p, "push expects at least 1 argument")
		}
		array.items = append(array.items, args...)
		return float64(len(array.items)), nil
	case "join":
		if len(args) > 1 {
			return nil, r.fail(p, "join expects at most 1 argument")
		}
		sep := ","
		if len(args) == 1 {
			var ok bool
			sep, ok = args[0].(string)
			if !ok {
				return nil, r.fail(p, "join separator must be a string")
			}
		}
		parts := make([]string, len(array.items))
		for i, v := range array.items {
			if v != nil {
				parts[i] = valueString(v)
			}
		}
		return strings.Join(parts, sep), nil
	case "splice":
		if len(args) == 0 {
			return nil, r.fail(p, "splice expects at least 1 argument")
		}
		start, ok := integerArg(args[0])
		if !ok {
			return nil, r.fail(p, "splice start must be an integer")
		}
		if start < 0 {
			start = len(array.items) + start
		}
		start = clamp(start, 0, len(array.items))
		deleteCount := len(array.items) - start
		if len(args) >= 2 {
			deleteCount, ok = integerArg(args[1])
			if !ok {
				return nil, r.fail(p, "splice delete count must be an integer")
			}
			deleteCount = clamp(deleteCount, 0, len(array.items)-start)
		}
		removed := append([]any(nil), array.items[start:start+deleteCount]...)
		var replacement []any
		if len(args) > 2 {
			replacement = append([]any(nil), args[2:]...)
		}
		items := make([]any, 0, len(array.items)-deleteCount+len(replacement))
		items = append(items, array.items[:start]...)
		items = append(items, replacement...)
		items = append(items, array.items[start+deleteCount:]...)
		array.items = items
		return &arrayValue{removed}, nil
	case "indexOf", "lastIndexOf":
		if len(args) < 1 || len(args) > 2 {
			return nil, r.fail(p, "%s expects 1 or 2 arguments", name)
		}
		if name == "indexOf" {
			start := 0
			if len(args) == 2 {
				var ok bool
				start, ok = integerArg(args[1])
				if !ok {
					return nil, r.fail(p, "indexOf start must be an integer")
				}
			}
			if start < 0 {
				start = len(array.items) + start
			}
			start = clamp(start, 0, len(array.items))
			for i := start; i < len(array.items); i++ {
				if reflect.DeepEqual(array.items[i], args[0]) {
					return float64(i), nil
				}
			}
			return float64(-1), nil
		}
		start := len(array.items) - 1
		if len(args) == 2 {
			var ok bool
			start, ok = integerArg(args[1])
			if !ok {
				return nil, r.fail(p, "lastIndexOf start must be an integer")
			}
			if start < 0 {
				start = len(array.items) + start
			}
		}
		if start >= len(array.items) {
			start = len(array.items) - 1
		}
		for i := start; i >= 0; i-- {
			if reflect.DeepEqual(array.items[i], args[0]) {
				return float64(i), nil
			}
		}
		return float64(-1), nil
	default:
		return nil, r.fail(p, "unknown method %q", name)
	}
}

func integerArg(v any) (int, bool) {
	n, ok := v.(float64)
	if !ok || math.IsNaN(n) || math.IsInf(n, 0) || math.Trunc(n) != n || n < float64(-int(^uint(0)>>1)-1) || n > float64(int(^uint(0)>>1)) {
		return 0, false
	}
	return int(n), true
}

func clamp(v, low, high int) int {
	if v < low {
		return low
	}
	if v > high {
		return high
	}
	return v
}

func valueString(v any) string {
	switch x := v.(type) {
	case nil:
		return "null"
	case string:
		return x
	case bool:
		return strconv.FormatBool(x)
	case float64:
		return strconv.FormatFloat(x, 'f', -1, 64)
	case *arrayValue:
		parts := make([]string, len(x.items))
		for i, item := range x.items {
			if item != nil {
				parts[i] = valueString(item)
			}
		}
		return strings.Join(parts, ",")
	case map[string]any:
		b, err := json.Marshal(exportValue(x))
		if err == nil {
			return string(b)
		}
	}
	return fmt.Sprint(v)
}

func truth(v any) bool {
	switch x := v.(type) {
	case nil:
		return false
	case bool:
		return x
	case float64:
		return x != 0
	case string:
		return x != ""
	default:
		return true
	}
}

func Execute(src string, root any, maxSteps int) (any, any, error) {
	return ExecuteWithOutput(src, root, maxSteps, io.Discard)
}

func ExecuteWithOutput(src string, root any, maxSteps int, output io.Writer) (any, any, error) {
	p, e := Parse(src)
	if e != nil {
		return nil, nil, e
	}
	r := NewRuntime(root, maxSteps)
	if output != nil {
		r.Output = output
	}
	e = r.Run(p)
	return r.Root(), exportValue(r.Last), e
}

func importValue(v any) any {
	switch x := v.(type) {
	case []any:
		a := make([]any, len(x))
		for i, v := range x {
			a[i] = importValue(v)
		}
		return &arrayValue{a}
	case map[string]any:
		o := make(map[string]any, len(x))
		for k, v := range x {
			o[k] = importValue(v)
		}
		return o
	default:
		return v
	}
}

func exportValue(v any) any {
	switch x := v.(type) {
	case *arrayValue:
		a := make([]any, len(x.items))
		for i, v := range x.items {
			a[i] = exportValue(v)
		}
		return a
	case map[string]any:
		o := make(map[string]any, len(x))
		for k, v := range x {
			o[k] = exportValue(v)
		}
		return o
	default:
		return v
	}
}
