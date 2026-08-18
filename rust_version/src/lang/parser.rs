use crate::value::Value;

use super::ast::{ArrowBody, ChainStep, Expr, Program, Stmt};
use super::token::{Error, Pos, Tok, Token};

struct Parser {
    src: String,
    ts: Vec<Token>,
    i: usize,
    loops: usize,
}

pub fn parse(src: &str) -> Result<Program, Error> {
    let ts = super::lexer::lex(src)?;
    let mut p = Parser {
        src: src.to_string(),
        ts,
        i: 0,
        loops: 0,
    };
    let mut list = Vec::new();
    while p.peek().kind != Tok::Eof {
        if p.match_kind(Tok::Semi) {
            continue;
        }
        let s = p.stmt()?;
        list.push(s);
    }
    Ok(Program { list })
}

impl Parser {
    fn peek(&self) -> &Token {
        &self.ts[self.i]
    }

    fn next(&mut self) -> Token {
        let t = self.ts[self.i].clone();
        if self.i < self.ts.len() - 1 {
            self.i += 1;
        }
        t
    }

    fn match_kind(&mut self, k: Tok) -> bool {
        if self.peek().kind == k {
            self.next();
            true
        } else {
            false
        }
    }

    fn need(&mut self, k: Tok, msg: &str) -> Result<Token, Error> {
        if self.peek().kind != k {
            return Err(self.err(self.peek(), msg));
        }
        Ok(self.next())
    }

    fn err(&self, t: &Token, msg: &str) -> Error {
        Error::new("SyntaxError", t.pos, msg.to_string())
    }

    fn err_pos(&self, pos: Pos, msg: &str) -> Error {
        Error::new("SyntaxError", pos, msg.to_string())
    }

    fn stmt(&mut self) -> Result<Stmt, Error> {
        let t = self.peek().clone();
        match t.kind {
            Tok::LBrace => {
                if !self.starts_object_literal() {
                    return self.block();
                }
                let x = self.expression()?;
                self.end_stmt()?;
                Ok(Stmt::Expr(t.pos, x))
            }
            Tok::If => self.if_stmt(),
            Tok::For => self.for_stmt(),
            Tok::Delete => {
                self.next();
                let x = self.expression()?;
                if !matches!(x, Expr::Member(..)) {
                    return Err(self.err(&t, "delete target must be a member"));
                }
                self.end_stmt()?;
                Ok(Stmt::Delete(t.pos, x))
            }
            Tok::Break => {
                self.next();
                if self.loops == 0 {
                    return Err(self.err(&t, "break outside loop"));
                }
                self.end_stmt()?;
                Ok(Stmt::Break(t.pos))
            }
            Tok::Continue => {
                self.next();
                if self.loops == 0 {
                    return Err(self.err(&t, "continue outside loop"));
                }
                self.end_stmt()?;
                Ok(Stmt::Continue(t.pos))
            }
            Tok::Return => {
                self.next();
                let v = if self.peek().kind == Tok::Semi
                    || self.peek().kind == Tok::RBrace
                    || self.peek().kind == Tok::Eof
                    || self.has_line_break()
                {
                    None
                } else {
                    Some(Box::new(self.expression()?))
                };
                self.end_stmt()?;
                Ok(Stmt::Return(t.pos, v))
            }
            _ => {
                let x = self.expression()?;
                self.end_stmt()?;
                Ok(Stmt::Expr(t.pos, x))
            }
        }
    }

    fn starts_object_literal(&self) -> bool {
        if self.peek().kind != Tok::LBrace || self.i + 1 >= self.ts.len() {
            return false;
        }
        let next = self.ts[self.i + 1].kind;
        if next == Tok::RBrace {
            return true;
        }
        self.i + 2 < self.ts.len()
            && (next == Tok::Ident || next == Tok::String)
            && self.ts[self.i + 2].kind == Tok::Colon
    }

    fn end_stmt(&mut self) -> Result<(), Error> {
        if self.match_kind(Tok::Semi)
            || self.peek().kind == Tok::RBrace
            || self.peek().kind == Tok::Eof
            || self.peek().kind == Tok::Else
            || self.has_line_break()
        {
            return Ok(());
        }
        Err(self.err(self.peek(), "expected ';' or newline between statements"))
    }

    fn has_line_break(&self) -> bool {
        self.i > 0 && self.ts[self.i - 1].pos.line < self.peek().pos.line
    }

    fn block(&mut self) -> Result<Stmt, Error> {
        // Mirror Go: the missing-`{` error from need() is discarded (see Go's
        // `t, _ := p.need(tLBrace, "expected '{'")`), so a brace-less body is
        // scanned as statements until `}` or EOF, reporting "expected '}'".
        let t = self
            .need(Tok::LBrace, "expected '{'")
            .unwrap_or_else(|_| Token {
                kind: Tok::Eof,
                lit: String::new(),
                pos: Pos { line: 0, col: 0 },
                offset: 0,
            });
        let mut xs = Vec::new();
        while self.peek().kind != Tok::RBrace {
            if self.peek().kind == Tok::Eof {
                return Err(self.err(self.peek(), "expected '}'"));
            }
            if self.match_kind(Tok::Semi) {
                continue;
            }
            let s = self.stmt()?;
            xs.push(s);
        }
        self.next();
        Ok(Stmt::Block(t.pos, xs))
    }

    fn body(&mut self) -> Result<Stmt, Error> {
        if self.peek().kind == Tok::LBrace {
            self.block()
        } else if self.match_kind(Tok::Semi) {
            let pos = if self.i > 0 {
                self.ts[self.i - 1].pos
            } else {
                self.peek().pos
            };
            Ok(Stmt::Block(pos, vec![]))
        } else {
            let s = self.stmt()?;
            Ok(Stmt::Block(s.pos(), vec![s]))
        }
    }

    fn if_stmt(&mut self) -> Result<Stmt, Error> {
        let t = self.next();
        self.need(Tok::LParen, "expected '(' after if")?;
        let c = self.expression()?;
        self.need(Tok::RParen, "expected ')' after condition")?;
        let b = self.body()?;
        let mut alt = None;
        if self.match_kind(Tok::Else) {
            alt = Some(if self.peek().kind == Tok::If {
                Box::new(self.if_stmt()?)
            } else {
                Box::new(self.body()?)
            });
        }
        Ok(Stmt::If(t.pos, c, Box::new(b), alt))
    }

    fn for_stmt(&mut self) -> Result<Stmt, Error> {
        let t = self.next();
        self.need(Tok::LParen, "expected '(' after for")?;
        if self.match_kind(Tok::Semi) {
            return self.for_c_stmt(t.pos, None);
        }
        let next_is_in_of = self.peek().kind == Tok::Ident
            && self
                .ts
                .get(self.i + 1)
                .map(|x| x.kind == Tok::In || x.kind == Tok::Of)
                .unwrap_or(false);
        if next_is_in_of {
            let n = self.need(Tok::Ident, "expected loop variable")?;
            let of = if self.match_kind(Tok::In) {
                false
            } else if self.match_kind(Tok::Of) {
                true
            } else {
                return Err(self.err(self.peek(), "expected 'in' or 'of'"));
            };
            let src = self.expression()?;
            self.need(Tok::RParen, "expected ')' after source")?;
            self.loops += 1;
            let b = self.body();
            self.loops -= 1;
            let b = b?;
            return Ok(Stmt::For(t.pos, n.lit, of, src, Box::new(b)));
        }
        let init = self.expression()?;
        self.need(Tok::Semi, "expected ';' after for initializer")?;
        self.for_c_stmt(t.pos, Some(Stmt::Expr(init.pos(), init)))
    }

    fn for_c_stmt(&mut self, pos: Pos, init: Option<Stmt>) -> Result<Stmt, Error> {
        let cond = if self.peek().kind == Tok::Semi {
            None
        } else {
            Some(self.expression()?)
        };
        self.need(Tok::Semi, "expected ';' after for condition")?;
        let update = if self.peek().kind == Tok::RParen {
            None
        } else {
            Some(self.expression()?)
        };
        self.need(Tok::RParen, "expected ')' after for update")?;
        self.loops += 1;
        let b = self.body();
        self.loops -= 1;
        let b = b?;
        Ok(Stmt::ForC(
            pos,
            init.map(Box::new),
            cond,
            update,
            Box::new(b),
        ))
    }

    fn expression(&mut self) -> Result<Expr, Error> {
        self.assignment()
    }

    fn assignment(&mut self) -> Result<Expr, Error> {
        let left = self.ternary()?;
        let op = self.peek().clone();
        if is_assign_op(op.kind) {
            match &left {
                Expr::Variable(..) | Expr::Member(..) => {}
                _ => return Err(self.err(&op, "invalid assignment target")),
            }
            self.next();
            let right = self.assignment()?;
            return Ok(Expr::Assign(op.pos, op.kind, Box::new(left), Box::new(right)));
        }
        Ok(left)
    }

    fn ternary(&mut self) -> Result<Expr, Error> {
        let cond = self.binary(2)?;
        if self.match_kind(Tok::Question) {
            let then = self.assignment()?;
            self.need(Tok::Colon, "expected ':' in conditional expression")?;
            let els = self.assignment()?;
            return Ok(Expr::Ternary(
                cond.pos(),
                Box::new(cond),
                Box::new(then),
                Box::new(els),
            ));
        }
        Ok(cond)
    }

    fn binary(&mut self, min: i32) -> Result<Expr, Error> {
        let mut left = self.unary()?;
        loop {
            let op = self.peek().clone();
            let q = match prec(op.kind) {
                Some(q) => q,
                None => break,
            };
            if q < min {
                break;
            }
            self.next();
            let next = q + 1;
            let right = self.binary(next)?;
            left = Expr::Binary(op.pos, op.kind, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn unary(&mut self) -> Result<Expr, Error> {
        if self.peek().kind == Tok::Bang
            || self.peek().kind == Tok::Minus
            || self.peek().kind == Tok::Plus
            || self.peek().kind == Tok::Typeof
            || self.peek().kind == Tok::BitNot
        {
            let t = self.next();
            let x = self.unary()?;
            return Ok(Expr::Unary(t.pos, t.kind, Box::new(x)));
        }
        if self.peek().kind == Tok::Inc || self.peek().kind == Tok::Dec {
            let t = self.next();
            let x = self.unary()?;
            return Ok(Expr::Update(t.pos, t.kind, Box::new(x), true));
        }
        if self.match_kind(Tok::New) {
            let t = self.need(Tok::Ident, "expected constructor name")?;
            self.need(Tok::LParen, "expected '(' after constructor name")?;
            let args = self.call_args()?;
            return self.postfix_loop(Expr::New(t.pos, t.lit, args));
        }
        self.postfix()
    }

    fn postfix(&mut self) -> Result<Expr, Error> {
        let x = self.primary()?;
        self.postfix_loop(x)
    }

    fn postfix_loop(&mut self, mut x: Expr) -> Result<Expr, Error> {
        loop {
            if self.match_kind(Tok::Dot) {
                let n = self.need(Tok::Ident, "expected property name")?;
                x = Expr::Member(
                    n.pos,
                    Box::new(x),
                    Box::new(Expr::Literal(n.pos, Value::String(n.lit))),
                );
                continue;
            }
            if self.match_kind(Tok::LBracket) {
                let pos = self.peek().pos;
                let k = self.expression()?;
                self.need(Tok::RBracket, "expected ']'")?;
                x = Expr::Member(pos, Box::new(x), Box::new(k));
                continue;
            }
            if self.match_kind(Tok::LParen) {
                let m = match x {
                    Expr::Member(p, obj, key) => (p, obj, key),
                    _ => {
                        return Err(
                            self.err(self.peek(), "only named functions and methods can be called")
                        )
                    }
                };
                let name = match *m.2 {
                    Expr::Literal(_, Value::String(s)) => s,
                    _ => return Err(self.err_pos(m.0, "method name must be a property name")),
                };
                let args = self.call_args()?;
                x = Expr::MethodCall(m.0, m.1, name, args);
                continue;
            }
            if self.match_kind(Tok::QuestionDot) {
                x = self.parse_optional(x)?;
                continue;
            }
            if self.peek().kind == Tok::Inc || self.peek().kind == Tok::Dec {
                let t = self.next();
                x = Expr::Update(t.pos, t.kind, Box::new(x), false);
                continue;
            }
            break;
        }
        Ok(x)
    }

    fn parse_optional(&mut self, base: Expr) -> Result<Expr, Error> {
        let pos = base.pos();
        let mut steps: Vec<ChainStep> = Vec::new();
        loop {
            if self.peek().kind == Tok::LBracket {
                self.next();
                let k = self.expression()?;
                self.need(Tok::RBracket, "expected ']' after '?.['")?;
                steps.push(ChainStep::Prop(k));
            } else if self.peek().kind == Tok::Ident {
                let n = self.next();
                if self.match_kind(Tok::LParen) {
                    let args = self.call_args()?;
                    steps.push(ChainStep::Method(n.lit, args));
                } else {
                    steps.push(ChainStep::Prop(Expr::Literal(
                        n.pos,
                        Value::String(n.lit),
                    )));
                }
            } else if self.peek().kind == Tok::LParen {
                return Err(self.err(self.peek(), "optional call of a receiver is not supported"));
            } else {
                return Err(
                    self.err(self.peek(), "expected property name, '[' or '(' after '?.'")
                );
            }

            match self.peek().kind {
                Tok::Dot => {
                    self.next();
                }
                Tok::LBracket | Tok::LParen => {}
                Tok::QuestionDot => {
                    self.next();
                    let inner = Expr::Optional(pos, Box::new(base), steps);
                    return self.parse_optional(inner);
                }
                _ => break,
            }
        }
        Ok(Expr::Optional(pos, Box::new(base), steps))
    }

    fn regex_literal(
        &mut self,
        start: usize,
        pos: Pos,
    ) -> Result<Expr, Error> {
        let bytes = self.src.as_bytes();
        let mut i = start + 1;
        let mut in_class = false;
        while i < bytes.len() {
            let b = bytes[i];
            if in_class {
                if b == b'\\' && i + 1 < bytes.len() {
                    i += 2;
                    continue;
                }
                if b == b']' {
                    in_class = false;
                }
                i += 1;
            } else {
                if b == b'/' {
                    break;
                }
                if b == b'\\' && i + 1 < bytes.len() {
                    i += 2;
                    continue;
                }
                if b == b'[' {
                    in_class = true;
                }
                if b == b'\n' || b == b'\r' {
                    return Err(self.err_pos(pos, "unterminated regular expression"));
                }
                i += 1;
            }
        }
        if i >= bytes.len() {
            return Err(self.err_pos(pos, "unterminated regular expression"));
        }
        let pattern = &self.src[start + 1..i];
        let mut flags = String::new();
        let mut j = i + 1;
        while j < bytes.len() && bytes[j].is_ascii_alphabetic() {
            flags.push(bytes[j] as char);
            j += 1;
        }
        let end = j;
        while self.i < self.ts.len() && self.ts[self.i].offset < end {
            self.i += 1;
        }
        let re = crate::regex::Regex::new(pattern, &flags)
            .map_err(|e| self.err_pos(pos, &e))?;
        Ok(Expr::Literal(pos, Value::regex(re)))
    }

    fn primary(&mut self) -> Result<Expr, Error> {
        let t = self.next();
        match t.kind {
            Tok::Number => {
                let n: f64 = t.lit.parse().unwrap_or(0.0);
                Ok(Expr::Literal(t.pos, Value::Number(n)))
            }
            Tok::String => Ok(Expr::Literal(t.pos, Value::String(t.lit))),
            Tok::True => Ok(Expr::Literal(t.pos, Value::Bool(true))),
            Tok::False => Ok(Expr::Literal(t.pos, Value::Bool(false))),
            Tok::Null => Ok(Expr::Literal(t.pos, Value::Null)),
            Tok::Dollar => Ok(Expr::Variable(t.pos, "$".to_string())),
            Tok::Slash => self.regex_literal(t.offset, t.pos),
            Tok::Char(c) => Err(self.err(
                &t,
                &format!("unexpected character {:?}", c),
            )),
            Tok::Ident => {
                if self.peek().kind == Tok::Arrow {
                    self.need(Tok::Arrow, "expected '=>'")?;
                    let body = self.arrow_body()?;
                    return Ok(Expr::Arrow(t.pos, vec![t.lit], Box::new(body)));
                }
                if self.match_kind(Tok::LParen) {
                    let args = self.call_args()?;
                    Ok(Expr::Call(t.pos, t.lit, args))
                } else {
                    Ok(Expr::Variable(t.pos, t.lit))
                }
            }
            Tok::LParen => {
                if self.follows_arrow_param_list() {
                    return self.arrow_expr(t.pos);
                }
                let x = self.expression()?;
                self.need(Tok::RParen, "expected ')'")?;
                Ok(x)
            }
            Tok::LBracket => {
                let mut xs = Vec::new();
                if !self.match_kind(Tok::RBracket) {
                    loop {
                        let x = self.expression()?;
                        xs.push(x);
                        if self.match_kind(Tok::RBracket) {
                            break;
                        }
                        self.need(Tok::Comma, "expected ',' or ']'")?;
                        if self.match_kind(Tok::RBracket) {
                            break;
                        }
                    }
                }
                Ok(Expr::Array(t.pos, xs))
            }
            Tok::LBrace => {
                let mut xs: Vec<(String, Expr)> = Vec::new();
                if !self.match_kind(Tok::RBrace) {
                    loop {
                        let k = self.next();
                        if k.kind != Tok::String && k.kind != Tok::Ident {
                            return Err(self.err(&k, "expected object key"));
                        }
                        self.need(Tok::Colon, "expected ':'")?;
                        let v = self.expression()?;
                        xs.push((k.lit, v));
                        if self.match_kind(Tok::RBrace) {
                            break;
                        }
                        self.need(Tok::Comma, "expected ',' or '}'")?;
                        if self.match_kind(Tok::RBrace) {
                            break;
                        }
                    }
                }
                Ok(Expr::Object(t.pos, xs))
            }
            _ => Err(self.err(&t, &format!("unexpected token {:?}", t.lit))),
        }
    }

    fn follows_arrow_param_list(&self) -> bool {
        let mut j = self.i;
        if self.ts.get(j).map(|t| t.kind) == Some(Tok::RParen) {
            j += 1;
            return self.ts.get(j).map(|t| t.kind) == Some(Tok::Arrow);
        }
        loop {
            match self.ts.get(j).map(|t| t.kind) {
                Some(Tok::Ident) => j += 1,
                _ => return false,
            }
            match self.ts.get(j).map(|t| t.kind) {
                Some(Tok::Comma) => {
                    j += 1;
                    continue;
                }
                Some(Tok::RParen) => {
                    j += 1;
                    return self.ts.get(j).map(|t| t.kind) == Some(Tok::Arrow);
                }
                _ => return false,
            }
        }
    }

    fn arrow_expr(&mut self, pos: Pos) -> Result<Expr, Error> {
        let mut params = Vec::new();
        if !self.match_kind(Tok::RParen) {
            loop {
                let n = self.need(Tok::Ident, "expected parameter name")?;
                params.push(n.lit);
                if self.match_kind(Tok::RParen) {
                    break;
                }
                self.need(Tok::Comma, "expected ',' or ')'")?;
            }
        }
        self.need(Tok::Arrow, "expected '=>'")?;
        let body = self.arrow_body()?;
        Ok(Expr::Arrow(pos, params, Box::new(body)))
    }

    fn arrow_body(&mut self) -> Result<ArrowBody, Error> {
        if self.peek().kind == Tok::LBrace {
            let b = self.block()?;
            let stmts = match b {
                Stmt::Block(_, xs) => xs,
                _ => unreachable!(),
            };
            Ok(ArrowBody {
                block: true,
                expr: None,
                stmts,
            })
        } else {
            let e = self.expression()?;
            Ok(ArrowBody {
                block: false,
                expr: Some(Box::new(e)),
                stmts: Vec::new(),
            })
        }
    }

    fn call_args(&mut self) -> Result<Vec<Expr>, Error> {
        let mut args = Vec::new();
        if self.match_kind(Tok::RParen) {
            return Ok(args);
        }
        loop {
            let x = self.expression()?;
            args.push(x);
            if self.match_kind(Tok::RParen) {
                return Ok(args);
            }
            self.need(Tok::Comma, "expected ',' or ')'")?;
        }
    }
}

fn is_assign_op(k: Tok) -> bool {
    matches!(
        k,
        Tok::Assign
            | Tok::PlusAssign
            | Tok::MinusAssign
            | Tok::StarAssign
            | Tok::SlashAssign
            | Tok::PercentAssign
            | Tok::BitAndAssign
            | Tok::BitOrAssign
            | Tok::BitXorAssign
            | Tok::ShlAssign
            | Tok::ShrAssign
            | Tok::UShrAssign
    )
}

fn prec(k: Tok) -> Option<i32> {
    match k {
        Tok::Or => Some(2),
        Tok::And => Some(3),
        Tok::BitOr => Some(4),
        Tok::BitXor => Some(5),
        Tok::BitAnd => Some(6),
        Tok::Eq | Tok::Ne => Some(7),
        Tok::GT | Tok::GE | Tok::LT | Tok::LE => Some(8),
        Tok::Shl | Tok::Shr | Tok::UShr => Some(9),
        Tok::Plus | Tok::Minus => Some(10),
        Tok::Star | Tok::Slash | Tok::Percent => Some(11),
        _ => None,
    }
}
