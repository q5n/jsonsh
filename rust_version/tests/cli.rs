use std::io::{Cursor, Read};

use jsonsh::cli::{self, Input};

struct FailingReader;

impl Read for FailingReader {
    fn read(&mut self, _buf: &mut [u8]) -> std::io::Result<usize> {
        Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            "input should not be read",
        ))
    }
}

fn args(a: &[&str]) -> Vec<String> {
    a.iter().map(|s| s.to_string()).collect()
}

fn run(a: &[&str], input: Input<impl Read>, out: &mut Vec<u8>) -> Result<(), String> {
    cli::run(&args(a), input, out)
}

fn stdout_str(out: &[u8]) -> String {
    String::from_utf8(out.to_vec()).unwrap()
}

#[test]
fn run_mutation_and_result() {
    let mut out = Vec::new();
    run(&["-e", "$.n += 2", "-c"], Input::stream(Cursor::new("{\"n\":3}")), &mut out).unwrap();
    assert_eq!(stdout_str(&out), "{\"n\":5}\n");

    out.clear();
    run(
        &["-e", "$.items.length", "-r", "-c"],
        Input::stream(Cursor::new("{\"items\":[1,2]}")),
        &mut out,
    )
    .unwrap();
    assert_eq!(stdout_str(&out), "2\n");
}

#[test]
fn run_empty_expression_does_nothing() {
    let src = "{\n  // keep this comment\n  \"items\": [1, 2],\n}\n";
    let mut out = Vec::new();
    run(&["-e", ""], Input::stream(Cursor::new(src)), &mut out).unwrap();
    assert_eq!(stdout_str(&out), src);

    out.clear();
    run(
        &["--expression", "", "--compact"],
        Input::stream(Cursor::new(src)),
        &mut out,
    )
    .unwrap();
    assert_eq!(stdout_str(&out), "{\"items\":[1,2]}\n");

    out.clear();
    run(
        &["-e", "", "--result", "--compact"],
        Input::stream(Cursor::new(src)),
        &mut out,
    )
    .unwrap();
    assert_eq!(stdout_str(&out), "null\n");
}

#[test]
fn script_file_supports_newline_separated_statements() {
    let script_path = temp_path("update.js");
    let code = "\n$.count += 2\n$.name = $.name\n  .padEnd(4, \"!\")\n$.ready = true\n";
    std::fs::write(&script_path, code).unwrap();

    let mut out = Vec::new();
    run(
        &["-f", script_path.to_str().unwrap(), "-c"],
        Input::stream(Cursor::new("{\"count\":1,\"name\":\"go\"}")),
        &mut out,
    )
    .unwrap();
    assert_eq!(stdout_str(&out), "{\"count\":3,\"name\":\"go!!\",\"ready\":true}\n");

    let _ = std::fs::remove_file(&script_path);
}

#[test]
fn null_input_initializes_root_without_reading() {
    let mut out = Vec::new();
    run(
        &["-e", "$ = {ready: true}", "-c"],
        Input::terminal(FailingReader),
        &mut out,
    )
    .unwrap();
    assert_eq!(stdout_str(&out), "{\"ready\":true}\n");

    out.clear();
    run(
        &["-e", "$", "-r", "-c"],
        Input::terminal(FailingReader),
        &mut out,
    )
    .unwrap();
    assert_eq!(stdout_str(&out), "null\n");
}

#[test]
fn null_input_can_return_object_literal() {
    let mut out = Vec::new();
    run(
        &["-r", "-e", "{age:18}"],
        Input::terminal(FailingReader),
        &mut out,
    )
    .unwrap();
    assert_eq!(stdout_str(&out), "{\n  \"age\": 18\n}\n");
}

#[test]
fn combined_short_options() {
    let cases = vec![
        (vec!["-re", "{age:18}"], "{\n  \"age\": 18\n}\n"),
        (vec!["-re{age:18}"], "{\n  \"age\": 18\n}\n"),
        (vec!["-re", "-1"], "-1\n"),
    ];
    for (a, want) in cases {
        let mut out = Vec::new();
        run(&a, Input::terminal(FailingReader), &mut out).unwrap();
        assert_eq!(stdout_str(&out), want, "args {:?}", a);
    }
}

#[test]
fn log_output_precedes_processed_json() {
    let mut out = Vec::new();
    run(
        &["-e", "log(\"created\", 1); $ = {ok:true}", "-c"],
        Input::terminal(FailingReader),
        &mut out,
    )
    .unwrap();
    assert_eq!(stdout_str(&out), "created 1\n{\"ok\":true}\n");
}

#[test]
fn no_output_suppresses_final_value_but_keeps_log() {
    for a in [
        vec!["-n", "-e", "log(\"visible\", 2); $ = {hidden:true}"],
        vec!["-ne", "log(\"visible\", 2); $ = {hidden:true}"],
    ] {
        let mut out = Vec::new();
        run(&a, Input::terminal(FailingReader), &mut out).unwrap();
        assert_eq!(stdout_str(&out), "visible 2\n", "args {:?}", a);
    }

    let mut out = Vec::new();
    run(
        &["--no-output", "-e", "$ = 1"],
        Input::terminal(FailingReader),
        &mut out,
    )
    .unwrap();
    assert!(out.is_empty());
}

#[test]
fn no_output_rejects_explicit_output_targets() {
    for a in [
        vec!["-n", "-e", "$", "-o", "out.json"],
        vec!["-n", "-e", "$", "-i", "input.json"],
    ] {
        let mut out = Vec::new();
        let e = run(&a, Input::stream(FailingReader), &mut out);
        match e {
            Err(err) => assert!(err.contains("no-output cannot be used"), "args {:?} err {}", a, err),
            Ok(_) => panic!("should error for {:?}", a),
        }
    }
}

#[test]
fn combined_short_options_preserve_single_dash_long_flags() {
    for option in ["-h", "-help", "--help"] {
        let mut out = Vec::new();
        run(&[option], Input::stream(FailingReader), &mut out).unwrap();
        assert!(stdout_str(&out).contains("Usage:"), "option {}", option);
    }
}

#[test]
fn empty_redirected_input_is_not_treated_as_no_input() {
    let mut out = Vec::new();
    let e = run(&["-e", "$"], Input::stream(Cursor::new("")), &mut out);
    assert!(e.is_err());
}

#[test]
fn obsolete_options_are_removed() {
    for option in ["-q", "--null-input"] {
        let mut out = Vec::new();
        let e = run(
            &[option, "-e", "$"],
            Input::terminal(FailingReader),
            &mut out,
        );
        match e {
            Err(err) => assert!(err.contains("flag provided but not defined"), "option {}", option),
            Ok(_) => panic!("should error for {}", option),
        }
    }
}

#[test]
fn run_rejects_trailing_json() {
    let mut out = Vec::new();
    let e = run(&["-e", "x=1"], Input::stream(Cursor::new("{} {}")), &mut out);
    assert!(e.is_err());
}

#[test]
fn help() {
    for option in ["-h", "-help", "--help"] {
        let mut out = Vec::new();
        run(&[option], Input::stream(Cursor::new("")), &mut out).unwrap();
        let text = stdout_str(&out);
        assert!(text.contains(&format!("jsonsh {} -", cli::VERSION)));
        assert!(text.contains("Usage:"));
        assert!(text.contains("--max-steps"));
        assert!(text.contains("--version"));
        assert!(text.contains("--syntax"));
        assert!(text.contains("--pretty"));
        assert!(text.contains("--no-output"));
        assert!(text.contains("JSON/JSONC"));
        assert!(text.contains("$ = value"));
        for hidden in ["Properties:", "Built-in functions:", "String methods:", "Array methods:"] {
            assert!(!text.contains(hidden), "option {} has hidden {}", option, hidden);
        }
    }
}

#[test]
fn language_help() {
    let mut out = Vec::new();
    run(&["--syntax"], Input::stream(Cursor::new("not JSON")), &mut out).unwrap();
    let text = stdout_str(&out);
    for want in [
        "scripting language reference",
        "Values and literals:",
        "Operators, from lowest",
        "for (value of array)",
        "log(value, ...)",
        "env(name)",
        "typeof(value)",
        "string.length",
        "array.length",
        "toLowerCase()",
        "lastIndexOf(text[, start])",
        "padStart(targetLength[, padString])",
        "padEnd(targetLength[, padString])",
        "matchAll(pattern)",
        "replaceAll(pattern, replacement)",
        "splice(start[, deleteCount, ...items])",
        "reverse()",
        "lastIndexOf(value[, start])",
        "Go regular expressions",
        "typeof(null)",
    ] {
        assert!(text.contains(want), "missing {}", want);
    }
}

#[test]
fn no_arguments_shows_help() {
    let mut out = Vec::new();
    run(&[], Input::stream(Cursor::new("")), &mut out).unwrap();
    let text = stdout_str(&out);
    assert!(text.contains(&format!("jsonsh {} -", cli::VERSION)));
    assert!(text.contains("Usage:"));
}

#[test]
fn version() {
    for option in ["-v", "--version"] {
        let mut out = Vec::new();
        run(&[option], Input::stream(Cursor::new("")), &mut out).unwrap();
        assert_eq!(stdout_str(&out), format!("jsonsh {}\n", cli::VERSION));
    }
}

#[test]
fn run_preserves_jsonc_structure_by_default() {
    let src = "{\r\n\t// keep\r\n\t\"price\" : 100,\r\n\t\"name\": \"book\"\r\n}\r\n";
    let mut out = Vec::new();
    run(&["-e", "$.price = 80"], Input::stream(Cursor::new(src)), &mut out).unwrap();
    assert_eq!(stdout_str(&out), src.replace("100", "80"));
}

#[test]
fn run_jsonc_output_modes() {
    let src = "{/* note */\"a\":1}";
    let mut out = Vec::new();
    run(
        &["-e", "$.a = 2", "--compact"],
        Input::stream(Cursor::new(src)),
        &mut out,
    )
    .unwrap();
    assert_eq!(stdout_str(&out), "{\"a\":2}\n");

    out.clear();
    run(
        &["-e", "$.a = 2", "--pretty"],
        Input::stream(Cursor::new(src)),
        &mut out,
    )
    .unwrap();
    assert!(stdout_str(&out).contains("/* note */"));

    let mut out = Vec::new();
    let e = run(
        &["-e", "$.a = 2", "--pretty", "--compact"],
        Input::stream(Cursor::new(src)),
        &mut out,
    );
    assert!(e.is_err());
}

#[test]
fn run_push_preserves_existing_array_content() {
    let src = "{\n  \"items\": [\n    1 // existing\n  ]\n}\n";
    let mut out = Vec::new();
    run(&["-e", "$.items.push(2)"], Input::stream(Cursor::new(src)), &mut out).unwrap();
    let text = stdout_str(&out);
    assert!(text.contains("1, // existing"));
    assert!(text.contains("2"));
    jsonsh::jsonc::parse(text).unwrap();
}

#[test]
fn run_can_replace_root_and_keeps_outer_trivia() {
    let src = "// before\n{\"old\":true}\n// after\n";
    let mut out = Vec::new();
    run(&["-e", "$ = [1, 2]"], Input::stream(Cursor::new(src)), &mut out).unwrap();
    assert_eq!(stdout_str(&out), "// before\n[1,2]\n// after\n");
}

#[test]
fn run_result_and_compact_preserve_unicode_escapes() {
    let src = "{\"icon\":\"\\uee63\",\"name\":\"Ubuntu 24.04.1 LTS\"}";
    let mut out = Vec::new();
    run(&["-e", "$.hidden = true", "-c"], Input::stream(Cursor::new(src)), &mut out).unwrap();
    assert_eq!(
        stdout_str(&out),
        "{\"hidden\":true,\"icon\":\"\\uee63\",\"name\":\"Ubuntu 24.04.1 LTS\"}\n"
    );

    out.clear();
    run(&["-e", "$.icon", "-r", "-c"], Input::stream(Cursor::new(src)), &mut out).unwrap();
    assert_eq!(stdout_str(&out), "\"\\uee63\"\n");
}

fn temp_path(name: &str) -> std::path::PathBuf {
    let uniq = format!(
        "{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    std::env::temp_dir().join(format!("jsonsh-{}-{}", uniq, name))
}

#[test]
fn in_place_replaces_input_file() {
    let path = temp_path("inplace.json");
    std::fs::write(&path, "{\n  \"price\": 100\n}").unwrap();
    let mut out = Vec::new();
    run(
        &["-e", "$.price = 80", "-i", path.to_str().unwrap()],
        Input::stream(FailingReader),
        &mut out,
    )
    .unwrap();
    let result = std::fs::read_to_string(&path).unwrap();
    assert_eq!(result, "{\n  \"price\": 80\n}");
    let _ = std::fs::remove_file(&path);
}

#[test]
fn output_file_writes_result() {
    let path = temp_path("out.json");
    let mut out = Vec::new();
    run(
        &["-e", "$.price = 80", "-o", path.to_str().unwrap()],
        Input::stream(Cursor::new("{\"price\":100}")),
        &mut out,
    )
    .unwrap();
    let result = std::fs::read_to_string(&path).unwrap();
    assert_eq!(result, "{\"price\":80}");
    let _ = std::fs::remove_file(&path);
}

#[test]
fn result_mode_preserves_chinese_utf8() {
    let mut out = Vec::new();
    run(
        &["-r", "-e", "{姓名: \"张三\", 年龄: 18}"],
        Input::terminal(FailingReader),
        &mut out,
    )
    .unwrap();
    assert_eq!(stdout_str(&out), "{\n  \"姓名\": \"张三\",\n  \"年龄\": 18\n}\n");
}

#[test]
fn preserve_mode_round_trips_chinese_comments_and_values() {
    let src = "{\n  // 商品价格\n  \"价格\": 100,\n  \"名称\": \"中文书籍\"\n}";
    let mut out = Vec::new();
    run(&["-e", "$.价格 = 80"], Input::stream(Cursor::new(src)), &mut out).unwrap();
    assert_eq!(stdout_str(&out), src.replace("100", "80"));
}

#[test]
fn compact_mode_preserves_chinese_utf8() {
    let mut out = Vec::new();
    run(
        &["-e", "$.中文键 = \"中文值\"", "-c"],
        Input::stream(Cursor::new("{}")),
        &mut out,
    )
    .unwrap();
    assert_eq!(stdout_str(&out), "{\"中文键\":\"中文值\"}\n");
}
