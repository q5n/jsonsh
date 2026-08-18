use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Pos {
    pub line: usize,
    pub col: usize,
}

#[derive(Debug)]
pub struct Error {
    pub kind: String,
    pub pos: Pos,
    pub msg: String,
}

impl Error {
    pub fn new(kind: &str, pos: Pos, msg: String) -> Error {
        Error {
            kind: kind.to_string(),
            pos,
            msg,
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}: {}: {}", self.pos.line, self.pos.col, self.kind, self.msg)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tok {
    Eof,
    Ident,
    Number,
    String,
    Char(char),
    Dollar,
    True,
    False,
    Null,
    If,
    Else,
    For,
    In,
    Of,
    Delete,
    Break,
    Continue,
    Return,
    Typeof,
    New,
    Inc,
    Dec,
    BitAnd,
    BitOr,
    BitXor,
    BitNot,
    Shl,
    Shr,
    UShr,
    BitAndAssign,
    BitOrAssign,
    BitXorAssign,
    ShlAssign,
    ShrAssign,
    UShrAssign,
    Percent,
    PercentAssign,
    Question,
    QuestionDot,
    LParen,
    RParen,
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    Dot,
    Comma,
    Colon,
    Semi,
    Plus,
    Minus,
    Star,
    Slash,
    Bang,
    Assign,
    Arrow,
    PlusAssign,
    MinusAssign,
    StarAssign,
    SlashAssign,
    Eq,
    Ne,
    GT,
    GE,
    LT,
    LE,
    And,
    Or,
}

#[derive(Clone, Debug)]
pub struct Token {
    pub kind: Tok,
    pub lit: String,
    pub pos: Pos,
    pub offset: usize,
}
