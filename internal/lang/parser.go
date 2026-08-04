package lang

import (
	"fmt"
	"strconv"
)

type parser struct {
	ts       []token
	i, loops int
}

func Parse(src string) (*Program, error) {
	ts, e := lex(src)
	if e != nil {
		return nil, e
	}
	p := &parser{ts: ts}
	var list []Stmt
	for p.peek().kind != tEOF {
		s, e := p.stmt()
		if e != nil {
			return nil, e
		}
		list = append(list, s)
	}
	return &Program{list}, nil
}
func (p *parser) peek() token { return p.ts[p.i] }
func (p *parser) next() token {
	t := p.peek()
	if p.i < len(p.ts)-1 {
		p.i++
	}
	return t
}
func (p *parser) match(k tokenKind) bool {
	if p.peek().kind == k {
		p.next()
		return true
	}
	return false
}
func (p *parser) need(k tokenKind, msg string) (token, error) {
	if p.peek().kind != k {
		return token{}, p.err(p.peek(), msg)
	}
	return p.next(), nil
}
func (p *parser) err(t token, msg string) error { return &LangError{"SyntaxError", t.pos, msg} }

func (p *parser) stmt() (Stmt, error) {
	t := p.peek()
	switch t.kind {
	case tLBrace:
		return p.block()
	case tIf:
		return p.ifStmt()
	case tFor:
		return p.forStmt()
	case tDelete:
		p.next()
		x, e := p.expression()
		if e != nil {
			return nil, e
		}
		if _, ok := x.(*Member); !ok {
			return nil, p.err(t, "delete target must be a member")
		}
		if e = p.endStmt(); e != nil {
			return nil, e
		}
		return &DeleteStmt{t.pos, x}, nil
	case tBreak:
		p.next()
		if p.loops == 0 {
			return nil, p.err(t, "break outside loop")
		}
		if e := p.endStmt(); e != nil {
			return nil, e
		}
		return &BreakStmt{t.pos}, nil
	case tContinue:
		p.next()
		if p.loops == 0 {
			return nil, p.err(t, "continue outside loop")
		}
		if e := p.endStmt(); e != nil {
			return nil, e
		}
		return &ContinueStmt{t.pos}, nil
	default:
		x, e := p.expression()
		if e != nil {
			return nil, e
		}
		if e := p.endStmt(); e != nil {
			return nil, e
		}
		return &ExprStmt{t.pos, x}, nil
	}
}
func (p *parser) endStmt() error {
	if p.match(tSemi) || p.peek().kind == tRBrace || p.peek().kind == tEOF {
		return nil
	}
	return p.err(p.peek(), "expected ';' between statements")
}
func (p *parser) block() (*Block, error) {
	t, _ := p.need(tLBrace, "expected '{'")
	var xs []Stmt
	for p.peek().kind != tRBrace {
		if p.peek().kind == tEOF {
			return nil, p.err(p.peek(), "expected '}'")
		}
		s, e := p.stmt()
		if e != nil {
			return nil, e
		}
		xs = append(xs, s)
	}
	p.next()
	return &Block{t.pos, xs}, nil
}
func (p *parser) ifStmt() (Stmt, error) {
	t := p.next()
	if _, e := p.need(tLParen, "expected '(' after if"); e != nil {
		return nil, e
	}
	c, e := p.expression()
	if e != nil {
		return nil, e
	}
	if _, e = p.need(tRParen, "expected ')' after condition"); e != nil {
		return nil, e
	}
	b, e := p.block()
	if e != nil {
		return nil, e
	}
	var alt Stmt
	if p.match(tElse) {
		if p.peek().kind == tIf {
			alt, e = p.ifStmt()
		} else {
			alt, e = p.block()
		}
		if e != nil {
			return nil, e
		}
	}
	return &IfStmt{t.pos, c, b, alt}, nil
}
func (p *parser) forStmt() (Stmt, error) {
	t := p.next()
	if _, e := p.need(tLParen, "expected '(' after for"); e != nil {
		return nil, e
	}
	n, e := p.need(tIdent, "expected loop variable")
	if e != nil {
		return nil, e
	}
	if _, e = p.need(tIn, "expected 'in'"); e != nil {
		return nil, e
	}
	src, e := p.expression()
	if e != nil {
		return nil, e
	}
	if _, e = p.need(tRParen, "expected ')' after source"); e != nil {
		return nil, e
	}
	p.loops++
	b, e := p.block()
	p.loops--
	if e != nil {
		return nil, e
	}
	return &ForStmt{t.pos, n.lit, src, b}, nil
}

var prec = map[tokenKind]int{tAssign: 1, tPlusAssign: 1, tMinusAssign: 1, tStarAssign: 1, tSlashAssign: 1, tOr: 2, tAnd: 3, tEq: 4, tNe: 4, tGT: 5, tGE: 5, tLT: 5, tLE: 5, tPlus: 6, tMinus: 6, tStar: 7, tSlash: 7}

func (p *parser) expression() (Expr, error) { return p.binary(1) }
func (p *parser) binary(min int) (Expr, error) {
	left, e := p.unary()
	if e != nil {
		return nil, e
	}
	for {
		op := p.peek()
		q, ok := prec[op.kind]
		if !ok || q < min {
			break
		}
		p.next()
		next := q + 1
		if q == 1 {
			next = q
		}
		right, e := p.binary(next)
		if e != nil {
			return nil, e
		}
		if q == 1 {
			switch left.(type) {
			case *Variable:
			case *Member:
			default:
				return nil, p.err(op, "invalid assignment target")
			}
			left = &Assign{op.pos, op.kind, left, right}
		} else {
			left = &Binary{op.pos, op.kind, left, right}
		}
	}
	return left, nil
}
func (p *parser) unary() (Expr, error) {
	if p.peek().kind == tBang || p.peek().kind == tMinus {
		t := p.next()
		x, e := p.unary()
		if e != nil {
			return nil, e
		}
		return &Unary{t.pos, t.kind, x}, nil
	}
	return p.postfix()
}
func (p *parser) postfix() (Expr, error) {
	x, e := p.primary()
	if e != nil {
		return nil, e
	}
	for {
		if p.match(tDot) {
			n, e := p.need(tIdent, "expected property name")
			if e != nil {
				return nil, e
			}
			x = &Member{n.pos, x, &Literal{n.pos, n.lit}}
			continue
		}
		if p.match(tLBracket) {
			pos := p.peek().pos
			k, e := p.expression()
			if e != nil {
				return nil, e
			}
			if _, e = p.need(tRBracket, "expected ']'"); e != nil {
				return nil, e
			}
			x = &Member{pos, x, k}
			continue
		}
		if p.match(tLParen) {
			m, ok := x.(*Member)
			if !ok {
				return nil, p.err(p.peek(), "only named functions and methods can be called")
			}
			name, ok := m.Key.(*Literal)
			if !ok {
				return nil, p.err(token{pos: m.P}, "method name must be a property name")
			}
			method, ok := name.Value.(string)
			if !ok {
				return nil, p.err(token{pos: m.P}, "method name must be a string")
			}
			args, e := p.callArgs()
			if e != nil {
				return nil, e
			}
			x = &MethodCall{m.P, m.Object, method, args}
			continue
		}
		break
	}
	return x, nil
}
func (p *parser) primary() (Expr, error) {
	t := p.next()
	switch t.kind {
	case tNumber:
		n, _ := strconv.ParseFloat(t.lit, 64)
		return &Literal{t.pos, n}, nil
	case tString:
		return &Literal{t.pos, t.lit}, nil
	case tTrue:
		return &Literal{t.pos, true}, nil
	case tFalse:
		return &Literal{t.pos, false}, nil
	case tNull:
		return &Literal{t.pos, nil}, nil
	case tDollar:
		return &Variable{t.pos, "$"}, nil
	case tIdent:
		if p.match(tLParen) {
			args, e := p.callArgs()
			if e != nil {
				return nil, e
			}
			return &Call{t.pos, t.lit, args}, nil
		}
		return &Variable{t.pos, t.lit}, nil
	case tLParen:
		x, e := p.expression()
		if e != nil {
			return nil, e
		}
		_, e = p.need(tRParen, "expected ')'")
		return x, e
	case tLBracket:
		var xs []Expr
		if !p.match(tRBracket) {
			for {
				x, e := p.expression()
				if e != nil {
					return nil, e
				}
				xs = append(xs, x)
				if p.match(tRBracket) {
					break
				}
				if _, e = p.need(tComma, "expected ',' or ']'"); e != nil {
					return nil, e
				}
				if p.match(tRBracket) {
					break
				}
			}
		}
		return &ArrayExpr{t.pos, xs}, nil
	case tLBrace:
		var xs []ObjectItem
		if !p.match(tRBrace) {
			for {
				k := p.next()
				if k.kind != tString && k.kind != tIdent {
					return nil, p.err(k, "expected object key")
				}
				if _, e := p.need(tColon, "expected ':'"); e != nil {
					return nil, e
				}
				v, e := p.expression()
				if e != nil {
					return nil, e
				}
				xs = append(xs, ObjectItem{k.lit, v})
				if p.match(tRBrace) {
					break
				}
				if _, e = p.need(tComma, "expected ',' or '}'"); e != nil {
					return nil, e
				}
				if p.match(tRBrace) {
					break
				}
			}
		}
		return &ObjectExpr{t.pos, xs}, nil
	default:
		return nil, p.err(t, fmt.Sprintf("unexpected token %q", t.lit))
	}
}

func (p *parser) callArgs() ([]Expr, error) {
	var args []Expr
	if p.match(tRParen) {
		return args, nil
	}
	for {
		x, e := p.expression()
		if e != nil {
			return nil, e
		}
		args = append(args, x)
		if p.match(tRParen) {
			return args, nil
		}
		if _, e = p.need(tComma, "expected ',' or ')'"); e != nil {
			return nil, e
		}
	}
}
