use crate::value::{is_digit, is_letter};

use super::token::{Error, Pos, Tok, Token};

struct Lexer {
    src: String,
    off: usize,
    line: usize,
    col: usize,
}

pub fn lex(src: &str) -> Result<Vec<Token>, Error> {
    let mut l = Lexer {
        src: src.to_string(),
        off: 0,
        line: 1,
        col: 1,
    };
    let mut out = Vec::new();
    loop {
        l.skip_space()?;
        let p = l.pos();
        if l.off >= l.src.len() {
            out.push(Token {
                kind: Tok::Eof,
                lit: String::new(),
                pos: p,
            });
            return Ok(out);
        }
        let r = l.src[l.off..].chars().next().unwrap();
        if is_letter(r) || r == '_' {
            out.push(l.ident());
            continue;
        }
        if is_digit(r) {
            let t = l.number()?;
            out.push(t);
            continue;
        }
        if r == '\'' || r == '"' {
            let t = l.str()?;
            out.push(t);
            continue;
        }
        let pairs: [(&str, Tok); 11] = [
            ("+=", Tok::PlusAssign),
            ("-=", Tok::MinusAssign),
            ("*=", Tok::StarAssign),
            ("/=", Tok::SlashAssign),
            ("==", Tok::Eq),
            ("!=", Tok::Ne),
            (">=", Tok::GE),
            ("<=", Tok::LE),
            ("&&", Tok::And),
            ("||", Tok::Or),
            ("=>", Tok::Arrow),
        ];
        if l.off + 2 <= l.src.len() {
            let two = &l.src.as_bytes()[l.off..l.off + 2];
            if let Some((_, k)) = pairs.iter().find(|(s, _)| s.as_bytes() == two) {
                out.push(Token {
                    kind: *k,
                    lit: String::from_utf8_lossy(two).into_owned(),
                    pos: p,
                });
                l.advance();
                l.advance();
                continue;
            }
        }
        let single: [(char, Tok); 18] = [
            ('$', Tok::Dollar),
            ('(', Tok::LParen),
            (')', Tok::RParen),
            ('{', Tok::LBrace),
            ('}', Tok::RBrace),
            ('[', Tok::LBracket),
            (']', Tok::RBracket),
            ('.', Tok::Dot),
            (',', Tok::Comma),
            (':', Tok::Colon),
            (';', Tok::Semi),
            ('+', Tok::Plus),
            ('-', Tok::Minus),
            ('*', Tok::Star),
            ('/', Tok::Slash),
            ('!', Tok::Bang),
            ('=', Tok::Assign),
            ('>', Tok::GT),
        ];
        let mut matched = None;
        for &(ch, k) in &single {
            if ch == r {
                matched = Some(k);
                break;
            }
        }
        // '<' is not in the map (Go treats it specially below); add it here.
        let k = matched.or_else(|| if r == '<' { Some(Tok::LT) } else { None });
        if let Some(k) = k {
            out.push(Token {
                kind: k,
                lit: r.to_string(),
                pos: p,
            });
            l.advance();
            continue;
        }
        return Err(Error::new(
            "LexError",
            p,
            format!("unexpected character {:?}", r),
        ));
    }
}

impl Lexer {
    fn pos(&self) -> Pos {
        Pos {
            line: self.line,
            col: self.col,
        }
    }

    fn advance(&mut self) -> char {
        let r = self.src[self.off..].chars().next().unwrap();
        self.off += r.len_utf8();
        if r == '\n' {
            self.line += 1;
            self.col = 1;
        } else {
            self.col += 1;
        }
        r
    }

    fn skip_space(&mut self) -> Result<(), Error> {
        loop {
            if self.off >= self.src.len() {
                break;
            }
            let r = self.src[self.off..].chars().next().unwrap();
            if r.is_whitespace() {
                self.advance();
                continue;
            }
            if self.src[self.off..].starts_with("//") {
                while self.off < self.src.len() && self.advance() != '\n' {}
                continue;
            }
            if self.src[self.off..].starts_with("/*") {
                let p = self.pos();
                self.advance();
                self.advance();
                while self.off < self.src.len() && !self.src[self.off..].starts_with("*/") {
                    self.advance();
                }
                if self.off >= self.src.len() {
                    return Err(Error::new("LexError", p, "unterminated comment".to_string()));
                }
                self.advance();
                self.advance();
                continue;
            }
            break;
        }
        Ok(())
    }

    fn ident(&mut self) -> Token {
        let (p, start) = (self.pos(), self.off);
        while self.off < self.src.len() {
            let r = self.src[self.off..].chars().next().unwrap();
            if !is_letter(r) && !is_digit(r) && r != '_' {
                break;
            }
            self.advance();
        }
        let s = self.src[start..self.off].to_string();
        let kw = match s.as_str() {
            "true" => Some(Tok::True),
            "false" => Some(Tok::False),
            "null" => Some(Tok::Null),
            "if" => Some(Tok::If),
            "else" => Some(Tok::Else),
            "for" => Some(Tok::For),
            "in" => Some(Tok::In),
            "of" => Some(Tok::Of),
            "delete" => Some(Tok::Delete),
            "break" => Some(Tok::Break),
            "continue" => Some(Tok::Continue),
            "return" => Some(Tok::Return),
            "typeof" => Some(Tok::Typeof),
            _ => None,
        };
        match kw {
            Some(k) => Token { kind: k, lit: s, pos: p },
            None => Token {
                kind: Tok::Ident,
                lit: s,
                pos: p,
            },
        }
    }

    fn number(&mut self) -> Result<Token, Error> {
        let (p, start) = (self.pos(), self.off);
        let digits = |l: &mut Lexer| {
            while l.off < l.src.len() && l.src.as_bytes()[l.off].is_ascii_digit() {
                l.advance();
            }
        };
        digits(self);
        if self.off < self.src.len() && self.src.as_bytes()[self.off] == b'.' {
            self.advance();
            digits(self);
        }
        if self.off < self.src.len()
            && (self.src.as_bytes()[self.off] == b'e' || self.src.as_bytes()[self.off] == b'E')
        {
            self.advance();
            if self.off < self.src.len()
                && (self.src.as_bytes()[self.off] == b'+' || self.src.as_bytes()[self.off] == b'-')
            {
                self.advance();
            }
            digits(self);
        }
        let s = self.src[start..self.off].to_string();
        if s.parse::<f64>().is_err() {
            return Err(Error::new("LexError", p, format!("invalid number {:?}", s)));
        }
        Ok(Token {
            kind: Tok::Number,
            lit: s,
            pos: p,
        })
    }

    fn str(&mut self) -> Result<Token, Error> {
        let p = self.pos();
        let quote = self.advance();
        let mut b = String::new();
        while self.off < self.src.len() {
            let r = self.advance();
            if r == quote {
                return Ok(Token {
                    kind: Tok::String,
                    lit: b,
                    pos: p,
                });
            }
            if r == '\n' || r == '\r' {
                return Err(Error::new("LexError", p, "unterminated string".to_string()));
            }
            if r != '\\' {
                b.push(r);
                continue;
            }
            if self.off >= self.src.len() {
                break;
            }
            let e = self.advance();
            match e {
                'n' => b.push('\n'),
                'r' => b.push('\r'),
                't' => b.push('\t'),
                'b' => b.push('\u{0008}'),
                'f' => b.push('\u{000C}'),
                '\\' => b.push('\\'),
                '\'' => b.push('\''),
                '"' => b.push('"'),
                'u' => {
                    if self.off + 4 > self.src.len() {
                        return Err(Error::new("LexError", p, "invalid unicode escape".to_string()));
                    }
                    let x = &self.src[self.off..self.off + 4];
                    let n = u32::from_str_radix(x, 16)
                        .map_err(|_| Error::new("LexError", p, "invalid unicode escape".to_string()))?;
                    for _ in 0..4 {
                        self.advance();
                    }
                    b.push(char::from_u32(n).unwrap_or('\u{FFFD}'));
                }
                _ => {
                    return Err(Error::new(
                        "LexError",
                        self.pos(),
                        "invalid escape".to_string(),
                    ))
                }
            }
        }
        Err(Error::new("LexError", p, "unterminated string".to_string()))
    }
}
