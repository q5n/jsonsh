use jsonsh::jsonc;
use jsonsh::value::Value;

fn obj(entries: Vec<(&str, Value)>) -> Value {
    Value::object_with(entries.into_iter().map(|(k, v)| (k.to_string(), v)).collect())
}

fn arr(items: Vec<Value>) -> Value {
    Value::array(items)
}

fn s(x: &str) -> Value {
    Value::String(x.to_string())
}

fn num(n: f64) -> Value {
    Value::Number(n)
}

fn get(v: &Value, key: &str) -> Value {
    match v {
        Value::Object(o) => o.borrow().get(key).unwrap().clone(),
        _ => panic!("not an object"),
    }
}

fn set_field(v: &Value, key: &str, val: Value) {
    if let Value::Object(o) = v {
        o.borrow_mut().insert(key.to_string(), val);
    }
}

#[test]
fn preserve_unchanged_exactly() {
    let src = "\u{FEFF}// header\r\n{\r\n\t\"n\" : 1e2, // number\r\n\t\"s\": \"a\\u0062\"\r\n}\r\n";
    let doc = jsonc::parse(src.to_string()).unwrap();
    let got = doc.preserve(&doc.root.value.deep_clone()).unwrap();
    assert_eq!(got, src);
}

#[test]
fn deleting_member_removes_its_inline_comment() {
    let doc = jsonc::parse("{\n  \"a\": 1 // owned\n}".to_string()).unwrap();
    let got = doc.preserve(&Value::object()).unwrap();
    assert!(!got.contains("owned"));
    jsonc::parse(got).unwrap();
}

#[test]
fn preserve_only_changed_scalar() {
    let src = "{\n    // 商品价格\n    \"price\" : 100,\n\n    \"name\": \"book\"\n}\n";
    let doc = jsonc::parse(src.to_string()).unwrap();
    let v = doc.root.value.deep_clone();
    set_field(&v, "price", num(80.0));
    let got = doc.preserve(&v).unwrap();
    let want = src.replace("100", "80");
    assert_eq!(got, want);
}

#[test]
fn add_and_delete_members_keep_style() {
    let src = "{\n  \"a\": 1,\n  // remove with b\n  \"b\": 2\n}\n";
    let doc = jsonc::parse(src.to_string()).unwrap();
    let v = doc.root.value.deep_clone();
    if let Value::Object(o) = &v {
        let mut m = o.borrow_mut();
        m.remove("b");
        m.insert("c".to_string(), num(3.0));
    }
    let got = doc.preserve(&v).unwrap();
    assert_eq!(got, "{\n  \"a\": 1,\n  \"c\": 3\n}\n");
}

#[test]
fn adding_to_empty_container_keeps_single_line_style() {
    let doc = jsonc::parse("{\"object\":{},\"array\":[ ]}".to_string()).unwrap();
    let v = doc.root.value.deep_clone();
    if let Value::Object(root) = &v {
        let mut m = root.borrow_mut();
        if let Some(Value::Object(o)) = m.get("object") {
            o.borrow_mut().insert("a".to_string(), num(1.0));
        }
        m.insert("array".to_string(), arr(vec![Value::Bool(true)]));
    }
    let got = doc.preserve(&v).unwrap();
    assert_eq!(got, "{\"object\":{\"a\": 1},\"array\":[ true ]}");
}

#[test]
fn array_deletion_reuses_remaining_nodes() {
    let src = "[\n  1,\n  // remove\n  2,\n  /* keep */ 3\n]";
    let doc = jsonc::parse(src.to_string()).unwrap();
    let v = arr(vec![num(1.0), num(3.0)]);
    let got = doc.preserve(&v).unwrap();
    assert!(!got.contains("remove"));
    assert!(got.contains("keep"));
    let reparsed = jsonc::parse(got).unwrap();
    assert_eq!(reparsed.root.value, v);
}

#[test]
fn pretty_preserves_comments() {
    let src = "{\"a\":1,// note\n\"empty\":{},\"items\":[true,false]}";
    let got = jsonc::pretty_preserve(src, "  ").unwrap();
    assert!(got.contains("// note"));
    assert!(got.contains("\n  \"items\": ["));
    jsonc::parse(got).unwrap();
}

#[test]
fn jsonc_errors() {
    for src in ["{\"a\":01}", "{/* broken", "{\"a\":1} extra", "{\"a\":1,\"a\":2}"] {
        assert!(jsonc::parse(src.to_string()).is_err(), "expected error for {:?}", src);
    }
}

#[test]
fn marshal_escapes_non_printable_runes() {
    let got = jsonc::marshal(&obj(vec![
        ("icon", s("\u{ee63}")),
        ("name", s("Ubuntu 24.04.1 LTS")),
        ("中文", s("保留")),
    ]))
    .unwrap();
    assert_eq!(got, "{\"icon\":\"\\uee63\",\"name\":\"Ubuntu 24.04.1 LTS\",\"中文\":\"保留\"}");
}

#[test]
fn marshal_escapes_control_format_and_private_use() {
    let cases: Vec<(String, &str)> = vec![
        ("\u{0001}".to_string(), "\"\\u0001\""),
        ("\u{2028}".to_string(), "\"\\u2028\""),
        ("\u{200e}".to_string(), "\"\\u200e\""),
        (char::from_u32(0xF0000).unwrap().to_string(), "\"\\udb80\\udc00\""),
        ("<>&".to_string(), "\"\\u003c\\u003e\\u0026\""),
    ];
    for (input, want) in cases {
        let got = jsonc::marshal(&Value::String(input)).unwrap();
        assert_eq!(got, want, "marshal failed for input");
    }
}

#[test]
fn jsonc_clone_and_get_helpers() {
    let v = obj(vec![("a", num(1.0)), ("b", arr(vec![num(2.0)]))]);
    assert_eq!(get(&v, "a"), num(1.0));
    let c = v.deep_clone();
    assert_eq!(c, v);
}
