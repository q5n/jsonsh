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

fn get_num(v: &Value, key: &str) -> f64 {
    match v {
        Value::Object(o) => match o.borrow().get(key) {
            Some(Value::Number(n)) => *n,
            _ => panic!("missing number key {}", key),
        },
        _ => panic!("not an object"),
    }
}

fn get_str(v: &Value, key: &str) -> String {
    match v {
        Value::Object(o) => match o.borrow().get(key) {
            Some(Value::String(s)) => s.clone(),
            _ => panic!("missing string key {}", key),
        },
        _ => panic!("not an object"),
    }
}

#[test]
fn global_parse_int() {
    let (r, _) = run(
        r#"
        $.dec = parseInt("42");
        $.hex = parseInt("0x1A");
        $.bin = parseInt("101", 2);
        $.ws = parseInt("   7abc");
        $.neg = parseInt("-8");
        $.nan = parseInt("x") != parseInt("x");
        if (parseInt("x")) { $.falsy = 1; } else { $.falsy = 2; }
    "#,
        obj(vec![]),
    );
    let want = obj(vec![
        ("dec", num(42.0)),
        ("hex", num(26.0)),
        ("bin", num(5.0)),
        ("ws", num(7.0)),
        ("neg", num(-8.0)),
        ("nan", Value::Bool(true)),
        ("falsy", num(2.0)),
    ]);
    assert_eq!(r, want);
}

#[test]
fn global_parse_float() {
    let (r, _) = run(
        r#"
        $.a = parseFloat("3.25abc");
        $.b = parseFloat("  2.5e2x");
        $.c = parseFloat("-0.5");
        $.nan = parseFloat("xyz") != parseFloat("xyz");
    "#,
        obj(vec![]),
    );
    let want = obj(vec![
        ("a", num(3.25)),
        ("b", num(250.0)),
        ("c", num(-0.5)),
        ("nan", Value::Bool(true)),
    ]);
    assert_eq!(r, want);
}

#[test]
fn global_uri_encoding() {
    let (r, _) = run(
        r#"
        $.c1 = encodeURIComponent("a b&c");
        $.c2 = encodeURIComponent(":/?#[]@!$&'()*+,;=");
        $.u1 = encodeURI("a b&c");
        $.u2 = encodeURI("a/b c");
        $.d1 = decodeURIComponent("a%20b%26c");
        $.d2 = decodeURI("a%20b&c");
        $.d3 = decodeURIComponent("%E4%B8%AD");
    "#,
        obj(vec![]),
    );
    let want = obj(vec![
        ("c1", s("a%20b%26c")),
        (
            "c2",
            s("%3A%2F%3F%23%5B%5D%40!%24%26'()*%2B%2C%3B%3D"),
        ),
        ("u1", s("a%20b&c")),
        ("u2", s("a/b%20c")),
        ("d1", s("a b&c")),
        ("d2", s("a b&c")),
        ("d3", s("中")),
    ]);
    assert_eq!(r, want);
}

#[test]
fn static_object_methods() {
    let (r, _) = run(
        r#"
        $.keys = Object.keys({b: 1, a: 2});
        $.values = Object.values({b: 1, a: 2});
        $.entries = Object.entries({b: 1, a: 2});
        $.assigned = Object.assign({a: 1}, {b: 2});
    "#,
        obj(vec![]),
    );
    let want = obj(vec![
        ("keys", arr(vec![s("a"), s("b")])),
        ("values", arr(vec![num(2.0), num(1.0)])),
        (
            "entries",
            arr(vec![
                arr(vec![s("a"), num(2.0)]),
                arr(vec![s("b"), num(1.0)]),
            ]),
        ),
        ("assigned", obj(vec![("a", num(1.0)), ("b", num(2.0))])),
    ]);
    assert_eq!(r, want);
}

#[test]
fn static_other_methods() {
    let (r, _) = run(
        r#"
        $.isArr1 = Array.isArray([1]);
        $.isArr2 = Array.isArray({});
        $.chr = String.fromCharCode(72, 105);
        $.int1 = Number.isInteger(3);
        $.int2 = Number.isInteger(3.5);
        $.nan1 = Number.isNaN(parseInt("x"));
        $.nan2 = Number.isNaN(3);
        $.fin1 = Number.isFinite(3);
        $.fin2 = Number.isFinite(parseFloat("Infinity"));
    "#,
        obj(vec![]),
    );
    let want = obj(vec![
        ("isArr1", Value::Bool(true)),
        ("isArr2", Value::Bool(false)),
        ("chr", s("Hi")),
        ("int1", Value::Bool(true)),
        ("int2", Value::Bool(false)),
        ("nan1", Value::Bool(true)),
        ("nan2", Value::Bool(false)),
        ("fin1", Value::Bool(true)),
        ("fin2", Value::Bool(false)),
    ]);
    assert_eq!(r, want);
}

#[test]
fn number_instance_methods() {
    let (r, _) = run(
        r#"
        $.fixed = (2.567).toFixed(2);
        $.hex = (255).toString(16);
        $.bin = (3).toString(2);
        $.plain = (3).toString();
        $.val = (5).valueOf();
    "#,
        obj(vec![]),
    );
    let want = obj(vec![
        ("fixed", s("2.57")),
        ("hex", s("ff")),
        ("bin", s("11")),
        ("plain", s("3")),
        ("val", num(5.0)),
    ]);
    assert_eq!(r, want);
}

#[test]
fn string_instance_methods() {
    let (r, _) = run(
        r#"
        $.at = "hello".charAt(1);
        $.code = "hello".charCodeAt(1);
        $.cat = "foo".concat("bar", "baz");
        $.inc1 = "hello world".includes("world");
        $.inc2 = "hello world".includes("xyz");
        $.start = "hello".startsWith("he");
        $.end = "hello".endsWith("lo");
        $.slice = "hello".slice(1, 3);
        $.sliceNeg = "hello".slice(-3);
        $.rep = "ab".repeat(3);
    "#,
        obj(vec![]),
    );
    let want = obj(vec![
        ("at", s("e")),
        ("code", num(101.0)),
        ("cat", s("foobarbaz")),
        ("inc1", Value::Bool(true)),
        ("inc2", Value::Bool(false)),
        ("start", Value::Bool(true)),
        ("end", Value::Bool(true)),
        ("slice", s("el")),
        ("sliceNeg", s("llo")),
        ("rep", s("ababab")),
    ]);
    assert_eq!(r, want);
}

#[test]
fn array_instance_methods() {
    let (r, _) = run(
        r#"
        $.concat = [1, 2].concat([3], 4);
        $.slice = [1, 2, 3, 4].slice(1, 3);
        $.inc1 = [1, 2, 3].includes(2);
        $.inc2 = [1, 2, 3].includes(5);
        $.mapped = [1, 2, 3].map(x => x * 2);
        $.filtered = [1, 2, 3, 4].filter(x => x > 2);
        $.reduced = [1, 2, 3].reduce((a, b) => a + b, 0);
        $.found = [1, 2, 3].find(x => x > 1);
        $.some = [1, 2, 3].some(x => x > 2);
        $.every = [1, 2, 3].every(x => x > 0);
        $.sorted = [3, 1, 2].sort();
    "#,
        obj(vec![]),
    );
    let want = obj(vec![
        ("concat", arr(vec![num(1.0), num(2.0), num(3.0), num(4.0)])),
        ("slice", arr(vec![num(2.0), num(3.0)])),
        ("inc1", Value::Bool(true)),
        ("inc2", Value::Bool(false)),
        ("mapped", arr(vec![num(2.0), num(4.0), num(6.0)])),
        ("filtered", arr(vec![num(3.0), num(4.0)])),
        ("reduced", num(6.0)),
        ("found", num(2.0)),
        ("some", Value::Bool(true)),
        ("every", Value::Bool(true)),
        ("sorted", arr(vec![num(1.0), num(2.0), num(3.0)])),
    ]);
    assert_eq!(r, want);
}

#[test]
fn object_and_boolean_instance_methods() {
    let (r, _) = run(
        r#"
        $.o = {a: 1};
        $.has1 = $.o.hasOwnProperty("a");
        $.has2 = $.o.hasOwnProperty("b");
        $.b = true.valueOf();
    "#,
        obj(vec![]),
    );
    let want = obj(vec![
        ("o", obj(vec![("a", num(1.0))])),
        ("has1", Value::Bool(true)),
        ("has2", Value::Bool(false)),
        ("b", Value::Bool(true)),
    ]);
    assert_eq!(r, want);
}

#[test]
fn date_constructor_and_methods() {
    let (r, _) = run(
        r#"
        $.ms = new Date(0).getTime();
        $.iso = new Date(0).toISOString();
        $.year = new Date(0).getUTCFullYear();
        $.month = new Date(0).getUTCMonth();
        $.date = new Date(0).getUTCDate();
        $.day = new Date(0).getUTCDay();
        $.hours = new Date(0).getUTCHours();
        $.parsed = Date.parse("1970-01-01T00:00:00.000Z");
        $.utc = Date.UTC(1970, 0, 1);
        $.constructed = new Date(2020, 0, 2).getDate();
        $.fromStr = new Date("2020-01-02T00:00:00.000Z").getTime();
        $.val = new Date(0).valueOf();
        $.nowPositive = Date.now() > 0;
        $.nowNum = typeof Date.now();
        $.type = typeof new Date(0);
        $.json = {d: new Date(0)}.toString();
        $.tzOffset = new Date(0).getTimezoneOffset();
        $.localHours = new Date(0).getHours();
        $.localIso = new Date(0).toString();
        $.utcFromIso = new Date(0).toISOString();
        $.roundTrip = new Date($.localIso).getTime();
    "#,
        obj(vec![]),
    );
    let tz_offset = get_num(&r, "tzOffset");
    let local_hours = get_num(&r, "localHours");
    let local_iso = get_str(&r, "localIso").to_string();
    let want = obj(vec![
        ("ms", num(0.0)),
        ("iso", s("1970-01-01T00:00:00.000Z")),
        ("year", num(1970.0)),
        ("month", num(0.0)),
        ("date", num(1.0)),
        ("day", num(4.0)),
        ("hours", num(0.0)),
        ("parsed", num(0.0)),
        ("utc", num(0.0)),
        ("constructed", num(2.0)),
        ("fromStr", num(1577923200000.0)),
        ("val", num(0.0)),
        ("nowPositive", Value::Bool(true)),
        ("nowNum", s("number")),
        ("type", s("object")),
        ("json", s(&format!("{{\"d\":\"{}\"}}", local_iso))),
        ("tzOffset", num(tz_offset)),
        ("localHours", num(local_hours)),
        ("localIso", s(&local_iso)),
        ("utcFromIso", s("1970-01-01T00:00:00.000Z")),
        ("roundTrip", num(0.0)),
    ]);
    assert_eq!(r, want);
    // local time = UTC - offset (in minutes); the floor gives the hour
    let total_local_min = 0 - tz_offset as i64;
    let expected_hour = total_local_min.rem_euclid(24 * 60) / 60;
    assert_eq!(local_hours as i64, expected_hour);
    // toString carries a ±HH:mm suffix matching getTimezoneOffset
    assert!(local_iso.len() >= 6);
    let off_suffix = &local_iso[23..];
    assert!(off_suffix.starts_with('+') || off_suffix.starts_with('-'));
    assert_eq!(off_suffix.len(), 6);
    assert_eq!(off_suffix.as_bytes()[3], b':');
    let expected_suffix = {
        let sign = if tz_offset <= 0.0 { '+' } else { '-' };
        let mins = tz_offset.abs() as i64;
        format!("{}{:02}:{:02}", sign, mins / 60, mins % 60)
    };
    assert_eq!(off_suffix, expected_suffix);
}

#[test]
fn date_timezone_offsets_and_local_parse() {
    // Explicit offset parsing: 20:00 +08:00 == 12:00 UTC, independent of host TZ.
    let (r, _) = run(
        r#"
        $.fromOffset = Date.parse("2026-08-20T20:00:00.123+08:00");
        $.expectedUtc = Date.UTC(2026, 7, 20, 12, 0, 0, 123);
        $.uYear = new Date($.fromOffset).getUTCFullYear();
        $.uMonth = new Date($.fromOffset).getUTCMonth();
        $.uDate = new Date($.fromOffset).getUTCDate();
        $.uHours = new Date($.fromOffset).getUTCHours();
        $.uMin = new Date($.fromOffset).getUTCMinutes();
        $.uMs = new Date($.fromOffset).getUTCMilliseconds();
        $.ctorOffset = new Date("2026-08-20T20:00:00.123+08:00").getTime();
        $.compactOffset = Date.parse("2026-08-20T20:00:00+0800");
        $.negative = Date.parse("2026-01-01T00:00:00-05:00");
        $.negExpected = Date.UTC(2026, 0, 1, 5, 0, 0, 0);
        $.zulu = Date.parse("2026-08-20T20:00:00.123Z");
        $.zuluExpected = Date.UTC(2026, 7, 20, 20, 0, 0, 123);
    "#,
        obj(vec![]),
    );
    let want = obj(vec![
        ("fromOffset", num(get_num(&r, "expectedUtc"))),
        ("expectedUtc", num(get_num(&r, "expectedUtc"))),
        ("uYear", num(2026.0)),
        ("uMonth", num(7.0)),
        ("uDate", num(20.0)),
        ("uHours", num(12.0)),
        ("uMin", num(0.0)),
        ("uMs", num(123.0)),
        ("ctorOffset", num(get_num(&r, "expectedUtc"))),
        ("compactOffset", num(get_num(&r, "expectedUtc") - 123.0)),
        ("negative", num(get_num(&r, "negExpected"))),
        ("negExpected", num(get_num(&r, "negExpected"))),
        ("zulu", num(get_num(&r, "zuluExpected"))),
        ("zuluExpected", num(get_num(&r, "zuluExpected"))),
    ]);
    assert_eq!(r, want);
    // zulu is 8 hours ahead of the +08:00 instant (20:00Z vs 12:00Z)
    assert_eq!(
        get_num(&r, "zuluExpected") - get_num(&r, "expectedUtc"),
        8.0 * 3_600_000.0
    );

    // No offset => local time. Round-trips through local getters.
    let (r, _) = run(
        r#"
        $.d = new Date("2026-08-20T20:00:00.500");
        $.ly = $.d.getFullYear();
        $.lm = $.d.getMonth();
        $.ld = $.d.getDate();
        $.lh = $.d.getHours();
        $.lmin = $.d.getMinutes();
        $.lsec = $.d.getSeconds();
        $.lms = $.d.getMilliseconds();
        $.ms = $.d.getTime();
        $.iso = $.d.toISOString();
        $.dateOnlyY = new Date(Date.parse("2020-01-02")).getFullYear();
        $.dateOnlyM = new Date(Date.parse("2020-01-02")).getMonth();
        $.dateOnlyD = new Date(Date.parse("2020-01-02")).getDate();
        $.dateOnlyH = new Date(Date.parse("2020-01-02")).getHours();
    "#,
        obj(vec![]),
    );
    let want = obj(vec![
        ("d", Value::Date(get_num(&r, "ms"))),
        ("ly", num(2026.0)),
        ("lm", num(7.0)),
        ("ld", num(20.0)),
        ("lh", num(20.0)),
        ("lmin", num(0.0)),
        ("lsec", num(0.0)),
        ("lms", num(500.0)),
        ("ms", num(get_num(&r, "ms"))),
        ("iso", s(&get_str(&r, "iso"))),
        ("dateOnlyY", num(2020.0)),
        ("dateOnlyM", num(0.0)),
        ("dateOnlyD", num(2.0)),
        ("dateOnlyH", num(0.0)),
    ]);
    assert_eq!(r, want);
    assert!(get_str(&r, "iso").ends_with('Z'));

    // Multi-arg constructor uses local time; local getters round-trip.
    let (r, _) = run(
        r#"
        $.d = new Date(2026, 7, 20, 20, 0, 0, 500);
        $.ly = $.d.getFullYear();
        $.lm = $.d.getMonth();
        $.ld = $.d.getDate();
        $.lh = $.d.getHours();
        $.lmin = $.d.getMinutes();
        $.lms = $.d.getMilliseconds();
        $.ms = $.d.getTime();
        $.uY = $.d.getUTCFullYear();
        $.uM = $.d.getUTCMonth();
        $.uD = $.d.getUTCDate();
        $.uH = $.d.getUTCHours();
        $.uMi = $.d.getUTCMinutes();
        $.uS = $.d.getUTCSeconds();
        $.uMs = $.d.getUTCMilliseconds();
        $.rebuilt = Date.UTC($.uY, $.uM, $.uD, $.uH, $.uMi, $.uS, $.uMs);
    "#,
        obj(vec![]),
    );
    let want = obj(vec![
        ("d", Value::Date(get_num(&r, "ms"))),
        ("ly", num(2026.0)),
        ("lm", num(7.0)),
        ("ld", num(20.0)),
        ("lh", num(20.0)),
        ("lmin", num(0.0)),
        ("lms", num(500.0)),
        ("ms", num(get_num(&r, "ms"))),
        ("uY", num(get_num(&r, "uY"))),
        ("uM", num(get_num(&r, "uM"))),
        ("uD", num(get_num(&r, "uD"))),
        ("uH", num(get_num(&r, "uH"))),
        ("uMi", num(get_num(&r, "uMi"))),
        ("uS", num(get_num(&r, "uS"))),
        ("uMs", num(get_num(&r, "uMs"))),
        ("rebuilt", num(get_num(&r, "ms"))),
    ]);
    assert_eq!(r, want);
}

#[test]
fn math_static_methods() {
    let (r, _) = run(
        r#"
        $.pi = Math.PI;
        $.e = Math.E;
        $.abs = Math.abs(-3.5);
        $.floor = Math.floor(3.7);
        $.ceil = Math.ceil(3.2);
        $.round1 = Math.round(3.5);
        $.round2 = Math.round(-2.5);
        $.trunc = Math.trunc(3.9);
        $.sign = Math.sign(-7);
        $.max = Math.max(1, 5, 3);
        $.min = Math.min(1, 5, 3);
        $.pow = Math.pow(2, 10);
        $.sqrt = Math.sqrt(16);
        $.cbrt = Math.cbrt(27);
        $.exp = Math.exp(0);
        $.log = Math.log(Math.E);
        $.log2 = Math.log2(8);
        $.log10 = Math.log10(1000);
        $.sin = Math.sin(0);
        $.cos = Math.cos(0);
        $.atan2 = Math.atan2(1, 1);
        $.hypot = Math.hypot(3, 4);
        $.randNum = typeof Math.random();
        $.randInRange = Math.random() >= 0 && Math.random() < 1;
    "#,
        obj(vec![]),
    );
    let want = obj(vec![
        ("pi", num(std::f64::consts::PI)),
        ("e", num(std::f64::consts::E)),
        ("abs", num(3.5)),
        ("floor", num(3.0)),
        ("ceil", num(4.0)),
        ("round1", num(4.0)),
        ("round2", num(-2.0)),
        ("trunc", num(3.0)),
        ("sign", num(-1.0)),
        ("max", num(5.0)),
        ("min", num(1.0)),
        ("pow", num(1024.0)),
        ("sqrt", num(4.0)),
        ("cbrt", num(3.0)),
        ("exp", num(1.0)),
        ("log", num(1.0)),
        ("log2", num(3.0)),
        ("log10", num(3.0)),
        ("sin", num(0.0)),
        ("cos", num(1.0)),
        ("atan2", num(std::f64::consts::FRAC_PI_4)),
        ("hypot", num(5.0)),
        ("randNum", s("number")),
        ("randInRange", Value::Bool(true)),
    ]);
    assert_eq!(r, want);
}

#[test]
fn nan_infinity_serialization() {
    let (r, _) = run(
        r#"
        $.concatNan = "x" + parseInt("y");
        $.concatInf = "x" + parseFloat("Infinity");
        $.concatNegInf = "x" + parseFloat("-Infinity");
        $.json = {a: parseInt("z"), b: 1}.toString();
    "#,
        obj(vec![]),
    );
    let want = obj(vec![
        ("concatNan", s("xNaN")),
        ("concatInf", s("xInfinity")),
        ("concatNegInf", s("x-Infinity")),
        ("json", s("{\"a\":null,\"b\":1}")),
    ]);
    assert_eq!(r, want);
}

#[test]
fn increment_decrement() {
    let (r, _) = run(
        r#"
        x = 5;
        $.post = x++;
        $.after = x;
        y = 5;
        $.pre = ++y;
        z = 5;
        $.postDec = z--;
        w = 5;
        $.preDec = --w;
        $.arr = [10, 20];
        $.v0 = $.arr[0]++;
        $.arr0 = $.arr[0];
        $.o = {n: 7};
        $.o1 = $.o.n--;
        $.o2 = $.o.n;
    "#,
        obj(vec![]),
    );
    let want = obj(vec![
        ("post", num(5.0)),
        ("after", num(6.0)),
        ("pre", num(6.0)),
        ("postDec", num(5.0)),
        ("preDec", num(4.0)),
        ("arr", arr(vec![num(11.0), num(20.0)])),
        ("v0", num(10.0)),
        ("arr0", num(11.0)),
        ("o", obj(vec![("n", num(6.0))])),
        ("o1", num(7.0)),
        ("o2", num(6.0)),
    ]);
    assert_eq!(r, want);
}

#[test]
fn bitwise_and_modulo() {
    let (r, _) = run(
        r#"
        $.and = 12 & 10;
        $.or = 12 | 10;
        $.xor = 12 ^ 10;
        $.not = ~5;
        $.shl = 1 << 3;
        $.shr = -8 >> 2;
        $.ushr = -8 >>> 29;
        $.mod = 7 % 3;
        $.modNeg = -7 % 3;
        $.prec1 = 10 + 3 % 4;
        $.prec2 = 1 << 2 + 1;
        m = 7;
        m %= 4;
        $.modAssign = m;
    "#,
        obj(vec![]),
    );
    let want = obj(vec![
        ("and", num(8.0)),
        ("or", num(14.0)),
        ("xor", num(6.0)),
        ("not", num(-6.0)),
        ("shl", num(8.0)),
        ("shr", num(-2.0)),
        ("ushr", num(7.0)),
        ("mod", num(1.0)),
        ("modNeg", num(-1.0)),
        ("prec1", num(13.0)),
        ("prec2", num(8.0)),
        ("modAssign", num(3.0)),
    ]);
    assert_eq!(r, want);
}

#[test]
fn ternary_conditional() {
    let (r, _) = run(
        r#"
        $.t1 = 1 ? "yes" : "no";
        $.t2 = 0 ? "yes" : "no";
        $.t3 = null ? "a" : "b";
        $.t4 = "" ? 1 : 2;
        $.nested = 1 ? (0 ? 1 : 2) : 3;
        $.rightAssoc = 0 ? 1 : 0 ? 2 : 3;
        a = 1;
        x = a ? 10 : 20;
        $.condAssign = x;
    "#,
        obj(vec![]),
    );
    let want = obj(vec![
        ("t1", s("yes")),
        ("t2", s("no")),
        ("t3", s("b")),
        ("t4", num(2.0)),
        ("nested", num(2.0)),
        ("rightAssoc", num(3.0)),
        ("condAssign", num(10.0)),
    ]);
    assert_eq!(r, want);
}

#[test]
fn optional_chaining() {
    let (r, _) = run(
        r#"
        $.a = {b: {c: 42}};
        $.v1 = $.a?.b?.c;
        $.v2 = $.a?.missing?.c;
        $.v3 = $.missing?.c;
        $.v4 = $.a?.b;
        $.arr = [10, 20];
        $.v5 = $.arr?.[1];
        $.o = {f: x => x * 2};
        $.v6 = $.o?.f(21);
        $.v7 = $.missing?.f(1);
        $.v8 = $.a?.b.c;
    "#,
        obj(vec![]),
    );
    // The stored arrow function in `o` cannot be compared structurally, so
    // assert the numeric/object members directly.
    let o = match &r {
        Value::Object(o) => o.borrow(),
        _ => panic!("root must be object"),
    };
    assert_eq!(o.get("v1"), Some(&num(42.0)));
    assert_eq!(o.get("v2"), Some(&Value::Null));
    assert_eq!(o.get("v3"), Some(&Value::Null));
    assert_eq!(o.get("v4"), Some(&obj(vec![("c", num(42.0))])));
    assert_eq!(o.get("v5"), Some(&num(20.0)));
    assert_eq!(o.get("v6"), Some(&num(42.0)));
    assert_eq!(o.get("v7"), Some(&Value::Null));
    assert_eq!(o.get("v8"), Some(&num(42.0)));
    assert!(matches!(o.get("o"), Some(Value::Object(_))));
}

#[test]
fn new_operator_and_constructors() {
    let (r, _) = run(
        r#"
        $.a = new Array(1, 2, 3);
        $.b = Array();
        $.s = new String(42);
        $.n = Number("3.5");
        $.b2 = Boolean(0);
        $.b3 = Boolean("x");
        $.o = Object();
    "#,
        obj(vec![]),
    );
    let want = obj(vec![
        ("a", arr(vec![num(1.0), num(2.0), num(3.0)])),
        ("b", arr(vec![])),
        ("s", s("42")),
        ("n", num(3.5)),
        ("b2", Value::Bool(false)),
        ("b3", Value::Bool(true)),
        ("o", obj(vec![])),
    ]);
    assert_eq!(r, want);
}

#[test]
fn for_loop_increment_and_shift_masking() {
    let (r, _) = run(
        r#"
        sum = 0;
        for (i = 0; i < 4; i++) { sum = sum + i; }
        $.loopSum = sum;
        $.shlWrap = 1 << 32;
        $.shlWrap2 = 1 << 34;
    "#,
        obj(vec![]),
    );
    let want = obj(vec![
        ("loopSum", num(6.0)),
        ("shlWrap", num(1.0)),
        ("shlWrap2", num(4.0)),
    ]);
    assert_eq!(r, want);
}

#[test]
fn json_parse_basic() {
    let (r, _) = run(
        r#"
        $.a = JSON.parse('{"x":1,"y":[true,false,null]}');
        $.b = JSON.parse('42');
        $.c = JSON.parse('"hi"');
        $.d = JSON.parse('null');
    "#,
        obj(vec![]),
    );
    let want = obj(vec![
        (
            "a",
            obj(vec![
                ("x", num(1.0)),
                ("y", arr(vec![Value::Bool(true), Value::Bool(false), Value::Null])),
            ]),
        ),
        ("b", num(42.0)),
        ("c", s("hi")),
        ("d", Value::Null),
    ]);
    assert_eq!(r, want);
}

#[test]
fn json_parse_accepts_jsonc() {
    let (r, _) = run(
        r#"
        $.a = JSON.parse('{ /* comment */ "x": 1, }');
        $.b = JSON.parse('[1, 2, 3,]');
    "#,
        obj(vec![]),
    );
    let want = obj(vec![
        ("a", obj(vec![("x", num(1.0))])),
        ("b", arr(vec![num(1.0), num(2.0), num(3.0)])),
    ]);
    assert_eq!(r, want);
}

#[test]
fn json_parse_invalid_input_errors() {
    let err = lang::execute("JSON.parse('{x:1}')", obj(vec![]), 10000);
    assert!(err.is_err());

    let err = lang::execute("JSON.parse(42)", obj(vec![]), 10000);
    assert!(err.is_err());

    let err = lang::execute("JSON.parse()", obj(vec![]), 10000);
    assert!(err.is_err());
}

#[test]
fn json_stringify_basic() {
    let (r, _) = run(
        r#"
        $.a = JSON.stringify({x: 1, y: [true, false, null]});
        $.b = JSON.stringify(42);
        $.c = JSON.stringify("hi");
        $.d = JSON.stringify(null);
        $.e = JSON.stringify([1, 2, 3]);
    "#,
        obj(vec![]),
    );
    let want = obj(vec![
        ("a", s("{\"x\":1,\"y\":[true,false,null]}")),
        ("b", s("42")),
        ("c", s("\"hi\"")),
        ("d", s("null")),
        ("e", s("[1,2,3]")),
    ]);
    assert_eq!(r, want);
}

#[test]
fn json_stringify_functions_become_null() {
    let (r, _) = run(
        r#"
        $.a = JSON.stringify({f: (x) => x * 2, n: 1});
        $.b = JSON.stringify([(x) => x, 2]);
        $.c = JSON.stringify((x) => x);
    "#,
        obj(vec![]),
    );
    let want = obj(vec![
        ("a", s("{\"f\":null,\"n\":1}")),
        ("b", s("[null,2]")),
        ("c", s("null")),
    ]);
    assert_eq!(r, want);
}

#[test]
fn json_roundtrip() {
    let (r, _) = run(
        r#"
        $.out = JSON.stringify(JSON.parse('{"a":[1,2,3],"b":"hi"}'));
    "#,
        obj(vec![]),
    );
    let want = obj(vec![("out", s("{\"a\":[1,2,3],\"b\":\"hi\"}"))]);
    assert_eq!(r, want);
}
