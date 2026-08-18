use crate::value::{is_print, Value};

/// Marshal encodes v as compact standard JSON, mirroring jsonc.Marshal.
/// Object keys are emitted in sorted (lexicographic) order. Strings escape
/// non-printable runes as \uXXXX while keeping printable text as raw UTF-8.
pub fn marshal(v: &Value) -> Result<String, String> {
    let mut dst = String::new();
    append_json(&mut dst, v)?;
    Ok(dst)
}

fn append_json(dst: &mut String, v: &Value) -> Result<(), String> {
    match v {
        Value::Null => dst.push_str("null"),
        Value::Bool(b) => dst.push_str(if *b { "true" } else { "false" }),
        Value::String(s) => append_json_string(dst, s),
        Value::Number(n) => {
            if n.is_nan() || n.is_infinite() {
                dst.push_str("null");
            } else {
                dst.push_str(&format_float(*n));
            }
        }
        Value::Array(a) => {
            dst.push('[');
            for (i, item) in a.borrow().iter().enumerate() {
                if i > 0 {
                    dst.push(',');
                }
                append_json(dst, item)?;
            }
            dst.push(']');
        }
        Value::Object(o) => {
            dst.push('{');
            for (i, (k, item)) in o.borrow().iter().enumerate() {
                if i > 0 {
                    dst.push(',');
                }
                append_json_string(dst, k);
                dst.push(':');
                append_json(dst, item)?;
            }
            dst.push('}');
        }
        Value::Function(_) | Value::Builtin(_) | Value::Constructor(_) | Value::Regex(_) => {
            dst.push_str("null")
        }
        Value::Date(ms) => {
            if ms.is_nan() {
                dst.push_str("null");
            } else {
                append_json_string(dst, &crate::date::to_iso_string(*ms));
            }
        }
    }
    Ok(())
}

pub fn append_json_string(dst: &mut String, s: &str) {
    dst.push('"');
    for c in s.chars() {
        match c {
            '"' => dst.push_str("\\\""),
            '\\' => dst.push_str("\\\\"),
            '\n' => dst.push_str("\\n"),
            '\r' => dst.push_str("\\r"),
            '\t' => dst.push_str("\\t"),
            c if (c as u32) < 0x20 => append_u4(dst, c as u32),
            '<' | '>' | '&' => append_u4(dst, c as u32),
            '\u{2028}' => dst.push_str("\\u2028"),
            '\u{2029}' => dst.push_str("\\u2029"),
            c if (c as u32) >= 0x80 && !is_print(c) => append_escaped_rune(dst, c),
            c => dst.push(c),
        }
    }
    dst.push('"');
}

fn append_escaped_rune(dst: &mut String, c: char) {
    let n = c as u32;
    if n < 0x10000 {
        append_u4(dst, n);
    } else {
        let v = n - 0x10000;
        append_u4(dst, 0xD800 + (v >> 10));
        append_u4(dst, 0xDC00 + (v & 0x3FF));
    }
}

fn append_u4(dst: &mut String, code: u32) {
    dst.push_str("\\u");
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut buf = [b'0'; 4];
    buf[0] = HEX[((code >> 12) & 0xF) as usize];
    buf[1] = HEX[((code >> 8) & 0xF) as usize];
    buf[2] = HEX[((code >> 4) & 0xF) as usize];
    buf[3] = HEX[(code & 0xF) as usize];
    dst.push_str(std::str::from_utf8(&buf).unwrap());
}

/// format_float mirrors jsonc's appendFloat (which mirrors encoding/json):
/// fixed notation by default, switching to scientific notation for
/// |f| < 1e-6 or |f| >= 1e21. Uses Rust's shortest round-trip formatting.
pub fn format_float(f: f64) -> String {
    let abs = f.abs();
    if abs != 0.0 && (abs < 1e-6 || abs >= 1e21) {
        let s = format!("{:e}", f);
        let (mantissa, exp) = s.split_once('e').unwrap();
        let e: i32 = exp.parse().unwrap();
        if e >= 0 {
            format!("{}e+{}", mantissa, e)
        } else {
            format!("{}e{}", mantissa, e)
        }
    } else {
        format!("{}", f)
    }
}
