use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use crate::value::Value;

mod encode;
pub use encode::marshal;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Null,
    Bool,
    Number,
    String,
    Array,
    Object,
}

pub struct Node {
    pub kind: Kind,
    pub start: usize,
    pub end: usize,
    pub value: Value,
    pub items: Vec<Item>,
    pub close_trivia: String,
}

pub struct Item {
    pub key: String,
    pub leading: String,
    pub head: String,
    pub trailing: String,
    pub value: Box<Node>,
    pub comma: bool,
}

pub struct Document {
    pub source: String,
    pub prefix: String,
    pub suffix: String,
    pub root: Node,
    pub newline: String,
}

#[derive(Debug)]
pub struct Error {
    pub line: usize,
    pub col: usize,
    pub message: String,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}: JSONCError: {}", self.line, self.col, self.message)
    }
}

struct Parser {
    src: String,
    i: usize,
    line: usize,
    col: usize,
}

pub fn parse(src: String) -> Result<Document, Error> {
    let mut p = Parser {
        src,
        i: 0,
        line: 1,
        col: 1,
    };
    if p.src.starts_with('\u{FEFF}') {
        p.i = 3;
        p.col = 2;
    }
    let mut a = 0;
    p.trivia()?;
    let prefix = p.slice(a, p.i);
    let n = p.value()?;
    a = p.i;
    p.trivia()?;
    let suffix = p.slice(a, p.i);
    if p.i != p.src.len() {
        return Err(p.err("unexpected content after JSON value"));
    }
    let newline = if p.src.contains("\r\n") { "\r\n" } else { "\n" };
    Ok(Document {
        source: p.src,
        prefix,
        suffix,
        root: n,
        newline: newline.to_string(),
    })
}

impl Parser {
    fn err(&self, msg: &str) -> Error {
        Error {
            line: self.line,
            col: self.col,
            message: msg.to_string(),
        }
    }

    fn slice(&self, a: usize, b: usize) -> String {
        self.src[a..b].to_string()
    }

    fn advance(&mut self) -> u8 {
        let c = self.src.as_bytes()[self.i];
        self.i += 1;
        if c == b'\n' {
            self.line += 1;
            self.col = 1;
        } else {
            self.col += 1;
        }
        c
    }

    fn trivia(&mut self) -> Result<(), Error> {
        loop {
            if self.i >= self.src.len() {
                break;
            }
            match self.src.as_bytes()[self.i] {
                b' ' | b'\t' | b'\r' | b'\n' => {
                    self.advance();
                    continue;
                }
                _ => {}
            }
            if self.src.as_bytes()[self.i..].starts_with(b"//") {
                while self.i < self.src.len() && self.advance() != b'\n' {}
                continue;
            }
            if self.src.as_bytes()[self.i..].starts_with(b"/*") {
                self.advance();
                self.advance();
                while self.i < self.src.len() && !self.src.as_bytes()[self.i..].starts_with(b"*/") {
                    self.advance();
                }
                if self.i >= self.src.len() {
                    return Err(self.err("unterminated block comment"));
                }
                self.advance();
                self.advance();
                continue;
            }
            break;
        }
        Ok(())
    }

    fn value(&mut self) -> Result<Node, Error> {
        if self.i >= self.src.len() {
            return Err(self.err("expected JSON value"));
        }
        let s = self.i;
        match self.src.as_bytes()[self.i] {
            b'{' => self.object(),
            b'[' => self.array(),
            b'"' => {
                let v = self.string()?;
                Ok(Node {
                    kind: Kind::String,
                    start: s,
                    end: self.i,
                    value: Value::String(v),
                    items: vec![],
                    close_trivia: String::new(),
                })
            }
            b't' => self.word("true", Kind::Bool, Value::Bool(true)),
            b'f' => self.word("false", Kind::Bool, Value::Bool(false)),
            b'n' => self.word("null", Kind::Null, Value::Null),
            b'-' => self.number(),
            b'0'..=b'9' => self.number(),
            _ => Err(self.err("expected JSON value")),
        }
    }

    fn word(&mut self, w: &str, kind: Kind, value: Value) -> Result<Node, Error> {
        let s = self.i;
        if !self.src[self.i..].starts_with(w) {
            return Err(self.err("invalid literal"));
        }
        for _ in 0..w.len() {
            self.advance();
        }
        Ok(Node {
            kind,
            start: s,
            end: self.i,
            value,
            items: vec![],
            close_trivia: String::new(),
        })
    }

    fn string(&mut self) -> Result<String, Error> {
        let s = self.i;
        self.advance();
        loop {
            if self.i >= self.src.len() {
                break;
            }
            let c = self.advance();
            if c == b'"' {
                let raw = self.slice(s, self.i);
                return match decode_json_string(&raw) {
                    Some(v) => Ok(v),
                    None => Err(self.err("invalid string")),
                };
            }
            if c == b'\\' {
                if self.i >= self.src.len() {
                    break;
                }
                self.advance();
            } else if c < b' ' {
                return Err(self.err("control character in string"));
            }
        }
        Err(self.err("unterminated string"))
    }

    fn number(&mut self) -> Result<Node, Error> {
        let s = self.i;
        if self.src.as_bytes()[self.i] == b'-' {
            self.advance();
            if self.i >= self.src.len() {
                return Err(self.err("invalid number"));
            }
        }
        if self.src.as_bytes()[self.i] == b'0' {
            self.advance();
            if self.i < self.src.len() && self.src.as_bytes()[self.i].is_ascii_digit() {
                return Err(self.err("leading zero in number"));
            }
        } else {
            if !(b'1'..=b'9').contains(&self.src.as_bytes()[self.i]) {
                return Err(self.err("invalid number"));
            }
            while self.i < self.src.len() && self.src.as_bytes()[self.i].is_ascii_digit() {
                self.advance();
            }
        }
        if self.i < self.src.len() && self.src.as_bytes()[self.i] == b'.' {
            self.advance();
            if self.i >= self.src.len() || !self.src.as_bytes()[self.i].is_ascii_digit() {
                return Err(self.err("invalid fraction"));
            }
            while self.i < self.src.len() && self.src.as_bytes()[self.i].is_ascii_digit() {
                self.advance();
            }
        }
        if self.i < self.src.len()
            && (self.src.as_bytes()[self.i] == b'e' || self.src.as_bytes()[self.i] == b'E')
        {
            self.advance();
            if self.i < self.src.len()
                && (self.src.as_bytes()[self.i] == b'+' || self.src.as_bytes()[self.i] == b'-')
            {
                self.advance();
            }
            if self.i >= self.src.len() || !self.src.as_bytes()[self.i].is_ascii_digit() {
                return Err(self.err("invalid exponent"));
            }
            while self.i < self.src.len() && self.src.as_bytes()[self.i].is_ascii_digit() {
                self.advance();
            }
        }
        let raw = self.slice(s, self.i);
        let v: f64 = raw
            .parse()
            .map_err(|_| self.err("invalid number"))?;
        Ok(Node {
            kind: Kind::Number,
            start: s,
            end: self.i,
            value: Value::Number(v),
            items: vec![],
            close_trivia: String::new(),
        })
    }

    fn object(&mut self) -> Result<Node, Error> {
        let mut n = Node {
            kind: Kind::Object,
            start: self.i,
            end: 0,
            value: Value::Null,
            items: vec![],
            close_trivia: String::new(),
        };
        self.advance();
        let mut m: BTreeMap<String, Value> = BTreeMap::new();
        loop {
            let a = self.i;
            self.trivia()?;
            let leading = self.slice(a, self.i);
            if self.i < self.src.len() && self.src.as_bytes()[self.i] == b'}' {
                n.close_trivia = leading;
                self.advance();
                n.end = self.i;
                n.value = Value::object_with(m.into_iter().collect());
                return Ok(n);
            }
            if self.i >= self.src.len() || self.src.as_bytes()[self.i] != b'"' {
                return Err(self.err("object key must be a quoted string"));
            }
            let head_start = self.i;
            let k = self.string()?;
            if m.contains_key(&k) {
                return Err(self.err(&format!("duplicate object key {:?}", k)));
            }
            self.trivia()?;
            if self.i >= self.src.len() || self.src.as_bytes()[self.i] != b':' {
                return Err(self.err("expected ':'"));
            }
            self.advance();
            self.trivia()?;
            let v = self.value()?;
            let vvalue = v.value.clone();
            let head = self.slice(head_start, v.start);
            let a = self.i;
            self.trivia()?;
            let trailing = self.slice(a, self.i);
            let mut comma = false;
            if self.i < self.src.len() && self.src.as_bytes()[self.i] == b',' {
                comma = true;
                self.advance();
            }
            n.items.push(Item {
                key: k.clone(),
                leading,
                head,
                trailing,
                value: Box::new(v),
                comma,
            });
            m.insert(k, vvalue);
            if !comma {
                if self.i >= self.src.len() || self.src.as_bytes()[self.i] != b'}' {
                    return Err(self.err("expected ',' or '}'"));
                }
            }
        }
    }

    fn array(&mut self) -> Result<Node, Error> {
        let mut n = Node {
            kind: Kind::Array,
            start: self.i,
            end: 0,
            value: Value::Null,
            items: vec![],
            close_trivia: String::new(),
        };
        self.advance();
        let mut vals: Vec<Value> = Vec::new();
        loop {
            let a = self.i;
            self.trivia()?;
            let leading = self.slice(a, self.i);
            if self.i < self.src.len() && self.src.as_bytes()[self.i] == b']' {
                n.close_trivia = leading;
                self.advance();
                n.end = self.i;
                n.value = Value::array(vals);
                return Ok(n);
            }
            let v = self.value()?;
            let vv = v.value.clone();
            let a = self.i;
            self.trivia()?;
            let trailing = self.slice(a, self.i);
            let mut comma = false;
            if self.i < self.src.len() && self.src.as_bytes()[self.i] == b',' {
                comma = true;
                self.advance();
            }
            n.items.push(Item {
                key: String::new(),
                leading,
                head: String::new(),
                trailing,
                value: Box::new(v),
                comma,
            });
            vals.push(vv);
            if !comma {
                if self.i >= self.src.len() || self.src.as_bytes()[self.i] != b']' {
                    return Err(self.err("expected ',' or ']'"));
                }
            }
        }
    }
}

fn decode_json_string(raw: &str) -> Option<String> {
    let bytes = raw.as_bytes();
    if bytes.len() < 2 || bytes[0] != b'"' || bytes[bytes.len() - 1] != b'"' {
        return None;
    }
    let inner = &raw[1..raw.len() - 1];
    let b = inner.as_bytes();
    let mut out = String::new();
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        if c == b'"' {
            return None;
        }
        if c < 0x20 {
            return None;
        }
        if c == b'\\' {
            i += 1;
            if i >= b.len() {
                return None;
            }
            let e = b[i];
            match e {
                b'"' => {
                    out.push('"');
                    i += 1;
                }
                b'\\' => {
                    out.push('\\');
                    i += 1;
                }
                b'/' => {
                    out.push('/');
                    i += 1;
                }
                b'b' => {
                    out.push('\u{0008}');
                    i += 1;
                }
                b'f' => {
                    out.push('\u{000C}');
                    i += 1;
                }
                b'n' => {
                    out.push('\n');
                    i += 1;
                }
                b'r' => {
                    out.push('\r');
                    i += 1;
                }
                b't' => {
                    out.push('\t');
                    i += 1;
                }
                b'u' => {
                    if i + 5 > b.len() {
                        return None;
                    }
                    let n = parse_hex4(&b[i + 1..i + 5])?;
                    i += 5;
                    if (0xD800..=0xDBFF).contains(&n) {
                        if i + 6 <= b.len() && b[i] == b'\\' && b[i + 1] == b'u' {
                            let n2 = parse_hex4(&b[i + 2..i + 6])?;
                            if (0xDC00..=0xDFFF).contains(&n2) {
                                let cp = 0x10000 + ((n - 0xD800) << 10) + (n2 - 0xDC00);
                                out.push(char::from_u32(cp)?);
                                i += 6;
                            } else {
                                out.push('\u{FFFD}');
                            }
                        } else {
                            out.push('\u{FFFD}');
                        }
                    } else if (0xDC00..=0xDFFF).contains(&n) {
                        out.push('\u{FFFD}');
                    } else {
                        out.push(char::from_u32(n)?);
                    }
                }
                _ => return None,
            }
        } else {
            let ch = inner[i..].chars().next()?;
            out.push(ch);
            i += ch.len_utf8();
        }
    }
    Some(out)
}

fn parse_hex4(b: &[u8]) -> Option<u32> {
    let mut n = 0u32;
    for &byte in b {
        let d = match byte {
            b'0'..=b'9' => byte - b'0',
            b'a'..=b'f' => byte - b'a' + 10,
            b'A'..=b'F' => byte - b'A' + 10,
            _ => return None,
        };
        n = n * 16 + d as u32;
    }
    Some(n)
}

pub fn compact(v: &Value) -> Result<String, String> {
    marshal(v)
}

impl Document {
    pub fn preserve(&self, v: &Value) -> Result<String, String> {
        let body = self.render(&self.root, v)?;
        Ok(format!("{}{}{}", self.prefix, body, self.suffix))
    }

    fn render(&self, n: &Node, v: &Value) -> Result<String, String> {
        if n.value == *v {
            return Ok(self.source[n.start..n.end].to_string());
        }
        match n.kind {
            Kind::Object => {
                if let Value::Object(o) = v {
                    return self.render_object(n, o);
                }
            }
            Kind::Array => {
                if let Value::Array(a) = v {
                    return self.render_array(n, a);
                }
            }
            _ => {}
        }
        Ok(marshal(v)?)
    }

    fn render_object(
        &self,
        n: &Node,
        v: &RefCell<BTreeMap<String, Value>>,
    ) -> Result<String, String> {
        let v = v.borrow();
        let mut b = String::new();
        b.push('{');
        let mut kept: Vec<&Item> = Vec::new();
        let mut seen: BTreeSet<String> = BTreeSet::new();
        for it in &n.items {
            if v.contains_key(&it.key) {
                kept.push(it);
                seen.insert(it.key.clone());
            }
        }
        let mut new_keys: Vec<&String> = Vec::new();
        for k in v.keys() {
            if !seen.contains(k) {
                new_keys.push(k);
            }
        }
        let total = kept.len() + new_keys.len();
        for (i, it) in kept.iter().enumerate() {
            b.push_str(&it.leading);
            b.push_str(&it.head);
            let s = self.render(it.value.as_ref(), &v[&it.key])?;
            b.push_str(&s);
            let needs_comma = i < kept.len() - 1 || !new_keys.is_empty();
            let preserve_trailing = !needs_comma && kept.len() == n.items.len() && it.comma;
            if needs_comma && !it.comma {
                b.push(',');
                b.push_str(&it.trailing);
            } else {
                b.push_str(&it.trailing);
                if needs_comma || preserve_trailing {
                    b.push(',');
                }
            }
        }
        let style = self.style(n);
        for (j, k) in new_keys.iter().enumerate() {
            if kept.is_empty() && j == 0 {
                b.push_str(&style.first);
            } else {
                b.push_str(&style.next);
            }
            let mut kb = String::new();
            encode::append_json_string(&mut kb, k);
            b.push_str(&kb);
            b.push_str(&style.colon);
            let s = marshal(&v[k.as_str()])?;
            b.push_str(&s);
            let need_final_comma = kept.len() + j < total - 1
                || (!n.items.is_empty() && n.items.last().unwrap().comma);
            if need_final_comma {
                b.push(',');
            }
        }
        let mut close = n.close_trivia.clone();
        let last_original_kept = new_keys.is_empty()
            && !kept.is_empty()
            && std::ptr::eq(*kept.last().unwrap(), n.items.last().unwrap());
        if close.is_empty() && !last_original_kept {
            if !n.items.is_empty() {
                close = closing_whitespace(&n.items.last().unwrap().trailing);
            } else if total > 0 && style.first.contains('\n') {
                close = self.newline.clone();
            }
        }
        b.push_str(&close);
        b.push('}');
        Ok(b)
    }

    fn render_array(&self, n: &Node, v: &RefCell<Vec<Value>>) -> Result<String, String> {
        let v = v.borrow();
        let mut b = String::new();
        b.push('[');
        let pairs = match_items(&n.items, &v);
        let style = self.style(n);
        for (i, pair) in pairs.iter().enumerate() {
            let needs_comma = i < pairs.len() - 1;
            if pair.0 >= 0 {
                let it = &n.items[pair.0 as usize];
                b.push_str(&it.leading);
                let s = self.render(it.value.as_ref(), &v[pair.1 as usize])?;
                b.push_str(&s);
                let preserve_trailing =
                    !needs_comma && pairs.len() == n.items.len() && it.comma;
                if needs_comma && !it.comma {
                    b.push(',');
                    b.push_str(&it.trailing);
                } else {
                    b.push_str(&it.trailing);
                    if needs_comma || preserve_trailing {
                        b.push(',');
                    }
                }
            } else {
                if i == 0 {
                    b.push_str(&style.first);
                } else {
                    b.push_str(&style.next);
                }
                let s = marshal(&v[pair.1 as usize])?;
                b.push_str(&s);
                if needs_comma
                    || (i == pairs.len() - 1 && !n.items.is_empty() && n.items.last().unwrap().comma)
                {
                    b.push(',');
                }
            }
        }
        let mut close = n.close_trivia.clone();
        let last_original_kept =
            !pairs.is_empty() && pairs.last().unwrap().0 == (n.items.len() as i64) - 1;
        if close.is_empty() && !last_original_kept {
            if !n.items.is_empty() {
                close = closing_whitespace(&n.items.last().unwrap().trailing);
            } else if !pairs.is_empty() && style.first.contains('\n') {
                close = self.newline.clone();
            }
        }
        b.push_str(&close);
        b.push(']');
        Ok(b)
    }

    fn style(&self, n: &Node) -> StyleInfo {
        let mut s = StyleInfo {
            first: String::new(),
            next: String::new(),
            colon: ": ".to_string(),
        };
        if !n.items.is_empty() {
            let it = &n.items[0];
            s.first = clean_trivia(&it.leading);
            s.next = s.first.clone();
            s.colon = key_separator(&it.head);
        } else if n.close_trivia.contains('\n') || n.close_trivia.contains('\r') {
            s.first = format!("{}  ", self.newline);
            s.next = s.first.clone();
        } else {
            s.first = n.close_trivia.clone();
            s.next = " ".to_string();
        }
        if s.first.is_empty() {
            s.next = " ".to_string();
        }
        s
    }
}

struct StyleInfo {
    first: String,
    next: String,
    colon: String,
}

fn match_items(old: &[Item], v: &[Value]) -> Vec<(i64, i64)> {
    let m = old.len();
    let n = v.len();
    if m == n {
        return (0..n).map(|i| (i as i64, i as i64)).collect();
    }
    let mut dp = vec![vec![0i64; n + 1]; m + 1];
    for i in (0..m).rev() {
        for j in (0..n).rev() {
            if old[i].value.value == v[j] {
                dp[i][j] = dp[i + 1][j + 1] + 1;
            } else if dp[i + 1][j] >= dp[i][j + 1] {
                dp[i][j] = dp[i + 1][j];
            } else {
                dp[i][j] = dp[i][j + 1];
            }
        }
    }
    let mut r = Vec::new();
    let (mut i, mut j) = (0usize, 0usize);
    while j < n {
        if i < m && old[i].value.value == v[j] {
            r.push((i as i64, j as i64));
            i += 1;
            j += 1;
        } else if i < m && dp[i + 1][j] >= dp[i][j + 1] {
            i += 1;
        } else {
            r.push((-1, j as i64));
            j += 1;
        }
    }
    r
}

fn key_separator(head: &str) -> String {
    let b = head.as_bytes();
    let mut in_escape = false;
    for i in 1..b.len() {
        let c = b[i];
        if in_escape {
            in_escape = false;
            continue;
        }
        if c == b'\\' {
            in_escape = true;
            continue;
        }
        if c == b'"' {
            let sep = &head[i + 1..];
            if sep.contains('/') || sep.contains('\r') || sep.contains('\n') {
                return ": ".to_string();
            }
            return sep.to_string();
        }
    }
    ": ".to_string()
}

fn clean_trivia(s: &str) -> String {
    if s.contains('\n') {
        let i = s.rfind('\n').unwrap();
        return s[i..].to_string();
    }
    if s.contains('\r') {
        let i = s.rfind('\r').unwrap();
        return s[i..].to_string();
    }
    " ".to_string()
}

fn closing_whitespace(s: &str) -> String {
    let b = s.as_bytes();
    let mut last_end: i64 = -1;
    let mut i = 0;
    while i < b.len() {
        if b[i..].starts_with(b"//") {
            let mut j = i + 2;
            while j < b.len() && b[j] != b'\n' && b[j] != b'\r' {
                j += 1;
            }
            last_end = j as i64;
            i = j;
            continue;
        }
        if b[i..].starts_with(b"/*") {
            let rest = &s[i + 2..];
            match rest.find("*/") {
                None => return String::new(),
                Some(j) => {
                    let j = i + 2 + j + 2;
                    last_end = j as i64;
                    i = j;
                    continue;
                }
            }
        }
        i += 1;
    }
    if last_end >= 0 {
        return s[last_end as usize..].to_string();
    }
    s.to_string()
}

#[allow(unused_assignments)]
pub fn pretty_preserve(src: &str, indent: &str) -> Result<String, String> {
    let indent = if indent.is_empty() { "  " } else { indent };
    let mut out: Vec<u8> = Vec::new();
    let b = src.as_bytes();
    let mut level: isize = 0;
    let mut need_indent = false;
    let mut in_string = false;
    let mut escape = false;
    let mut i = 0;
    while i < b.len() {
        if in_string {
            let c = b[i];
            out.push(c);
            i += 1;
            if escape {
                escape = false;
            } else if c == b'\\' {
                escape = true;
            } else if c == b'"' {
                in_string = false;
            }
            continue;
        }
        if b[i] == b'"' {
            if need_indent {
                out.extend_from_slice(indent.repeat(level as usize).as_bytes());
                need_indent = false;
            }
            in_string = true;
            out.push(b[i]);
            i += 1;
            continue;
        }
        if b[i..].starts_with(b"//") {
            if need_indent {
                out.extend_from_slice(indent.repeat(level as usize).as_bytes());
                need_indent = false;
            } else if !out.is_empty() {
                out.push(b' ');
            }
            let mut j = i + 2;
            while j < b.len() && b[j] != b'\n' && b[j] != b'\r' {
                j += 1;
            }
            out.extend_from_slice(&b[i..j]);
            out.push(b'\n');
            need_indent = true;
            i = j;
            while i < b.len() && (b[i] == b'\n' || b[i] == b'\r') {
                i += 1;
            }
            continue;
        }
        if b[i..].starts_with(b"/*") {
            if need_indent {
                out.extend_from_slice(indent.repeat(level as usize).as_bytes());
                need_indent = false;
            } else if !out.is_empty() {
                out.push(b' ');
            }
            let rest = &src[i + 2..];
            let j = match rest.find("*/") {
                Some(j) => i + 2 + j + 2,
                None => return Err("unterminated comment".to_string()),
            };
            out.extend_from_slice(&b[i..j]);
            i = j;
            continue;
        }
        let c = b[i];
        match c {
            b' ' | b'\t' | b'\r' | b'\n' => {
                i += 1;
                continue;
            }
            b'{' | b'[' => {
                if need_indent {
                    out.extend_from_slice(indent.repeat(level as usize).as_bytes());
                    need_indent = false;
                }
                out.push(c);
                let mut j = i + 1;
                while j < b.len()
                    && (b[j] == b' ' || b[j] == b'\t' || b[j] == b'\r' || b[j] == b'\n')
                {
                    j += 1;
                }
                let matching = if c == b'[' { b']' } else { b'}' };
                if j < b.len() && b[j] == matching {
                    out.push(matching);
                    i = j + 1;
                    continue;
                } else {
                    level += 1;
                    out.push(b'\n');
                    need_indent = true;
                }
            }
            b'}' | b']' => {
                level -= 1;
                if !need_indent {
                    out.push(b'\n');
                }
                out.extend_from_slice(indent.repeat(level as usize).as_bytes());
                out.push(c);
                need_indent = false;
            }
            b',' => {
                out.push(c);
                out.push(b'\n');
                need_indent = true;
            }
            b':' => {
                out.extend_from_slice(b": ");
            }
            b'/' => {
                out.push(c);
            }
            _ => {
                if need_indent {
                    out.extend_from_slice(indent.repeat(level as usize).as_bytes());
                    need_indent = false;
                }
                out.push(c);
            }
        }
        i += 1;
    }
    let s = String::from_utf8(out).map_err(|_| "invalid utf-8 in output".to_string())?;
    Ok(s.trim().to_string())
}
