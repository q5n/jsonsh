use crate::value::Value;

use super::token::{Pos, Tok};

#[derive(Debug)]
pub enum Expr {
    Literal(Pos, Value),
    Variable(Pos, String),
    Array(Pos, Vec<Expr>),
    Object(Pos, Vec<(String, Expr)>),
    Unary(Pos, Tok, Box<Expr>),
    Binary(Pos, Tok, Box<Expr>, Box<Expr>),
    Assign(Pos, Tok, Box<Expr>, Box<Expr>),
    Member(Pos, Box<Expr>, Box<Expr>),
    Call(Pos, String, Vec<Expr>),
    MethodCall(Pos, Box<Expr>, String, Vec<Expr>),
}

impl Expr {
    pub fn pos(&self) -> Pos {
        match self {
            Expr::Literal(p, _) => *p,
            Expr::Variable(p, _) => *p,
            Expr::Array(p, _) => *p,
            Expr::Object(p, _) => *p,
            Expr::Unary(p, _, _) => *p,
            Expr::Binary(p, _, _, _) => *p,
            Expr::Assign(p, _, _, _) => *p,
            Expr::Member(p, _, _) => *p,
            Expr::Call(p, _, _) => *p,
            Expr::MethodCall(p, _, _, _) => *p,
        }
    }
}

#[derive(Debug)]
pub enum Stmt {
    Expr(Pos, Expr),
    Block(Pos, Vec<Stmt>),
    If(Pos, Expr, Box<Stmt>, Option<Box<Stmt>>),
    For(Pos, String, bool, Expr, Box<Stmt>),
    ForC(
        Pos,
        Option<Box<Stmt>>,
        Option<Expr>,
        Option<Expr>,
        Box<Stmt>,
    ),
    Delete(Pos, Expr),
    Break(Pos),
    Continue(Pos),
}

impl Stmt {
    pub fn pos(&self) -> Pos {
        match self {
            Stmt::Expr(p, _) => *p,
            Stmt::Block(p, _) => *p,
            Stmt::If(p, _, _, _) => *p,
            Stmt::For(p, _, _, _, _) => *p,
            Stmt::ForC(p, _, _, _, _) => *p,
            Stmt::Delete(p, _) => *p,
            Stmt::Break(p) => *p,
            Stmt::Continue(p) => *p,
        }
    }
}

pub struct Program {
    pub list: Vec<Stmt>,
}
