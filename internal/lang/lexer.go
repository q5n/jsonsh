package lang

import (
	"strconv"
	"strings"
	"unicode"
	"unicode/utf8"
)

type lexer struct {
	src       string
	off       int
	line, col int
}

func lex(src string) ([]token, error) {
	l := &lexer{src: src, line: 1, col: 1}
	var out []token
	for {
		if err := l.skipSpace(); err != nil {
			return nil, err
		}
		p := l.pos()
		if l.off >= len(l.src) {
			return append(out, token{kind: tEOF, pos: p}), nil
		}
		r, _ := utf8.DecodeRuneInString(l.src[l.off:])
		if unicode.IsLetter(r) || r == '_' {
			out = append(out, l.ident())
			continue
		}
		if unicode.IsDigit(r) {
			t, err := l.number()
			if err != nil {
				return nil, err
			}
			out = append(out, t)
			continue
		}
		if r == '\'' || r == '"' {
			t, err := l.str()
			if err != nil {
				return nil, err
			}
			out = append(out, t)
			continue
		}
		pairs := map[string]tokenKind{"+=": tPlusAssign, "-=": tMinusAssign, "*=": tStarAssign, "/=": tSlashAssign, "==": tEq, "!=": tNe, ">=": tGE, "<=": tLE, "&&": tAnd, "||": tOr}
		if l.off+2 <= len(l.src) {
			if k, ok := pairs[l.src[l.off:l.off+2]]; ok {
				out = append(out, token{k, l.src[l.off : l.off+2], p})
				l.advance()
				l.advance()
				continue
			}
		}
		single := map[rune]tokenKind{'$': tDollar, '(': tLParen, ')': tRParen, '{': tLBrace, '}': tRBrace, '[': tLBracket, ']': tRBracket, '.': tDot, ',': tComma, ':': tColon, ';': tSemi, '+': tPlus, '-': tMinus, '*': tStar, '/': tSlash, '!': tBang, '=': tAssign, '>': tGT, '<': tLT}
		if k, ok := single[r]; ok {
			out = append(out, token{k, string(r), p})
			l.advance()
			continue
		}
		return nil, &LangError{"LexError", p, "unexpected character " + strconv.QuoteRune(r)}
	}
}

func (l *lexer) pos() Pos { return Pos{l.line, l.col} }
func (l *lexer) advance() rune {
	r, n := utf8.DecodeRuneInString(l.src[l.off:])
	l.off += n
	if r == '\n' {
		l.line++
		l.col = 1
	} else {
		l.col++
	}
	return r
}
func (l *lexer) skipSpace() error {
	for l.off < len(l.src) {
		r, _ := utf8.DecodeRuneInString(l.src[l.off:])
		if unicode.IsSpace(r) {
			l.advance()
			continue
		}
		if strings.HasPrefix(l.src[l.off:], "//") {
			for l.off < len(l.src) && l.advance() != '\n' {
			}
			continue
		}
		if strings.HasPrefix(l.src[l.off:], "/*") {
			p := l.pos()
			l.advance()
			l.advance()
			for l.off < len(l.src) && !strings.HasPrefix(l.src[l.off:], "*/") {
				l.advance()
			}
			if l.off >= len(l.src) {
				return &LangError{"LexError", p, "unterminated comment"}
			}
			l.advance()
			l.advance()
			continue
		}
		break
	}
	return nil
}
func (l *lexer) ident() token {
	p, start := l.pos(), l.off
	for l.off < len(l.src) {
		r, _ := utf8.DecodeRuneInString(l.src[l.off:])
		if !unicode.IsLetter(r) && !unicode.IsDigit(r) && r != '_' {
			break
		}
		l.advance()
	}
	s := l.src[start:l.off]
	kws := map[string]tokenKind{"true": tTrue, "false": tFalse, "null": tNull, "if": tIf, "else": tElse, "for": tFor, "in": tIn, "delete": tDelete, "break": tBreak, "continue": tContinue}
	if k, ok := kws[s]; ok {
		return token{k, s, p}
	}
	return token{tIdent, s, p}
}
func (l *lexer) number() (token, error) {
	p, start := l.pos(), l.off
	digits := func() {
		for l.off < len(l.src) && l.src[l.off] >= '0' && l.src[l.off] <= '9' {
			l.advance()
		}
	}
	digits()
	if l.off < len(l.src) && l.src[l.off] == '.' {
		l.advance()
		digits()
	}
	if l.off < len(l.src) && (l.src[l.off] == 'e' || l.src[l.off] == 'E') {
		l.advance()
		if l.off < len(l.src) && (l.src[l.off] == '+' || l.src[l.off] == '-') {
			l.advance()
		}
		digits()
	}
	s := l.src[start:l.off]
	if _, err := strconv.ParseFloat(s, 64); err != nil {
		return token{}, &LangError{"LexError", p, "invalid number " + strconv.Quote(s)}
	}
	return token{tNumber, s, p}, nil
}
func (l *lexer) str() (token, error) {
	p := l.pos()
	quote := l.advance()
	var b strings.Builder
	for l.off < len(l.src) {
		r := l.advance()
		if r == quote {
			return token{tString, b.String(), p}, nil
		}
		if r == '\n' || r == '\r' {
			return token{}, &LangError{"LexError", p, "unterminated string"}
		}
		if r != '\\' {
			b.WriteRune(r)
			continue
		}
		if l.off >= len(l.src) {
			break
		}
		e := l.advance()
		escapes := map[rune]rune{'n': '\n', 'r': '\r', 't': '\t', 'b': '\b', 'f': '\f', '\\': '\\', '\'': '\'', '"': '"'}
		if v, ok := escapes[e]; ok {
			b.WriteRune(v)
			continue
		}
		if e == 'u' {
			if l.off+4 > len(l.src) {
				return token{}, &LangError{"LexError", p, "invalid unicode escape"}
			}
			x := l.src[l.off : l.off+4]
			n, err := strconv.ParseUint(x, 16, 16)
			if err != nil {
				return token{}, &LangError{"LexError", p, "invalid unicode escape"}
			}
			for range 4 {
				l.advance()
			}
			b.WriteRune(rune(n))
			continue
		}
		return token{}, &LangError{"LexError", l.pos(), "invalid escape"}
	}
	return token{}, &LangError{"LexError", p, "unterminated string"}
}
