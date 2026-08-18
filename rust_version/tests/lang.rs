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

fn execute_exit(code: &str, root: Value) -> (Value, Option<Value>, i32) {
    lang::execute_with_output_exit(code, root, 10000, &mut std::io::sink()).unwrap()
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
		$.keys = Object.keys({b: 1, a: 2});
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
        r#"key="item"; $.same = [1,{a:true}] == [1,{a:true}]; $[key] = 3; $[key] *= 2; $.first = Object.keys({b:1,a:2})[0];"#,
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
		$.split = "a, b;c".split(/[,;]\s*/);
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
		$.all = "a1 b22".matchAll(/([a-z])(\d+)/g);
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
fn object_keys_on_array_returns_indexes() {
    let (r, _) = run("$.a = Object.keys([3,1,2]);", obj(vec![]));
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
    let (r, _) = run("$.m = \"abc\".matchAll(/\\d+/g);", obj(vec![]));
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
fn brace_less_if_single_line() {
    let (r, _) = run("if ($.x > 0) $.a = 1;", obj(vec![("x", num(1.0))]));
    if let Value::Object(o) = &r {
        assert_eq!(o.borrow().get("a"), Some(&num(1.0)));
    } else {
        panic!("not object");
    }

    let (r, _) = run("if ($.x > 0) $.a = 1; else $.a = 2;", obj(vec![("x", num(0.0))]));
    if let Value::Object(o) = &r {
        assert_eq!(o.borrow().get("a"), Some(&num(2.0)));
    } else {
        panic!("not object");
    }
}

#[test]
fn brace_less_if_else_same_line() {
    let (r, _) = run(
        "if ($.x > 0) $.a = 1 else $.a = 2",
        obj(vec![("x", num(5.0))]),
    );
    if let Value::Object(o) = &r {
        assert_eq!(o.borrow().get("a"), Some(&num(1.0)));
    } else {
        panic!("not object");
    }
}

#[test]
fn brace_less_if_newline_else_binds() {
    let (r, _) = run(
        "if ($.x > 0)\n  $.a = 1\nelse\n  $.a = 2",
        obj(vec![("x", num(0.0))]),
    );
    if let Value::Object(o) = &r {
        assert_eq!(o.borrow().get("a"), Some(&num(2.0)));
    } else {
        panic!("not object");
    }
}

#[test]
fn brace_less_else_if_chain() {
    let code = "if ($.x == 1) $.a = 1\nelse if ($.x == 2) $.a = 2\nelse $.a = 9";
    let (r, _) = run(code, obj(vec![("x", num(2.0))]));
    if let Value::Object(o) = &r {
        assert_eq!(o.borrow().get("a"), Some(&num(2.0)));
    } else {
        panic!("not object");
    }
}

#[test]
fn brace_less_nested_if_dangling_else() {
    let code = "if ($.outer) if ($.inner) $.a = 1 else $.a = 2";
    let (r, _) = run(
        code,
        obj(vec![("outer", Value::Bool(true)), ("inner", Value::Bool(false))]),
    );
    if let Value::Object(o) = &r {
        assert_eq!(o.borrow().get("a"), Some(&num(2.0)));
    } else {
        panic!("not object");
    }

    let (r, _) = run(
        code,
        obj(vec![("outer", Value::Bool(false)), ("inner", Value::Bool(true))]),
    );
    if let Value::Object(o) = &r {
        assert!(o.borrow().get("a").is_none());
    } else {
        panic!("not object");
    }
}

#[test]
fn brace_less_for_loops() {
    let code = "for (k in $.src) $.dst[k] = $.src[k]";
    let (r, _) = run(
        code,
        obj(vec![
            ("src", obj(vec![("a", num(1.0)), ("b", num(2.0))])),
            ("dst", obj(vec![])),
        ]),
    );
    if let Value::Object(o) = &r {
        let m = o.borrow();
        let dst = m.get("dst").unwrap();
        if let Value::Object(d) = dst {
            let dm = d.borrow();
            assert_eq!(dm.get("a"), Some(&num(1.0)));
            assert_eq!(dm.get("b"), Some(&num(2.0)));
        } else {
            panic!("dst not object");
        }
    } else {
        panic!("not object");
    }

    let code = "for (v of $.src) $.out.push(v)";
    let (r, _) = run(
        code,
        obj(vec![("src", arr(vec![num(10.0), num(20.0)])), ("out", arr(vec![]))]),
    );
    if let Value::Object(o) = &r {
        let m = o.borrow();
        let out = m.get("out").unwrap();
        if let Value::Array(a) = out {
            assert_eq!(a.borrow().len(), 2);
        } else {
            panic!("out not array");
        }
    } else {
        panic!("not object");
    }

    let code = "for (i = 0; i < 3; i += 1) $.a[i] = i";
    let (r, _) = run(code, obj(vec![("a", arr(vec![]))]));
    if let Value::Object(o) = &r {
        let m = o.borrow();
        let a = m.get("a").unwrap();
        if let Value::Array(x) = a {
            let b = x.borrow();
            assert_eq!(b.len(), 3);
            assert_eq!(b[0], num(0.0));
            assert_eq!(b[1], num(1.0));
            assert_eq!(b[2], num(2.0));
        } else {
            panic!("a not array");
        }
    } else {
        panic!("not object");
    }
}

#[test]
fn brace_less_if_wrapping_braced_for() {
    let code = "if ($.ok) for (v of $.src) $.out.push(v)\n$.done = 1";
    let (r, _) = run(
        code,
        obj(vec![
            ("ok", Value::Bool(true)),
            ("src", arr(vec![num(1.0), num(2.0)])),
            ("out", arr(vec![])),
        ]),
    );
    if let Value::Object(o) = &r {
        let m = o.borrow();
        let out = m.get("out").unwrap();
        if let Value::Array(a) = out {
            assert_eq!(a.borrow().len(), 2);
        } else {
            panic!("out not array");
        }
        assert_eq!(m.get("done"), Some(&num(1.0)));
    } else {
        panic!("not object");
    }
}

#[test]
fn brace_less_for_break_continue() {
    let code = "for (i = 0; i < 5; i += 1) if (i == 2) continue; else if (i == 4) break; else $.a.push(i)";
    let (r, _) = run(code, obj(vec![("a", arr(vec![]))]));
    if let Value::Object(o) = &r {
        let m = o.borrow();
        let a = m.get("a").unwrap();
        if let Value::Array(x) = a {
            let b = x.borrow();
            assert_eq!(b.len(), 3);
            assert_eq!(b[0], num(0.0));
            assert_eq!(b[1], num(1.0));
            assert_eq!(b[2], num(3.0));
        } else {
            panic!("a not array");
        }
    } else {
        panic!("not object");
    }
}

#[test]
fn brace_less_semicolon_ends_body() {
    let (r, _) = run("if ($.x > 0) $.a = 1; $.b = 2;", obj(vec![("x", num(0.0))]));
    if let Value::Object(o) = &r {
        let m = o.borrow();
        assert!(m.get("a").is_none());
        assert_eq!(m.get("b"), Some(&num(2.0)));
    } else {
        panic!("not object");
    }
}

#[test]
fn brace_less_missing_body_errors() {
    let e = execute("if (true)", obj(vec![]), 100);
    assert!(e.is_err(), "should error on missing body");
}

#[test]
fn brace_less_empty_body_semicolon() {
    let (r, _) = run("if ($.x > 0); $.a = 1;", obj(vec![("x", num(1.0))]));
    if let Value::Object(o) = &r {
        let m = o.borrow();
        assert_eq!(m.get("a"), Some(&num(1.0)));
    } else {
        panic!("not object");
    }

    let (_, last) = run("for (i = 0; i < 3; i += 1);", obj(vec![]));
    assert_eq!(last, Some(num(3.0)));
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
fn array_index_out_of_range_reads_null_and_grows_on_assign() {
    let (r, last) = run(
        "$.a = [1, 2]; x = $.a[5]; $.a[5] = 6; $.a[3] = 4; [x, $.a.length, $.a];",
        obj(vec![("a", arr(vec![num(1.0), num(2.0)]))]),
    );
    assert_eq!(
        last,
        Some(arr(vec![
            Value::Null,
            num(6.0),
            arr(vec![
                num(1.0),
                num(2.0),
                Value::Null,
                num(4.0),
                Value::Null,
                num(6.0),
            ]),
        ]))
    );
    let _ = r;
}

#[test]
fn array_negative_indexes() {
    let (r, last) = run(
        "a = [1, 2, 3]; first = a[-3]; last = a[-1]; a[-2] = 20; delete a[-1]; [first, last, a];",
        Value::Null,
    );
    assert_eq!(
        last,
        Some(arr(vec![
            num(1.0),
            num(3.0),
            arr(vec![num(1.0), num(20.0)]),
        ]))
    );
    let _ = r;
}

#[test]
fn array_negative_index_out_of_bounds_errors() {
    let root = obj(vec![("a", arr(vec![num(1.0), num(2.0)]))]);
    for code in ["$.a[-3]", "$.a[-3] = 1", "delete $.a[-3]"] {
        let e = execute(code, root.deep_clone(), 100);
        match e {
            Err(err) => assert!(err.to_string().contains("out of range"), "{} error={:?}", code, err),
            Ok(_) => panic!("{} should error", code),
        }
    }
}

#[test]
fn array_delete_out_of_range_is_silent() {
    let (r, _) = run(
        "$.a = [1, 2]; delete $.a[99]; $.a;",
        obj(vec![("a", arr(vec![num(1.0), num(2.0)]))]),
    );
    assert_eq!(
        r,
        obj(vec![("a", arr(vec![num(1.0), num(2.0)]))])
    );
}

#[test]
fn array_compound_assign_into_hole_reads_null() {
    let (r, last) = run(
        "$.a = [1]; $.a[3] = 3; x = $.a[1]; $.a[2] = x; [x, $.a];",
        obj(vec![("a", arr(vec![num(1.0)]))]),
    );
    assert_eq!(
        last,
        Some(arr(vec![
            Value::Null,
            arr(vec![num(1.0), Value::Null, Value::Null, num(3.0)]),
        ]))
    );
    let _ = r;
}

#[test]
fn array_index_invalid_type_errors() {
    let e = execute("$.a = [1]; $.a[\"x\"];", obj(vec![]), 100);
    match e {
        Err(err) => assert!(err.to_string().contains("array index must be an integer")),
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
    let (r, _) = run("$.k = Object.keys({中文:1, 中:2, a:3});", obj(vec![]));
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

#[test]
fn arrow_expression_body_forms() {
    let (_, last) = run("f = (a, b) => a + b; '' + f", obj(vec![]));
    assert_eq!(last, Some(s("[Function]")), "arrow stringifies");

    let (_, last) = run("add = (a, b) => a + b; add(2, 3)", obj(vec![]));
    assert_eq!(last, Some(num(5.0)));

    let (_, last) = run("square = x => x * x; square(4)", obj(vec![]));
    assert_eq!(last, Some(num(16.0)));

    let (_, last) = run("one = () => 42; one()", obj(vec![]));
    assert_eq!(last, Some(num(42.0)));
}

#[test]
fn arrow_block_body_and_return() {
    let code = "f = (n) => { return n + 1 }; f(10)";
    let (_, last) = run(code, obj(vec![]));
    assert_eq!(last, Some(num(11.0)));

    let code = "g = () => { return; }; g()";
    let (_, last) = run(code, obj(vec![]));
    assert_eq!(last, Some(Value::Null));

    let code = "h = (n) => { if (n > 0) return n; }; h(0)";
    let (_, last) = run(code, obj(vec![]));
    assert_eq!(last, Some(Value::Null));
}

#[test]
fn arrow_arg_arity_rules() {
    let (_, last) = run("f = (a) => a; f()", obj(vec![]));
    assert_eq!(last, Some(Value::Null), "missing arg is null");

    let (_, last) = run("f = (a) => a; f(1, 2, 3)", obj(vec![]));
    assert_eq!(last, Some(num(1.0)), "extra args ignored");
}

#[test]
fn arrow_lexical_closure() {
    let code = "makeAdder = x => (y => x + y); add5 = makeAdder(5); add5(3)";
    let (_, last) = run(code, obj(vec![]));
    assert_eq!(last, Some(num(8.0)));

    let code = "counter = () => { n = n + 1; return n }; n = 0; counter(); counter()";
    let (_, last) = run(code, obj(vec![]));
    assert_eq!(last, Some(num(2.0)), "closure mutates shared global");
}

#[test]
fn arrow_recursion_via_named_variable() {
    let code = "fact = n => { if (n <= 1) return 1; return n * fact(n - 1) }; fact(5)";
    let (_, last) = run(code, obj(vec![]));
    assert_eq!(last, Some(num(120.0)));
}

#[test]
fn builtins_can_be_shadowed() {
    let mut output = Vec::new();
    let code = "log = x => x + 1; log(10)";
    let (_, last) =
        lang::execute_with_output(code, obj(vec![]), 1000, &mut output).unwrap();
    assert_eq!(last, Some(num(11.0)));
    assert_eq!(
        String::from_utf8(output).unwrap(),
        "",
        "shadowed log must not write output"
    );

    let (_, last) = run("env = x => x; env(9)", obj(vec![]));
    assert_eq!(last, Some(num(9.0)));
}

#[test]
fn function_value_behaviors() {
    let (r, last) = run("f = x => x; $.f = f; typeof f", obj(vec![]));
    assert_eq!(last, Some(s("function")));
    if let Value::Object(o) = &r {
        assert!(matches!(o.borrow().get("f"), Some(Value::Function(_))));
    } else {
        panic!("not object");
    }

    let (_, last) = run("f = () => 1; '' + f", obj(vec![]));
    assert_eq!(last, Some(s("[Function]")));

    let (r, _) = run("$.cb = () => 1", obj(vec![]));
    let out = jsonsh::jsonc::marshal(&r).unwrap();
    assert_eq!(out, "{\"cb\":null}", "function marshals to null");

    let (_, last) = run("f = () => 1; if (f) 1 else 0", obj(vec![]));
    assert_eq!(last, Some(num(1.0)), "function is truthy");
}

#[test]
fn object_method_call_arrow() {
    let code = "$.o.f = x => x + 1; $.o.f(10)";
    let (_, last) = run(code, obj(vec![("o", obj(vec![]))]));
    assert_eq!(last, Some(num(11.0)));
}

#[test]
fn top_level_return_sets_exit_code() {
    let (_, _, code) = execute_exit("return 5", obj(vec![]));
    assert_eq!(code, 5);
}

#[test]
fn top_level_return_defaults_clamps_and_truncates() {
    assert_eq!(execute_exit("return", obj(vec![])).2, 0);
    assert_eq!(execute_exit("return 0", obj(vec![])).2, 0);
    assert_eq!(execute_exit("return 300", obj(vec![])).2, 255);
    assert_eq!(execute_exit("return -5", obj(vec![])).2, 0);
    assert_eq!(execute_exit("return 3.9", obj(vec![])).2, 3);
}

#[test]
fn top_level_return_stops_execution_and_keeps_output() {
    let (root, _, code) = execute_exit("$.a = 1; return 2; $.b = 1", obj(vec![]));
    assert_eq!(code, 2);
    match &root {
        Value::Object(o) => {
            let o = o.borrow();
            assert_eq!(o.get("a"), Some(&num(1.0)));
            assert!(!o.contains_key("b"));
        }
        _ => panic!("root must be object"),
    }
}

#[test]
fn top_level_return_requires_a_number() {
    let e = execute("return \"abc\"", obj(vec![]), 100);
    assert!(e.is_err(), "non-number return should error");
}

#[test]
fn function_return_does_not_set_exit_code() {
    let (_, last, code) = execute_exit("f = () => { return 7 }; f()", obj(vec![]));
    assert_eq!(code, 0);
    assert_eq!(last, Some(num(7.0)));
}

#[test]
fn typeof_expression_and_function() {
    let (_, last) = run("typeof (() => 1)", obj(vec![]));
    assert_eq!(last, Some(s("function")));

    let (_, last) = run("typeof typeof 1", obj(vec![]));
    assert_eq!(last, Some(s("string")));
}

#[test]
fn arrow_return_propagates_through_loop() {
    let code = "f = () => { for (i = 0; i < 5; i += 1) { if (i == 3) return i; } return -1 }; f()";
    let (_, last) = run(code, obj(vec![]));
    assert_eq!(last, Some(num(3.0)));
}

#[test]
fn arrow_empty_block_body_returns_null() {
    let (_, last) = run("f = () => {}; f()", obj(vec![]));
    assert_eq!(last, Some(Value::Null));
}

#[test]
fn arrow_block_without_return_returns_null() {
    let (_, last) = run("f = (x) => { x + 1 }; f(10)", obj(vec![]));
    assert_eq!(last, Some(Value::Null));
}

#[test]
fn arrow_return_first_wins() {
    let (_, last) = run("f = () => { return 1; return 2 }; f()", obj(vec![]));
    assert_eq!(last, Some(num(1.0)));
}

#[test]
fn arrow_function_reference_identity_and_equality() {
    let (_, last) = run("f = () => 1; g = f; g == f", obj(vec![]));
    assert_eq!(last, Some(Value::Bool(true)));

    let (_, last) = run("a = () => 1; b = () => 1; a == b", obj(vec![]));
    assert_eq!(last, Some(Value::Bool(false)));
}

#[test]
fn arrow_passed_as_callback() {
    let code = "apply = (f, x) => f(x); apply(y => y * 10, 5)";
    let (_, last) = run(code, obj(vec![]));
    assert_eq!(last, Some(num(50.0)));

    let code = "map = (f, a) => { for (i = 0; i < a.length; i += 1) a[i] = f(a[i]); return a }; map(x => x * 10, [1, 2, 3])";
    let (_, last) = run(code, obj(vec![]));
    assert_eq!(last, Some(arr(vec![num(10.0), num(20.0), num(30.0)])));
}

#[test]
fn arrow_closure_captures_outer_variables() {
    let code = "make = (base) => (x => base + x); add10 = make(10); add10(5)";
    let (_, last) = run(code, obj(vec![]));
    assert_eq!(last, Some(num(15.0)));
}

#[test]
fn arrow_captures_variable_by_reference() {
    let (_, last) = run("x = 10; f = () => x; x = 20; f()", obj(vec![]));
    assert_eq!(last, Some(num(20.0)));
}

#[test]
fn arrow_parameter_shadows_outer() {
    let (_, last) = run("x = 1; f = (x) => { return x }; f(99)", obj(vec![]));
    assert_eq!(last, Some(num(99.0)));
    let (_, last) = run("x = 1; f = (x) => { return x }; f(99); x", obj(vec![]));
    assert_eq!(last, Some(num(1.0)), "outer x unchanged after param shadow");
}

#[test]
fn arrow_assignment_to_param_is_local() {
    let (_, last) = run("f = (a) => { a = 99; return a }; f(1)", obj(vec![]));
    assert_eq!(last, Some(num(99.0)));
}

#[test]
fn arrow_nested_closure() {
    let code = "f = x => y => z => x + y + z; g = f(1); h = g(2); h(3)";
    let (_, last) = run(code, obj(vec![]));
    assert_eq!(last, Some(num(6.0)));
}

#[test]
fn arrow_return_object_literal() {
    let code = "f = () => ({ a: 1 }); f().a";
    let (_, last) = run(code, obj(vec![]));
    assert_eq!(last, Some(num(1.0)));
}

#[test]
fn non_function_value_called_is_runtime_error() {
    let e = execute("x = 1; x()", obj(vec![]), 100);
    assert!(e.is_err(), "calling non-function should error");

    let e = execute("log = 5; log(1)", obj(vec![]), 100);
    assert!(e.is_err(), "shadowing with non-function then calling should error");
}

#[test]
fn break_continue_outside_loop_inside_function_errors() {
    let e = execute("f = () => { break }; f()", obj(vec![]), 100);
    assert!(e.is_err(), "break outside loop should error");

    let e = execute("f = () => { continue }; f()", obj(vec![]), 100);
    assert!(e.is_err(), "continue outside loop should error");
}

#[test]
fn top_level_return_in_if_sets_exit_code() {
    let (_, _, code) = execute_exit("if (true) return 3", obj(vec![]));
    assert_eq!(code, 3);
}

#[test]
fn max_call_depth_exceeded() {
    let e = execute("f = () => f(); f()", obj(vec![]), 1_000_000);
    match e {
        Err(err) => assert!(
            err.to_string().contains("maximum call stack depth exceeded"),
            "{:?}",
            err
        ),
        Ok(_) => panic!("should error on infinite recursion"),
    }
}

#[test]
fn typeof_function_returns_function() {
    let cases = [
        ("typeof (() => 1)", "function"),
        ("typeof (x => x)", "function"),
        ("typeof log", "function"),
        ("typeof ((a, b) => a + b)", "function"),
    ];
    for (code, want) in cases {
        let (_, last) = run(code, obj(vec![]));
        assert_eq!(last, Some(s(want)), "{}", code);
    }
}

#[test]
fn typeof_preserves_existing_types() {
    let cases = [
        ("typeof 1", "number"),
        ("typeof 'x'", "string"),
        ("typeof true", "boolean"),
        ("typeof null", "object"),
        ("typeof [1]", "array"),
        ("typeof ({})", "object"),
    ];
    for (code, want) in cases {
        let (_, last) = run(code, obj(vec![]));
        assert_eq!(last, Some(s(want)), "{}", code);
    }
}

#[test]
fn function_in_array_join_and_log() {
    let (_, last) = run("[() => 1, 2].join(',')", obj(vec![]));
    assert_eq!(last, Some(s("[Function],2")));

    let mut output = Vec::new();
    lang::execute_with_output("log(() => 1)", obj(vec![]), 1000, &mut output).unwrap();
    assert_eq!(
        String::from_utf8(output).unwrap(),
        "[Function]\n",
        "logging a function prints [Function]"
    );
}

#[test]
fn function_in_compact_output_is_null() {
    let (r, _) = run("$.fns = [() => 1, () => 2]", obj(vec![]));
    let out = jsonsh::jsonc::marshal(&r).unwrap();
    assert_eq!(out, "{\"fns\":[null,null]}");
}

#[test]
fn arrow_as_loop_body() {
    let (r, _) = run(
        "for (i = 0; i < 3; i += 1) f = i; $.last = f",
        obj(vec![]),
    );
    if let Value::Object(o) = &r {
        assert_eq!(o.borrow().get("last"), Some(&num(2.0)));
    } else {
        panic!("not object");
    }
}

#[test]
fn arrow_return_propagates_through_nested_if() {
    let code = "f = (x) => { if (x > 0) { if (x > 10) return 'big'; return 'small'; } return 'neg' }; f(5)";
    let (_, last) = run(code, obj(vec![]));
    assert_eq!(last, Some(s("small")));
    let (_, last) = run(&code.replace("f(5)", "f(20)"), obj(vec![]));
    assert_eq!(last, Some(s("big")));
}

#[test]
fn arrow_return_through_for_of() {
    let code = "f = () => { for (c of 'ab') return c }; f()";
    let (_, last) = run(code, obj(vec![]));
    assert_eq!(last, Some(s("a")));
}

#[test]
fn arrow_deep_recursion_within_limit_succeeds() {
    let code = "f = (n) => { if (n <= 0) return 0; return 1 + f(n - 1) }; f(50)";
    let (_, last) = run(code, obj(vec![]));
    assert_eq!(last, Some(num(50.0)));
}

#[test]
fn arrow_accesses_root_dollar() {
    let (_, last) = run("f = () => $.x; f()", obj(vec![("x", num(42.0))]));
    assert_eq!(last, Some(num(42.0)));
}

#[test]
fn arrow_mutates_root() {
    let (r, _) = run("f = () => { $.a = 1 }; f()", obj(vec![]));
    if let Value::Object(o) = &r {
        assert_eq!(o.borrow().get("a"), Some(&num(1.0)));
    } else {
        panic!("not object");
    }
}

#[test]
fn arrow_builtin_object_keys_still_work() {
    let (_, last) = run("Object.keys({ b: 1, a: 2 })", obj(vec![]));
    assert_eq!(last, Some(arr(vec![s("a"), s("b")])));
}

#[test]
fn builtin_function_is_truthy() {
    let (_, last) = run("if (log) 1 else 0", obj(vec![]));
    assert_eq!(last, Some(num(1.0)));
}

#[test]
fn arrow_multiple_params_and_string_concat() {
    let (_, last) = run("greet = (name, age) => name + ':' + age; greet('Tom', 30)", obj(vec![]));
    assert_eq!(last, Some(s("Tom:30")));
}

#[test]
fn regex_literals_and_regexp_constructor() {
    let (r, _) = run(
        r#"
		$.lit = /\d+/g.source;
		$.flags = /a/im.flags;
		$.g = /a/g.global;
		$.i = /a/i.ignoreCase;
		$.m = /a/m.multiline;
		$.made = RegExp("\\d+", "g").source;
	"#,
        obj(vec![]),
    );
    let want = obj(vec![
        ("lit", s("\\d+")),
        ("flags", s("im")),
        ("g", Value::Bool(true)),
        ("i", Value::Bool(true)),
        ("m", Value::Bool(true)),
        ("made", s("\\d+")),
    ]);
    assert_eq!(r, want);
}

#[test]
fn regex_test_and_exec() {
    let (r, _) = run(
        r#"
		$.t = /[a-z]+/.test("abc");
		$.f = /\d+/.test("abc");
		$.e = /(\w+)-(\d+)/.exec("id-42");
		$.none = /z/.exec("abc");
	"#,
        obj(vec![]),
    );
    if let Value::Object(o) = &r {
        let m = o.borrow();
        assert_eq!(m.get("t"), Some(&Value::Bool(true)));
        assert_eq!(m.get("f"), Some(&Value::Bool(false)));
        assert_eq!(m.get("none"), Some(&Value::Null));
        if let Some(Value::Object(e)) = m.get("e") {
            let em = e.borrow();
            assert_eq!(em.get("0"), Some(&s("id-42")));
            assert_eq!(em.get("1"), Some(&s("id")));
            assert_eq!(em.get("2"), Some(&s("42")));
            assert_eq!(em.get("index"), Some(&num(0.0)));
            assert_eq!(em.get("input"), Some(&s("id-42")));
        } else {
            panic!("exec result missing");
        }
    } else {
        panic!("root not object");
    }
}

#[test]
fn regex_string_methods() {
    let (r, _) = run(
        r#"
		$.m1 = "id-42".match(/([a-z]+)-(\d+)/);
		$.m2 = "a1 a2".match(/a(\d)/g);
		$.r1 = "a1 a2".replace(/a(\d)/, "x$1");
		$.r2 = "a1 a2".replaceAll(/a(\d)/g, "x$1");
		$.s = "a, b;c".split(/[,;]\s*/);
		$.scap = "a-b-c".split(/(-)/);
	"#,
        obj(vec![]),
    );
    let want = obj(vec![
        ("m1", arr(vec![s("id-42"), s("id"), s("42")])),
        ("m2", arr(vec![s("a1"), s("a2")])),
        ("r1", s("x1 a2")),
        ("r2", s("x1 x2")),
        ("s", arr(vec![s("a"), s("b"), s("c")])),
        (
            "scap",
            arr(vec![s("a"), s("-"), s("b"), s("-"), s("c")]),
        ),
    ]);
    assert_eq!(r, want);
}

#[test]
fn regex_replacement_expansion() {
    let (r, _) = run(
        r#"$.a = "ab".replace(/b/, "[$&][$`][$']");"#,
        obj(vec![]),
    );
    if let Value::Object(o) = &r {
        assert_eq!(o.borrow().get("a"), Some(&s("a[b][a][]")));
    } else {
        panic!("root not object");
    }
}

#[test]
fn regex_division_disambiguation() {
    let (r, _) = run(
        r#"$.div = 10 / 2 / 1; $.lit = /a/g.source;"#,
        obj(vec![]),
    );
    if let Value::Object(o) = &r {
        let m = o.borrow();
        assert_eq!(m.get("div"), Some(&num(5.0)));
        assert_eq!(m.get("lit"), Some(&s("a")));
    } else {
        panic!("root not object");
    }
}

#[test]
fn regex_invalid_pattern_errors() {
    let cases = vec![
        ("/[/;", "unterminated"),
        ("RegExp(\"(\");", "unterminated"),
        ("RegExp(\"a\", \"x\");", "invalid regex flag"),
        ("\"x\".matchAll(/a/);", "g flag"),
        ("\"x\".replaceAll(/a/, \"b\");", "g flag"),
        ("\"x\".matchAll(\"a\");", "requires a regular expression"),
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
fn regex_case_insensitive_and_multiline() {
    let (r, _) = run(
        r#"$.ci = /abc/i.test("ABC"); $.m = /^b/m.test("a\nb"); $.nm = /^b/.test("a\nb");"#,
        obj(vec![]),
    );
    if let Value::Object(o) = &r {
        let m = o.borrow();
        assert_eq!(m.get("ci"), Some(&Value::Bool(true)));
        assert_eq!(m.get("m"), Some(&Value::Bool(true)));
        assert_eq!(m.get("nm"), Some(&Value::Bool(false)));
    } else {
        panic!("root not object");
    }
}
