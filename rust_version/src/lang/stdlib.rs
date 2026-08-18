use std::sync::atomic::{AtomicU64, Ordering};

use crate::date;
use crate::value::{Constructor, Value};

pub fn global_functions() -> Vec<(&'static str, Value)> {
    vec![
        ("parseInt", Value::native(parse_int)),
        ("parseFloat", Value::native(parse_float)),
        ("encodeURI", Value::native(encode_uri)),
        ("decodeURI", Value::native(decode_uri)),
        ("encodeURIComponent", Value::native(encode_uri_component)),
        ("decodeURIComponent", Value::native(decode_uri_component)),
    ]
}

pub fn constructors() -> Vec<(&'static str, Constructor)> {
    vec![
        ("Object", Constructor::Object),
        ("Array", Constructor::Array),
        ("String", Constructor::String),
        ("Number", Constructor::Number),
        ("Boolean", Constructor::Boolean),
        ("Date", Constructor::Date),
    ]
}

pub fn math_object() -> Value {
    let f = |name: &'static str, fun: fn(f64) -> f64| Value::native(unary(name, fun));
    let b = |name: &'static str, fun: fn(f64, f64) -> f64| Value::native(binary(name, fun));
    Value::object_with(vec![
        ("PI".to_string(), Value::Number(std::f64::consts::PI)),
        ("E".to_string(), Value::Number(std::f64::consts::E)),
        ("LN2".to_string(), Value::Number(std::f64::consts::LN_2)),
        ("LN10".to_string(), Value::Number(std::f64::consts::LN_10)),
        ("LOG2E".to_string(), Value::Number(std::f64::consts::LOG2_E)),
        ("LOG10E".to_string(), Value::Number(std::f64::consts::LOG10_E)),
        ("SQRT2".to_string(), Value::Number(std::f64::consts::SQRT_2)),
        ("SQRT1_2".to_string(), Value::Number(std::f64::consts::FRAC_1_SQRT_2)),
        ("abs".to_string(), f("Math.abs", f64::abs)),
        ("floor".to_string(), f("Math.floor", f64::floor)),
        ("ceil".to_string(), f("Math.ceil", f64::ceil)),
        ("round".to_string(), f("Math.round", math_round)),
        ("trunc".to_string(), f("Math.trunc", f64::trunc)),
        ("sign".to_string(), f("Math.sign", math_sign)),
        ("sqrt".to_string(), f("Math.sqrt", f64::sqrt)),
        ("cbrt".to_string(), f("Math.cbrt", f64::cbrt)),
        ("exp".to_string(), f("Math.exp", f64::exp)),
        ("log".to_string(), f("Math.log", f64::ln)),
        ("log2".to_string(), f("Math.log2", f64::log2)),
        ("log10".to_string(), f("Math.log10", f64::log10)),
        ("sin".to_string(), f("Math.sin", f64::sin)),
        ("cos".to_string(), f("Math.cos", f64::cos)),
        ("tan".to_string(), f("Math.tan", f64::tan)),
        ("asin".to_string(), f("Math.asin", f64::asin)),
        ("acos".to_string(), f("Math.acos", f64::acos)),
        ("atan".to_string(), f("Math.atan", f64::atan)),
        ("pow".to_string(), b("Math.pow", f64::powf)),
        ("atan2".to_string(), b("Math.atan2", f64::atan2)),
        ("hypot".to_string(), Value::native(math_hypot)),
        ("max".to_string(), Value::native(math_max)),
        ("min".to_string(), Value::native(math_min)),
        ("random".to_string(), Value::native(math_random)),
    ])
}

fn unary(name: &'static str, fun: fn(f64) -> f64) -> impl Fn(&[Value]) -> Result<Value, String> {
    move |args: &[Value]| {
        require_args(name, args, 1, 1)?;
        Ok(Value::Number(fun(to_number(Some(&args[0])))))
    }
}

fn binary(name: &'static str, fun: fn(f64, f64) -> f64) -> impl Fn(&[Value]) -> Result<Value, String> {
    move |args: &[Value]| {
        require_args(name, args, 2, 2)?;
        Ok(Value::Number(fun(
            to_number(Some(&args[0])),
            to_number(Some(&args[1])),
        )))
    }
}

fn math_round(n: f64) -> f64 {
    if n.is_nan() || n.is_infinite() || n == 0.0 {
        return n;
    }
    (n + 0.5).floor()
}

fn math_hypot(args: &[Value]) -> Result<Value, String> {
    let mut sum = 0.0f64;
    for a in args {
        let n = to_number(Some(a));
        sum += n * n;
    }
    Ok(Value::Number(sum.sqrt()))
}

fn math_sign(n: f64) -> f64 {
    if n.is_nan() {
        return f64::NAN;
    }
    if n > 0.0 {
        return 1.0;
    }
    if n < 0.0 {
        return -1.0;
    }
    n
}

fn math_max(args: &[Value]) -> Result<Value, String> {
    let mut m = f64::NEG_INFINITY;
    for a in args {
        let n = to_number(Some(a));
        if n.is_nan() {
            return Ok(Value::Number(f64::NAN));
        }
        if n > m {
            m = n;
        }
    }
    Ok(Value::Number(m))
}

fn math_min(args: &[Value]) -> Result<Value, String> {
    let mut m = f64::INFINITY;
    for a in args {
        let n = to_number(Some(a));
        if n.is_nan() {
            return Ok(Value::Number(f64::NAN));
        }
        if n < m {
            m = n;
        }
    }
    Ok(Value::Number(m))
}

fn math_random(_args: &[Value]) -> Result<Value, String> {
    Ok(Value::Number(random_f64()))
}

static RAND_STATE: AtomicU64 = AtomicU64::new(0);

fn random_f64() -> f64 {
    let mut x = RAND_STATE.load(Ordering::Relaxed);
    if x == 0 {
        x = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0x9E37_79B9_7F4A_7C15)
            | 1;
    }
    x ^= x >> 12;
    x ^= x << 25;
    x ^= x >> 27;
    RAND_STATE.store(x, Ordering::Relaxed);
    let v = x.wrapping_mul(0x2545_F491_4F6C_DD1D);
    ((v >> 11) as f64) * (1.0 / (1u64 << 53) as f64)
}

pub fn construct(c: Constructor, args: &[Value]) -> Result<Value, String> {
    match c {
        Constructor::Object => object_ctor(args),
        Constructor::Array => array_ctor(args),
        Constructor::String => string_ctor(args),
        Constructor::Number => number_ctor(args),
        Constructor::Boolean => boolean_ctor(args),
        Constructor::Date => date_ctor(args),
    }
}

pub fn static_method(c: Constructor, name: &str) -> Option<Value> {
    match (c, name) {
        (Constructor::Object, "keys") => Some(Value::native(object_keys)),
        (Constructor::Object, "values") => Some(Value::native(object_values)),
        (Constructor::Object, "entries") => Some(Value::native(object_entries)),
        (Constructor::Object, "assign") => Some(Value::native(object_assign)),
        (Constructor::Array, "isArray") => Some(Value::native(array_is_array)),
        (Constructor::String, "fromCharCode") => Some(Value::native(string_from_char_code)),
        (Constructor::Number, "isInteger") => Some(Value::native(number_is_integer)),
        (Constructor::Number, "isNaN") => Some(Value::native(number_is_nan)),
        (Constructor::Number, "isFinite") => Some(Value::native(number_is_finite)),
        (Constructor::Number, "parseInt") => Some(Value::native(parse_int)),
        (Constructor::Number, "parseFloat") => Some(Value::native(parse_float)),
        (Constructor::Date, "now") => Some(Value::native(date_now)),
        (Constructor::Date, "parse") => Some(Value::native(date_parse)),
        (Constructor::Date, "UTC") => Some(Value::native(date_utc)),
        _ => None,
    }
}

fn require_args(name: &str, args: &[Value], min: usize, max: usize) -> Result<(), String> {
    if args.len() < min || args.len() > max {
        if min == max {
            return Err(format!("{} expects {} argument(s)", name, min));
        }
        return Err(format!("{} expects {} to {} arguments", name, min, max));
    }
    Ok(())
}

fn object_keys(args: &[Value]) -> Result<Value, String> {
    require_args("Object.keys", args, 1, 1)?;
    match &args[0] {
        Value::Object(o) => {
            let items = o.borrow().keys().cloned().map(Value::String).collect();
            Ok(Value::array(items))
        }
        Value::Array(a) => {
            let items = (0..a.borrow().len()).map(|i| Value::Number(i as f64)).collect();
            Ok(Value::array(items))
        }
        _ => Err("Object.keys requires an object or array".to_string()),
    }
}

fn object_values(args: &[Value]) -> Result<Value, String> {
    require_args("Object.values", args, 1, 1)?;
    match &args[0] {
        Value::Object(o) => {
            let items = o.borrow().values().map(|v| v.deep_clone()).collect();
            Ok(Value::array(items))
        }
        Value::Array(a) => {
            let items = a.borrow().iter().map(|v| v.deep_clone()).collect();
            Ok(Value::array(items))
        }
        _ => Err("Object.values requires an object or array".to_string()),
    }
}

fn object_entries(args: &[Value]) -> Result<Value, String> {
    require_args("Object.entries", args, 1, 1)?;
    match &args[0] {
        Value::Object(o) => {
            let items = o
                .borrow()
                .iter()
                .map(|(k, v)| Value::array(vec![Value::String(k.clone()), v.deep_clone()]))
                .collect();
            Ok(Value::array(items))
        }
        Value::Array(a) => {
            let items = a
                .borrow()
                .iter()
                .enumerate()
                .map(|(i, v)| Value::array(vec![Value::Number(i as f64), v.deep_clone()]))
                .collect();
            Ok(Value::array(items))
        }
        _ => Err("Object.entries requires an object or array".to_string()),
    }
}

fn object_assign(args: &[Value]) -> Result<Value, String> {
    require_args("Object.assign", args, 1, usize::MAX)?;
    let target = match &args[0] {
        Value::Object(o) => o.clone(),
        _ => return Err("Object.assign target must be an object".to_string()),
    };
    for src in &args[1..] {
        if let Value::Object(o) = src {
            for (k, v) in o.borrow().iter() {
                target.borrow_mut().insert(k.clone(), v.clone());
            }
        }
    }
    Ok(Value::Object(target))
}

fn array_is_array(args: &[Value]) -> Result<Value, String> {
    require_args("Array.isArray", args, 1, 1)?;
    Ok(Value::Bool(matches!(args[0], Value::Array(_))))
}

fn string_from_char_code(args: &[Value]) -> Result<Value, String> {
    let mut out = String::new();
    for a in args {
        let code = match a {
            Value::Number(n) => *n as u32,
            _ => return Err("String.fromCharCode requires numeric arguments".to_string()),
        };
        match char::from_u32(code) {
            Some(c) => out.push(c),
            None => return Err("invalid character code".to_string()),
        }
    }
    Ok(Value::String(out))
}

fn number_is_integer(args: &[Value]) -> Result<Value, String> {
    require_args("Number.isInteger", args, 1, 1)?;
    Ok(Value::Bool(match &args[0] {
        Value::Number(n) => n.fract() == 0.0 && !n.is_infinite(),
        _ => false,
    }))
}

fn number_is_nan(args: &[Value]) -> Result<Value, String> {
    require_args("Number.isNaN", args, 1, 1)?;
    Ok(Value::Bool(matches!(&args[0], Value::Number(n) if n.is_nan())))
}

fn number_is_finite(args: &[Value]) -> Result<Value, String> {
    require_args("Number.isFinite", args, 1, 1)?;
    Ok(Value::Bool(matches!(&args[0], Value::Number(n) if n.is_finite())))
}

fn object_ctor(args: &[Value]) -> Result<Value, String> {
    match args.first() {
        None | Some(Value::Null) => Ok(Value::object()),
        Some(Value::Object(_)) | Some(Value::Array(_)) => Ok(args[0].deep_clone()),
        Some(_) => Ok(Value::object()),
    }
}

fn array_ctor(args: &[Value]) -> Result<Value, String> {
    match args {
        [] => Ok(Value::array(vec![])),
        [Value::Number(n)] => {
            let len = n.trunc() as i64;
            if len < 0 || n.fract() != 0.0 {
                return Err("invalid array length".to_string());
            }
            Ok(Value::array(vec![Value::Null; len as usize]))
        }
        _ => Ok(Value::array(args.to_vec())),
    }
}

fn string_ctor(args: &[Value]) -> Result<Value, String> {
    match args.first() {
        None => Ok(Value::String(String::new())),
        Some(v) => Ok(Value::String(to_str(v))),
    }
}

fn number_ctor(args: &[Value]) -> Result<Value, String> {
    Ok(Value::Number(to_number(args.first())))
}

fn boolean_ctor(args: &[Value]) -> Result<Value, String> {
    Ok(Value::Bool(truthy(args.first())))
}

fn date_ctor(args: &[Value]) -> Result<Value, String> {
    match args {
        [] => Ok(Value::date(date::now_ms())),
        [Value::Number(n)] => Ok(Value::date(*n)),
        [Value::String(s)] => Ok(Value::date(date::parse_iso_date(s).unwrap_or(f64::NAN))),
        [Value::Null] => Ok(Value::date(0.0)),
        _ => {
            let nums: Vec<f64> = args.iter().map(|a| to_number(Some(a))).collect();
            let ms = date::make_date_ms(
                nums[0].trunc() as i64,
                nums.get(1).map_or(0.0, |n| n.trunc()) as i64,
                nums.get(2).map_or(1.0, |n| n.trunc()) as i64,
                nums.get(3).map_or(0.0, |n| n.trunc()) as i64,
                nums.get(4).map_or(0.0, |n| n.trunc()) as i64,
                nums.get(5).map_or(0.0, |n| n.trunc()) as i64,
                nums.get(6).map_or(0.0, |n| n.trunc()) as i64,
            );
            Ok(Value::date(ms))
        }
    }
}

fn date_now(args: &[Value]) -> Result<Value, String> {
    require_args("Date.now", args, 0, 0)?;
    Ok(Value::Number(date::now_ms()))
}

fn date_parse(args: &[Value]) -> Result<Value, String> {
    require_args("Date.parse", args, 1, 1)?;
    let s = match &args[0] {
        Value::String(s) => s,
        _ => return Err("Date.parse requires a string".to_string()),
    };
    Ok(Value::Number(date::parse_iso_date(s).unwrap_or(f64::NAN)))
}

fn date_utc(args: &[Value]) -> Result<Value, String> {
    if args.is_empty() {
        return Err("Date.UTC expects at least 1 argument".to_string());
    }
    let nums: Vec<f64> = args.iter().map(|a| to_number(Some(a))).collect();
    let ms = date::make_date_ms(
        nums[0].trunc() as i64,
        nums.get(1).map_or(0.0, |n| n.trunc()) as i64,
        nums.get(2).map_or(1.0, |n| n.trunc()) as i64,
        nums.get(3).map_or(0.0, |n| n.trunc()) as i64,
        nums.get(4).map_or(0.0, |n| n.trunc()) as i64,
        nums.get(5).map_or(0.0, |n| n.trunc()) as i64,
        nums.get(6).map_or(0.0, |n| n.trunc()) as i64,
    );
    Ok(Value::Number(ms))
}

fn parse_int(args: &[Value]) -> Result<Value, String> {
    let s = to_str(args.first().unwrap_or(&Value::Null));
    let radix = match args.get(1) {
        Some(v) => to_number(Some(v)).trunc() as i32,
        None => 0,
    };
    let s = s.trim();
    let (neg, rest) = match s.strip_prefix('-') {
        Some(r) => (true, r),
        None => match s.strip_prefix('+') {
            Some(r) => (false, r),
            None => (false, s),
        },
    };
    let (radix, digits) = if radix == 0 {
        if let Some(hex) = rest.strip_prefix("0x").or_else(|| rest.strip_prefix("0X")) {
            (16, hex)
        } else {
            (10, rest)
        }
    } else {
        (radix, rest)
    };
    if !(2..=36).contains(&radix) {
        return Ok(Value::Number(f64::NAN));
    }
    let mut val: f64 = 0.0;
    let mut any = false;
    for c in digits.chars() {
        match c.to_digit(36) {
            Some(d) if (d as i32) < radix => {
                val = val * radix as f64 + d as f64;
                any = true;
            }
            _ => break,
        }
    }
    if !any {
        return Ok(Value::Number(f64::NAN));
    }
    if neg {
        val = -val;
    }
    Ok(Value::Number(val))
}

fn parse_float(args: &[Value]) -> Result<Value, String> {
    let s = to_str(args.first().unwrap_or(&Value::Null));
    let s = s.trim();
    let lower = s.to_ascii_lowercase();
    if lower == "infinity" || lower == "+infinity" {
        return Ok(Value::Number(f64::INFINITY));
    }
    if lower == "-infinity" {
        return Ok(Value::Number(f64::NEG_INFINITY));
    }
    let bytes = s.as_bytes();
    let mut i = 0;
    if i < bytes.len() && (bytes[i] == b'+' || bytes[i] == b'-') {
        i += 1;
    }
    let digits_start = i;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        i += 1;
    }
    let mut any = i > digits_start;
    if i < bytes.len() && bytes[i] == b'.' {
        i += 1;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
            any = true;
        }
    }
    if i < bytes.len() && (bytes[i] == b'e' || bytes[i] == b'E') {
        let mut j = i + 1;
        if j < bytes.len() && (bytes[j] == b'+' || bytes[j] == b'-') {
            j += 1;
        }
        let exp_start = j;
        while j < bytes.len() && bytes[j].is_ascii_digit() {
            j += 1;
        }
        if j > exp_start {
            i = j;
        }
    }
    if !any {
        return Ok(Value::Number(f64::NAN));
    }
    let n: f64 = s[..i].parse().unwrap_or(f64::NAN);
    Ok(Value::Number(n))
}

fn encode_uri(args: &[Value]) -> Result<Value, String> {
    if args.len() != 1 {
        return Err("encodeURI expects 1 argument".to_string());
    }
    let s = match &args[0] {
        Value::String(s) => s,
        _ => return Err("encodeURI requires a string".to_string()),
    };
    Ok(Value::String(encode(true, s)))
}

fn decode_uri(args: &[Value]) -> Result<Value, String> {
    if args.len() != 1 {
        return Err("decodeURI expects 1 argument".to_string());
    }
    let s = match &args[0] {
        Value::String(s) => s,
        _ => return Err("decodeURI requires a string".to_string()),
    };
    Ok(Value::String(decode(true, s)?))
}

fn encode_uri_component(args: &[Value]) -> Result<Value, String> {
    if args.len() != 1 {
        return Err("encodeURIComponent expects 1 argument".to_string());
    }
    let s = match &args[0] {
        Value::String(s) => s,
        _ => return Err("encodeURIComponent requires a string".to_string()),
    };
    Ok(Value::String(encode(false, s)))
}

fn decode_uri_component(args: &[Value]) -> Result<Value, String> {
    if args.len() != 1 {
        return Err("decodeURIComponent expects 1 argument".to_string());
    }
    let s = match &args[0] {
        Value::String(s) => s,
        _ => return Err("decodeURIComponent requires a string".to_string()),
    };
    Ok(Value::String(decode(false, s)?))
}

fn encode(uri: bool, s: &str) -> String {
    let mut out = String::new();
    for &b in s.as_bytes() {
        if is_unreserved(b) || (uri && is_reserved(b)) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{:02X}", b));
        }
    }
    out
}

fn decode(uri: bool, s: &str) -> Result<String, String> {
    let bytes = s.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            if i + 2 >= bytes.len() {
                return Err("malformed URI sequence".to_string());
            }
            let hi = hex_val(bytes[i + 1]).ok_or_else(|| "malformed URI sequence".to_string())?;
            let lo = hex_val(bytes[i + 2]).ok_or_else(|| "malformed URI sequence".to_string())?;
            let b = (hi << 4) | lo;
            if uri && is_reserved(b) {
                out.extend_from_slice(&bytes[i..i + 3]);
            } else {
                out.push(b);
            }
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).map_err(|_| "malformed UTF-8 in URI sequence".to_string())
}

fn is_unreserved(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'!' | b'~' | b'*' | b'\'' | b'(' | b')')
}

fn is_reserved(b: u8) -> bool {
    matches!(b, b';' | b'/' | b'?' | b':' | b'@' | b'&' | b'=' | b'+' | b'$' | b',' | b'#')
}

fn hex_val(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

pub fn number_to_string(n: f64) -> String {
    if n.is_nan() {
        return "NaN".to_string();
    }
    if n.is_infinite() {
        return if n > 0.0 { "Infinity" } else { "-Infinity" }.to_string();
    }
    format!("{}", n)
}

/// ECMAScript `Number.prototype.toFixed`: fixed-point notation with the given
/// number of fractional digits, rounding half toward positive infinity. Uses
/// exponential notation for magnitudes >= 1e21 (like `toString`).
pub fn number_to_fixed(n: f64, digits: usize) -> String {
    if n.is_nan() {
        return "NaN".to_string();
    }
    if n.is_infinite() {
        return if n > 0.0 { "Infinity" } else { "-Infinity" }.to_string();
    }
    if n.abs() >= 1e21 {
        return exponential(n);
    }
    let factor = 10f64.powi(digits as i32);
    let rounded = (n * factor + 0.5).floor();
    let val = rounded / factor;
    format!("{:.*}", digits, val)
}

fn exponential(n: f64) -> String {
    let s = format!("{:e}", n);
    let (mantissa, exp) = s.split_once('e').unwrap();
    let e: i32 = exp.parse().unwrap();
    if e >= 0 {
        format!("{}e+{}", mantissa, e)
    } else {
        format!("{}e{}", mantissa, e)
    }
}

pub fn to_str(v: &Value) -> String {
    match v {
        Value::Null => "null".to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => number_to_string(*n),
        Value::String(s) => s.clone(),
        _ => String::new(),
    }
}

fn to_number(v: Option<&Value>) -> f64 {
    match v {
        None => 0.0,
        Some(Value::Number(n)) => *n,
        Some(Value::Bool(b)) => {
            if *b {
                1.0
            } else {
                0.0
            }
        }
        Some(Value::Null) => 0.0,
        Some(Value::String(s)) => {
            let t = s.trim();
            if t.is_empty() {
                return 0.0;
            }
            t.parse().unwrap_or(f64::NAN)
        }
        Some(_) => f64::NAN,
    }
}

pub fn truthy(v: Option<&Value>) -> bool {
    match v {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => !n.is_nan() && *n != 0.0,
        Some(Value::String(s)) => !s.is_empty(),
        Some(_) => true,
    }
}
