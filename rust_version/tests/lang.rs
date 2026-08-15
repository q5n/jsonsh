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

fn execute(code: &str, root: Value, max: usize) -> Result<(Value, Option<Value>), lang::Error> {
    lang::execute(code, root, max)
}

#[test]
fn literals_operators_and_builtins() {
    let (r, _) = run(
        r#"
		$.text = 'go' + "lang";
		$.math = 1 + 2 * 3;
		$.logic = 0 || (2 > 1 && !false);
		$.array = [1, {name: "x"}, true, null,];
		$.len = "中文a".length;
		$.has = $.text.indexOf("lang") >= 0;
		$.keys = keys({b: 1, a: 2});
	"#,
        obj(vec![]),
    );
    let want = obj(vec![
        ("text", s("golang")),
        ("math", num(7.0)),
        ("logic", Value::Bool(true)),
        (
            "array",
            arr(vec![num(1.0), obj(vec![("name", s("x"))]), Value::Bool(true), Value::Null]),
        ),
        ("len", num(3.0)),
        ("has", Value::Bool(true)),
        ("keys", arr(vec![s("a"), s("b")])),
    ]);
    assert_eq!(r, want);
}

#[test]
fn control_flow_mutation_and_delete() {
    let root = obj(vec![(
        "users",
        arr(vec![
            obj(vec![
                ("score", num(70.0)),
                ("tags", arr(vec![s("go")])),
                ("secret", Value::Bool(true)),
            ]),
            obj(vec![
                ("score", num(20.0)),
                ("tags", arr(vec![s("blocked")])),
                ("secret", Value::Bool(true)),
            ]),
            obj(vec![
                ("score", num(90.0)),
                ("tags", arr(vec![s("go")])),
                ("secret", Value::Bool(true)),
            ]),
        ]),
    )]);
    let (r, _) = run(
        r#"
		total = 0;
		for (i in $.users) {
			u = $.users[i];
			if (u.tags[0] == "blocked") { delete u.secret; continue; }
			total += u.score;
			if (total > 100) { break; }
		}
		$.total = total;
		delete $.users[1];
	"#,
        root,
    );
    if let Value::Object(o) = &r {
        let m = o.borrow();
        assert_eq!(m.get("total"), Some(&num(160.0)));
        if let Some(Value::Array(users)) = m.get("users") {
            assert_eq!(users.borrow().len(), 2);
        } else {
            panic!("users missing");
        }
    } else {
        panic!("root not object");
    }
}

#[test]
fn short_circuit_and_errors() {
    run(
        "x = false && missing.value; y = true || missing.value;",
        obj(vec![]),
    );
    let cases = vec![
        ("x = 1 / 0;", "division by zero"),
        ("x = true - 1;", "incompatible operand types"),
        ("break;", "outside loop"),
        ("x = 1 y = 2;", "expected ';'"),
    ];
    for (code, contains) in cases {
        let e = execute(code, obj(vec![]), 100);
        match e {
            Err(err) => assert!(
                err.to_string().contains(contains),
                "{} error={:?}",
                code,
                err
            ),
            Ok(_) => panic!("{} should error", code),
        }
    }
}

#[test]
fn newlines_separate_statements() {
    let (root, last) = run(
        r#"
		x = 1
		y = x +
			2
		// A comment-ending newline also separates statements.
		$ = {
			total: y,
			label: "go"
				.padEnd(4, "!")
		}
		$.done = true
		$.total
	"#,
        Value::Null,
    );
    let want = obj(vec![("total", num(3.0)), ("label", s("go!!")), ("done", Value::Bool(true))]);
    assert_eq!(root, want);
    assert_eq!(last, Some(num(3.0)));
}

#[test]
fn missing_object_property_returns_null() {
    let (root, last) = run(
        r#"
		$.dot = $.missing;
		$.bracket = $["alsoMissing"];
		$.compound = {};
		$.compound.value += "suffix";
		$.dot;
	"#,
        obj(vec![]),
    );
    let want = obj(vec![
        ("dot", Value::Null),
        ("bracket", Value::Null),
        ("compound", obj(vec![("value", s("nullsuffix"))])),
    ]);
    assert_eq!(root, want);
    assert_eq!(last, Some(Value::Null));
}

#[test]
fn deep_equality_and_dynamic_access() {
    let (r, _) = run(
        r#"key="item"; $.same = [1,{a:true}] == [1,{a:true}]; $[key] = 3; $[key] *= 2; $.first = keys({b:1,a:2})[0];"#,
        obj(vec![]),
    );
    if let Value::Object(o) = &r {
        let m = o.borrow();
        assert_eq!(m.get("same"), Some(&Value::Bool(true)));
        assert_eq!(m.get("item"), Some(&num(6.0)));
        assert_eq!(m.get("first"), Some(&s("a")));
    } else {
        panic!("root not object");
    }
}

#[test]
fn array_push_mutates_shared_array_and_returns_length() {
    let (r, last) = run(
        "items=$.items; size=items.push(2, {name:\"three\"}); size;",
        obj(vec![("items", arr(vec![num(1.0)]))]),
    );
    if let Value::Object(o) = &r {
        let m = o.borrow();
        if let Some(Value::Array(items)) = m.get("items") {
            let items = items.borrow();
            assert_eq!(items.len(), 3);
            assert_eq!(items[1], num(2.0));
            if let Value::Object(name) = &items[2] {
                assert_eq!(name.borrow().get("name"), Some(&s("three")));
            }
        }
    }
    assert_eq!(last, Some(num(3.0)));
}

#[test]
fn array_push_errors() {
    let cases = vec![
        ("$.items.push();", "at least 1"),
        ("$.name.push(1);", "array receiver"),
        ("$.items.unknown(1);", "unknown method"),
    ];
    let root = obj(vec![("items", arr(vec![])), ("name", s("x"))]);
    for (code, want) in cases {
        let e = execute(code, root.deep_clone(), 100);
        match e {
            Err(err) => assert!(err.to_string().contains(want), "{} error={:?}", code, err),
            Ok(_) => panic!("{} should error", code),
        }
    }
}

#[test]
fn array_reverse_reverses_in_place_and_returns_same_array() {
    let (r, last) = run(
        "items=$.items; result=items.reverse(); result.push(4); [items.length, result.length, items];",
        obj(vec![("items", arr(vec![num(1.0), num(2.0), num(3.0)]))]),
    );
    assert_eq!(
        last,
        Some(arr(vec![
            num(4.0),
            num(4.0),
            arr(vec![num(3.0), num(2.0), num(1.0), num(4.0)]),
        ]))
    );
    if let Value::Object(o) = &r {
        let m = o.borrow();
        if let Some(Value::Array(items)) = m.get("items") {
            let items = items.borrow();
            assert_eq!(&*items, &[num(3.0), num(2.0), num(1.0), num(4.0)]);
        }
    }
}

#[test]
fn array_reverse_chains_with_join() {
    let (_, last) = run(
        "$.tags.reverse().join(\", \");",
        obj(vec![("tags", arr(vec![s("a"), s("b"), s("c")]))]),
    );
    assert_eq!(last, Some(s("c, b, a")));
}

#[test]
fn array_reverse_empty_and_single() {
    let (empty, _) = run(
        "$ = {items: []}; $.items.reverse(); $;",
        obj(vec![("items", arr(vec![num(1.0)]))]),
    );
    if let Value::Object(o) = &empty {
        if let Some(Value::Array(items)) = o.borrow().get("items") {
            assert!(items.borrow().is_empty());
        }
    }

    let (single, _) = run("$ = {items: [42]}; $.items.reverse(); $;", Value::Null);
    if let Value::Object(o) = &single {
        if let Some(Value::Array(items)) = o.borrow().get("items") {
            assert_eq!(&*items.borrow(), &[num(42.0)]);
        }
    }
}

#[test]
fn array_reverse_errors() {
    let cases = vec![
        ("$.items.reverse(1);", "no arguments"),
        ("$.name.reverse();", "array receiver"),
    ];
    let root = obj(vec![("items", arr(vec![num(1.0)])), ("name", s("x"))]);
    for (code, want) in cases {
        let e = execute(code, root.deep_clone(), 100);
        match e {
            Err(err) => assert!(err.to_string().contains(want), "{} error={:?}", code, err),
            Ok(_) => panic!("{} should error", code),
        }
    }
}

#[test]
fn root_can_be_assigned_directly() {
    let (r, last) = run(
        "$ = {items: [1]}; $.items.push(2); $;",
        obj(vec![("old", Value::Bool(true))]),
    );
    let want = obj(vec![("items", arr(vec![num(1.0), num(2.0)]))]);
    assert_eq!(r, want);
    assert_eq!(last, Some(want.clone()));

    let (r, _) = run("$ = 5; $ += 2;", Value::Null);
    assert_eq!(r, num(7.0));
}

#[test]
fn top_level_object_literal_is_an_expression() {
    let (root, last) = run("{age: 18}", Value::Null);
    assert_eq!(root, Value::Null);
    assert_eq!(last, Some(obj(vec![("age", num(18.0))])));

    let (_, empty) = run("{}", Value::Null);
    assert_eq!(empty, Some(obj(vec![])));
}

#[test]
fn env_reads_environment_variables() {
    std::env::set_var("JSONSH_ENV_TEST", "available");
    std::env::set_var("JSONSH_EMPTY_ENV_TEST", "");
    std::env::remove_var("JSONSH_MISSING_ENV_TEST_7B3D2A");

    let (root, _, ) = lang::execute(
        r#"
		$ = {
			value: env("JSONSH_ENV_TEST"),
			empty: env("JSONSH_EMPTY_ENV_TEST"),
			missing: env("JSONSH_MISSING_ENV_TEST_7B3D2A"),
		};
	"#,
        Value::Null,
        1000,
    )
    .unwrap();
    let want = obj(vec![
        ("value", s("available")),
        ("empty", s("")),
        ("missing", Value::Null),
    ]);
    assert_eq!(root, want);
}

#[test]
fn env_rejects_invalid_arguments() {
    let cases = vec![
        ("env();", "expects 1 argument"),
        ("env(\"A\", \"B\");", "expects 1 argument"),
        ("env(1);", "requires a string argument"),
    ];
    for (code, want) in cases {
        let e = execute(code, Value::Null, 100);
        match e {
            Err(err) => assert!(err.to_string().contains(want), "{} error={:?}", code, err),
            Ok(_) => panic!("{} should error", code),
        }
    }
}

#[test]
fn log_writes_values_and_returns_null() {
    let mut output = Vec::new();
    let root = obj(vec![("unchanged", Value::Bool(true))]);
    let (got_root, last) = lang::execute_with_output(
        r#"
		log("hello", 2, true, null, {a:1}, [1,2]);
		log();
	"#,
        root.clone(),
        1000,
        &mut output,
    )
    .unwrap();
    let want = "hello 2 true null {\"a\":1} 1,2\n\n";
    assert_eq!(String::from_utf8(output).unwrap(), want);
    assert_eq!(got_root, root);
    assert_eq!(last, Some(Value::Null));
}

#[test]
fn execute_discards_log_output_by_default() {
    lang::execute("log(\"hidden\");", Value::Null, 100).unwrap();
}

#[test]
fn log_preserves_unicode_escapes() {
    let mut output = Vec::new();
    lang::execute_with_output("log({icon: \"\\uee63\"});", Value::Null, 1000, &mut output).unwrap();
    assert_eq!(String::from_utf8(output).unwrap(), "{\"icon\":\"\\uee63\"}\n");
}

#[test]
fn for_of_array_values_and_control_flow() {
    let (root, _, ) = lang::execute(
        r#"
		$.out = [];
		for (item of $.values) {
			if (item == 2) { continue; }
			if (item == 4) { break; }
			$.out.push(item);
		}
	"#,
        obj(vec![(
            "values",
            arr(vec![num(1.0), num(2.0), num(3.0), num(4.0), num(5.0)]),
        )]),
        10000,
    )
    .unwrap();
    let want = obj(vec![
        (
            "values",
            arr(vec![num(1.0), num(2.0), num(3.0), num(4.0), num(5.0)]),
        ),
        ("out", arr(vec![num(1.0), num(3.0)])),
    ]);
    assert_eq!(root, want);
}

#[test]
fn for_of_array_uses_live_iterator() {
    let (root, _, ) = lang::execute(
        r#"
		$.seen = [];
		for (item of $.values) {
			$.seen.push(item);
			if (item == 1) { delete $.values[0]; }
			if (item == 3) { $.values.push(4); }
		}
	"#,
        obj(vec![(
            "values",
            arr(vec![num(1.0), num(2.0), num(3.0)]),
        )]),
        10000,
    )
    .unwrap();
    let want = obj(vec![
        ("values", arr(vec![num(2.0), num(3.0), num(4.0)])),
        ("seen", arr(vec![num(1.0), num(3.0), num(4.0)])),
    ]);
    assert_eq!(root, want);
}

#[test]
fn for_of_string_uses_unicode_code_points() {
    let (root, _, ) =
        lang::execute("$.out = []; for (ch of \"A😀界\") { $.out.push(ch); }", obj(vec![]), 10000)
            .unwrap();
    let want = obj(vec![("out", arr(vec![s("A"), s("😀"), s("界")]))]);
    assert_eq!(root, want);
}

#[test]
fn for_of_rejects_non_iterable() {
    let e = execute("for (item of $) {}", obj(vec![]), 1000);
    match e {
        Err(err) => assert!(err.to_string().contains("for..of requires array or string")),
        Ok(_) => panic!("should error"),
    }
}

#[test]
fn c_style_for_counts_and_accepts_empty_parts() {
    let (root, _, ) = lang::execute(
        r#"
        $.out = [];
        for (i = 0; i < 4; i += 1) {
            $.out.push(i);
        }
        for (j = 0; j < 3; j += 1) {}
    "#,
        obj(vec![]),
        10000,
    )
    .unwrap();
    assert_eq!(
        root,
        obj(vec![("out", arr(vec![num(0.0), num(1.0), num(2.0), num(3.0)]))])
    );

    let (root, _, ) = lang::execute(
        r#"
        $.out = [];
        i = 5;
        for (; i < 8; i += 1) {
            $.out.push(i);
        }
        for (;;) {
            if ($.out.length >= 6) { break; }
            $.out.push(99);
        }
    "#,
        obj(vec![]),
        10000,
    )
    .unwrap();
    assert_eq!(
        root,
        obj(vec![(
            "out",
            arr(vec![num(5.0), num(6.0), num(7.0), num(99.0), num(99.0), num(99.0)])
        )])
    );
}

#[test]
fn c_style_for_continue_still_runs_update() {
    let (root, _, ) = lang::execute(
        r#"
        $.out = [];
        for (i = 0; i < 5; i += 1) {
            if (i == 1 || i == 3) { continue; }
            $.out.push(i);
        }
    "#,
        obj(vec![]),
        10000,
    )
    .unwrap();
    assert_eq!(
        root,
        obj(vec![("out", arr(vec![num(0.0), num(2.0), num(4.0)]))])
    );
}

#[test]
fn c_style_for_empty_condition_is_limited_by_max_steps() {
    let e = execute("for (;;) {}", Value::Null, 10);
    match e {
        Err(err) => assert!(err.to_string().contains("maximum execution steps exceeded")),
        Ok(_) => panic!("should error"),
    }
}

#[test]
fn c_style_for_syntax_errors() {
    let cases = vec![
        ("for (i = 0 i < 2; i += 1) {}", "after for initializer"),
        ("for (i = 0; i < 2 i += 1) {}", "after for condition"),
        ("for (i = 0; i < 2; i += 1 {}", "after for update"),
    ];
    for (code, want) in cases {
        let e = execute(code, Value::Null, 100);
        match e {
            Err(err) => assert!(err.to_string().contains(want), "{} error={:?}", code, err),
            Ok(_) => panic!("{} should error", code),
        }
    }
}

#[test]
fn empty_statements_and_trailing_block_semicolon() {
    let (root, _, ) = lang::execute(
        ";;; { $.a = 1;;; };;;; if (true) { $.b = 2; };;; for (k in $) { break; };;;",
        obj(vec![]),
        10000,
    )
    .unwrap();
    let want = obj(vec![("a", num(1.0)), ("b", num(2.0))]);
    assert_eq!(root, want);
}

#[test]
fn additional_string_methods() {
    let (root, _) = run(
        r#"
		$.last = "a😀a😀".lastIndexOf("😀");
		$.lastFrom = "a😀a😀".lastIndexOf("😀", 2);
		$.compare = ["a".localeCompare("b"), "b".localeCompare("b"), "c".localeCompare("b")];
		$.split = "a, b;c".split("[,;]\\s*");
		$.limited = "a,b,c".split(",", 2);
	"#,
        obj(vec![]),
    );
    let want = obj(vec![
        ("last", num(3.0)),
        ("lastFrom", num(1.0)),
        ("compare", arr(vec![num(-1.0), num(0.0), num(1.0)])),
        ("split", arr(vec![s("a"), s("b"), s("c")])),
        ("limited", arr(vec![s("a"), s("b")])),
    ]);
    assert_eq!(root, want);
}

#[test]
fn regexp_match_and_replace_methods() {
    let (root, _) = run(
        r#"
		$.match = "id-42".match("([a-z]+)-(\\d+)");
		$.missing = "abc".match("\\d+");
		$.optional = "b".match("(a)?b");
		$.all = "a1 b22".matchAll("([a-z])(\\d+)");
		$.first = "a1 a2".replace("a(\\d)", "x$1");
		$.every = "a1 a2".replaceAll("a(\\d)", "x$1");
	"#,
        obj(vec![]),
    );
    let want = obj(vec![
        ("match", arr(vec![s("id-42"), s("id"), s("42")])),
        ("missing", Value::Null),
        ("optional", arr(vec![s("b"), Value::Null])),
        (
            "all",
            arr(vec![
                arr(vec![s("a1"), s("a"), s("1")]),
                arr(vec![s("b22"), s("b"), s("22")]),
            ]),
        ),
        ("first", s("x1 a2")),
        ("every", s("x1 x2")),
    ]);
    assert_eq!(root, want);
}

#[test]
fn array_index_methods() {
    let (root, _) = run(
        r#"
		a = [1, {name:"x"}, 1, 2];
		$.first = a.indexOf(1);
		$.from = a.indexOf(1, 1);
		$.negative = a.indexOf(1, -2);
		$.object = a.indexOf({name:"x"});
		$.last = a.lastIndexOf(1);
		$.lastFrom = a.lastIndexOf(1, 1);
		$.missing = a.lastIndexOf(9);
	"#,
        obj(vec![]),
    );
    let want = obj(vec![
        ("first", num(0.0)),
        ("from", num(2.0)),
        ("negative", num(2.0)),
        ("object", num(1.0)),
        ("last", num(2.0)),
        ("lastFrom", num(0.0)),
        ("missing", num(-1.0)),
    ]);
    assert_eq!(root, want);
}

#[test]
fn regexp_method_errors() {
    let cases = vec![
        ("\"x\".match(\"[\");", "invalid regular expression"),
        ("\"x\".split(1);", "pattern must be a string"),
        ("\"x\".split(\"x\", -1);", "non-negative integer"),
        ("\"x\".replace(\"x\", 1);", "replacement must be a string"),
        ("\"x\".localeCompare(1);", "string argument"),
        ("[1].indexOf();", "1 or 2 arguments"),
    ];
    for (code, want) in cases {
        let e = execute(code, obj(vec![]), 100);
        match e {
            Err(err) => assert!(err.to_string().contains(want), "{} error={:?}", code, err),
            Ok(_) => panic!("{} should error", code),
        }
    }
}

#[test]
fn string_properties_and_methods() {
    let (root, _) = run(
        r#"
		$.length = " A中b ".length;
		$.lower = "Go语言".toLowerCase();
		$.upper = "Go语言".toUpperCase();
		$.trimmed = "  hello \n".trim();
		$.substring = "A中BC".substring(3, 1);
		$.index = "A中BC中".indexOf("中", 2);
		$.missing = "abc".indexOf("z");
		$.padStart = "中x".padStart(5, "ab");
		$.padEnd = "中x".padEnd(5, "😀文");
		$.defaultPad = "x".padStart(3);
		$.emptyPad = "x".padEnd(3, "");
		$.noPad = "hello".padStart(3, "0");
	"#,
        obj(vec![]),
    );
    let want = obj(vec![
        ("length", num(5.0)),
        ("lower", s("go语言")),
        ("upper", s("GO语言")),
        ("trimmed", s("hello")),
        ("substring", s("中B")),
        ("index", num(4.0)),
        ("missing", num(-1.0)),
        ("padStart", s("aba中x")),
        ("padEnd", s("中x😀文😀")),
        ("defaultPad", s("  x")),
        ("emptyPad", s("x")),
        ("noPad", s("hello")),
    ]);
    assert_eq!(root, want);
}

#[test]
fn array_length_splice_and_join() {
    let (root, last) = run(
        r#"
		a = $.items;
		$.before = a.length;
		$.removed = a.splice(-3, 2, "x", "y");
		$.joined = a.join("|");
		a.length;
	"#,
        obj(vec![("items", arr(vec![num(1.0), num(2.0), num(3.0), num(4.0)]))]),
    );
    let want = obj(vec![
        ("items", arr(vec![num(1.0), s("x"), s("y"), num(4.0)])),
        ("before", num(4.0)),
        ("removed", arr(vec![num(2.0), num(3.0)])),
        ("joined", s("1|x|y|4")),
    ]);
    assert_eq!(root, want);
    assert_eq!(last, Some(num(4.0)));
}

#[test]
fn string_and_array_method_edge_cases() {
    let (root, _) = run(
        r#"
		$.clamped = "A😀BC".substring(-10, 99);
		$.unicodeIndex = "A😀BC".indexOf("B");
		$.defaultJoin = [1, null, "x"].join();
		a = [1, 4];
		$.none = a.splice(1, 0, 2, 3);
		$.tail = a.splice(2);
		$.remaining = a;
	"#,
        obj(vec![]),
    );
    let want = obj(vec![
        ("clamped", s("A😀BC")),
        ("unicodeIndex", num(2.0)),
        ("defaultJoin", s("1,,x")),
        ("none", arr(vec![])),
        ("tail", arr(vec![num(3.0), num(4.0)])),
        ("remaining", arr(vec![num(1.0), num(2.0)])),
    ]);
    assert_eq!(root, want);
}

#[test]
fn typeof_and_to_string() {
    let (root, _) = run(
        r#"
		$.types = [typeof("x"), typeof([]), typeof({}), typeof(true), typeof(1), typeof(null)];
		$.strings = ["x".toString(), [1,"x",null].toString(), {b:2,a:1}.toString(), true.toString(), (12.5).toString()];
	"#,
        obj(vec![]),
    );
    let want = obj(vec![
        (
            "types",
            arr(vec![
                s("string"),
                s("array"),
                s("object"),
                s("boolean"),
                s("number"),
                s("object"),
            ]),
        ),
        (
            "strings",
            arr(vec![
                s("x"),
                s("1,x,"),
                s("{\"a\":1,\"b\":2}"),
                s("true"),
                s("12.5"),
            ]),
        ),
    ]);
    assert_eq!(root, want);
}

#[test]
fn plus_uses_to_string_when_either_operand_is_string() {
    let (root, _) = run(
        r#"
		$.number = "value=" + 2;
		$.boolean = false + "!";
		$.array = "items=" + [1,2];
		$.object = {b:2,a:1} + "";
		$.sum = 2 + 3;
	"#,
        obj(vec![]),
    );
    let want = obj(vec![
        ("number", s("value=2")),
        ("boolean", s("false!")),
        ("array", s("items=1,2")),
        ("object", s("{\"a\":1,\"b\":2}")),
        ("sum", num(5.0)),
    ]);
    assert_eq!(root, want);
}

#[test]
fn removed_length_and_has_builtins() {
    for code in ["length(\"x\");", "has([], 1);"] {
        let e = execute(code, obj(vec![]), 100);
        match e {
            Err(err) => assert!(err.to_string().contains("unknown function")),
            Ok(_) => panic!("{} should error", code),
        }
    }
}

#[test]
fn new_method_argument_errors() {
    let cases = vec![
        ("\"x\".substring();", "1 or 2 arguments"),
        ("\"x\".indexOf(1);", "string needle"),
        ("\"x\".padStart();", "1 or 2 arguments"),
        ("\"x\".padEnd(-1);", "non-negative integer"),
        ("\"x\".padStart(3, 1);", "padding must be a string"),
        ("[1].join(2);", "separator must be a string"),
        ("[1].splice();", "at least 1 argument"),
        ("typeof();", "expects 1 argument"),
        ("true.toString(1);", "expects no arguments"),
    ];
    for (code, want) in cases {
        let e = execute(code, obj(vec![]), 100);
        match e {
            Err(err) => assert!(err.to_string().contains(want), "{} error={:?}", code, err),
            Ok(_) => panic!("{} should error", code),
        }
    }
}

#[test]
fn for_in_over_object_iterates_sorted_keys() {
    let (root, _, ) = lang::execute(
        "for (k in $) { $.out.push(k); }",
        obj(vec![
            ("b", num(2.0)),
            ("a", num(1.0)),
            ("c", num(3.0)),
            ("out", arr(vec![])),
        ]),
        10000,
    )
    .unwrap();
    if let Value::Object(o) = &root {
        if let Some(Value::Array(out)) = o.borrow().get("out") {
            let out: Vec<Value> = out.borrow().clone();
            assert_eq!(
                out,
                vec![s("a"), s("b"), s("c"), s("out")],
                "keys must be lexicographically sorted"
            );
        } else {
            panic!("out missing");
        }
    } else {
        panic!("root not object");
    }
}

#[test]
fn for_in_skips_members_deleted_before_their_turn() {
    let (root, _, ) = lang::execute(
        r#"
		for (k in $) {
			if (k == "b") { delete $.b; }
			$.out.push(k);
		}
	"#,
        obj(vec![
            ("b", num(2.0)),
            ("a", num(1.0)),
            ("c", num(3.0)),
            ("out", arr(vec![])),
        ]),
        10000,
    )
    .unwrap();
    if let Value::Object(o) = &root {
        let m = o.borrow();
        assert!(!m.contains_key("b"), "b should be deleted");
        if let Some(Value::Array(out)) = m.get("out") {
            assert_eq!(out.borrow().clone(), vec![s("a"), s("b"), s("c"), s("out")]);
        }
    } else {
        panic!("root not object");
    }
}

#[test]
fn keys_on_array_returns_indexes() {
    let (r, _) = run("$.a = keys([3,1,2]);", obj(vec![]));
    if let Value::Object(o) = &r {
        assert_eq!(o.borrow().get("a"), Some(&arr(vec![num(0.0), num(1.0), num(2.0)])));
    }
}

#[test]
fn string_comparison_operators() {
    let (r, _) = run(
        "$.lt = \"a\" < \"b\"; $.gt = \"b\" > \"a\"; $.eq = \"a\" == \"a\"; $.ne = \"a\" != \"b\"; $.le = \"a\" <= \"a\"; $.ge = \"b\" >= \"b\";",
        obj(vec![]),
    );
    if let Value::Object(o) = &r {
        let m = o.borrow();
        assert_eq!(m.get("lt"), Some(&Value::Bool(true)));
        assert_eq!(m.get("gt"), Some(&Value::Bool(true)));
        assert_eq!(m.get("eq"), Some(&Value::Bool(true)));
        assert_eq!(m.get("ne"), Some(&Value::Bool(true)));
        assert_eq!(m.get("le"), Some(&Value::Bool(true)));
        assert_eq!(m.get("ge"), Some(&Value::Bool(true)));
    }
}

#[test]
fn matchall_with_no_matches_returns_empty_array() {
    let (r, _) = run("$.m = \"abc\".matchAll(\"\\\\d+\");", obj(vec![]));
    if let Value::Object(o) = &r {
        assert_eq!(o.borrow().get("m"), Some(&arr(vec![])));
    }
}

#[test]
fn split_empty_pattern() {
    let (r, _) = run("$.s = \"abc\".split(\"\");", obj(vec![]));
    if let Value::Object(o) = &r {
        assert_eq!(
            o.borrow().get("s"),
            Some(&arr(vec![s("a"), s("b"), s("c")]))
        );
    }
}

#[test]
fn replace_with_no_match_returns_original() {
    let (r, _) = run("$.a = \"abc\".replace(\"z\", \"x\"); $.b = \"abc\".replaceAll(\"z\", \"x\");", obj(vec![]));
    if let Value::Object(o) = &r {
        let m = o.borrow();
        assert_eq!(m.get("a"), Some(&s("abc")));
        assert_eq!(m.get("b"), Some(&s("abc")));
    }
}

#[test]
fn logical_operators_return_booleans() {
    let (r, _) = run("$.a = 1 && 2; $.b = 0 || 3; $.c = 0 && 1; $.d = 1 || 0;", obj(vec![]));
    if let Value::Object(o) = &r {
        let m = o.borrow();
        assert_eq!(m.get("a"), Some(&Value::Bool(true)));
        assert_eq!(m.get("b"), Some(&Value::Bool(true)));
        assert_eq!(m.get("c"), Some(&Value::Bool(false)));
        assert_eq!(m.get("d"), Some(&Value::Bool(true)));
    }
}

#[test]
fn unicode_case_mapping_matches_go_simple_mapping() {
    let (r, _) = run(
        "$.u = \"ß\".toUpperCase(); $.l = \"İ\".toLowerCase(); $.f = \"ﬁ\".toUpperCase();",
        obj(vec![]),
    );
    if let Value::Object(o) = &r {
        let m = o.borrow();
        assert_eq!(m.get("u"), Some(&s("ß")));
        assert_eq!(m.get("l"), Some(&s("i")));
        assert_eq!(m.get("f"), Some(&s("ﬁ")));
    }
}

#[test]
fn max_steps_is_enforced() {
    let e = execute("1; 2; 3; 4; 5;", Value::Null, 2);
    match e {
        Err(err) => assert!(err.to_string().contains("maximum execution steps exceeded")),
        Ok(_) => panic!("should exceed max steps"),
    }
}

#[test]
fn block_comments_in_script() {
    let (r, _) = run("/* block */ $.a = 1; // line\n/* more */ $.b = 2;", obj(vec![]));
    if let Value::Object(o) = &r {
        let m = o.borrow();
        assert_eq!(m.get("a"), Some(&num(1.0)));
        assert_eq!(m.get("b"), Some(&num(2.0)));
    }
}

#[test]
fn brace_less_if_reports_expected_brace_error() {
    let e = execute("if (true) $.a = 1;", obj(vec![]), 100);
    match e {
        Err(err) => assert!(
            err.to_string().contains("expected '}'"),
            "error={:?}",
            err
        ),
        Ok(_) => panic!("should error"),
    }
}

#[test]
fn string_index_with_non_string_key_errors() {
    let e = execute("$.x = \"abc\"[1];", obj(vec![]), 100);
    match e {
        Err(err) => assert!(err.to_string().contains("string property"), "error={:?}", err),
        Ok(_) => panic!("should error"),
    }
}

#[test]
fn chinese_identifiers_and_member_access() {
    let (r, _) = run("$.价格 = 80; $.新字段 = \"中文值\";", obj(vec![]));
    if let Value::Object(o) = &r {
        let m = o.borrow();
        assert_eq!(m.get("价格"), Some(&num(80.0)));
        assert_eq!(m.get("新字段"), Some(&s("中文值")));
    } else {
        panic!("root not object");
    }
}

#[test]
fn chinese_object_literal_keys() {
    let (r, last) = run("{姓名: \"张三\", 年龄: 18}", Value::Null);
    let want = obj(vec![("姓名", s("张三")), ("年龄", num(18.0))]);
    assert_eq!(last, Some(want.clone()));
    assert_eq!(r, Value::Null);
}

#[test]
fn chinese_keys_sorted_lexicographically() {
    let (r, _) = run("$.k = keys({中文:1, 中:2, a:3});", obj(vec![]));
    if let Value::Object(o) = &r {
        assert_eq!(o.borrow().get("k"), Some(&arr(vec![s("a"), s("中"), s("中文")])));
    }
}

#[test]
fn chinese_string_methods() {
    let (r, _) = run(
        "$.len = \"中文\".length; $.sub = \"中文\".substring(1); $.idx = \"中文\".indexOf(\"文\"); $.s = \"中,文\".split(\",\");",
        obj(vec![]),
    );
    if let Value::Object(o) = &r {
        let m = o.borrow();
        assert_eq!(m.get("len"), Some(&num(2.0)));
        assert_eq!(m.get("sub"), Some(&s("文")));
        assert_eq!(m.get("idx"), Some(&num(1.0)));
        assert_eq!(m.get("s"), Some(&arr(vec![s("中"), s("文")])));
    }
}

#[test]
fn chinese_for_of_iterates_code_points() {
    let (r, _, ) = lang::execute("$.out = []; for (ch of \"中文\") { $.out.push(ch); }", obj(vec![]), 10000).unwrap();
    if let Value::Object(o) = &r {
        assert_eq!(o.borrow().get("out"), Some(&arr(vec![s("中"), s("文")])));
    }
}
