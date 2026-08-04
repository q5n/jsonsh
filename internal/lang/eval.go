package lang

import (
	"fmt"
	"math"
	"reflect"
	"sort"
	"strings"
	"unicode/utf8"
)

type Runtime struct {
	Globals         map[string]any
	MaxSteps, steps int
	Last            any
}

// arrayValue gives arrays reference identity, matching JavaScript array behavior.
type arrayValue struct{ items []any }

func NewRuntime(root any, max int) *Runtime {
	if max <= 0 {
		max = 1000000
	}
	return &Runtime{Globals: map[string]any{"$": importValue(root)}, MaxSteps: max}
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
	case map[string]any:
		k, ok := key.(string)
		if !ok {
			return nil, r.fail(p, "object key must be string")
		}
		v, ok := x[k]
		if !ok {
			return nil, r.fail(p, "object property %q does not exist", k)
		}
		return v, nil
	case *arrayValue:
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
		if x, ok := a.(string); ok {
			y, ok := b.(string)
			if !ok {
				return nil, r.fail(p, "'+' operands must both be strings or numbers")
			}
			return x + y, nil
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
				return nil, r.fail(p, "object property %q does not exist", k)
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
	case "length":
		if len(args) != 1 {
			return nil, r.fail(c.P, "length expects 1 argument")
		}
		switch x := args[0].(type) {
		case string:
			return float64(utf8.RuneCountInString(x)), nil
		case *arrayValue:
			return float64(len(x.items)), nil
		case map[string]any:
			return float64(len(x)), nil
		}
		return nil, r.fail(c.P, "length requires string, array, or object")
	case "has":
		if len(args) != 2 {
			return nil, r.fail(c.P, "has expects 2 arguments")
		}
		switch x := args[0].(type) {
		case string:
			y, ok := args[1].(string)
			if !ok {
				return nil, r.fail(c.P, "string has requires string needle")
			}
			return strings.Contains(x, y), nil
		case *arrayValue:
			for _, v := range x.items {
				if reflect.DeepEqual(v, args[1]) {
					return true, nil
				}
			}
			return false, nil
		case map[string]any:
			y, ok := args[1].(string)
			if !ok {
				return nil, r.fail(c.P, "object has requires string key")
			}
			_, ok = x[y]
			return ok, nil
		}
		return nil, r.fail(c.P, "has requires string, array, or object")
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
	if c.Name != "push" {
		return nil, r.fail(c.P, "unknown method %q", c.Name)
	}
	array, ok := receiver.(*arrayValue)
	if !ok {
		return nil, r.fail(c.P, "push requires an array receiver")
	}
	if len(c.Args) == 0 {
		return nil, r.fail(c.P, "push expects at least 1 argument")
	}
	values := make([]any, len(c.Args))
	for i, arg := range c.Args {
		v, e := r.eval(arg)
		if e != nil {
			return nil, e
		}
		values[i] = v
	}
	array.items = append(array.items, values...)
	return float64(len(array.items)), nil
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
	p, e := Parse(src)
	if e != nil {
		return nil, nil, e
	}
	r := NewRuntime(root, maxSteps)
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
