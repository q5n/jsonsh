package lang

import "fmt"

type Pos struct{ Line, Col int }

type LangError struct {
	Kind string
	Pos  Pos
	Msg  string
}

func (e *LangError) Error() string {
	return fmt.Sprintf("%d:%d: %s: %s", e.Pos.Line, e.Pos.Col, e.Kind, e.Msg)
}

type tokenKind int

const (
	tEOF tokenKind = iota
	tIdent
	tNumber
	tString
	tDollar
	tTrue
	tFalse
	tNull
	tIf
	tElse
	tFor
	tIn
	tDelete
	tBreak
	tContinue
	tLParen
	tRParen
	tLBrace
	tRBrace
	tLBracket
	tRBracket
	tDot
	tComma
	tColon
	tSemi
	tPlus
	tMinus
	tStar
	tSlash
	tBang
	tAssign
	tPlusAssign
	tMinusAssign
	tStarAssign
	tSlashAssign
	tEq
	tNe
	tGT
	tGE
	tLT
	tLE
	tAnd
	tOr
)

type token struct {
	kind tokenKind
	lit  string
	pos  Pos
}
