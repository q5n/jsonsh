use std::fs;
use std::io::{Read, Write};

use crate::jsonc;
use crate::lang;
use crate::value::Value;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

pub struct Input<R: Read> {
    pub reader: R,
    pub terminal: bool,
}

impl<R: Read> Input<R> {
    pub fn stream(reader: R) -> Self {
        Input {
            reader,
            terminal: false,
        }
    }

    pub fn terminal(reader: R) -> Self {
        Input {
            reader,
            terminal: true,
        }
    }
}

#[derive(Default)]
struct Options {
    expr: Option<String>,
    script: Option<String>,
    output: Option<String>,
    result: bool,
    compact: bool,
    pretty: bool,
    in_place: bool,
    no_output: bool,
    show_version: bool,
    syntax_help: bool,
    max_steps: usize,
    expr_set: bool,
    script_set: bool,
}

pub fn run<R: Read, W: Write>(
    args: &[String],
    mut input: Input<R>,
    stdout: &mut W,
) -> Result<(), String> {
    let args = expand_short_options(args);
    let mut o = Options {
        max_steps: 1_000_000,
        ..Default::default()
    };
    let mut positional: Vec<String> = Vec::new();

    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        if arg == "--" {
            for a in &args[i + 1..] {
                positional.push(a.clone());
            }
            break;
        }
        if arg == "-" || !arg.starts_with('-') {
            for a in &args[i..] {
                positional.push(a.clone());
            }
            break;
        }
        let (name, inline) = split_flag(arg);
        match name.as_str() {
            "h" | "help" => {
                print_usage(stdout)?;
                return Ok(());
            }
            "v" | "version" => {
                o.show_version = true;
                i += 1;
            }
            "syntax" => {
                o.syntax_help = true;
                i += 1;
            }
            "r" | "result" => {
                o.result = true;
                i += 1;
            }
            "c" | "compact" => {
                o.compact = true;
                i += 1;
            }
            "p" | "pretty" => {
                o.pretty = true;
                i += 1;
            }
            "i" | "in-place" => {
                o.in_place = true;
                i += 1;
            }
            "n" | "no-output" => {
                o.no_output = true;
                i += 1;
            }
            "e" | "expression" => {
                o.expr_set = true;
                let (v, consumed) = take_value(inline, &name, &args, i)?;
                o.expr = Some(v);
                i += consumed;
            }
            "f" | "script" => {
                o.script_set = true;
                let (v, consumed) = take_value(inline, &name, &args, i)?;
                o.script = Some(v);
                i += consumed;
            }
            "o" | "output" => {
                let (v, consumed) = take_value(inline, &name, &args, i)?;
                o.output = Some(v);
                i += consumed;
            }
            "max-steps" => {
                let (v, consumed) = take_value(inline, &name, &args, i)?;
                o.max_steps = v
                    .parse()
                    .map_err(|_| format!("invalid value {:?} for flag -max-steps", v))?;
                i += consumed;
            }
            _ => return Err(format!("flag provided but not defined: {}", name)),
        }
    }

    if args.is_empty() {
        print_usage(stdout)?;
        return Ok(());
    }
    if o.show_version {
        writeln!(stdout, "jsonsh {}", VERSION).map_err(|e| e.to_string())?;
        return Ok(());
    }
    if o.syntax_help {
        print_language_help(stdout)?;
        return Ok(());
    }
    if o.expr_set == o.script_set {
        return Err("exactly one of -e or -f is required".to_string());
    }
    if o.output.is_some() && o.in_place {
        return Err("-o and -i are mutually exclusive".to_string());
    }
    if o.no_output && (o.output.is_some() || o.in_place) {
        return Err("-n/--no-output cannot be used with -o/--output or -i/--in-place".to_string());
    }
    if o.compact && o.pretty {
        return Err("--compact and --pretty are mutually exclusive".to_string());
    }
    if positional.len() > 1 {
        return Err("only one input file is supported".to_string());
    }
    let input_file = positional.first().cloned().unwrap_or_default();
    if o.in_place && input_file.is_empty() {
        return Err("-i requires an input file".to_string());
    }
    if o.max_steps == 0 {
        return Err("--max-steps must be positive".to_string());
    }

    let mut code = o.expr.clone().unwrap_or_default();
    if let Some(script) = &o.script {
        if !script.is_empty() {
            code = fs::read_to_string(script).map_err(|e| format!("read script: {}", e))?;
        }
    }

    let raw: String;
    if input_file.is_empty() {
        if input.terminal {
            raw = "null".to_string();
        } else {
            let mut buf = String::new();
            input
                .reader
                .read_to_string(&mut buf)
                .map_err(|e| format!("read input: {}", e))?;
            raw = buf;
        }
    } else {
        raw = fs::read_to_string(&input_file).map_err(|e| format!("open input: {}", e))?;
    }

    let doc = jsonc::parse(raw).map_err(|e| e.to_string())?;
    let mut root = doc.root.value.deep_clone();
    let mut last: Option<Value> = None;
    if !code.is_empty() {
        let (r, l) = lang::execute_with_output(&code, root, o.max_steps, stdout)
            .map_err(|e| e.to_string())?;
        root = r;
        last = l;
    }
    if o.no_output {
        return Ok(());
    }

    let output = if o.result {
        let data = jsonc::marshal(last.as_ref().unwrap_or(&Value::Null))?;
        if o.compact {
            format!("{}\n", data)
        } else {
            format!("{}\n", indent_json(&data, "  "))
        }
    } else if o.compact {
        format!("{}\n", jsonc::compact(&root)?)
    } else {
        let mut out = doc.preserve(&root)?;
        if o.pretty {
            out = jsonc::pretty_preserve(&out, "  ")?;
            out.push('\n');
        }
        out
    };

    let data = output.as_bytes();
    if o.in_place {
        return replace_file(&input_file, data);
    }
    if let Some(path) = &o.output {
        return fs::write(path, data).map_err(|e| e.to_string());
    }
    stdout.write_all(data).map_err(|e| e.to_string())
}

fn take_value(
    inline: Option<String>,
    name: &str,
    args: &[String],
    i: usize,
) -> Result<(String, usize), String> {
    match inline {
        Some(v) => Ok((v, 1)),
        None => {
            if i + 1 < args.len() {
                Ok((args[i + 1].clone(), 2))
            } else {
                Err(format!("flag needs an argument: -{}", name))
            }
        }
    }
}

fn split_flag(arg: &str) -> (String, Option<String>) {
    if let Some(eq) = arg.find('=') {
        let name = arg[..eq].trim_start_matches('-').to_string();
        let value = arg[eq + 1..].to_string();
        (name, Some(value))
    } else {
        (arg.trim_start_matches('-').to_string(), None)
    }
}

fn expand_short_options(args: &[String]) -> Vec<String> {
    let boolean_options: [char; 7] = ['c', 'h', 'i', 'n', 'p', 'r', 'v'];
    let value_options: [char; 3] = ['e', 'f', 'o'];
    let single_dash_long: [&str; 12] = [
        "-compact",
        "-expression",
        "-help",
        "-in-place",
        "-max-steps",
        "-no-output",
        "-output",
        "-pretty",
        "-result",
        "-script",
        "-syntax",
        "-version",
    ];
    let long_value_options: [&str; 8] = [
        "-expression",
        "-max-steps",
        "-output",
        "-script",
        "--expression",
        "--max-steps",
        "--output",
        "--script",
    ];

    let mut expanded: Vec<String> = Vec::new();
    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        if arg == "--" || arg == "-" || !arg.starts_with('-') {
            expanded.extend_from_slice(&args[i..]);
            break;
        }
        if arg.starts_with("--") || single_dash_long.contains(&arg.as_str()) || arg.contains('=') {
            expanded.push(arg.clone());
            if long_value_options.contains(&arg.as_str()) && i + 1 < args.len() {
                i += 1;
                expanded.push(args[i].clone());
            }
            i += 1;
            continue;
        }
        if arg.len() <= 2 {
            expanded.push(arg.clone());
            if arg.len() == 2 && value_options.contains(&(arg.as_bytes()[1] as char)) && i + 1 < args.len() {
                i += 1;
                expanded.push(args[i].clone());
            }
            i += 1;
            continue;
        }

        let cluster = &arg[1..];
        let mut cluster_expansion: Vec<String> = Vec::new();
        let mut valid = true;
        let chars: Vec<char> = cluster.chars().collect();
        let mut j = 0;
        while j < chars.len() {
            let option = chars[j];
            if boolean_options.contains(&option) {
                cluster_expansion.push(format!("-{}", option));
                j += 1;
                continue;
            }
            if value_options.contains(&option) {
                cluster_expansion.push(format!("-{}", option));
                if j + 1 < chars.len() {
                    cluster_expansion.push(chars[j + 1..].iter().collect());
                } else if i + 1 < args.len() {
                    i += 1;
                    cluster_expansion.push(args[i].clone());
                }
                break;
            }
            valid = false;
            break;
        }
        if valid {
            expanded.extend(cluster_expansion);
        } else {
            expanded.push(arg.clone());
        }
        i += 1;
    }
    expanded
}

fn indent_json(src: &str, indent: &str) -> String {
    let mut out = String::new();
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escape = false;
    let mut chars = src.chars().peekable();
    while let Some(c) = chars.next() {
        if in_string {
            out.push(c);
            if escape {
                escape = false;
            } else if c == '\\' {
                escape = true;
            } else if c == '"' {
                in_string = false;
            }
            continue;
        }
        match c {
            '"' => {
                out.push('"');
                in_string = true;
            }
            '{' | '[' => {
                let close = if c == '{' { '}' } else { ']' };
                if let Some(&next) = chars.peek() {
                    if next == close {
                        out.push(c);
                        out.push(close);
                        chars.next();
                        continue;
                    }
                }
                out.push(c);
                depth += 1;
                out.push('\n');
                for _ in 0..depth {
                    out.push_str(indent);
                }
            }
            '}' | ']' => {
                depth -= 1;
                out.push('\n');
                for _ in 0..depth {
                    out.push_str(indent);
                }
                out.push(c);
            }
            ',' => {
                out.push(',');
                out.push('\n');
                for _ in 0..depth {
                    out.push_str(indent);
                }
            }
            ':' => {
                out.push(':');
                out.push(' ');
            }
            _ => out.push(c),
        }
    }
    out
}

fn replace_file(path: &str, data: &[u8]) -> Result<(), String> {
    let dir = std::path::Path::new(path)
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| std::path::PathBuf::from("."));
    let uniq = format!("{}-{}", std::process::id(), nanos());
    let tmp_path = dir.join(format!(".jsonsh-{}", uniq));
    let backup_path = dir.join(format!(".jsonsh-backup-{}", uniq));

    {
        let mut f = fs::File::create(&tmp_path).map_err(|e| e.to_string())?;
        f.write_all(data).map_err(|e| e.to_string())?;
        f.sync_all().map_err(|e| e.to_string())?;
    }

    fs::rename(path, &backup_path).map_err(|e| e.to_string())?;
    if let Err(e) = fs::rename(&tmp_path, path) {
        let _ = fs::rename(&backup_path, path);
        return Err(e.to_string());
    }
    let _ = fs::remove_file(&backup_path);
    Ok(())
}

fn nanos() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0)
}

fn print_usage<W: Write>(w: &mut W) -> Result<(), String> {
    write!(
        w,
        "jsonsh {} - process JSON/JSONC with JavaScript-like expressions\n\
\n\
Usage:\n\
  jsonsh (-e CODE | -f SCRIPT) [options] [INPUT]\n\
\n\
Boolean short options may be grouped. A value-taking option may appear last:\n\
  jsonsh -re \"{{age: 18}}\"\n\
\n\
If INPUT is omitted and standard input is redirected or piped, input is read\n\
from standard input. Otherwise, $ is initialized to null. Line comments, block\n\
comments, and trailing commas are supported. By default, only changed values\n\
are replaced, preserving the original formatting and comments.\n\
\n\
Scripts:\n\
  -e, --expression CODE  Execute the specified code\n\
  -f, --script FILE      Read code from a UTF-8 file\n\
\n\
Root variable:\n\
  $                       Current JSON root value\n\
  $ = value               Replace the entire JSON root value\n\
\n\
Output:\n\
  -r, --result            Print the last expression result (default: modified $)\n\
  -p, --pretty            Pretty-print output while preserving comments\n\
  -c, --compact           Print compact standard JSON without comments\n\
  -o, --output FILE       Write output to a file\n\
  -i, --in-place          Safely replace the input file\n\
  -n, --no-output         Suppress final output; log output remains visible\n\
\n\
Other:\n\
      --max-steps N       Maximum execution steps (default: 1000000)\n\
      --syntax             Show the scripting language reference\n\
  -v, --version           Show version\n\
  -h, -help, --help       Show help\n\
\n\
Examples:\n\
  jsonsh -e \"$.price *= 0.8\" input.json\n\
  jsonsh -e \"$.users.length\" -r input.json\n\
  jsonsh -f update.js -i input.json\n\
",
        VERSION
    )
    .map_err(|e| e.to_string())
}

fn print_language_help<W: Write>(w: &mut W) -> Result<(), String> {
    write!(
        w,
        "jsonsh {} scripting language reference\n\
\n\
Values and literals:\n\
  null, boolean, number, string, array, object, function\n\
  null  true  false  12.5  \"text\"  'text'  [1, 2]  {{name: \"Tom\"}}\n\
  Strings support common escapes and \\uXXXX. Arrays and objects allow trailing commas.\n\
\n\
Variables:\n\
  $                       Current JSON root value\n\
  $ = value               Replace the entire root value\n\
  name = value            Create or update a global variable\n\
  object.name             Access an object property\n\
  value[key]              Object property or array element; negative indexes\n\
                          count from the end, and assigning beyond length\n\
                          grows the array with null holes\n\
\n\
Functions:\n\
  (a, b) => expression    Arrow function returning the expression\n\
  (a, b) => {{ ... }}       Arrow function with a block body; use return\n\
  x => expression         Single parameter needs no parentheses\n\
  () => expression        Zero parameters\n\
  return [value]          Return from a function; return; gives null\n\
  Functions are lexical closures. Missing arguments are null; extra arguments\n\
  are ignored. A function stringifies as [Function], is truthy, and marshals\n\
  to null. log/env/keys are globals and may be reassigned.\n\
\n\
Statements:\n\
  expression;\n\
  {{ statements }}\n\
  if (condition) {{ ... }} else if (condition) {{ ... }} else {{ ... }}\n\
  for (key in object) {{ ... }}\n\
  for (index in array) {{ ... }}\n\
  for (value of array) {{ ... }}\n\
  for (character of string) {{ ... }}\n\
  for (init; condition; update) {{ ... }}\n\
  delete object.member;\n\
  break;\n\
  continue;\n\
\n\
  A control-flow body may also be a single brace-less statement, ending at\n\
  a semicolon, line break, or else; for example: if (x > 0) log(x) else log(0).\n\
  A dangling else binds to the nearest preceding if.\n\
\n\
  Statements are separated by semicolons or line breaks. Repeated semicolons\n\
  and semicolons after blocks are allowed. Expressions may continue across\n\
  lines when syntactically incomplete. In a traditional for loop, init,\n\
  condition, and update are each optional; an omitted condition loops forever\n\
  (subject to --max-steps). continue runs update before the next iteration;\n\
  break exits without running update.\n\
\n\
Operators, from lowest to highest precedence:\n\
  =  +=  -=  *=  /=\n\
  ||\n\
  &&\n\
  ==  !=\n\
  >  >=  <  <=\n\
  +  -\n\
  *  /\n\
  !  -  typeof value\n\
  member access and method calls\n\
\n\
  Assignments are right-associative. Logical operators short-circuit. The +\n\
  operator adds numbers, or converts both operands with toString() and\n\
  concatenates them when either operand is a string.\n\
\n\
Properties:\n\
  string.length                 Number of Unicode code points\n\
  array.length                  Number of array elements\n\
\n\
Built-in functions:\n\
  log(value, ...)               Print values separated by spaces\n\
  env(name)                     Read an environment variable, or null if unset\n\
  keys(value)                   Ordered object keys or numeric array indexes\n\
\n\
Operators include the prefix expression typeof value, which returns string,\n\
array, object, boolean, number, or function (typeof null is \"object\").\n\
\n\
String methods:\n\
  toString()\n\
  toLowerCase()\n\
  toUpperCase()\n\
  substring(start[, end])\n\
  indexOf(text[, start])\n\
  lastIndexOf(text[, start])\n\
  localeCompare(text)\n\
  padStart(targetLength[, padString])\n\
  padEnd(targetLength[, padString])\n\
  trim()\n\
  split(pattern[, limit])\n\
  match(pattern)\n\
  matchAll(pattern)\n\
  replace(pattern, replacement)\n\
  replaceAll(pattern, replacement)\n\
\n\
  Pattern arguments are strings containing Go regular expressions, not\n\
  JavaScript /pattern/flags literals. Replacement strings use Go expansion\n\
  syntax such as $1. match() returns the full match and capture groups, or null;\n\
  matchAll() returns an array of match arrays.\n\
\n\
Array methods:\n\
  toString()\n\
  push(value, ...)\n\
  reverse()\n\
  splice(start[, deleteCount, ...items])\n\
  join([separator])\n\
  indexOf(value[, start])\n\
  lastIndexOf(value[, start])\n\
\n\
  Array searches use recursive deep equality. Array and object variables retain\n\
  reference identity. for..of uses live array iteration, so splice, deletion,\n\
  and push can affect later iterations.\n\
\n\
Type conversion:\n\
  string, array, object, boolean, number, and null provide toString(). Objects\n\
  are encoded as compact single-line JSON. typeof(null) returns \"object\".\n\
\n\
Execution limits:\n\
  --max-steps limits evaluated statements and expressions. Reading an undefined\n\
  variable, using an invalid member type, an incompatible type, or an invalid\n\
  regular expression is a runtime error with a line and column position. Reading\n\
  a missing object property returns null.\n\
",
        VERSION
    )
    .map_err(|e| e.to_string())
}
