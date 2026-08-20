use jsonsh::lang;
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

fn run(code: &str, root: Value) -> (Value, Option<Value>) {
    lang::execute(code, root, 10000).unwrap()
}

fn execute(code: &str, root: Value) -> Result<(Value, Option<Value>), lang::Error> {
    lang::execute(code, root, 10000)
}

#[test]
fn to_fixed_rounds_half_up() {
    let (r, _) = run(
        r#"
        $.a = (2.35).toFixed(1);
        $.c = (1.25).toFixed(1);
        $.d = (-1.25).toFixed(1);
        $.e = (2.5).toFixed(0);
        $.f = (-2.5).toFixed(0);
        $.g = (12345.6789).toFixed(2);
        $.h = (12345.6789).toFixed(6);
        $.i = (2.34).toFixed(1);
        $.j = (-2.34).toFixed(1);
        $.k = (0.3).toFixed(1);
        $.l = (1.005).toFixed(2);
    "#,
        obj(vec![]),
    );
    let want = obj(vec![
        ("a", s("2.4")),
        ("c", s("1.3")),
        ("d", s("-1.2")),
        ("e", s("3")),
        ("f", s("-2")),
        ("g", s("12345.68")),
        ("h", s("12345.678900")),
        ("i", s("2.3")),
        ("j", s("-2.3")),
        ("k", s("0.3")),
        ("l", s("1.00")),
    ]);
    assert_eq!(r, want);
}

#[test]
fn parse_int_float_edge_cases() {
    let (r, _) = run(
        r#"
        $.a = parseInt("-0x10");
        $.b = parseInt("08");
        $.c = parseInt("10", 2);
        $.d = parseInt("0x10", 10);
        $.e = parseInt("  12px");
        $.f = parseInt("10", 37) != parseInt("x");
        $.g = parseInt("1e3");
        $.pf1 = parseFloat(".5");
        $.pf2 = parseFloat("1e");
        $.pf3 = parseFloat("+.5");
        $.pf4 = parseFloat("12 34");
        $.pf5 = parseFloat("");
        $.pf6 = parseFloat("-2.5x");
        $.pf7 = parseFloat("2.5e2");
    "#,
        obj(vec![]),
    );
    let want = obj(vec![
        ("a", num(-16.0)),
        ("b", num(8.0)),
        ("c", num(2.0)),
        ("d", num(0.0)),
        ("e", num(12.0)),
        ("f", Value::Bool(true)),
        ("g", num(1.0)),
        ("pf1", num(0.5)),
        ("pf2", num(1.0)),
        ("pf3", num(0.5)),
        ("pf4", num(12.0)),
        ("pf5", num(f64::NAN)),
        ("pf6", num(-2.5)),
        ("pf7", num(250.0)),
    ]);
    // NaN compares unequal, so compare everything except pf5.
    if let Value::Object(o) = &r {
        for (k, v) in [
            ("a", num(-16.0)),
            ("b", num(8.0)),
            ("c", num(2.0)),
            ("d", num(0.0)),
            ("e", num(12.0)),
            ("g", num(1.0)),
            ("pf1", num(0.5)),
            ("pf2", num(1.0)),
            ("pf3", num(0.5)),
            ("pf4", num(12.0)),
            ("pf6", num(-2.5)),
            ("pf7", num(250.0)),
        ] {
            assert_eq!(o.borrow().get(k), Some(&v), "key {}", k);
        }
        assert_eq!(o.borrow().get("f"), Some(&Value::Bool(true)));
        match o.borrow().get("pf5") {
            Some(Value::Number(n)) => assert!(n.is_nan()),
            _ => panic!("pf5 must be NaN"),
        }
    } else {
        panic!("root must be object");
    }
    let _ = want;
}

#[test]
fn math_edge_cases() {
    let (r, _) = run(
        r#"
        $.maxNone = Math.max() == parseFloat("-Infinity");
        $.minNone = Math.min() == parseFloat("Infinity");
        $.maxNan = Math.max(1, parseInt("x")) != parseInt("x");
        $.truncNeg = Math.trunc(-2.7);
        $.signNeg = Math.sign(-0.5);
        $.absNeg = Math.abs(-7);
        $.powFrac = Math.pow(4, 0.5);
        $.hypotNone = Math.hypot();
        $.roundNeg = Math.round(-2.5);
    "#,
        obj(vec![]),
    );
    let want = obj(vec![
        ("maxNone", Value::Bool(true)),
        ("minNone", Value::Bool(true)),
        ("maxNan", Value::Bool(true)),
        ("truncNeg", num(-2.0)),
        ("signNeg", num(-1.0)),
        ("absNeg", num(7.0)),
        ("powFrac", num(2.0)),
        ("hypotNone", num(0.0)),
        ("roundNeg", num(-2.0)),
    ]);
    assert_eq!(r, want);
}

#[test]
fn bitwise_edge_cases() {
    let (r, _) = run(
        r#"
        $.neg1Ushr0 = -1 >>> 0;
        $.shl31 = 1 << 31;
        $.shrSign = -2147483648 >> 31;
        $.nanOr0 = parseInt("x") | 0;
        $.xorSelf = 5 ^ 5;
        $.notZero = ~0;
        $.ushrZero = -1 >>> 0;
    "#,
        obj(vec![]),
    );
    let want = obj(vec![
        ("neg1Ushr0", num(4294967295.0)),
        ("shl31", num(-2147483648.0)),
        ("shrSign", num(-1.0)),
        ("nanOr0", num(0.0)),
        ("xorSelf", num(0.0)),
        ("notZero", num(-1.0)),
        ("ushrZero", num(4294967295.0)),
    ]);
    assert_eq!(r, want);
}

#[test]
fn string_edge_cases() {
    let (r, _) = run(
        r#"
        $.sliceNeg = "hello".slice(-2);
        $.sliceNegBoth = "hello".slice(1, -1);
        $.charAtOOB = "hi".charAt(5);
        $.charCodeOOB = "hi".charCodeAt(5) != "hi".charCodeAt(5);
        $.startsWithPos = "hello".startsWith("ell", 1);
        $.endsWithPos = "hello".endsWith("ell", 4);
        $.repeat0 = "ab".repeat(0);
        $.includesEmpty = "hello".includes("");
        $.includesPos = "hello".includes("h", 1);
        $.concatEmpty = "".concat("a", "b");
    "#,
        obj(vec![]),
    );
    let want = obj(vec![
        ("sliceNeg", s("lo")),
        ("sliceNegBoth", s("ell")),
        ("charAtOOB", s("")),
        ("charCodeOOB", Value::Bool(true)),
        ("startsWithPos", Value::Bool(true)),
        ("endsWithPos", Value::Bool(true)),
        ("repeat0", s("")),
        ("includesEmpty", Value::Bool(true)),
        ("includesPos", Value::Bool(false)),
        ("concatEmpty", s("ab")),
    ]);
    assert_eq!(r, want);
}

#[test]
fn array_edge_cases() {
    let (r, _) = run(
        r#"
        $.reduceNoInit = [1, 2, 3].reduce((a, b) => a + b);
        $.reduceInit = [1, 2, 3].reduce((a, b) => a + b, 10);
        $.sliceNeg = [1, 2, 3, 4].slice(-2);
        $.sliceNegBoth = [1, 2, 3, 4].slice(1, -1);
        $.includesFrom = [1, 2, 3].includes(2, 2);
        $.sortString = [10, 2, 9].sort();
        $.someFalse = [1, 2, 3].some(x => x > 5);
        $.everyTrue = [1, 2, 3].every(x => x > 0);
        $.findNone = [1, 2, 3].find(x => x > 5);
        $.mapIndex = [10, 20].map((x, i) => i);
        $.concatNested = [1].concat([2, [3]], 4);
    "#,
        obj(vec![]),
    );
    let want = obj(vec![
        ("reduceNoInit", num(6.0)),
        ("reduceInit", num(16.0)),
        ("sliceNeg", arr(vec![num(3.0), num(4.0)])),
        ("sliceNegBoth", arr(vec![num(2.0), num(3.0)])),
        ("includesFrom", Value::Bool(false)),
        ("sortString", arr(vec![num(10.0), num(2.0), num(9.0)])),
        ("someFalse", Value::Bool(false)),
        ("everyTrue", Value::Bool(true)),
        ("findNone", Value::Null),
        ("mapIndex", arr(vec![num(0.0), num(1.0)])),
        (
            "concatNested",
            arr(vec![
                num(1.0),
                num(2.0),
                arr(vec![num(3.0)]),
                num(4.0),
            ]),
        ),
    ]);
    assert_eq!(r, want);
}

#[test]
fn date_edge_cases() {
    let (r, _) = run(
        r#"
        $.rolloverDate = new Date(2019, 1, 29).getDate();
        $.rolloverMonth = new Date(2019, 1, 29).getMonth();
        $.leap = new Date(2020, 1, 29).getDate();
        $.negTime = new Date(-1).getTime();
        $.negDate = new Date(-86400000).getUTCDate();
        $.daySunday = new Date(2024, 0, 7).getDay();
        $.dateOnlyYear = new Date(Date.parse("2020-01-02")).getFullYear();
        $.dateOnlyMonth = new Date(Date.parse("2020-01-02")).getMonth();
        $.dateOnlyDay = new Date(Date.parse("2020-01-02")).getDate();
        $.dateOnlyHours = new Date(Date.parse("2020-01-02")).getHours();
    "#,
        obj(vec![]),
    );
    let want = obj(vec![
        ("rolloverDate", num(1.0)),
        ("rolloverMonth", num(2.0)),
        ("leap", num(29.0)),
        ("negTime", num(-1.0)),
        ("negDate", num(31.0)),
        ("daySunday", num(0.0)),
        ("dateOnlyYear", num(2020.0)),
        ("dateOnlyMonth", num(0.0)),
        ("dateOnlyDay", num(2.0)),
        ("dateOnlyHours", num(0.0)),
    ]);
    assert_eq!(r, want);
}

#[test]
fn uri_edge_cases() {
    let (r, _) = run(
        r#"
        $.dUri1 = decodeURI("%26");
        $.dUri2 = decodeURIComponent("%26");
        $.surrogate = encodeURIComponent("😀");
        $.keepUnreserved = encodeURIComponent("AZaz09-_.!~*'()");
    "#,
        obj(vec![]),
    );
    let want = obj(vec![
        ("dUri1", s("%26")),
        ("dUri2", s("&")),
        ("surrogate", s("%F0%9F%98%80")),
        ("keepUnreserved", s("AZaz09-_.!~*'()")),
    ]);
    assert_eq!(r, want);
}

#[test]
fn optional_chaining_null_short_circuit() {
    let (r, _) = run(
        r#"
        $.a = {b: null};
        $.v1 = $.a.b?.c;
    "#,
        obj(vec![]),
    );
    // $.a.b?.c : b is null, so short circuit -> null
    let o = match &r {
        Value::Object(o) => o.borrow(),
        _ => panic!("root must be object"),
    };
    assert_eq!(o.get("v1"), Some(&Value::Null));
}

#[test]
fn optional_chaining_intermediate_null_throws() {
    let res = execute("$.a = {b: null}; $.x = $.a?.b.c;", obj(vec![]));
    assert!(res.is_err(), "expected error for a?.b.c with a.b == null");
}
