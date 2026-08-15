use std::cell::RefCell;
use std::collections::BTreeMap;
use std::io::Write;
use std::rc::Rc;

use regex::Regex;

use crate::jsonc;
use crate::value::Value;

use super::ast::{Expr, Program, Stmt};
use super::parser;
use super::token::{Error, Pos, Tok};

pub struct Runtime<'a> {
    globals: BTreeMap<String, Value>,
    max_steps: usize,
    steps: usize,
    last: Option<Value>,
    output: Option<&'a mut dyn Write>,
}

enum Ref {
    Var(String, Pos),
    ObjField(Rc<RefCell<BTreeMap<String, Value>>>, String, Pos),
    ArrElem(Rc<RefCell<Vec<Value>>>, usize, Pos),
}

impl<'a> Runtime<'a> {
    fn new(root: Value, max: usize) -> Runtime<'a> {
        let max = if max == 0 { 1_000_000 } else { max };
        let mut globals = BTreeMap::new();
        globals.insert("$".to_string(), root);
        Runtime {
            globals,
            max_steps: max,
            steps: 0,
            last: None,
            output: None,
        }
    }

    fn root(&self) -> Value {
        self.globals["$"].clone()
    }

    fn last(&self) -> Option<Value> {
        self.last.clone()
    }

    fn run(&mut self, p: &Program) -> Result<(), Error> {
        if let Some(sig) = self.exec_list(&p.list)? {
            return Err(self.fail(Pos { line: 1, col: 1 }, &format!("{} outside loop", sig)));
        }
        Ok(())
    }

    fn step(&mut self, p: Pos) -> Result<(), Error> {
        self.steps += 1;
        if self.steps > self.max_steps {
            return Err(self.fail(p, "maximum execution steps exceeded"));
        }
        Ok(())
    }

    fn fail(&self, p: Pos, msg: &str) -> Error {
        Error::new("RuntimeError", p, msg.to_string())
    }

    fn exec_list(&mut self, xs: &[Stmt]) -> Result<Option<&'static str>, Error> {
        for s in xs {
            self.step(s.pos())?;
            if let Some(sig) = self.exec(s)? {
                return Ok(Some(sig));
            }
        }
        Ok(None)
    }

    fn exec(&mut self, s: &Stmt) -> Result<Option<&'static str>, Error> {
        match s {
            Stmt::Expr(_, x) => {
                let v = self.eval(x)?;
                self.last = Some(v);
                Ok(None)
            }
            Stmt::Block(_, xs) => self.exec_list(xs),
            Stmt::If(_, cond, then, els) => {
                let v = self.eval(cond)?;
                if truth(&v) {
                    self.exec(then)
                } else if let Some(e) = els {
                    self.exec(e)
                } else {
                    Ok(None)
                }
            }
            Stmt::Delete(_, target) => {
                let r = self.reference(target)?;
                self.ref_del(&r)?;
                Ok(None)
            }
            Stmt::Break(_) => Ok(Some("break")),
            Stmt::Continue(_) => Ok(Some("continue")),
            Stmt::For(p, name, of, source, body) => self.exec_for(*p, name, *of, source, body),
        }
    }

    fn exec_for(
        &mut self,
        p: Pos,
        name: &str,
        of: bool,
        source: &Expr,
        body: &Stmt,
    ) -> Result<Option<&'static str>, Error> {
        let v = self.eval(source)?;
        if of {
            return self.exec_for_of(p, name, &v, body);
        }
        let keys: Vec<Value> = match &v {
            Value::Array(a) => (0..a.borrow().len()).map(|i| Value::Number(i as f64)).collect(),
            Value::Object(o) => o.borrow().keys().cloned().map(Value::String).collect(),
            _ => return Err(self.fail(p, "for..in requires array or object")),
        };
        for k in keys {
            let current = self.eval(source)?;
            if !exists(&current, &k) {
                continue;
            }
            self.globals.insert(name.to_string(), k);
            match self.exec(body)? {
                Some("break") => break,
                Some("continue") => continue,
                Some(sig) => return Ok(Some(sig)),
                None => {}
            }
        }
        Ok(None)
    }

    fn exec_for_of(
        &mut self,
        p: Pos,
        name: &str,
        iterable: &Value,
        body: &Stmt,
    ) -> Result<Option<&'static str>, Error> {
        match iterable {
            Value::Array(a) => {
                let mut i = 0usize;
                loop {
                    let len = a.borrow().len();
                    if i >= len {
                        break;
                    }
                    let value = a.borrow()[i].clone();
                    self.globals.insert(name.to_string(), value);
                    match self.exec(body)? {
                        None | Some("continue") => {}
                        Some("break") => return Ok(None),
                        Some(sig) => {
                            return Err(self.fail(p, &format!("unexpected loop signal {:?}", sig)))
                        }
                    }
                    i += 1;
                }
                Ok(None)
            }
            Value::String(s) => {
                for ch in s.chars() {
                    self.globals.insert(name.to_string(), Value::String(ch.to_string()));
                    match self.exec(body)? {
                        None | Some("continue") => {}
                        Some("break") => return Ok(None),
                        Some(sig) => {
                            return Err(self.fail(p, &format!("unexpected loop signal {:?}", sig)))
                        }
                    }
                }
                Ok(None)
            }
            _ => Err(self.fail(p, "for..of requires array or string")),
        }
    }

    fn eval(&mut self, e: &Expr) -> Result<Value, Error> {
        self.step(e.pos())?;
        match e {
            Expr::Literal(_, v) => Ok(v.clone()),
            Expr::Variable(p, name) => self
                .globals
                .get(name)
                .cloned()
                .ok_or_else(|| self.fail(*p, &format!("undefined variable {:?}", name))),
            Expr::Array(_, items) => {
                let mut a = Vec::with_capacity(items.len());
                for q in items {
                    a.push(self.eval(q)?);
                }
                Ok(Value::array(a))
            }
            Expr::Object(_, items) => {
                let mut m = BTreeMap::new();
                for (k, v) in items {
                    let val = self.eval(v)?;
                    m.insert(k.clone(), val);
                }
                Ok(Value::object_with(m.into_iter().collect()))
            }
            Expr::Unary(p, op, x) => {
                let v = self.eval(x)?;
                if *op == Tok::Bang {
                    Ok(Value::Bool(!truth(&v)))
                } else {
                    match v {
                        Value::Number(n) => Ok(Value::Number(-n)),
                        _ => Err(self.fail(*p, "unary '-' requires number")),
                    }
                }
            }
            Expr::Binary(p, op, l, r) => {
                let a = self.eval(l)?;
                if *op == Tok::And {
                    if !truth(&a) {
                        return Ok(Value::Bool(false));
                    }
                    let b = self.eval(r)?;
                    return Ok(Value::Bool(truth(&b)));
                }
                if *op == Tok::Or {
                    if truth(&a) {
                        return Ok(Value::Bool(true));
                    }
                    let b = self.eval(r)?;
                    return Ok(Value::Bool(truth(&b)));
                }
                let b = self.eval(r)?;
                self.apply(*p, *op, &a, &b)
            }
            Expr::Assign(p, op, target, value) => self.assign(*p, *op, target, value),
            Expr::Member(p, obj, key) => {
                let obj_v = self.eval(obj)?;
                let key_v = self.eval(key)?;
                self.member_value(*p, &obj_v, &key_v)
            }
            Expr::Call(p, name, args) => self.call(*p, name, args),
            Expr::MethodCall(p, recv, name, args) => self.method_call(*p, recv, name, args),
        }
    }

    fn member_value(&mut self, p: Pos, obj: &Value, key: &Value) -> Result<Value, Error> {
        match obj {
            Value::String(s) => {
                if matches!(key, Value::String(k) if k == "length") {
                    return Ok(Value::Number(s.chars().count() as f64));
                }
                Err(self.fail(
                    p,
                    &format!("string property {:?} does not exist", value_string(key)),
                ))
            }
            Value::Object(o) => {
                let k = match key {
                    Value::String(k) => k,
                    _ => return Err(self.fail(p, "object key must be string")),
                };
                Ok(o.borrow().get(k).cloned().unwrap_or(Value::Null))
            }
            Value::Array(a) => {
                if matches!(key, Value::String(k) if k == "length") {
                    return Ok(Value::Number(a.borrow().len() as f64));
                }
                let i = match index(key) {
                    Some(i) => i,
                    None => return Err(self.fail(p, "array index must be a non-negative integer")),
                };
                let a = a.borrow();
                if i >= a.len() {
                    return Err(self.fail(p, &format!("array index {} out of range", i)));
                }
                Ok(a[i].clone())
            }
            _ => Err(self.fail(p, "member access requires array or object")),
        }
    }

    fn apply(&mut self, p: Pos, op: Tok, a: &Value, b: &Value) -> Result<Value, Error> {
        match op {
            Tok::Eq => return Ok(Value::Bool(a == b)),
            Tok::Ne => return Ok(Value::Bool(a != b)),
            _ => {}
        }
        if op == Tok::Plus {
            if matches!(a, Value::String(_)) || matches!(b, Value::String(_)) {
                return Ok(Value::String(format!("{}{}", value_string(a), value_string(b))));
            }
        }
        if let (Value::Number(x), Value::Number(y)) = (a, b) {
            let (x, y) = (*x, *y);
            match op {
                Tok::Plus => return Ok(Value::Number(x + y)),
                Tok::Minus => return Ok(Value::Number(x - y)),
                Tok::Star => return Ok(Value::Number(x * y)),
                Tok::Slash => {
                    if y == 0.0 {
                        return Err(self.fail(p, "division by zero"));
                    }
                    return Ok(Value::Number(x / y));
                }
                Tok::GT => return Ok(Value::Bool(x > y)),
                Tok::GE => return Ok(Value::Bool(x >= y)),
                Tok::LT => return Ok(Value::Bool(x < y)),
                Tok::LE => return Ok(Value::Bool(x <= y)),
                _ => {}
            }
        }
        if let (Value::String(x), Value::String(y)) = (a, b) {
            match op {
                Tok::GT => return Ok(Value::Bool(x > y)),
                Tok::GE => return Ok(Value::Bool(x >= y)),
                Tok::LT => return Ok(Value::Bool(x < y)),
                Tok::LE => return Ok(Value::Bool(x <= y)),
                _ => {}
            }
        }
        Err(self.fail(p, "operator has incompatible operand types"))
    }

    fn assign(&mut self, p: Pos, op: Tok, target: &Expr, value: &Expr) -> Result<Value, Error> {
        let r = self.reference(target)?;
        let mut v = self.eval(value)?;
        if op != Tok::Assign {
            let old = self.ref_get(&r)?;
            let apply_op = match op {
                Tok::PlusAssign => Tok::Plus,
                Tok::MinusAssign => Tok::Minus,
                Tok::StarAssign => Tok::Star,
                Tok::SlashAssign => Tok::Slash,
                _ => Tok::Plus,
            };
            v = self.apply(p, apply_op, &old, &v)?;
        }
        self.ref_set(&r, v.clone())?;
        Ok(v)
    }

    fn reference(&mut self, e: &Expr) -> Result<Ref, Error> {
        match e {
            Expr::Variable(p, name) => Ok(Ref::Var(name.clone(), *p)),
            Expr::Member(p, obj, key) => {
                let parent = self.reference(obj)?;
                let obj_v = self.ref_get(&parent)?;
                let key_v = self.eval(key)?;
                self.member_ref(*p, &obj_v, &key_v)
            }
            _ => Err(self.fail(e.pos(), "invalid assignment target")),
        }
    }

    fn member_ref(&mut self, p: Pos, obj: &Value, key: &Value) -> Result<Ref, Error> {
        match obj {
            Value::Object(o) => {
                let k = match key {
                    Value::String(k) => k.clone(),
                    _ => return Err(self.fail(p, "object key must be string")),
                };
                Ok(Ref::ObjField(o.clone(), k, p))
            }
            Value::Array(a) => {
                let i = match index(key) {
                    Some(i) => i,
                    None => return Err(self.fail(p, "array index must be a non-negative integer")),
                };
                Ok(Ref::ArrElem(a.clone(), i, p))
            }
            _ => Err(self.fail(p, "member access requires array or object")),
        }
    }

    fn ref_get(&self, r: &Ref) -> Result<Value, Error> {
        match r {
            Ref::Var(name, p) => self
                .globals
                .get(name)
                .cloned()
                .ok_or_else(|| self.fail(*p, &format!("undefined variable {:?}", name))),
            Ref::ObjField(o, k, _) => Ok(o.borrow().get(k).cloned().unwrap_or(Value::Null)),
            Ref::ArrElem(a, i, p) => {
                let a = a.borrow();
                if *i >= a.len() {
                    Err(self.fail(*p, &format!("array index {} out of range", *i)))
                } else {
                    Ok(a[*i].clone())
                }
            }
        }
    }

    fn ref_set(&mut self, r: &Ref, v: Value) -> Result<(), Error> {
        match r {
            Ref::Var(name, _) => {
                self.globals.insert(name.clone(), v);
                Ok(())
            }
            Ref::ObjField(o, k, _) => {
                o.borrow_mut().insert(k.clone(), v);
                Ok(())
            }
            Ref::ArrElem(a, i, p) => {
                let mut a = a.borrow_mut();
                if *i >= a.len() {
                    return Err(self.fail(*p, &format!("array index {} out of range", *i)));
                }
                a[*i] = v;
                Ok(())
            }
        }
    }

    fn ref_del(&mut self, r: &Ref) -> Result<(), Error> {
        match r {
            Ref::Var(_, p) => Err(self.fail(*p, "cannot delete variable")),
            Ref::ObjField(o, k, p) => {
                let mut o = o.borrow_mut();
                if !o.contains_key(k) {
                    return Err(self.fail(*p, &format!("object property {:?} does not exist", k)));
                }
                o.remove(k);
                Ok(())
            }
            Ref::ArrElem(a, i, p) => {
                let mut a = a.borrow_mut();
                if *i >= a.len() {
                    return Err(self.fail(*p, &format!("array index {} out of range", *i)));
                }
                a.remove(*i);
                Ok(())
            }
        }
    }

    fn call(&mut self, p: Pos, name: &str, args: &[Expr]) -> Result<Value, Error> {
        let mut values = Vec::with_capacity(args.len());
        for e in args {
            values.push(self.eval(e)?);
        }
        match name {
            "log" => {
                let parts: Vec<String> = values.iter().map(value_string).collect();
                let line = format!("{}\n", parts.join(" "));
                let write_result = match self.output.as_deref_mut() {
                    Some(out) => out.write_all(line.as_bytes()),
                    None => Ok(()),
                };
                if let Err(e) = write_result {
                    return Err(self.fail(p, &format!("write log output: {}", e)));
                }
                Ok(Value::Null)
            }
            "env" => {
                if values.len() != 1 {
                    return Err(self.fail(p, "env expects 1 argument"));
                }
                let name = match &values[0] {
                    Value::String(s) => s.clone(),
                    _ => return Err(self.fail(p, "env requires a string argument")),
                };
                Ok(std::env::var(&name)
                    .ok()
                    .map(Value::String)
                    .unwrap_or(Value::Null))
            }
            "typeof" => {
                if values.len() != 1 {
                    return Err(self.fail(p, "typeof expects 1 argument"));
                }
                let t = match &values[0] {
                    Value::String(_) => "string",
                    Value::Array(_) => "array",
                    Value::Object(_) | Value::Null => "object",
                    Value::Bool(_) => "boolean",
                    Value::Number(_) => "number",
                };
                Ok(Value::String(t.to_string()))
            }
            "keys" => {
                if values.len() != 1 {
                    return Err(self.fail(p, "keys expects 1 argument"));
                }
                match &values[0] {
                    Value::Array(a) => {
                        let items =
                            (0..a.borrow().len()).map(|i| Value::Number(i as f64)).collect();
                        Ok(Value::array(items))
                    }
                    Value::Object(o) => {
                        let items = o.borrow().keys().cloned().map(Value::String).collect();
                        Ok(Value::array(items))
                    }
                    _ => Err(self.fail(p, "keys requires array or object")),
                }
            }
            _ => Err(self.fail(p, &format!("unknown function {:?}", name))),
        }
    }

    fn method_call(
        &mut self,
        p: Pos,
        receiver: &Expr,
        name: &str,
        args: &[Expr],
    ) -> Result<Value, Error> {
        let recv = self.eval(receiver)?;
        let mut values = Vec::with_capacity(args.len());
        for a in args {
            values.push(self.eval(a)?);
        }
        if name == "toString" {
            if !values.is_empty() {
                return Err(self.fail(p, "toString expects no arguments"));
            }
            return Ok(Value::String(value_string(&recv)));
        }
        if let Value::Array(a) = &recv {
            return self.array_method(p, a, name, &values);
        }
        if name == "push" || name == "splice" || name == "join" || name == "reverse" {
            return Err(self.fail(p, &format!("{} requires an array receiver", name)));
        }
        if let Value::String(s) = &recv {
            return self.string_method(p, s, name, &values);
        }
        Err(self.fail(p, &format!("unknown method {:?}", name)))
    }

    fn string_method(
        &mut self,
        p: Pos,
        s: &str,
        name: &str,
        args: &[Value],
    ) -> Result<Value, Error> {
        let runes: Vec<char> = s.chars().collect();
        match name {
            "toLowerCase" | "toUpperCase" | "trim" => {
                if !args.is_empty() {
                    return Err(self.fail(p, &format!("{} expects no arguments", name)));
                }
                match name {
                    "toLowerCase" => Ok(Value::String(simple_to_lowercase(s))),
                    "toUpperCase" => Ok(Value::String(simple_to_uppercase(s))),
                    _ => Ok(Value::String(s.trim().to_string())),
                }
            }
            "substring" => {
                if args.is_empty() || args.len() > 2 {
                    return Err(self.fail(p, "substring expects 1 or 2 arguments"));
                }
                let start = integer_arg(&args[0])
                    .ok_or_else(|| self.fail(p, "substring indexes must be integers"))?;
                let mut end = runes.len() as i64;
                if args.len() == 2 {
                    end = integer_arg(&args[1])
                        .ok_or_else(|| self.fail(p, "substring indexes must be integers"))?;
                }
                let n = runes.len() as i64;
                let mut start = clamp(start, 0, n);
                let mut end = clamp(end, 0, n);
                if start > end {
                    std::mem::swap(&mut start, &mut end);
                }
                let out: String = runes[start as usize..end as usize].iter().collect();
                Ok(Value::String(out))
            }
            "indexOf" => {
                if args.is_empty() || args.len() > 2 {
                    return Err(self.fail(p, "indexOf expects 1 or 2 arguments"));
                }
                let needle = match &args[0] {
                    Value::String(s) => s.clone(),
                    _ => return Err(self.fail(p, "indexOf requires a string needle")),
                };
                let mut from: i64 = 0;
                if args.len() == 2 {
                    from = integer_arg(&args[1])
                        .ok_or_else(|| self.fail(p, "indexOf start must be an integer"))?;
                }
                let n = runes.len() as i64;
                from = clamp(from, 0, n);
                let haystack: String = runes[from as usize..].iter().collect();
                match haystack.find(&needle) {
                    None => Ok(Value::Number(-1.0)),
                    Some(byte_idx) => {
                        let prefix = &haystack[..byte_idx];
                        Ok(Value::Number((from + prefix.chars().count() as i64) as f64))
                    }
                }
            }
            "lastIndexOf" => {
                if args.is_empty() || args.len() > 2 {
                    return Err(self.fail(p, "lastIndexOf expects 1 or 2 arguments"));
                }
                let needle = match &args[0] {
                    Value::String(s) => s.clone(),
                    _ => return Err(self.fail(p, "lastIndexOf requires a string needle")),
                };
                let mut from: i64 = runes.len() as i64;
                if args.len() == 2 {
                    from = integer_arg(&args[1])
                        .ok_or_else(|| self.fail(p, "lastIndexOf start must be an integer"))?;
                }
                let needle_runes: Vec<char> = needle.chars().collect();
                let n = runes.len() as i64;
                if needle_runes.is_empty() {
                    return Ok(Value::Number(clamp(from, 0, n) as f64));
                }
                from = clamp(from, 0, n - needle_runes.len() as i64);
                let mut i = from;
                while i >= 0 {
                    let slice: String = runes[i as usize..i as usize + needle_runes.len()]
                        .iter()
                        .collect();
                    if slice == needle {
                        return Ok(Value::Number(i as f64));
                    }
                    i -= 1;
                }
                Ok(Value::Number(-1.0))
            }
            "localeCompare" => {
                if args.len() != 1 {
                    return Err(self.fail(p, "localeCompare expects 1 argument"));
                }
                let other = match &args[0] {
                    Value::String(s) => s,
                    _ => return Err(self.fail(p, "localeCompare requires a string argument")),
                };
                let n = match s.cmp(other) {
                    std::cmp::Ordering::Less => -1.0,
                    std::cmp::Ordering::Equal => 0.0,
                    std::cmp::Ordering::Greater => 1.0,
                };
                Ok(Value::Number(n))
            }
            "padStart" | "padEnd" => {
                if args.is_empty() || args.len() > 2 {
                    return Err(self.fail(p, &format!("{} expects 1 or 2 arguments", name)));
                }
                let target_length = integer_arg(&args[0]).ok_or_else(|| {
                    self.fail(p, &format!("{} target length must be a non-negative integer", name))
                })?;
                if target_length < 0 {
                    return Err(self.fail(
                        p,
                        &format!("{} target length must be a non-negative integer", name),
                    ));
                }
                let mut pad = " ".to_string();
                if args.len() == 2 {
                    pad = match &args[1] {
                        Value::String(s) => s.clone(),
                        _ => return Err(self.fail(p, &format!("{} padding must be a string", name))),
                    };
                }
                let pad_runes: Vec<char> = pad.chars().collect();
                let target_length = target_length as usize;
                if target_length <= runes.len() || pad_runes.is_empty() {
                    return Ok(Value::String(s.to_string()));
                }
                let padding_length = target_length - runes.len();
                let mut result: Vec<char> = vec!['\0'; target_length];
                if name == "padStart" {
                    for i in 0..padding_length {
                        result[i] = pad_runes[i % pad_runes.len()];
                    }
                    result[padding_length..].copy_from_slice(&runes);
                } else {
                    result[..runes.len()].copy_from_slice(&runes);
                    for i in 0..padding_length {
                        result[runes.len() + i] = pad_runes[i % pad_runes.len()];
                    }
                }
                Ok(Value::String(result.into_iter().collect()))
            }
            "split" | "match" | "matchAll" | "replace" | "replaceAll" => {
                self.regexp_string_method(p, s, name, args)
            }
            _ => Err(self.fail(p, &format!("unknown method {:?}", name))),
        }
    }

    fn regexp_string_method(
        &mut self,
        p: Pos,
        s: &str,
        name: &str,
        args: &[Value],
    ) -> Result<Value, Error> {
        let (min_args, max_args) = match name {
            "split" => (1, 2),
            "replace" | "replaceAll" => (2, 2),
            _ => (1, 1),
        };
        if args.len() < min_args || args.len() > max_args {
            if min_args == max_args {
                return Err(self.fail(p, &format!("{} expects {} argument(s)", name, min_args)));
            }
            return Err(self.fail(
                p,
                &format!("{} expects {} or {} arguments", name, min_args, max_args),
            ));
        }
        let pattern = match &args[0] {
            Value::String(s) => s.clone(),
            _ => return Err(self.fail(p, &format!("{} pattern must be a string", name))),
        };
        let re = Regex::new(&pattern)
            .map_err(|e| self.fail(p, &format!("invalid regular expression: {}", e)))?;
        match name {
            "split" => {
                let mut limit: i64 = -1;
                if args.len() == 2 {
                    limit = integer_arg(&args[1]).ok_or_else(|| {
                        self.fail(p, "split limit must be a non-negative integer")
                    })?;
                    if limit < 0 {
                        return Err(self.fail(p, "split limit must be a non-negative integer"));
                    }
                }
                let mut parts = re_split_all(&re, s);
                if limit >= 0 && parts.len() as i64 > limit {
                    parts.truncate(limit as usize);
                }
                Ok(Value::array(parts.into_iter().map(Value::String).collect()))
            }
            "match" => match find_submatch_index(&re, s) {
                None => Ok(Value::Null),
                Some(indexes) => Ok(regexp_match_value(s, &indexes)),
            },
            "matchAll" => {
                let items = re
                    .captures_iter(s)
                    .map(|caps| regexp_match_value(s, &captures_to_indexes(&caps)))
                    .collect();
                Ok(Value::array(items))
            }
            "replace" | "replaceAll" => {
                let replacement = match &args[1] {
                    Value::String(s) => s.clone(),
                    _ => return Err(self.fail(p, &format!("{} replacement must be a string", name))),
                };
                if name == "replaceAll" {
                    Ok(Value::String(replace_all(&re, s, &replacement)))
                } else {
                    match find_submatch_index(&re, s) {
                        None => Ok(Value::String(s.to_string())),
                        Some(indexes) => {
                            let caps = re.captures(s).unwrap();
                            let expanded = expand(&re, &replacement, s, &caps);
                            let (a, b) = indexes[0];
                            Ok(Value::String(format!(
                                "{}{}{}",
                                &s[..a as usize],
                                expanded,
                                &s[b as usize..]
                            )))
                        }
                    }
                }
            }
            _ => Err(self.fail(p, &format!("unknown method {:?}", name))),
        }
    }

    fn array_method(
        &mut self,
        p: Pos,
        array: &Rc<RefCell<Vec<Value>>>,
        name: &str,
        args: &[Value],
    ) -> Result<Value, Error> {
        match name {
            "push" => {
                if args.is_empty() {
                    return Err(self.fail(p, "push expects at least 1 argument"));
                }
                let mut a = array.borrow_mut();
                a.extend_from_slice(args);
                Ok(Value::Number(a.len() as f64))
            }
            "join" => {
                if args.len() > 1 {
                    return Err(self.fail(p, "join expects at most 1 argument"));
                }
                let sep = if args.len() == 1 {
                    match &args[0] {
                        Value::String(s) => s.clone(),
                        _ => return Err(self.fail(p, "join separator must be a string")),
                    }
                } else {
                    ",".to_string()
                };
                let a = array.borrow();
                let parts: Vec<String> = a
                    .iter()
                    .map(|v| {
                        if matches!(v, Value::Null) {
                            String::new()
                        } else {
                            value_string(v)
                        }
                    })
                    .collect();
                Ok(Value::String(parts.join(&sep)))
            }
            "splice" => {
                if args.is_empty() {
                    return Err(self.fail(p, "splice expects at least 1 argument"));
                }
                let start = integer_arg(&args[0])
                    .ok_or_else(|| self.fail(p, "splice start must be an integer"))?;
                let mut a = array.borrow_mut();
                let len = a.len() as i64;
                let mut start = if start < 0 { len + start } else { start };
                start = clamp(start, 0, len);
                let mut delete_count = len - start;
                if args.len() >= 2 {
                    let dc = integer_arg(&args[1])
                        .ok_or_else(|| self.fail(p, "splice delete count must be an integer"))?;
                    delete_count = clamp(dc, 0, len - start);
                }
                let start = start as usize;
                let delete_count = delete_count as usize;
                let removed: Vec<Value> = a[start..start + delete_count].to_vec();
                let replacement: Vec<Value> = if args.len() > 2 {
                    args[2..].to_vec()
                } else {
                    vec![]
                };
                let mut items = Vec::with_capacity(a.len() - delete_count + replacement.len());
                items.extend_from_slice(&a[..start]);
                items.extend_from_slice(&replacement);
                items.extend_from_slice(&a[start + delete_count..]);
                *a = items;
                Ok(Value::array(removed))
            }
            "reverse" => {
                if !args.is_empty() {
                    return Err(self.fail(p, "reverse expects no arguments"));
                }
                array.borrow_mut().reverse();
                Ok(Value::Array(array.clone()))
            }
            "indexOf" | "lastIndexOf" => {
                if args.is_empty() || args.len() > 2 {
                    return Err(self.fail(p, &format!("{} expects 1 or 2 arguments", name)));
                }
                let a = array.borrow();
                if name == "indexOf" {
                    let mut start: i64 = 0;
                    if args.len() == 2 {
                        start = integer_arg(&args[1])
                            .ok_or_else(|| self.fail(p, "indexOf start must be an integer"))?;
                    }
                    let len = a.len() as i64;
                    if start < 0 {
                        start = len + start;
                    }
                    start = clamp(start, 0, len);
                    for i in (start as usize)..a.len() {
                        if a[i] == args[0] {
                            return Ok(Value::Number(i as f64));
                        }
                    }
                    Ok(Value::Number(-1.0))
                } else {
                    let mut start: i64 = a.len() as i64 - 1;
                    if args.len() == 2 {
                        start = integer_arg(&args[1])
                            .ok_or_else(|| self.fail(p, "lastIndexOf start must be an integer"))?;
                        if start < 0 {
                            start = a.len() as i64 + start;
                        }
                    }
                    if start >= a.len() as i64 {
                        start = a.len() as i64 - 1;
                    }
                    let mut i = start;
                    while i >= 0 {
                        if a[i as usize] == args[0] {
                            return Ok(Value::Number(i as f64));
                        }
                        i -= 1;
                    }
                    Ok(Value::Number(-1.0))
                }
            }
            _ => Err(self.fail(p, &format!("unknown method {:?}", name))),
        }
    }
}

fn index(v: &Value) -> Option<usize> {
    match v {
        Value::Number(n) => {
            let n = *n;
            if n < 0.0 || n.fract() != 0.0 || n > usize::MAX as f64 {
                None
            } else {
                Some(n as usize)
            }
        }
        _ => None,
    }
}

fn integer_arg(v: &Value) -> Option<i64> {
    match v {
        Value::Number(n) => {
            let n = *n;
            if n.is_nan()
                || n.is_infinite()
                || n.fract() != 0.0
                || n < i64::MIN as f64
                || n > i64::MAX as f64
            {
                None
            } else {
                Some(n as i64)
            }
        }
        _ => None,
    }
}

fn clamp(v: i64, low: i64, high: i64) -> i64 {
    v.max(low).min(high)
}

/// Go's strings.ToUpper/ToLower use Unicode *simple* (1:1) case mapping, while
/// Rust's std to_uppercase/to_lowercase use *full* mapping (which can expand to
/// multiple chars, e.g. ß -> "SS"). Mirror Go: apply the full mapping only when
/// it is 1:1; otherwise leave the rune unchanged.
fn simple_to_uppercase(s: &str) -> String {
    s.chars()
        .flat_map(|c| {
            let mut it = c.to_uppercase();
            let first = it.next();
            if first.is_some() && it.next().is_none() {
                first.into_iter().collect::<Vec<_>>()
            } else {
                vec![c]
            }
        })
        .collect()
}

fn simple_to_lowercase(s: &str) -> String {
    s.chars()
        .flat_map(|c| {
            // U+0130 (İ) is the sole rune whose full lowercase expands to two
            // chars ("i" + combining dot); Go's simple mapping yields just "i".
            if c == '\u{0130}' {
                return vec!['i'];
            }
            let mut it = c.to_lowercase();
            let first = it.next();
            if first.is_some() && it.next().is_none() {
                first.into_iter().collect::<Vec<_>>()
            } else {
                vec![c]
            }
        })
        .collect()
}

fn truth(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => *n != 0.0,
        Value::String(s) => !s.is_empty(),
        _ => true,
    }
}

fn exists(v: &Value, key: &Value) -> bool {
    match v {
        Value::Array(a) => match index(key) {
            Some(i) => i < a.borrow().len(),
            None => false,
        },
        Value::Object(o) => match key {
            Value::String(k) => o.borrow().contains_key(k),
            _ => false,
        },
        _ => false,
    }
}

fn value_string(v: &Value) -> String {
    match v {
        Value::Null => "null".to_string(),
        Value::String(s) => s.clone(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => format!("{}", n),
        Value::Array(a) => {
            let parts: Vec<String> = a
                .borrow()
                .iter()
                .map(|x| {
                    if matches!(x, Value::Null) {
                        String::new()
                    } else {
                        value_string(x)
                    }
                })
                .collect();
            parts.join(",")
        }
        Value::Object(_) => match jsonc::marshal(v) {
            Ok(s) => s,
            Err(_) => format!("{:?}", v),
        },
    }
}

fn captures_to_indexes(caps: &regex::Captures) -> Vec<(i64, i64)> {
    (0..caps.len())
        .map(|i| match caps.get(i) {
            Some(m) => (m.start() as i64, m.end() as i64),
            None => (-1, -1),
        })
        .collect()
}

fn find_submatch_index(re: &Regex, s: &str) -> Option<Vec<(i64, i64)>> {
    re.captures(s).map(|c| captures_to_indexes(&c))
}

fn regexp_match_value(s: &str, indexes: &[(i64, i64)]) -> Value {
    let items = indexes
        .iter()
        .map(|&(start, end)| {
            if start >= 0 {
                Value::String(s[start as usize..end as usize].to_string())
            } else {
                Value::Null
            }
        })
        .collect();
    Value::array(items)
}

fn re_split_all(re: &Regex, s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut beg = 0usize;
    let mut end = 0usize;
    for m in re.find_iter(s) {
        end = m.start();
        if m.end() != 0 {
            out.push(s[beg..end].to_string());
        }
        beg = m.end();
    }
    if end != s.len() {
        out.push(s[beg..].to_string());
    }
    out
}

fn replace_all(re: &Regex, s: &str, replacement: &str) -> String {
    let mut out = String::new();
    let mut last = 0usize;
    for caps in re.captures_iter(s) {
        let indexes = captures_to_indexes(&caps);
        let (a, b) = indexes[0];
        out.push_str(&s[last..a as usize]);
        out.push_str(&expand(re, replacement, s, &caps));
        last = b as usize;
    }
    out.push_str(&s[last..]);
    out
}

fn expand(re: &Regex, template: &str, s: &str, caps: &regex::Captures) -> String {
    let mut out = String::new();
    let mut template = template;
    while let Some(dollar) = template.find('$') {
        out.push_str(&template[..dollar]);
        template = &template[dollar + 1..];
        if let Some(rest) = template.strip_prefix('$') {
            out.push('$');
            template = rest;
            continue;
        }
        match extract_group(template) {
            None => {
                out.push('$');
                template = &template[1..];
            }
            Some((name, num, rest)) => {
                template = rest;
                if let Some(n) = num {
                    if let Some(m) = caps.get(n) {
                        out.push_str(&s[m.start()..m.end()]);
                    }
                } else {
                    for (idx, nm) in re.capture_names().enumerate() {
                        if nm == Some(name) {
                            if let Some(m) = caps.get(idx) {
                                out.push_str(&s[m.start()..m.end()]);
                            }
                            break;
                        }
                    }
                }
            }
        }
    }
    out.push_str(template);
    out
}

fn extract_group(template: &str) -> Option<(&str, Option<usize>, &str)> {
    let mut t = template;
    let mut brace = false;
    if t.starts_with('{') {
        brace = true;
        t = &t[1..];
    }
    let mut i = 0;
    for c in t.chars() {
        if c.is_alphabetic() || c.is_ascii_digit() || c == '_' {
            i += c.len_utf8();
        } else {
            break;
        }
    }
    if i == 0 {
        return None;
    }
    let name = &t[..i];
    t = &t[i..];
    if brace {
        if !t.starts_with('}') {
            return None;
        }
        t = &t[1..];
    }
    let mut num: usize = 0;
    let mut is_num = true;
    for b in name.bytes() {
        if !b.is_ascii_digit() || num >= 100_000_000 {
            is_num = false;
            break;
        }
        num = num * 10 + (b - b'0') as usize;
    }
    if is_num {
        Some((name, Some(num), t))
    } else {
        Some((name, None, t))
    }
}

pub fn execute(src: &str, root: Value, max_steps: usize) -> Result<(Value, Option<Value>), Error> {
    execute_with_output(src, root, max_steps, &mut std::io::sink())
}

pub fn execute_with_output(
    src: &str,
    root: Value,
    max_steps: usize,
    output: &mut dyn Write,
) -> Result<(Value, Option<Value>), Error> {
    let p = parser::parse(src)?;
    let mut r = Runtime::new(root, max_steps);
    r.output = Some(output);
    r.run(&p)?;
    Ok((r.root(), r.last()))
}
