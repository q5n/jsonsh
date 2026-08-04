package lang

type Expr interface{ exprPos() Pos }
type Stmt interface{ stmtPos() Pos }

type Literal struct {
	P     Pos
	Value any
}

func (e *Literal) exprPos() Pos { return e.P }

type Variable struct {
	P    Pos
	Name string
}

func (e *Variable) exprPos() Pos { return e.P }

type ArrayExpr struct {
	P     Pos
	Items []Expr
}

func (e *ArrayExpr) exprPos() Pos { return e.P }

type ObjectItem struct {
	Key   string
	Value Expr
}
type ObjectExpr struct {
	P     Pos
	Items []ObjectItem
}

func (e *ObjectExpr) exprPos() Pos { return e.P }

type Unary struct {
	P  Pos
	Op tokenKind
	X  Expr
}

func (e *Unary) exprPos() Pos { return e.P }

type Binary struct {
	P           Pos
	Op          tokenKind
	Left, Right Expr
}

func (e *Binary) exprPos() Pos { return e.P }

type Assign struct {
	P             Pos
	Op            tokenKind
	Target, Value Expr
}

func (e *Assign) exprPos() Pos { return e.P }

type Member struct {
	P           Pos
	Object, Key Expr
}

func (e *Member) exprPos() Pos { return e.P }

type Call struct {
	P    Pos
	Name string
	Args []Expr
}

func (e *Call) exprPos() Pos { return e.P }

type MethodCall struct {
	P        Pos
	Receiver Expr
	Name     string
	Args     []Expr
}

func (e *MethodCall) exprPos() Pos { return e.P }

type ExprStmt struct {
	P Pos
	X Expr
}

func (s *ExprStmt) stmtPos() Pos { return s.P }

type Block struct {
	P    Pos
	List []Stmt
}

func (s *Block) stmtPos() Pos { return s.P }

type IfStmt struct {
	P    Pos
	Cond Expr
	Then *Block
	Else Stmt
}

func (s *IfStmt) stmtPos() Pos { return s.P }

type ForStmt struct {
	P      Pos
	Name   string
	Source Expr
	Body   *Block
}

func (s *ForStmt) stmtPos() Pos { return s.P }

type DeleteStmt struct {
	P      Pos
	Target Expr
}

func (s *DeleteStmt) stmtPos() Pos { return s.P }

type BreakStmt struct{ P Pos }

func (s *BreakStmt) stmtPos() Pos { return s.P }

type ContinueStmt struct{ P Pos }

func (s *ContinueStmt) stmtPos() Pos { return s.P }

type Program struct{ List []Stmt }
