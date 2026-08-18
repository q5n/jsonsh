use std::any::Any;
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::io::Write;
use std::rc::Rc;

use crate::jsonc;
use crate::value::{Builtin, Function, Value};

use super::ast::{ArrowBody, ChainStep, Expr, Program, Stmt};
use super::parser;
use super::token::{Error, Pos, Tok};

struct Env {
    vars: RefCell<BTreeMap<String, Value>>,
    parent: Option<Rc<Env>>,
}

impl Env {
    fn new(parent: Option<Rc<Env>>) -> Rc<Env> {
        Rc::new(Env {
            vars: RefCell::new(BTreeMap::new()),
            parent,
        })
    }

    fn get(&self, name: &str) -> Option<Value> {
        if let Some(v) = self.vars.borrow().get(name) {
            return Some(v.clone());
        }
        self.parent.as_ref().and_then(|p| p.get(name))
    }

    fn assign(&self, name: &str, v: Value) -> bool {
        if self.vars.borrow().contains_key(name) {
            self.vars.borrow_mut().insert(name.to_string(), v);
            return true;
        }
        match &self.parent {
            Some(p) => p.assign(name, v),
            None => false,
        }
    }
}

#[derive(Debug)]
enum Signal {
    Break,
    Continue,
    Return(Value),
}

pub struct Runtime<'a> {
    scope: Rc<Env>,
    root: Rc<Env>,
    max_steps: usize,
    steps: usize,
    depth: usize,
    last: Option<Value>,
    output: Option<&'a mut dyn Write>,
}

const MAX_CALL_DEPTH: usize = 64;

enum Ref {
    Var(String, Pos),
    ObjField(Rc<RefCell<BTreeMap<String, Value>>>, String, Pos),
    ArrElem(Rc<RefCell<Vec<Value>>>, usize, Pos),
}

impl<'a> Runtime<'a> {
    fn new(root: Value, max: usize) -> Runtime<'a> {
        let max = if max == 0 { 1_000_000 } else { max };
        let root_env = Env::new(None);
        root_env.vars.borrow_mut().insert("$".to_string(), root);
        root_env
            .vars
            .borrow_mut()
            .insert("log".to_string(), Value::builtin(Builtin::Log));
        root_env
            .vars
            .borrow_mut()
            .insert("env".to_string(), Value::builtin(Builtin::Env));
        root_env
            .vars
            .borrow_mut()
            .insert("keys".to_string(), Value::builtin(Builtin::Keys));
        root_env
            .vars
            .borrow_mut()
            .insert("RegExp".to_string(), Value::builtin(Builtin::RegExp));
        for (name, c) in super::stdlib::constructors() {
            root_env
                .vars
                .borrow_mut()
                .insert(name.to_string(), Value::constructor(c));
        }
        for (name, f) in super::stdlib::global_functions() {
            root_env.vars.borrow_mut().insert(name.to_string(), f);
        }
        root_env
            .vars
            .borrow_mut()
            .insert("Math".to_string(), super::stdlib::math_object());
        Runtime {
            scope: root_env.clone(),
            root: root_env,
            max_steps: max,
            steps: 0,
            depth: 0,
            last: None,
            output: None,
        }
    }

    fn root(&self) -> Value {
        self.root.get("$").unwrap()
    }

    fn get_var(&self, name: &str) -> Option<Value> {
        self.scope.get(name)
    }

    fn set_var(&self, name: &str, v: Value) {
        if !self.scope.assign(name, v.clone()) {
            self.root
                .vars
                .borrow_mut()
                .insert(name.to_string(), v);
        }
    }

    fn last(&self) -> Option<Value> {
        self.last.clone()
    }

    fn run(&mut self, p: &Program) -> Result<(), Error> {
        if let Some(sig) = self.exec_list(&p.list)? {
            let label = match sig {
                Signal::Break => "break",
                Signal::Continue => "continue",
                Signal::Return(_) => "return",
            };
            return Err(self.fail(Pos { line: 1, col: 1 }, &format!("{} outside function", label)));
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

    fn exec_list(&mut self, xs: &[Stmt]) -> Result<Option<Signal>, Error> {
        for s in xs {
            self.step(s.pos())?;
            if let Some(sig) = self.exec(s)? {
                return Ok(Some(sig));
            }
        }
        Ok(None)
    }

    fn exec(&mut self, s: &Stmt) -> Result<Option<Signal>, Error> {
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
            Stmt::Break(_) => Ok(Some(Signal::Break)),
            Stmt::Continue(_) => Ok(Some(Signal::Continue)),
            Stmt::Return(_, e) => {
                let v = match e {
                    Some(x) => self.eval(x)?,
                    None => Value::Null,
                };
                Ok(Some(Signal::Return(v)))
            }
            Stmt::For(p, name, of, source, body) => self.exec_for(*p, name, *of, source, body),
            Stmt::ForC(p, init, cond, update, body) => {
                self.exec_for_c(*p, init, cond, update, body)
            }
        }
    }

    fn exec_for(
        &mut self,
        p: Pos,
        name: &str,
        of: bool,
        source: &Expr,
        body: &Stmt,
    ) -> Result<Option<Signal>, Error> {
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
            self.set_var(name, k);
            match self.exec(body)? {
                Some(Signal::Break) => break,
                Some(Signal::Continue) => continue,
                Some(sig) => return Ok(Some(sig)),
                None => {}
            }
        }
        Ok(None)
    }

    fn exec_for_c(
        &mut self,
        p: Pos,
        init: &Option<Box<Stmt>>,
        cond: &Option<Expr>,
        update: &Option<Expr>,
        body: &Stmt,
    ) -> Result<Option<Signal>, Error> {
        if let Some(init) = init {
            self.step(init.pos())?;
            self.exec(init)?;
        }
        loop {
            self.step(p)?;
            if let Some(cond) = cond {
                if !truth(&self.eval(cond)?) {
                    break;
                }
            }
            match self.exec(body)? {
                Some(Signal::Break) => break,
                Some(Signal::Continue) => {}
                Some(sig) => return Ok(Some(sig)),
                None => {}
            }
            if let Some(update) = update {
                self.step(update.pos())?;
                let v = self.eval(update)?;
                self.last = Some(v);
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
    ) -> Result<Option<Signal>, Error> {
        match iterable {
            Value::Array(a) => {
                let mut i = 0usize;
                loop {
                    let len = a.borrow().len();
                    if i >= len {
                        break;
                    }
                    let value = a.borrow()[i].clone();
                    self.set_var(name, value);
                    match self.exec(body)? {
                        None | Some(Signal::Continue) => {}
                        Some(Signal::Break) => return Ok(None),
                        Some(Signal::Return(v)) => return Ok(Some(Signal::Return(v))),
                    }
                    i += 1;
                }
                Ok(None)
            }
            Value::String(s) => {
                for ch in s.chars() {
                    self.set_var(name, Value::String(ch.to_string()));
                    match self.exec(body)? {
                        None | Some(Signal::Continue) => {}
                        Some(Signal::Break) => return Ok(None),
                        Some(Signal::Return(v)) => return Ok(Some(Signal::Return(v))),
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
                .get_var(name)
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
                if *op == Tok::Typeof {
                    let v = self.eval(x)?;
                    return Ok(Value::String(type_of(&v).to_string()));
                }
                let v = self.eval(x)?;
                match op {
                    Tok::Bang => Ok(Value::Bool(!truth(&v))),
                    Tok::BitNot => match v {
                        Value::Number(n) => Ok(Value::Number(!to_int32(n) as f64)),
                        _ => Err(self.fail(*p, "unary '~' requires number")),
                    },
                    _ => match v {
                        Value::Number(n) => Ok(Value::Number(-n)),
                        _ => Err(self.fail(*p, "unary '-' requires number")),
                    },
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
            Expr::New(p, name, args) => {
                let mut values = Vec::with_capacity(args.len());
                for e in args {
                    values.push(self.eval(e)?);
                }
                let f = self
                    .get_var(name)
                    .ok_or_else(|| self.fail(*p, &format!("unknown function {:?}", name)))?;
                if let Value::Constructor(c) = &f {
                    super::stdlib::construct(*c, &values)
                        .map_err(|msg| self.fail(*p, &msg))
                } else {
                    Err(self.fail(*p, &format!("{:?} is not a constructor", name)))
                }
            }
            Expr::MethodCall(p, recv, name, args) => self.method_call(*p, recv, name, args),
            Expr::Ternary(_p, cond, then, els) => {
                let c = self.eval(cond)?;
                if truth(&c) {
                    self.eval(then)
                } else {
                    self.eval(els)
                }
            }
            Expr::Update(p, op, target, prefix) => {
                let r = self.reference(target)?;
                let old = self.ref_get(&r)?;
                let old_n = match old {
                    Value::Number(n) => n,
                    _ => return Err(self.fail(*p, "increment/decrement requires a number")),
                };
                let delta = if *op == Tok::Inc { 1.0 } else { -1.0 };
                let new_n = old_n + delta;
                self.ref_set(&r, Value::Number(new_n))?;
                Ok(Value::Number(if *prefix { new_n } else { old_n }))
            }
            Expr::Optional(p, base, steps) => {
                let mut v = self.eval(base)?;
                if matches!(v, Value::Null) {
                    return Ok(Value::Null);
                }
                for step in steps {
                    match step {
                        ChainStep::Prop(key) => {
                            let k = self.eval(key)?;
                            v = self.member_value(*p, &v, &k)?;
                        }
                        ChainStep::Method(name, args) => {
                            let mut vals = Vec::with_capacity(args.len());
                            for a in args {
                                vals.push(self.eval(a)?);
                            }
                            v = self.invoke_method(*p, v, name, vals)?;
                        }
                    }
                }
                Ok(v)
            }
            Expr::Arrow(_, params, body) => {
                let body: Rc<dyn Any> = Rc::new((**body).clone());
                let env: Rc<dyn Any> = self.scope.clone();
                Ok(Value::closure(params.clone(), body, env))
            }
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
                let n = match index(key) {
                    Some(n) => n,
                    None => return Err(self.fail(p, "array index must be an integer")),
                };
                let a = a.borrow();
                let len = a.len();
                let i = resolve_index(p, n, len)?;
                if i >= len {
                    return Ok(Value::Null);
                }
                Ok(a[i].clone())
            }
            Value::Regex(re) => {
                let k = match key {
                    Value::String(k) => k,
                    _ => return Err(self.fail(p, "regex property key must be string")),
                };
                match k.as_str() {
                    "source" => Ok(Value::String(re.source().to_string())),
                    "flags" => Ok(Value::String(re.flags().to_string())),
                    "global" => Ok(Value::Bool(re.flags().global)),
                    "ignoreCase" => Ok(Value::Bool(re.flags().ignore_case)),
                    "multiline" => Ok(Value::Bool(re.flags().multiline)),
                    _ => Err(self.fail(p, &format!("regex property {:?} does not exist", k))),
                }
            }
            Value::Constructor(c) => {
                let k = match key {
                    Value::String(k) => k,
                    _ => return Err(self.fail(p, "constructor property key must be string")),
                };
                match super::stdlib::static_method(*c, k) {
                    Some(f) => Ok(f),
                    None => Err(self.fail(p, &format!("{:?}.{} does not exist", c, k))),
                }
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
                Tok::Percent => {
                    if y == 0.0 {
                        return Err(self.fail(p, "modulo by zero"));
                    }
                    return Ok(Value::Number(x % y));
                }
                Tok::BitAnd => return Ok(Value::Number((to_int32(x) & to_int32(y)) as f64)),
                Tok::BitOr => return Ok(Value::Number((to_int32(x) | to_int32(y)) as f64)),
                Tok::BitXor => return Ok(Value::Number((to_int32(x) ^ to_int32(y)) as f64)),
                Tok::Shl => {
                    let s = to_shift(y);
                    return Ok(Value::Number(to_int32(x).wrapping_shl(s) as f64));
                }
                Tok::Shr => {
                    let s = to_shift(y);
                    return Ok(Value::Number((to_int32(x) >> s) as f64));
                }
                Tok::UShr => {
                    let s = to_shift(y);
                    return Ok(Value::Number((to_uint32(x) >> s) as f64));
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
                Tok::PercentAssign => Tok::Percent,
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
                let n = match index(key) {
                    Some(n) => n,
                    None => return Err(self.fail(p, "array index must be an integer")),
                };
                let i = resolve_index(p, n, a.borrow().len())?;
                Ok(Ref::ArrElem(a.clone(), i, p))
            }
            _ => Err(self.fail(p, "member access requires array or object")),
        }
    }

    fn ref_get(&self, r: &Ref) -> Result<Value, Error> {
        match r {
            Ref::Var(name, p) => self
                .get_var(name)
                .ok_or_else(|| self.fail(*p, &format!("undefined variable {:?}", name))),
            Ref::ObjField(o, k, _) => Ok(o.borrow().get(k).cloned().unwrap_or(Value::Null)),
            Ref::ArrElem(a, i, _) => {
                let a = a.borrow();
                if *i >= a.len() {
                    Ok(Value::Null)
                } else {
                    Ok(a[*i].clone())
                }
            }
        }
    }

    fn ref_set(&mut self, r: &Ref, v: Value) -> Result<(), Error> {
        match r {
            Ref::Var(name, _) => {
                self.set_var(name, v);
                Ok(())
            }
            Ref::ObjField(o, k, _) => {
                o.borrow_mut().insert(k.clone(), v);
                Ok(())
            }
            Ref::ArrElem(a, i, p) => {
                let mut a = a.borrow_mut();
                if *i >= a.len() {
                    if *i > isize::MAX as usize {
                        return Err(self.fail(*p, &format!("array index {} out of range", *i)));
                    }
                    a.resize(*i + 1, Value::Null);
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
            Ref::ArrElem(a, i, _) => {
                let mut a = a.borrow_mut();
                if *i < a.len() {
                    a.remove(*i);
                }
                Ok(())
            }
        }
    }

    fn call(&mut self, p: Pos, name: &str, args: &[Expr]) -> Result<Value, Error> {
        let mut values = Vec::with_capacity(args.len());
        for e in args {
            values.push(self.eval(e)?);
        }
        let f = self
            .get_var(name)
            .ok_or_else(|| self.fail(p, &format!("unknown function {:?}", name)))?;
        self.invoke(p, &f, &values)
    }

    fn invoke(&mut self, p: Pos, f: &Value, values: &[Value]) -> Result<Value, Error> {
        match f {
            Value::Builtin(b) => self.run_builtin(p, *b, values),
            Value::Constructor(c) => super::stdlib::construct(*c, values)
                .map_err(|msg| self.fail(p, &msg)),
            Value::Function(f) => match f.as_ref() {
                Function::Native(n) => n(values).map_err(|msg| self.fail(p, &msg)),
                Function::Closure(c) => self.invoke_closure(p, c, values),
            },
            _ => Err(self.fail(p, "value is not callable")),
        }
    }

    fn invoke_closure(
        &mut self,
        p: Pos,
        c: &Rc<crate::value::ClosureData>,
        values: &[Value],
    ) -> Result<Value, Error> {
        let body = c
            .body
            .downcast_ref::<ArrowBody>()
            .ok_or_else(|| self.fail(p, "internal error: invalid closure body"))?;
        let parent: Rc<Env> = c
            .env
            .clone()
            .downcast::<Env>()
            .map_err(|_| self.fail(p, "internal error: invalid closure environment"))?;
        if self.depth >= MAX_CALL_DEPTH {
            return Err(self.fail(p, "maximum call stack depth exceeded"));
        }
        self.depth += 1;
        let local = Env::new(Some(parent));
        for (i, param) in c.params.iter().enumerate() {
            let v = values.get(i).cloned().unwrap_or(Value::Null);
            local.vars.borrow_mut().insert(param.clone(), v);
        }
        let saved_scope = self.scope.clone();
        self.scope = local;
        let result = self.run_arrow_body(p, body);
        self.scope = saved_scope;
        self.depth -= 1;
        result
    }

    fn run_arrow_body(&mut self, p: Pos, body: &ArrowBody) -> Result<Value, Error> {
        if !body.block {
            let e = body.expr.as_ref().unwrap();
            return self.eval(e);
        }
        match self.exec_list(&body.stmts)? {
            Some(Signal::Return(v)) => Ok(v),
            Some(_) => Err(self.fail(p, "control flow signal outside loop")),
            None => Ok(Value::Null),
        }
    }

    fn run_builtin(&mut self, p: Pos, b: Builtin, values: &[Value]) -> Result<Value, Error> {
        match b {
            Builtin::Log => {
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
            Builtin::Env => {
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
            Builtin::Keys => {
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
            Builtin::RegExp => {
                if values.is_empty() || values.len() > 2 {
                    return Err(self.fail(p, "RegExp expects 1 or 2 arguments"));
                }
                let pattern = match &values[0] {
                    Value::String(s) => s.clone(),
                    _ => return Err(self.fail(p, "RegExp pattern must be a string")),
                };
                let flags = values.get(1).map(|v| match v {
                    Value::String(s) => Ok(s.clone()),
                    _ => Err(self.fail(p, "RegExp flags must be a string")),
                }).unwrap_or(Ok(String::new()))?;
                let re = crate::regex::Regex::new(&pattern, &flags)
                    .map_err(|e| self.fail(p, &format!("invalid regular expression: {}", e)))?;
                Ok(Value::regex(re))
            }
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
        self.invoke_method(p, recv, name, values)
    }

    fn invoke_method(
        &mut self,
        p: Pos,
        recv: Value,
        name: &str,
        values: Vec<Value>,
    ) -> Result<Value, Error> {
        if let Value::Constructor(c) = &recv {
            return match super::stdlib::static_method(*c, name) {
                Some(f) => self.invoke(p, &f, &values),
                None => Err(self.fail(p, &format!("{:?}.{} is not a function", c, name))),
            };
        }
        if let Value::Number(n) = &recv {
            return self.number_method(p, *n, name, &values);
        }
        if let Value::Date(ms) = &recv {
            return self.date_method(p, *ms, name, &values);
        }
        if name == "toString" {
            if !values.is_empty() {
                return Err(self.fail(p, "toString expects no arguments"));
            }
            return Ok(Value::String(value_string(&recv)));
        }
        if name == "valueOf" {
            if !values.is_empty() {
                return Err(self.fail(p, "valueOf expects no arguments"));
            }
            return Ok(recv.clone());
        }
        if let Value::Object(o) = &recv {
            if let Some(member) = o.borrow().get(name) {
                if matches!(member, Value::Function(_) | Value::Builtin(_)) {
                    return self.invoke(p, member, &values);
                }
            }
            return self.object_method(p, o, name, &values);
        }
        if let Value::Array(a) = &recv {
            return self.array_method(p, a, name, &values);
        }
        if name == "push" || name == "splice" || name == "join" || name == "reverse" {
            return Err(self.fail(p, &format!("{} requires an array receiver", name)));
        }
        if let Value::Regex(re) = &recv {
            return self.regex_method(p, re, name, &values);
        }
        if let Value::String(s) = &recv {
            return self.string_method(p, s, name, &values);
        }
        Err(self.fail(p, &format!("unknown method {:?}", name)))
    }

    fn number_method(
        &mut self,
        p: Pos,
        n: f64,
        name: &str,
        args: &[Value],
    ) -> Result<Value, Error> {
        match name {
            "valueOf" => {
                if !args.is_empty() {
                    return Err(self.fail(p, "valueOf expects no arguments"));
                }
                Ok(Value::Number(n))
            }
            "toString" => {
                if args.len() > 1 {
                    return Err(self.fail(p, "toString expects 0 or 1 arguments"));
                }
                if args.len() == 1 {
                    let radix = integer_arg(&args[0])
                        .ok_or_else(|| self.fail(p, "toString radix must be an integer"))?;
                    if !(2..=36).contains(&radix) {
                        return Err(self.fail(p, "toString radix must be between 2 and 36"));
                    }
                    return Ok(Value::String(radix_string(n, radix)));
                }
                Ok(Value::String(super::stdlib::number_to_string(n)))
            }
            "toFixed" => {
                if args.len() > 1 {
                    return Err(self.fail(p, "toFixed expects 0 or 1 arguments"));
                }
                let digits = match args.first() {
                    None => 0,
                    Some(v) => {
                        let d = integer_arg(v)
                            .ok_or_else(|| self.fail(p, "toFixed digits must be an integer"))?;
                        if !(0..=100).contains(&d) {
                            return Err(self.fail(p, "toFixed digits must be between 0 and 100"));
                        }
                        d as usize
                    }
                };
                Ok(Value::String(super::stdlib::number_to_fixed(n, digits)))
            }
            _ => Err(self.fail(p, &format!("unknown method {:?}", name))),
        }
    }

    fn object_method(
        &mut self,
        p: Pos,
        o: &Rc<RefCell<BTreeMap<String, Value>>>,
        name: &str,
        args: &[Value],
    ) -> Result<Value, Error> {
        match name {
            "hasOwnProperty" => {
                if args.len() != 1 {
                    return Err(self.fail(p, "hasOwnProperty expects 1 argument"));
                }
                let key = match &args[0] {
                    Value::String(s) => s,
                    _ => return Err(self.fail(p, "hasOwnProperty requires a string key")),
                };
                Ok(Value::Bool(o.borrow().contains_key(key)))
            }
            _ => Err(self.fail(p, &format!("unknown method {:?}", name))),
        }
    }

    fn date_method(
        &mut self,
        p: Pos,
        ms: f64,
        name: &str,
        args: &[Value],
    ) -> Result<Value, Error> {
        if !args.is_empty() {
            return Err(self.fail(p, &format!("{} expects no arguments", name)));
        }
        let parts = crate::date::date_parts(ms);
        match name {
            "valueOf" | "getTime" => Ok(Value::Number(ms)),
            "getFullYear" | "getUTCFullYear" => Ok(Value::Number(parts.year as f64)),
            "getMonth" | "getUTCMonth" => Ok(Value::Number((parts.month - 1) as f64)),
            "getDate" | "getUTCDate" => Ok(Value::Number(parts.day as f64)),
            "getDay" | "getUTCDay" => Ok(Value::Number(parts.weekday as f64)),
            "getHours" | "getUTCHours" => Ok(Value::Number(parts.hours as f64)),
            "getMinutes" | "getUTCMinutes" => Ok(Value::Number(parts.minutes as f64)),
            "getSeconds" | "getUTCSeconds" => Ok(Value::Number(parts.seconds as f64)),
            "getMilliseconds" | "getUTCMilliseconds" => Ok(Value::Number(parts.millis as f64)),
            "toISOString" | "toString" => Ok(Value::String(crate::date::to_iso_string(ms))),
            _ => Err(self.fail(p, &format!("unknown method {:?}", name))),
        }
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
            "charAt" => {
                if args.len() != 1 {
                    return Err(self.fail(p, "charAt expects 1 argument"));
                }
                let i = integer_arg(&args[0])
                    .ok_or_else(|| self.fail(p, "charAt index must be an integer"))?;
                if i < 0 || i >= runes.len() as i64 {
                    return Ok(Value::String(String::new()));
                }
                Ok(Value::String(runes[i as usize].to_string()))
            }
            "charCodeAt" => {
                if args.len() != 1 {
                    return Err(self.fail(p, "charCodeAt expects 1 argument"));
                }
                let i = integer_arg(&args[0])
                    .ok_or_else(|| self.fail(p, "charCodeAt index must be an integer"))?;
                if i < 0 || i >= runes.len() as i64 {
                    return Ok(Value::Number(f64::NAN));
                }
                Ok(Value::Number(runes[i as usize] as u32 as f64))
            }
            "concat" => {
                let mut out = s.to_string();
                for a in args {
                    out.push_str(&value_string(a));
                }
                Ok(Value::String(out))
            }
            "includes" | "startsWith" | "endsWith" => {
                if args.is_empty() || args.len() > 2 {
                    return Err(self.fail(p, &format!("{} expects 1 or 2 arguments", name)));
                }
                let needle = match &args[0] {
                    Value::String(s) => s,
                    _ => return Err(self.fail(p, &format!("{} requires a string argument", name))),
                };
                let n = runes.len() as i64;
                let hay: String = match name {
                    "includes" => {
                        let from = if args.len() == 2 {
                            clamp(
                                integer_arg(&args[1]).ok_or_else(|| {
                                    self.fail(p, &format!("{} position must be an integer", name))
                                })?,
                                0,
                                n,
                            )
                        } else {
                            0
                        };
                        runes[from as usize..].iter().collect()
                    }
                    "startsWith" => {
                        let from = if args.len() == 2 {
                            clamp(
                                integer_arg(&args[1]).ok_or_else(|| {
                                    self.fail(p, &format!("{} position must be an integer", name))
                                })?,
                                0,
                                n,
                            )
                        } else {
                            0
                        };
                        runes[from as usize..].iter().collect()
                    }
                    _ => {
                        let end = if args.len() == 2 {
                            clamp(
                                integer_arg(&args[1]).ok_or_else(|| {
                                    self.fail(p, &format!("{} position must be an integer", name))
                                })?,
                                0,
                                n,
                            )
                        } else {
                            n
                        };
                        runes[..end as usize].iter().collect()
                    }
                };
                let found = match name {
                    "includes" => hay.contains(needle),
                    "startsWith" => hay.starts_with(needle),
                    _ => hay.ends_with(needle),
                };
                Ok(Value::Bool(found))
            }
            "slice" => {
                if args.is_empty() || args.len() > 2 {
                    return Err(self.fail(p, "slice expects 1 or 2 arguments"));
                }
                let start = integer_arg(&args[0])
                    .ok_or_else(|| self.fail(p, "slice indexes must be integers"))?;
                let end = if args.len() == 2 {
                    integer_arg(&args[1])
                        .ok_or_else(|| self.fail(p, "slice indexes must be integers"))?
                } else {
                    runes.len() as i64
                };
                let n = runes.len() as i64;
                let start = normalize_slice_index(start, n);
                let end = normalize_slice_index(end, n);
                if start > end {
                    return Ok(Value::String(String::new()));
                }
                let out: String = runes[start as usize..end as usize].iter().collect();
                Ok(Value::String(out))
            }
            "repeat" => {
                if args.len() != 1 {
                    return Err(self.fail(p, "repeat expects 1 argument"));
                }
                let count = integer_arg(&args[0])
                    .ok_or_else(|| self.fail(p, "repeat count must be an integer"))?;
                if count < 0 {
                    return Err(self.fail(p, "repeat count must be non-negative"));
                }
                Ok(Value::String(s.repeat(count as usize)))
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

        match name {
            "split" => {
                let mut limit: Option<usize> = None;
                if args.len() == 2 {
                    let n = integer_arg(&args[1])
                        .ok_or_else(|| self.fail(p, "split limit must be a non-negative integer"))?;
                    if n < 0 {
                        return Err(self.fail(p, "split limit must be a non-negative integer"));
                    }
                    limit = Some(n as usize);
                }
                let parts = match &args[0] {
                    Value::String(sep) => literal_split(s, sep, limit),
                    Value::Regex(re) => re.split(s, limit).map_err(|e| self.fail(p, &e))?,
                    _ => return Err(self.fail(p, "split pattern must be a string or regex")),
                };
                Ok(Value::array(parts.into_iter().map(Value::String).collect()))
            }
            "match" => {
                let re = match &args[0] {
                    Value::String(pat) => crate::regex::Regex::new(pat, "")
                        .map_err(|e| self.fail(p, &format!("invalid regular expression: {}", e)))?,
                    Value::Regex(re) => re.as_ref().clone(),
                    _ => return Err(self.fail(p, "match pattern must be a string or regex")),
                };
                if re.flags().global {
                    let ms = re.find_all(s).map_err(|e| self.fail(p, &e))?;
                    let items = ms
                        .into_iter()
                        .map(|m| {
                            let (a, b) = m.captures[0].unwrap();
                            Value::String(u16_range_to_str(s, a, b))
                        })
                        .collect();
                    Ok(Value::array(items))
                } else {
                    match re.find(s, 0).map_err(|e| self.fail(p, &e))? {
                        None => Ok(Value::Null),
                        Some(m) => Ok(regexp_match_value(s, &m.captures)),
                    }
                }
            }
            "matchAll" => {
                let re = match &args[0] {
                    Value::Regex(re) => re,
                    _ => return Err(self.fail(p, "matchAll requires a regular expression")),
                };
                if !re.flags().global {
                    return Err(self.fail(p, "matchAll requires a regular expression with g flag"));
                }
                let ms = re.find_all(s).map_err(|e| self.fail(p, &e))?;
                let items = ms.into_iter().map(|m| regexp_match_value(s, &m.captures)).collect();
                Ok(Value::array(items))
            }
            "replace" => {
                let re = match &args[0] {
                    Value::String(pat) => crate::regex::Regex::new(pat, "")
                        .map_err(|e| self.fail(p, &format!("invalid regular expression: {}", e)))?,
                    Value::Regex(re) => re.as_ref().clone(),
                    _ => return Err(self.fail(p, "replace pattern must be a string or regex")),
                };
                let replacement = match &args[1] {
                    Value::String(s) => s.clone(),
                    _ => return Err(self.fail(p, "replace replacement must be a string")),
                };
                let out = re.replace(s, &replacement).map_err(|e| self.fail(p, &e))?;
                Ok(Value::String(out))
            }
            "replaceAll" => {
                let re = match &args[0] {
                    Value::String(pat) => crate::regex::Regex::new(pat, "g")
                        .map_err(|e| self.fail(p, &format!("invalid regular expression: {}", e)))?,
                    Value::Regex(re) => {
                        if !re.flags().global {
                            return Err(self.fail(p, "replaceAll requires a regular expression with g flag"));
                        }
                        re.as_ref().clone()
                    }
                    _ => return Err(self.fail(p, "replaceAll pattern must be a string or regex")),
                };
                let replacement = match &args[1] {
                    Value::String(s) => s.clone(),
                    _ => return Err(self.fail(p, "replaceAll replacement must be a string")),
                };
                let out = re.replace(s, &replacement).map_err(|e| self.fail(p, &e))?;
                Ok(Value::String(out))
            }
            _ => Err(self.fail(p, &format!("unknown method {:?}", name))),
        }
    }

    fn regex_method(
        &mut self,
        p: Pos,
        re: &Rc<crate::regex::Regex>,
        name: &str,
        args: &[Value],
    ) -> Result<Value, Error> {
        match name {
            "test" => {
                if args.len() != 1 {
                    return Err(self.fail(p, "test expects 1 argument"));
                }
                let s = match &args[0] {
                    Value::String(s) => s,
                    _ => return Err(self.fail(p, "test argument must be a string")),
                };
                Ok(Value::Bool(re.test(s).map_err(|e| self.fail(p, &e))?))
            }
            "exec" => {
                if args.len() != 1 {
                    return Err(self.fail(p, "exec expects 1 argument"));
                }
                let s = match &args[0] {
                    Value::String(s) => s,
                    _ => return Err(self.fail(p, "exec argument must be a string")),
                };
                match re.find(s, 0).map_err(|e| self.fail(p, &e))? {
                    None => Ok(Value::Null),
                    Some(m) => {
                        let mut entries: Vec<(String, Value)> = Vec::new();
                        for (i, cap) in m.captures.iter().enumerate() {
                            let v = match cap {
                                Some((a, b)) => Value::String(u16_range_to_str(s, *a, *b)),
                                None => Value::Null,
                            };
                            entries.push((i.to_string(), v));
                        }
                        let (a, _) = m.captures[0].unwrap();
                        entries.push(("index".to_string(), Value::Number(a as f64)));
                        entries.push(("input".to_string(), Value::String(s.to_string())));
                        Ok(Value::object_with(entries))
                    }
                }
            }
            _ => Err(self.fail(p, &format!("unknown regex method {:?}", name))),
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
            "concat" => {
                let mut out = array.borrow().clone();
                for a in args {
                    match a {
                        Value::Array(inner) => out.extend_from_slice(&inner.borrow()),
                        v => out.push(v.clone()),
                    }
                }
                Ok(Value::array(out))
            }
            "slice" => {
                if args.is_empty() || args.len() > 2 {
                    return Err(self.fail(p, "slice expects 1 or 2 arguments"));
                }
                let len = array.borrow().len() as i64;
                let start = normalize_slice_index(
                    integer_arg(&args[0])
                        .ok_or_else(|| self.fail(p, "slice indexes must be integers"))?,
                    len,
                );
                let end = if args.len() == 2 {
                    normalize_slice_index(
                        integer_arg(&args[1])
                            .ok_or_else(|| self.fail(p, "slice indexes must be integers"))?,
                        len,
                    )
                } else {
                    len
                };
                if start >= end {
                    return Ok(Value::array(vec![]));
                }
                let out = array.borrow()[start as usize..end as usize].to_vec();
                Ok(Value::array(out))
            }
            "includes" => {
                if args.is_empty() || args.len() > 2 {
                    return Err(self.fail(p, "includes expects 1 or 2 arguments"));
                }
                let a = array.borrow();
                let len = a.len() as i64;
                let mut from = if args.len() == 2 {
                    integer_arg(&args[1])
                        .ok_or_else(|| self.fail(p, "includes start must be an integer"))?
                } else {
                    0
                };
                if from < 0 {
                    from = (len + from).max(0);
                }
                from = from.min(len);
                let found = a[from as usize..].iter().any(|v| v == &args[0]);
                Ok(Value::Bool(found))
            }
            "map" | "filter" | "forEach" | "find" | "some" | "every" => {
                if args.len() != 1 {
                    return Err(self.fail(p, &format!("{} expects 1 argument", name)));
                }
                let f = &args[0];
                let items = array.borrow().clone();
                let mut out: Vec<Value> = Vec::new();
                for (i, item) in items.iter().enumerate() {
                    self.step(p)?;
                    let v = self.invoke(p, f, &[item.clone(), Value::Number(i as f64)])?;
                    match name {
                        "map" => out.push(v),
                        "filter" => {
                            if truth(&v) {
                                out.push(item.clone());
                            }
                        }
                        "find" => {
                            if truth(&v) {
                                return Ok(item.clone());
                            }
                        }
                        "some" => {
                            if truth(&v) {
                                return Ok(Value::Bool(true));
                            }
                        }
                        "every" => {
                            if !truth(&v) {
                                return Ok(Value::Bool(false));
                            }
                        }
                        _ => {}
                    }
                }
                match name {
                    "map" | "filter" => Ok(Value::array(out)),
                    "forEach" => Ok(Value::Null),
                    "find" => Ok(Value::Null),
                    "some" => Ok(Value::Bool(false)),
                    "every" => Ok(Value::Bool(true)),
                    _ => unreachable!(),
                }
            }
            "reduce" => {
                if args.is_empty() || args.len() > 2 {
                    return Err(self.fail(p, "reduce expects 1 or 2 arguments"));
                }
                let f = &args[0];
                let items = array.borrow().clone();
                let mut acc: Value;
                let start: usize;
                if args.len() == 2 {
                    acc = args[1].clone();
                    start = 0;
                } else {
                    if items.is_empty() {
                        return Err(self.fail(p, "reduce of empty array with no initial value"));
                    }
                    acc = items[0].clone();
                    start = 1;
                }
                for (i, item) in items.iter().enumerate().skip(start) {
                    self.step(p)?;
                    acc = self.invoke(p, f, &[acc, item.clone(), Value::Number(i as f64)])?;
                }
                Ok(acc)
            }
            "sort" => {
                if args.len() > 1 {
                    return Err(self.fail(p, "sort expects 0 or 1 arguments"));
                }
                match args.first() {
                    None => {
                        array.borrow_mut().sort_by_key(value_string);
                    }
                    Some(f) => {
                        let mut sorted = array.borrow().clone();
                        let mut err: Option<Error> = None;
                        sorted.sort_by(|x, y| match self.invoke(p, f, &[x.clone(), y.clone()]) {
                            Ok(Value::Number(n)) => {
                                if n < 0.0 {
                                    std::cmp::Ordering::Less
                                } else if n > 0.0 {
                                    std::cmp::Ordering::Greater
                                } else {
                                    std::cmp::Ordering::Equal
                                }
                            }
                            Ok(_) => std::cmp::Ordering::Equal,
                            Err(e) => {
                                err = Some(e);
                                std::cmp::Ordering::Equal
                            }
                        });
                        if let Some(e) = err {
                            return Err(e);
                        }
                        *array.borrow_mut() = sorted;
                    }
                }
                Ok(Value::Array(array.clone()))
            }
            _ => Err(self.fail(p, &format!("unknown method {:?}", name))),
        }
    }
}

fn index(v: &Value) -> Option<i64> {
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

fn resolve_index(p: Pos, n: i64, len: usize) -> Result<usize, Error> {
    let i = if n < 0 { len as i64 + n } else { n };
    if i < 0 {
        return Err(Error::new(
            "RuntimeError",
            p,
            format!("array index {} out of range", n),
        ));
    }
    Ok(i as usize)
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

/// ECMAScript ToInt32: wrap the value into a signed 32-bit integer.
fn to_int32(n: f64) -> i32 {
    if !n.is_finite() {
        return 0;
    }
    n.rem_euclid(4_294_967_296.0) as u32 as i32
}

/// ECMAScript ToUint32: wrap the value into an unsigned 32-bit integer.
fn to_uint32(n: f64) -> u32 {
    if !n.is_finite() {
        return 0;
    }
    n.rem_euclid(4_294_967_296.0) as u32
}

/// Shift counts are taken modulo 32 (ToUint32 & 31).
fn to_shift(n: f64) -> u32 {
    to_uint32(n) & 31
}

fn clamp(v: i64, low: i64, high: i64) -> i64 {
    v.max(low).min(high)
}

fn normalize_slice_index(v: i64, n: i64) -> i64 {
    if v < 0 {
        (n + v).max(0)
    } else {
        v.min(n)
    }
}

fn radix_string(n: f64, radix: i64) -> String {
    const DIGITS: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyz";
    if n.is_nan() {
        return "NaN".to_string();
    }
    if n.is_infinite() {
        return if n > 0.0 { "Infinity" } else { "-Infinity" }.to_string();
    }
    let mut value = n.trunc().abs() as u64;
    let mut digits = String::new();
    if value == 0 {
        digits.push('0');
    } else {
        while value > 0 {
            digits.push(DIGITS[(value % radix as u64) as usize] as char);
            value /= radix as u64;
        }
    }
    if n < 0.0 {
        digits.push('-');
    }
    digits.chars().rev().collect()
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
        Value::Number(n) => !n.is_nan() && *n != 0.0,
        Value::String(s) => !s.is_empty(),
        _ => true,
    }
}

fn type_of(v: &Value) -> &'static str {
    match v {
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) | Value::Null | Value::Regex(_) | Value::Date(_) => "object",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::Function(_) | Value::Builtin(_) | Value::Constructor(_) => "function",
    }
}

fn exists(v: &Value, key: &Value) -> bool {
    match v {
        Value::Array(a) => match index(key) {
            Some(i) => i >= 0 && (i as usize) < a.borrow().len(),
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
        Value::Number(n) => super::stdlib::number_to_string(*n),
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
        Value::Function(_) | Value::Builtin(_) | Value::Constructor(_) => "[Function]".to_string(),
        Value::Regex(re) => format!("/{}/{}", re.source(), re.flags()),
        Value::Date(ms) => crate::date::to_iso_string(*ms),
        Value::Object(_) => match jsonc::marshal(v) {
            Ok(s) => s,
            Err(_) => format!("{:?}", v),
        },
    }
}

fn regexp_match_value(s: &str, caps: &[Option<(usize, usize)>]) -> Value {
    let items = caps
        .iter()
        .map(|c| match c {
            Some((a, b)) => Value::String(u16_range_to_str(s, *a, *b)),
            None => Value::Null,
        })
        .collect();
    Value::array(items)
}

fn u16_range_to_str(s: &str, start: usize, end: usize) -> String {
    let v = crate::regex::to_utf16(s);
    let start = start.min(v.len());
    let end = end.min(v.len());
    if start >= end {
        return String::new();
    }
    char::decode_utf16(v[start..end].iter().copied())
        .map(|r| r.unwrap_or('\u{FFFD}'))
        .collect()
}

fn literal_split(s: &str, sep: &str, limit: Option<usize>) -> Vec<String> {
    let mut parts: Vec<String> = if sep.is_empty() {
        s.chars().map(|c| c.to_string()).collect()
    } else {
        s.split(sep).map(|x| x.to_string()).collect()
    };
    if let Some(lim) = limit {
        parts.truncate(lim);
    }
    parts
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
