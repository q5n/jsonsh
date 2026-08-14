use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use unicode_general_category::get_general_category;
use unicode_general_category::GeneralCategory::*;

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Array(Rc<RefCell<Vec<Value>>>),
    Object(Rc<RefCell<BTreeMap<String, Value>>>),
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

    /// Deep copy: allocates fresh aggregate containers so that mutating the
    /// result never affects the source value (mirrors jsonc.Clone + importValue).
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
