package jsonc

import (
	"encoding/json"
	"fmt"
	"reflect"
	"sort"
	"strconv"
	"strings"
)

type Kind uint8

const (
	Null Kind = iota
	Bool
	Number
	String
	Array
	Object
)

type Node struct {
	Kind        Kind
	Start, End  int
	Value       any
	Items       []*Item
	CloseTrivia string
}
type Item struct {
	Key                     string
	Leading, Head, Trailing string
	Value                   *Node
	Comma                   bool
}
type Document struct {
	Source, Prefix, Suffix string
	Root                   *Node
	Newline                string
}

type Error struct {
	Line, Col int
	Message   string
}

func (e *Error) Error() string { return fmt.Sprintf("%d:%d: JSONCError: %s", e.Line, e.Col, e.Message) }

type parser struct {
	src          string
	i, line, col int
}

func Parse(src string) (*Document, error) {
	p := &parser{src: src, line: 1, col: 1}
	if strings.HasPrefix(src, "\xEF\xBB\xBF") {
		p.i = 3
		p.col = 2
	}
	a := 0
	if e := p.trivia(); e != nil {
		return nil, e
	}
	prefix := src[a:p.i]
	n, e := p.value()
	if e != nil {
		return nil, e
	}
	a = p.i
	if e = p.trivia(); e != nil {
		return nil, e
	}
	suffix := src[a:p.i]
	if p.i != len(src) {
		return nil, p.err("unexpected content after JSON value")
	}
	nl := "\n"
	if strings.Contains(src, "\r\n") {
		nl = "\r\n"
	}
	return &Document{src, prefix, suffix, n, nl}, nil
}
func (p *parser) err(msg string) error { return &Error{p.line, p.col, msg} }
func (p *parser) advance() byte {
	c := p.src[p.i]
	p.i++
	if c == '\n' {
		p.line++
		p.col = 1
	} else {
		p.col++
	}
	return c
}
func (p *parser) trivia() error {
	for p.i < len(p.src) {
		switch p.src[p.i] {
		case ' ', '\t', '\r', '\n':
			p.advance()
			continue
		}
		if strings.HasPrefix(p.src[p.i:], "//") {
			for p.i < len(p.src) && p.advance() != '\n' {
			}
			continue
		}
		if strings.HasPrefix(p.src[p.i:], "/*") {
			p.advance()
			p.advance()
			for p.i < len(p.src) && !strings.HasPrefix(p.src[p.i:], "*/") {
				p.advance()
			}
			if p.i >= len(p.src) {
				return p.err("unterminated block comment")
			}
			p.advance()
			p.advance()
			continue
		}
		break
	}
	return nil
}
func (p *parser) value() (*Node, error) {
	if p.i >= len(p.src) {
		return nil, p.err("expected JSON value")
	}
	s := p.i
	switch p.src[p.i] {
	case '{':
		return p.object()
	case '[':
		return p.array()
	case '"':
		v, e := p.string()
		if e != nil {
			return nil, e
		}
		return &Node{Kind: String, Start: s, End: p.i, Value: v}, nil
	case 't':
		return p.word("true", Bool, true)
	case 'f':
		return p.word("false", Bool, false)
	case 'n':
		return p.word("null", Null, nil)
	default:
		if p.src[p.i] == '-' || (p.src[p.i] >= '0' && p.src[p.i] <= '9') {
			return p.number()
		}
	}
	return nil, p.err("expected JSON value")
}
func (p *parser) word(w string, k Kind, v any) (*Node, error) {
	s := p.i
	if !strings.HasPrefix(p.src[p.i:], w) {
		return nil, p.err("invalid literal")
	}
	for range len(w) {
		p.advance()
	}
	return &Node{Kind: k, Start: s, End: p.i, Value: v}, nil
}
func (p *parser) string() (string, error) {
	s := p.i
	p.advance()
	for p.i < len(p.src) {
		c := p.advance()
		if c == '"' {
			var v string
			if e := json.Unmarshal([]byte(p.src[s:p.i]), &v); e != nil {
				return "", p.err("invalid string")
			}
			return v, nil
		}
		if c == '\\' {
			if p.i >= len(p.src) {
				break
			}
			p.advance()
		} else if c < ' ' {
			return "", p.err("control character in string")
		}
	}
	return "", p.err("unterminated string")
}
func (p *parser) number() (*Node, error) {
	s := p.i
	if p.src[p.i] == '-' {
		p.advance()
		if p.i >= len(p.src) {
			return nil, p.err("invalid number")
		}
	}
	if p.src[p.i] == '0' {
		p.advance()
		if p.i < len(p.src) && p.src[p.i] >= '0' && p.src[p.i] <= '9' {
			return nil, p.err("leading zero in number")
		}
	} else {
		if p.src[p.i] < '1' || p.src[p.i] > '9' {
			return nil, p.err("invalid number")
		}
		for p.i < len(p.src) && p.src[p.i] >= '0' && p.src[p.i] <= '9' {
			p.advance()
		}
	}
	if p.i < len(p.src) && p.src[p.i] == '.' {
		p.advance()
		if p.i >= len(p.src) || p.src[p.i] < '0' || p.src[p.i] > '9' {
			return nil, p.err("invalid fraction")
		}
		for p.i < len(p.src) && p.src[p.i] >= '0' && p.src[p.i] <= '9' {
			p.advance()
		}
	}
	if p.i < len(p.src) && (p.src[p.i] == 'e' || p.src[p.i] == 'E') {
		p.advance()
		if p.i < len(p.src) && (p.src[p.i] == '+' || p.src[p.i] == '-') {
			p.advance()
		}
		if p.i >= len(p.src) || p.src[p.i] < '0' || p.src[p.i] > '9' {
			return nil, p.err("invalid exponent")
		}
		for p.i < len(p.src) && p.src[p.i] >= '0' && p.src[p.i] <= '9' {
			p.advance()
		}
	}
	raw := p.src[s:p.i]
	v, e := strconv.ParseFloat(raw, 64)
	if e != nil {
		return nil, p.err("invalid number")
	}
	return &Node{Kind: Number, Start: s, End: p.i, Value: v}, nil
}
func (p *parser) object() (*Node, error) {
	n := &Node{Kind: Object, Start: p.i}
	p.advance()
	m := map[string]any{}
	for {
		a := p.i
		if e := p.trivia(); e != nil {
			return nil, e
		}
		leading := p.src[a:p.i]
		if p.i < len(p.src) && p.src[p.i] == '}' {
			n.CloseTrivia = leading
			p.advance()
			n.End = p.i
			n.Value = m
			return n, nil
		}
		if p.i >= len(p.src) || p.src[p.i] != '"' {
			return nil, p.err("object key must be a quoted string")
		}
		headStart := p.i
		k, e := p.string()
		if e != nil {
			return nil, e
		}
		if _, ok := m[k]; ok {
			return nil, p.err("duplicate object key " + strconv.Quote(k))
		}
		if e = p.trivia(); e != nil {
			return nil, e
		}
		if p.i >= len(p.src) || p.src[p.i] != ':' {
			return nil, p.err("expected ':'")
		}
		p.advance()
		if e = p.trivia(); e != nil {
			return nil, e
		}
		v, e := p.value()
		if e != nil {
			return nil, e
		}
		head := p.src[headStart:v.Start]
		a = p.i
		if e = p.trivia(); e != nil {
			return nil, e
		}
		trailing := p.src[a:p.i]
		comma := false
		if p.i < len(p.src) && p.src[p.i] == ',' {
			comma = true
			p.advance()
		}
		n.Items = append(n.Items, &Item{Key: k, Leading: leading, Head: head, Trailing: trailing, Value: v, Comma: comma})
		m[k] = v.Value
		if !comma {
			if p.i >= len(p.src) || p.src[p.i] != '}' {
				return nil, p.err("expected ',' or '}'")
			}
		}
	}
}
func (p *parser) array() (*Node, error) {
	n := &Node{Kind: Array, Start: p.i}
	p.advance()
	var vals []any
	for {
		a := p.i
		if e := p.trivia(); e != nil {
			return nil, e
		}
		leading := p.src[a:p.i]
		if p.i < len(p.src) && p.src[p.i] == ']' {
			n.CloseTrivia = leading
			p.advance()
			n.End = p.i
			n.Value = vals
			return n, nil
		}
		v, e := p.value()
		if e != nil {
			return nil, e
		}
		a = p.i
		if e = p.trivia(); e != nil {
			return nil, e
		}
		trailing := p.src[a:p.i]
		comma := false
		if p.i < len(p.src) && p.src[p.i] == ',' {
			comma = true
			p.advance()
		}
		n.Items = append(n.Items, &Item{Leading: leading, Trailing: trailing, Value: v, Comma: comma})
		vals = append(vals, v.Value)
		if !comma {
			if p.i >= len(p.src) || p.src[p.i] != ']' {
				return nil, p.err("expected ',' or ']'")
			}
		}
	}
}

func Clone(v any) any {
	switch x := v.(type) {
	case map[string]any:
		o := make(map[string]any, len(x))
		for k, v := range x {
			o[k] = Clone(v)
		}
		return o
	case []any:
		a := make([]any, len(x))
		for i, v := range x {
			a[i] = Clone(v)
		}
		return a
	default:
		return v
	}
}
func (d *Document) Preserve(v any) (string, error) {
	body, e := d.render(d.Root, v)
	if e != nil {
		return "", e
	}
	return d.Prefix + body + d.Suffix, nil
}
func (d *Document) render(n *Node, v any) (string, error) {
	if reflect.DeepEqual(n.Value, v) {
		return d.Source[n.Start:n.End], nil
	}
	switch n.Kind {
	case Object:
		if x, ok := v.(map[string]any); ok {
			return d.renderObject(n, x)
		}
	case Array:
		if x, ok := v.([]any); ok {
			return d.renderArray(n, x)
		}
	}
	return encode(v)
}
func (d *Document) renderObject(n *Node, v map[string]any) (string, error) {
	var b strings.Builder
	b.WriteByte('{')
	kept := make([]*Item, 0)
	seen := map[string]bool{}
	for _, it := range n.Items {
		if _, ok := v[it.Key]; ok {
			kept = append(kept, it)
			seen[it.Key] = true
		}
	}
	newKeys := make([]string, 0)
	for k := range v {
		if !seen[k] {
			newKeys = append(newKeys, k)
		}
	}
	sort.Strings(newKeys)
	total := len(kept) + len(newKeys)
	for i, it := range kept {
		b.WriteString(it.Leading)
		b.WriteString(it.Head)
		s, e := d.render(it.Value, v[it.Key])
		if e != nil {
			return "", e
		}
		b.WriteString(s)
		needsComma := i < len(kept)-1 || len(newKeys) > 0
		preserveTrailing := !needsComma && len(kept) == len(n.Items) && it.Comma
		if needsComma && !it.Comma {
			b.WriteByte(',')
			b.WriteString(it.Trailing)
		} else {
			b.WriteString(it.Trailing)
			if needsComma || preserveTrailing {
				b.WriteByte(',')
			}
		}
	}
	style := d.style(n)
	for j, k := range newKeys {
		if len(kept) == 0 && j == 0 {
			b.WriteString(style.first)
		} else {
			b.WriteString(style.next)
		}
		kb := appendJSONString(nil, k)
		b.Write(kb)
		b.WriteString(style.colon)
		s, e := encode(v[k])
		if e != nil {
			return "", e
		}
		b.WriteString(s)
		if len(kept)+j < total-1 || (len(n.Items) > 0 && n.Items[len(n.Items)-1].Comma) {
			b.WriteByte(',')
		}
	}
	close := n.CloseTrivia
	lastOriginalKept := len(newKeys) == 0 && len(kept) > 0 && kept[len(kept)-1] == n.Items[len(n.Items)-1]
	if close == "" && !lastOriginalKept {
		if len(n.Items) > 0 {
			close = closingWhitespace(n.Items[len(n.Items)-1].Trailing)
		} else if total > 0 && strings.Contains(style.first, "\n") {
			close = d.Newline
		}
	}
	b.WriteString(close)
	b.WriteByte('}')
	return b.String(), nil
}
func (d *Document) renderArray(n *Node, v []any) (string, error) {
	var b strings.Builder
	b.WriteByte('[')
	pairs := matchItems(n.Items, v)
	style := d.style(n)
	for i, pair := range pairs {
		needsComma := i < len(pairs)-1
		if pair.old >= 0 {
			it := n.Items[pair.old]
			b.WriteString(it.Leading)
			s, e := d.render(it.Value, v[pair.new])
			if e != nil {
				return "", e
			}
			b.WriteString(s)
			preserveTrailing := !needsComma && len(pairs) == len(n.Items) && it.Comma
			if needsComma && !it.Comma {
				b.WriteByte(',')
				b.WriteString(it.Trailing)
			} else {
				b.WriteString(it.Trailing)
				if needsComma || preserveTrailing {
					b.WriteByte(',')
				}
			}
		} else {
			if i == 0 {
				b.WriteString(style.first)
			} else {
				b.WriteString(style.next)
			}
			s, e := encode(v[pair.new])
			if e != nil {
				return "", e
			}
			b.WriteString(s)
			if needsComma || (i == len(pairs)-1 && len(n.Items) > 0 && n.Items[len(n.Items)-1].Comma) {
				b.WriteByte(',')
			}
		}
	}
	close := n.CloseTrivia
	lastOriginalKept := len(pairs) > 0 && pairs[len(pairs)-1].old == len(n.Items)-1
	if close == "" && !lastOriginalKept {
		if len(n.Items) > 0 {
			close = closingWhitespace(n.Items[len(n.Items)-1].Trailing)
		} else if len(pairs) > 0 && strings.Contains(style.first, "\n") {
			close = d.Newline
		}
	}
	b.WriteString(close)
	b.WriteByte(']')
	return b.String(), nil
}

type pair struct{ old, new int }

func matchItems(old []*Item, v []any) []pair {
	if len(old) == len(v) {
		r := make([]pair, len(v))
		for i := range v {
			r[i] = pair{i, i}
		}
		return r
	}
	m, n := len(old), len(v)
	dp := make([][]int, m+1)
	for i := range dp {
		dp[i] = make([]int, n+1)
	}
	for i := m - 1; i >= 0; i-- {
		for j := n - 1; j >= 0; j-- {
			if reflect.DeepEqual(old[i].Value.Value, v[j]) {
				dp[i][j] = dp[i+1][j+1] + 1
			} else if dp[i+1][j] >= dp[i][j+1] {
				dp[i][j] = dp[i+1][j]
			} else {
				dp[i][j] = dp[i][j+1]
			}
		}
	}
	var r []pair
	i, j := 0, 0
	for j < n {
		if i < m && reflect.DeepEqual(old[i].Value.Value, v[j]) {
			r = append(r, pair{i, j})
			i++
			j++
		} else if i < m && dp[i+1][j] >= dp[i][j+1] {
			i++
		} else {
			r = append(r, pair{-1, j})
			j++
		}
	}
	return r
}

type styleInfo struct{ first, next, colon string }

func (d *Document) style(n *Node) styleInfo {
	s := styleInfo{colon: ": "}
	if len(n.Items) > 0 {
		it := n.Items[0]
		s.first = cleanTrivia(it.Leading)
		s.next = s.first
		s.colon = keySeparator(it.Head)
	} else {
		if strings.Contains(n.CloseTrivia, "\n") || strings.Contains(n.CloseTrivia, "\r") {
			s.first = d.Newline + "  "
			s.next = s.first
		} else {
			s.first = n.CloseTrivia
			s.next = " "
		}
	}
	if s.first == "" {
		s.next = " "
	}
	return s
}

func keySeparator(head string) string {
	inEscape := false
	for i := 1; i < len(head); i++ {
		c := head[i]
		if inEscape {
			inEscape = false
			continue
		}
		if c == '\\' {
			inEscape = true
			continue
		}
		if c == '"' {
			sep := head[i+1:]
			if strings.ContainsAny(sep, "/\r\n") {
				return ": "
			}
			return sep
		}
	}
	return ": "
}
func cleanTrivia(s string) string {
	if strings.Contains(s, "\n") {
		i := strings.LastIndex(s, "\n")
		return s[i:]
	}
	if strings.Contains(s, "\r") {
		i := strings.LastIndex(s, "\r")
		return s[i:]
	}
	return " "
}

func closingWhitespace(s string) string {
	lastEnd := -1
	for i := 0; i < len(s); {
		if strings.HasPrefix(s[i:], "//") {
			j := i + 2
			for j < len(s) && s[j] != '\n' && s[j] != '\r' {
				j++
			}
			lastEnd, i = j, j
			continue
		}
		if strings.HasPrefix(s[i:], "/*") {
			j := strings.Index(s[i+2:], "*/")
			if j < 0 {
				return ""
			}
			j = i + 2 + j + 2
			lastEnd, i = j, j
			continue
		}
		i++
	}
	if lastEnd >= 0 {
		return s[lastEnd:]
	}
	return s
}
func encode(v any) (string, error) { b, e := Marshal(v); return string(b), e }

func Compact(v any) (string, error) { return encode(v) }
func PrettyPreserve(src string, indent string) (string, error) {
	if indent == "" {
		indent = "  "
	}
	var out strings.Builder
	level := 0
	needIndent := false
	inString := false
	escape := false
	for i := 0; i < len(src); {
		if inString {
			c := src[i]
			out.WriteByte(c)
			i++
			if escape {
				escape = false
			} else if c == '\\' {
				escape = true
			} else if c == '"' {
				inString = false
			}
			continue
		}
		if src[i] == '"' {
			if needIndent {
				out.WriteString(strings.Repeat(indent, level))
				needIndent = false
			}
			inString = true
			out.WriteByte(src[i])
			i++
			continue
		}
		if strings.HasPrefix(src[i:], "//") {
			if needIndent {
				out.WriteString(strings.Repeat(indent, level))
				needIndent = false
			} else if out.Len() > 0 {
				out.WriteByte(' ')
			}
			j := i + 2
			for j < len(src) && src[j] != '\n' && src[j] != '\r' {
				j++
			}
			out.WriteString(src[i:j])
			out.WriteByte('\n')
			needIndent = true
			i = j
			for i < len(src) && (src[i] == '\n' || src[i] == '\r') {
				i++
			}
			continue
		}
		if strings.HasPrefix(src[i:], "/*") {
			if needIndent {
				out.WriteString(strings.Repeat(indent, level))
				needIndent = false
			} else if out.Len() > 0 {
				out.WriteByte(' ')
			}
			j := strings.Index(src[i+2:], "*/")
			if j < 0 {
				return "", fmt.Errorf("unterminated comment")
			}
			j = i + 2 + j + 2
			out.WriteString(src[i:j])
			i = j
			continue
		}
		c := src[i]
		switch c {
		case ' ', '\t', '\r', '\n':
			i++
			continue
		case '{', '[':
			if needIndent {
				out.WriteString(strings.Repeat(indent, level))
				needIndent = false
			}
			out.WriteByte(c)
			j := i + 1
			for j < len(src) && (src[j] == ' ' || src[j] == '\t' || src[j] == '\r' || src[j] == '\n') {
				j++
			}
			matching := byte('}')
			if c == '[' {
				matching = ']'
			}
			if j < len(src) && src[j] == matching {
				out.WriteByte(matching)
				i = j + 1
				continue
			} else {
				level++
				out.WriteByte('\n')
				needIndent = true
			}
		case '}', ']':
			level--
			if !needIndent {
				out.WriteByte('\n')
			}
			out.WriteString(strings.Repeat(indent, level))
			out.WriteByte(c)
			needIndent = false
		case ',':
			out.WriteByte(c)
			out.WriteByte('\n')
			needIndent = true
		case ':':
			out.WriteString(": ")
		case '/':
			out.WriteByte(c)
		default:
			if needIndent {
				out.WriteString(strings.Repeat(indent, level))
				needIndent = false
			}
			out.WriteByte(c)
		}
		i++
	}
	return strings.TrimSpace(out.String()), nil
}
