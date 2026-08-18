use std::any::Any;
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::fmt;
use std::rc::Rc;

use unicode_general_category::get_general_category;
use unicode_general_category::GeneralCategory::*;

pub type NativeFn = Rc<dyn Fn(&[Value]) -> Result<Value, String>>;

pub struct ClosureData {
    pub params: Vec<String>,
    pub body: Rc<dyn Any>,
    pub env: Rc<dyn Any>,
}

impl fmt::Debug for ClosureData {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Closure")
            .field("params", &self.params)
            .finish()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Builtin {
    Log,
    Env,
    Keys,
}

#[derive(Clone)]
pub enum Function {
    Closure(Rc<ClosureData>),
    Native(NativeFn),
}

impl fmt::Debug for Function {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Function::Closure(c) => f.debug_tuple("Closure").field(c).finish(),
            Function::Native(_) => f.write_str("Native"),
        }
    }
}

#[derive(Clone, Debug)]
pub enum Value {
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Array(Rc<RefCell<Vec<Value>>>),
    Object(Rc<RefCell<BTreeMap<String, Value>>>),
    Function(Rc<Function>),
    Builtin(Builtin),
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Value::Null, Value::Null) => true,
            (Value::Bool(a), Value::Bool(b)) => a == b,
            (Value::Number(a), Value::Number(b)) => a == b,
            (Value::String(a), Value::String(b)) => a == b,
            (Value::Array(a), Value::Array(b)) => Rc::ptr_eq(a, b) || *a.borrow() == *b.borrow(),
            (Value::Object(a), Value::Object(b)) => {
                Rc::ptr_eq(a, b) || *a.borrow() == *b.borrow()
            }
            (Value::Function(a), Value::Function(b)) => Rc::ptr_eq(a, b),
            (Value::Builtin(a), Value::Builtin(b)) => a == b,
            _ => false,
        }
    }
}

impl Value {
    pub fn array(items: Vec<Value>) -> Value {
        Value::Array(Rc::new(RefCell::new(items)))
    }

    pub fn object() -> Value {
        Value::Object(Rc::new(RefCell::new(BTreeMap::new())))
    }

    pub fn object_with(entries: Vec<(String, Value)>) -> Value {
        Value::Object(Rc::new(RefCell::new(entries.into_iter().collect())))
    }

    pub fn closure(params: Vec<String>, body: Rc<dyn Any>, env: Rc<dyn Any>) -> Value {
        Value::Function(Rc::new(Function::Closure(Rc::new(ClosureData {
            params,
            body,
            env,
        }))))
    }

    pub fn native(f: impl Fn(&[Value]) -> Result<Value, String> + 'static) -> Value {
        Value::Function(Rc::new(Function::Native(Rc::new(f))))
    }

    pub fn builtin(b: Builtin) -> Value {
        Value::Builtin(b)
    }

    /// Deep copy: allocates fresh aggregate containers so that mutating the
    /// result never affects the source value (mirrors jsonc.Clone + importValue).
    /// Function values are shared by reference (their captured environment has
    /// reference semantics like arrays/objects).
    pub fn deep_clone(&self) -> Value {
        match self {
            Value::Null => Value::Null,
            Value::Bool(b) => Value::Bool(*b),
            Value::Number(n) => Value::Number(*n),
            Value::String(s) => Value::String(s.clone()),
            Value::Array(a) => Value::array(a.borrow().iter().map(|v| v.deep_clone()).collect()),
            Value::Object(o) => Value::object_with(
                o.borrow()
                    .iter()
                    .map(|(k, v)| (k.clone(), v.deep_clone()))
                    .collect(),
            ),
            Value::Function(f) => Value::Function(f.clone()),
            Value::Builtin(b) => Value::Builtin(*b),
        }
    }
}

/// Unicode "letter" classification (General Category L), mirroring Go's unicode.IsLetter.
pub fn is_letter(c: char) -> bool {
    matches!(
        get_general_category(c),
        UppercaseLetter | LowercaseLetter | TitlecaseLetter | ModifierLetter | OtherLetter
    )
}

/// Unicode "decimal digit" classification (category Nd), mirroring Go's unicode.IsDigit.
pub fn is_digit(c: char) -> bool {
    get_general_category(c) == DecimalNumber
}

/// Unicode "printable" classification, mirroring Go's unicode.IsPrint:
/// letters, marks, numbers, punctuation, and symbols. Everything else
/// (control, format, private-use, unassigned, surrogate, and all separators
/// including non-ASCII spaces and U+2028/U+2029) is non-printable.
pub fn is_print(c: char) -> bool {
    matches!(
        get_general_category(c),
        UppercaseLetter
            | LowercaseLetter
            | TitlecaseLetter
            | ModifierLetter
            | OtherLetter
            | NonspacingMark
            | SpacingMark
            | EnclosingMark
            | DecimalNumber
            | LetterNumber
            | OtherNumber
            | ConnectorPunctuation
            | DashPunctuation
            | OpenPunctuation
            | ClosePunctuation
            | InitialPunctuation
            | FinalPunctuation
            | OtherPunctuation
            | MathSymbol
            | CurrencySymbol
            | ModifierSymbol
            | OtherSymbol
    )
}
