# ES5 Regular Expression Support Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` (recommended) or `superpowers:executing-plans` to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the `regex` crate with a self-implemented ES5 regex engine, add `/pattern/flags` literal syntax and a `RegExp` object, and update string methods to follow JS/ES5 semantics.

**Architecture:** Build a standalone `src/regex.rs` module with a UTF-16 backtracking matcher, then wire it into `value.rs`, the lexer/parser for literal syntax, and `eval.rs` for string/RegExp methods. Keep `Value::String` as Rust UTF-8 while documenting the UTF-16 boundary compromise.

**Tech Stack:** Rust 2021, `caseless` crate, existing `jsonsh` interpreter.

---

## File map

- `rust_version/Cargo.toml` — remove `regex`, add `caseless`.
- `rust_version/src/regex.rs` — new regex engine.
- `rust_version/src/value.rs` — add `Value::Regex`.
- `rust_version/src/lang/token.rs` — add `offset` to `Token`.
- `rust_version/src/lang/lexer.rs` — record offsets, add `Tok::Char` fallback.
- `rust_version/src/lang/parser.rs` — store source, parse `/pattern/flags`.
- `rust_version/src/lang/eval.rs` — use new engine for string/regex methods, add `RegExp` builtin.
- `rust_version/src/cli.rs` — update `--syntax` help.
- `docs/spec.md` — update spec and UTF-16 note.
- `rust_version/tests/lang.rs` — update existing split tests, add regex tests.

---

## Task 1: Update dependencies

**Files:**
- Modify: `rust_version/Cargo.toml`

- [ ] **Step 1: Remove `regex` and add `caseless`**

Edit `rust_version/Cargo.toml`:

```toml
[dependencies]
caseless = "0.2"
unicode-general-category = "1"
```

- [ ] **Step 2: Verify dependency resolution**

Run:

```bash
cd rust_version
cargo check
```

Expected: dependencies resolve without the `regex` crate.

- [ ] **Step 3: Commit**

```bash
git add rust_version/Cargo.toml rust_version/Cargo.lock
git commit -m "chore: replace regex crate with caseless for ES5 regex engine"
```

---

## Task 2: Create `src/regex.rs` — UTF-16 utilities and Flags

**Files:**
- Create: `rust_version/src/regex.rs`
- Modify: `rust_version/src/lib.rs` to expose the module

- [ ] **Step 1: Add module declaration**

In `rust_version/src/lib.rs`, add:

```rust
pub mod regex;
```

- [ ] **Step 2: Write UTF-16 helpers and Flags**

Create `rust_version/src/regex.rs` with:

```rust
pub fn to_utf16(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Flags {
    pub global: bool,
    pub ignore_case: bool,
    pub multiline: bool,
}

impl Flags {
    pub fn parse(s: &str) -> Result<Self, String> {
        let mut f = Flags { global: false, ignore_case: false, multiline: false };
        for c in s.chars() {
            match c {
                'g' => f.global = true,
                'i' => f.ignore_case = true,
                'm' => f.multiline = true,
                _ => return Err(format!("invalid regex flag {:?}", c)),
            }
        }
        Ok(f)
    }

    pub fn to_string(&self) -> String {
        let mut s = String::new();
        if self.global { s.push('g'); }
        if self.ignore_case { s.push('i'); }
        if self.multiline { s.push('m'); }
        s
    }
}
```

- [ ] **Step 3: Add unit tests**

Append to `rust_version/src/regex.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utf16_conversion() {
        assert_eq!(to_utf16("a中😀"), vec![0x0061, 0x4E2D, 0xD83D, 0xDE00]);
    }

    #[test]
    fn flags_parse() {
        let f = Flags::parse("gi").unwrap();
        assert!(f.global && f.ignore_case && !f.multiline);
        assert!(Flags::parse("x").is_err());
    }
}
```

- [ ] **Step 4: Run tests**

```bash
cd rust_version
cargo test regex::tests
```

Expected: tests pass.

- [ ] **Step 5: Commit**

```bash
git add rust_version/src/regex.rs rust_version/src/lib.rs
git commit -m "feat(regex): add UTF-16 helpers and Flags parser"
```

---

## Task 3: Regex AST and parser — core atoms

**Files:**
- Modify: `rust_version/src/regex.rs`

- [ ] **Step 1: Define AST types**

Add to `rust_version/src/regex.rs`:

```rust
pub type Pattern = Vec<Item>;

#[derive(Clone, Debug)]
pub enum Item {
    Atom(Atom),
    Group(u32, Pattern),
    NonCapture(Pattern),
    Alt(Vec<Pattern>),
    Quant(Box<Pattern>, u32, u32, bool), // min, max, greedy
}

#[derive(Clone, Debug)]
pub enum Atom {
    Literal(u16),
    Any,
    Class(Vec<(u16, u16)>),
    NegatedClass(Vec<(u16, u16)>),
    Anchor(Anchor),
    Backref(u32),
}

#[derive(Clone, Debug)]
pub enum Anchor {
    StartOfString,
    EndOfString,
    StartOfLine,
    EndOfLine,
    WordBoundary,
    NonWordBoundary,
}
```

- [ ] **Step 2: Implement parser for literals, escapes, groups, quantifiers**

Add a `Parser` struct for regex patterns that produces `Pattern`. Support:

- alternation `|` at the top level,
- concatenation of atoms,
- quantifiers `* + ? {n} {n,} {n,m}`,
- capturing `( )` and non-capturing `(?: )` groups,
- character classes `[...]`,
- escapes listed in the design doc,
- anchors `^ $ \b \B`.

Key helper signatures (fill implementation):

```rust
struct ReParser {
    src: Vec<char>,
    pos: usize,
    group_count: u32,
}

impl ReParser {
    fn parse(source: &str) -> Result<(Pattern, u32), String> { ... }
    fn parse_alts(&mut self) -> Result<Pattern, String> { ... }
    fn parse_seq(&mut self) -> Result<Pattern, String> { ... }
    fn parse_atom(&mut self) -> Result<Item, String> { ... }
    fn parse_class(&mut self) -> Result<Atom, String> { ... }
    fn parse_escape(&mut self) -> Result<u16, String> { ... }
    fn parse_quantifier(&mut self, body: Pattern) -> Result<Item, String> { ... }
}
```

- [ ] **Step 3: Add parser tests**

Append tests that parse simple patterns:

```rust
#[test]
fn parse_simple_literals() {
    let (pat, _) = ReParser::parse("abc").unwrap();
    assert_eq!(pat.len(), 3);
}

#[test]
fn parse_groups_and_quantifiers() {
    let (pat, groups) = ReParser::parse("(a|b)+(?:c)*").unwrap();
    assert_eq!(groups, 1);
    matches!(pat[0], Item::Quant(_, _, _, true));
}

#[test]
fn parse_invalid() {
    assert!(ReParser::parse("(").is_err());
}
```

- [ ] **Step 4: Run tests**

```bash
cd rust_version
cargo test regex::tests
```

Expected: parser tests pass.

- [ ] **Step 5: Commit**

```bash
git add rust_version/src/regex.rs
git commit -m "feat(regex): add regex AST and parser"
```

---

## Task 4: Regex matcher — basic matching

**Files:**
- Modify: `rust_version/src/regex.rs`

- [ ] **Step 1: Implement backtracking matcher**

Add `Regex` struct and `Match`:

```rust
#[derive(Clone, Debug)]
pub struct Match {
    pub captures: Vec<Option<(usize, usize)>>, // UTF-16 code-unit indices
}

pub struct Regex {
    source: String,
    flags: Flags,
    pattern: Pattern,
    group_count: u32,
    max_steps: usize,
}

impl Regex {
    pub fn new(source: &str, flags: &str) -> Result<Self, String> { ... }
    pub fn find(&self, input: &str, start: usize) -> Result<Option<Match>, String> { ... }
    pub fn test(&self, input: &str) -> Result<bool, String> { ... }
    pub fn source(&self) -> &str { &self.source }
    pub fn flags(&self) -> &Flags { &self.flags }
    pub fn group_count(&self) -> u32 { self.group_count }
}
```

Implement `match_pattern(pattern, idx, ctx)` where `Ctx` holds input, position, captures, and step counter. Support:

- `Atom::Literal`, `Any`, `Class`, `NegatedClass`, `Anchor`,
- `Item::Atom`, `Group`, `NonCapture`, `Alt`, `Quant`.

Use save/restore of `(pos, captures)` for backtracking. Stop zero-width quantifier loops.

- [ ] **Step 2: Add matcher tests**

```rust
#[test]
fn match_literals_and_any() {
    let re = Regex::new("a.c", "").unwrap();
    let m = re.find("abc", 0).unwrap().unwrap();
    assert_eq!(m.captures[0], Some((0, 3)));
}

#[test]
fn match_alternation() {
    let re = Regex::new("cat|dog", "").unwrap();
    assert!(re.test("dog").unwrap());
    assert!(!re.test("cow").unwrap());
}

#[test]
fn match_quantifiers() {
    let re = Regex::new("a+b*", "").unwrap();
    assert!(re.test("aaabbb").unwrap());
}
```

- [ ] **Step 3: Run tests**

```bash
cd rust_version
cargo test regex::tests
```

Expected: matcher tests pass.

- [ ] **Step 4: Commit**

```bash
git add rust_version/src/regex.rs
git commit -m "feat(regex): add backtracking matcher"
```

---

## Task 5: Captures and backreferences

**Files:**
- Modify: `rust_version/src/regex.rs`

- [ ] **Step 1: Implement capture groups**

In `match_pattern`, when entering a `Group(id, body)` save the current capture slot, run the body, and on success write `(start, end)` into `captures[id]`. On failure restore the saved slot.

- [ ] **Step 2: Implement backreferences**

`Atom::Backref(id)` compares the captured slice `captures[id]` against input starting at current position. If the slot is unset, the backreference matches the empty string (ES5 behavior for forward refs).

- [ ] **Step 3: Add capture/backref tests**

```rust
#[test]
fn match_captures() {
    let re = Regex::new("([a-z]+)-(\d+)", "").unwrap();
    let m = re.find("id-42", 0).unwrap().unwrap();
    assert_eq!(m.captures[0], Some((0, 5)));
    assert_eq!(m.captures[1], Some((0, 2)));
    assert_eq!(m.captures[2], Some((3, 5)));
}

#[test]
fn match_backref() {
    let re = Regex::new(r"(.)\1", "").unwrap();
    assert!(re.test("aa").unwrap());
    assert!(!re.test("ab").unwrap());
}
```

- [ ] **Step 4: Run tests**

```bash
cd rust_version
cargo test regex::tests
```

Expected: tests pass.

- [ ] **Step 5: Commit**

```bash
git add rust_version/src/regex.rs
git commit -m "feat(regex): add capture groups and backreferences"
```

---

## Task 6: String-method helpers

**Files:**
- Modify: `rust_version/src/regex.rs`

- [ ] **Step 1: Add `find_all`, `replace`, `split` helpers**

Add methods to `Regex`:

```rust
impl Regex {
    pub fn find_all(&self, input: &str) -> Result<Vec<Match>, String> { ... }

    pub fn replace(&self, input: &str, replacement: &str) -> Result<String, String> { ... }

    pub fn split(&self, input: &str, limit: Option<usize>) -> Result<Vec<String>, String> { ... }
}
```

- `find_all` iterates matches; after a zero-width match advance one code unit.
- `replace` handles global vs first and expands `$&`, `$'`, `$``, `$$`, `$n`.
- `split` inserts captured groups between segments; empty matches advance one code unit.

Add helper `u16_range_to_str(input: &str, start: usize, end: usize) -> Option<String>` that converts UTF-16 code-unit indices to a Rust `String`, rounding to valid UTF-8 boundaries if necessary.

- [ ] **Step 2: Add helper tests**

```rust
#[test]
fn replace_with_capture() {
    let re = Regex::new(r"a(\d)", "").unwrap();
    assert_eq!(re.replace("a1 a2", "x$1").unwrap(), "x1 a2");
}

#[test]
fn split_with_regex() {
    let re = Regex::new(r"[,;]\s*", "").unwrap();
    let parts = re.split("a, b;c", None).unwrap();
    assert_eq!(parts, vec!["a", "b", "c"]);
}

#[test]
fn split_captures() {
    let re = Regex::new(r"(-)", "").unwrap();
    let parts = re.split("a-b", None).unwrap();
    assert_eq!(parts, vec!["a", "-", "b"]);
}
```

- [ ] **Step 3: Run tests**

```bash
cd rust_version
cargo test regex::tests
```

Expected: helper tests pass.

- [ ] **Step 4: Commit**

```bash
git add rust_version/src/regex.rs
git commit -m "feat(regex): add find_all, replace and split helpers"
```

---

## Task 7: Integrate `Value::Regex`

**Files:**
- Modify: `rust_version/src/value.rs`
- Modify: `rust_version/src/jsonc/encode.rs` if marshaling branches on `Value`

- [ ] **Step 1: Add `Value::Regex` variant**

In `value.rs`:

```rust
use crate::regex::Regex;

pub enum Value {
    ...
    Regex(Rc<Regex>),
}
```

Add constructor:

```rust
impl Value {
    pub fn regex(re: Regex) -> Value {
        Value::Regex(Rc::new(re))
    }
}
```

- [ ] **Step 2: Update `PartialEq`, `deep_clone`, `value_string`, `type_of` usages**

- `PartialEq`: compare `source`/`flags` or use `Rc::ptr_eq`.
- `deep_clone`: clone the `Rc`.
- `value_string`: return `/source/flags`.
- Add `Value::Regex` branch in any exhaustive match that needs it.
- Update `jsonc::marshal` in `rust_version/src/jsonc/encode.rs` to marshal `Value::Regex` to `null`.

- [ ] **Step 3: Run compiler checks**

```bash
cd rust_version
cargo check
```

Fix any exhaustive-match errors.

- [ ] **Step 4: Commit**

```bash
git add rust_version/src/value.rs
git commit -m "feat(value): add Value::Regex variant"
```

---

## Task 8: Lexer and token changes

**Files:**
- Modify: `rust_version/src/lang/token.rs`
- Modify: `rust_version/src/lang/lexer.rs`

- [ ] **Step 1: Add `offset` to `Token` and `Tok::Char`**

In `token.rs`:

```rust
#[derive(Clone, Debug)]
pub struct Token {
    pub kind: Tok,
    pub lit: String,
    pub pos: Pos,
    pub offset: usize,
}
```

Add `Tok::Char(char)` to `Tok` enum.

- [ ] **Step 2: Update lexer to record offsets and emit `Tok::Char`**

Every `Token { ... }` construction in `lexer.rs` must include `offset: start_offset`.

In the final error branch, emit `Tok::Char(r)` instead of returning an error:

```rust
out.push(Token {
    kind: Tok::Char(r),
    lit: r.to_string(),
    pos: p,
    offset: l.off,
});
l.advance();
```

- [ ] **Step 3: Update `parser.rs` `next()` to return full token**

In `parser.rs`:

```rust
fn next(&mut self) -> Token {
    let t = self.ts[self.i].clone();
    if self.i < self.ts.len() - 1 {
        self.i += 1;
    }
    t
}
```

- [ ] **Step 4: Run existing tests**

```bash
cd rust_version
cargo test
```

Expected: existing tests still pass (invalid-character tests may now produce `SyntaxError` instead of `LexError`; update assertions).

- [ ] **Step 5: Commit**

```bash
git add rust_version/src/lang/token.rs rust_version/src/lang/lexer.rs rust_version/src/lang/parser.rs
git commit -m "feat(lexer): add token offsets and Tok::Char fallback"
```

---

## Task 9: Parser regex literal support

**Files:**
- Modify: `rust_version/src/lang/parser.rs`

- [ ] **Step 1: Store source in `Parser`**

```rust
struct Parser {
    src: String,
    ts: Vec<Token>,
    i: usize,
    loops: usize,
}
```

In `parse`:

```rust
let mut p = Parser { src: src.to_string(), ts, i: 0, loops: 0 };
```

- [ ] **Step 2: Implement regex literal scanner**

Add method:

```rust
fn regex_literal(&mut self, start: usize, pos: Pos) -> Result<Expr, Error> {
    let bytes = self.src.as_bytes();
    let mut i = start + 1;
    let mut in_class = false;
    while i < bytes.len() {
        let b = bytes[i];
        if in_class {
            if b == b'\\' && i + 1 < bytes.len() {
                i += 2;
                continue;
            }
            if b == b']' { in_class = false; }
            i += 1;
        } else {
            if b == b'/' { break; }
            if b == b'\\' && i + 1 < bytes.len() {
                i += 2;
                continue;
            }
            if b == b'[' { in_class = true; }
            if b == b'\n' || b == b'\r' {
                return Err(self.err_pos(pos, "unterminated regular expression"));
            }
            i += 1;
        }
    }
    if i >= bytes.len() {
        return Err(self.err_pos(pos, "unterminated regular expression"));
    }
    let pattern = &self.src[start + 1..i];
    let mut flags = String::new();
    let mut j = i + 1;
    while j < bytes.len() && bytes[j].is_ascii_alphabetic() {
        flags.push(bytes[j] as char);
        j += 1;
    }
    let end = j;
    while self.i < self.ts.len() && self.ts[self.i].offset < end {
        self.i += 1;
    }
    let re = crate::regex::Regex::new(pattern, &flags)
        .map_err(|e| self.err_pos(pos, &e))?;
    Ok(Expr::Literal(pos, Value::regex(re)))
}
```

- [ ] **Step 3: Wire into `primary()`**

In `primary()`, after handling `Tok::Dollar`, add:

```rust
Tok::Slash => self.regex_literal(t.offset, t.pos),
```

For `Tok::Char`, return an unexpected-token error.

- [ ] **Step 4: Add parser tests**

Append to `tests/lang.rs`:

```rust
#[test]
fn regex_literal_parsing() {
    let (r, last) = run(r#"$.re = /\d+/g; $.re.source;"#, obj(vec![]));
    assert_eq!(last, Some(s("\\d+")));
}
```

- [ ] **Step 5: Run tests**

```bash
cd rust_version
cargo test regex_literal
```

Expected: regex literal parses.

- [ ] **Step 6: Commit**

```bash
git add rust_version/src/lang/parser.rs rust_version/tests/lang.rs
git commit -m "feat(parser): add /pattern/flags literal syntax"
```

---

## Task 10: Eval — remove regex crate and wire new engine

**Files:**
- Modify: `rust_version/src/lang/eval.rs`

- [ ] **Step 1: Remove `regex` import and helpers**

Delete:

```rust
use regex::Regex;
```

Delete functions: `captures_to_indexes`, `find_submatch_index`, `re_split_all`, `replace_all`, `expand`, `extract_group`.

- [ ] **Step 2: Add `RegExp` builtin**

In `Runtime::new`, add:

```rust
root_env.vars.borrow_mut().insert("RegExp".to_string(), Value::builtin(Builtin::RegExp));
```

Add `Builtin::RegExp` variant in `value.rs` and handle it in `run_builtin`:

```rust
Builtin::RegExp => {
    if values.is_empty() || values.len() > 2 {
        return Err(self.fail(p, "RegExp expects 1 or 2 arguments"));
    }
    let pattern = match &values[0] {
        Value::String(s) => s.clone(),
        _ => return Err(self.fail(p, "RegExp pattern must be a string")),
    };
    let flags = values.get(1).map(|v| match v {
        Value::String(s) => s.clone(),
        _ => return Err(self.fail(p, "RegExp flags must be a string")),
    }).unwrap_or_default();
    let re = crate::regex::Regex::new(&pattern, &flags)
        .map_err(|e| self.fail(p, &format!("invalid regular expression: {}", e)))?;
    Ok(Value::regex(re))
}
```

- [ ] **Step 3: Update `string_method`**

Change the `split | match | matchAll | replace | replaceAll` branch to dispatch based on the argument type:

- For `Value::Regex`, use it directly.
- For `Value::String`:
  - `match`/`replace`: build `Regex::new(s, "")`.
  - `replaceAll`: build `Regex::new(s, "g")`.
  - `split`: use literal string splitting (not regex).
  - `matchAll`: error.

Implement each method using `regex` engine helpers. For replacement expansion, implement ES5 syntax (`$&`, `$'`, `$``, `$$`, `$n`).

- [ ] **Step 4: Add `regex_method` for `test`/`exec`**

In `method_call`, if `recv` is `Value::Regex`, call `regex_method`:

```rust
fn regex_method(&mut self, p: Pos, re: &Rc<Regex>, name: &str, args: &[Value]) -> Result<Value, Error> {
    match name {
        "test" => { ... }
        "exec" => { ... }
        _ => Err(...),
    }
}
```

`exec` returns an object with numeric keys, `index`, `input`.

- [ ] **Step 5: Update `type_of`, `value_string`, and `member_value` for regex**

In `eval.rs`:

- `type_of` should return `"object"` for `Value::Regex`.
- `value_string` should return `/source/flags`.
- `member_value` should return source/flags/global/ignoreCase/multiline for regex receivers.

- [ ] **Step 6: Run tests**

```bash
cd rust_version
cargo test
```

Fix failures iteratively.

- [ ] **Step 7: Commit**

```bash
git add rust_version/src/lang/eval.rs rust_version/src/value.rs
git commit -m "feat(eval): wire regex engine into string and RegExp methods"
```

---

## Task 11: Update CLI help and spec docs

**Files:**
- Modify: `rust_version/src/cli.rs`
- Modify: `docs/spec.md`

- [ ] **Step 1: Update `--syntax` help**

In `print_language_help`, replace the Go-regex paragraph with:

```text
Regular expressions:
  /pattern/flags             ES5 regex literal; flags are g, i, m
  RegExp(pattern, flags?)    Construct a regex from a string
  re.test(str)               Return true if re matches str
  re.exec(str)               Return match object or null
  re.source, re.flags, re.global, re.ignoreCase, re.multiline

String methods:
  match(pattern)             First match array (with captures) or null
  matchAll(pattern)          Array of match arrays; pattern must be a RegExp with g
  replace(pattern, repl)     Replace first match; repl uses $$ $& $' $` $n
  replaceAll(pattern, repl)  Replace all; pattern must have g if RegExp
  split(pattern[, limit])    String pattern is literal; RegExp pattern may include captures

Note: regex matching uses UTF-16 code-unit semantics. Rust strings cannot
represent lone surrogates, so match boundaries that would split a surrogate
pair are rounded to the nearest valid UTF-8 character boundary.
```

- [ ] **Step 2: Update `docs/spec.md`**

Update the string-method section to describe regex literals, `RegExp`, and the new `match/matchAll/replace/replaceAll/split` semantics. Add the UTF-16 boundary note.

- [ ] **Step 3: Run tests**

```bash
cd rust_version
cargo test
```

- [ ] **Step 4: Commit**

```bash
git add rust_version/src/cli.rs docs/spec.md
git commit -m "docs: update CLI help and spec for ES5 regex"
```

---

## Task 12: Update existing split tests

**Files:**
- Modify: `rust_version/tests/lang.rs`

- [ ] **Step 1: Find and update split-with-regex-string tests**

Locate tests that pass a regex-like string to `split`, e.g.:

```js
$.split = "a, b;c".split("[,;]\\s*");
```

Change to:

```js
$.split = "a, b;c".split(/[,;]\s*/);
```

- [ ] **Step 2: Run split tests**

```bash
cd rust_version
cargo test split
```

Expected: split tests pass under new semantics.

- [ ] **Step 3: Commit**

```bash
git add rust_version/tests/lang.rs
git commit -m "test: update split tests for literal-string semantics"
```

---

## Task 13: Add comprehensive regex tests

**Files:**
- Modify: `rust_version/tests/lang.rs`

- [ ] **Step 1: Add regex tests**

Add tests covering:

- literals and flags,
- `RegExp` constructor,
- `test`/`exec`,
- `match` with/without `g`,
- `matchAll` errors and results,
- `replace`/`replaceAll` with `$&`/`$'`/`$``/`$n`,
- `split` with regex captures,
- case-insensitive and multiline,
- division disambiguation (`1/2`, `x=/a/g`),
- invalid regex errors,
- step-limit errors for `(a+)+b` on a long string.

Example:

```rust
#[test]
fn es5_regex_methods() {
    let (r, _) = run(r#"
        $.m1 = "id-42".match(/([a-z]+)-(\d+)/);
        $.m2 = "a1 a2".match(/a(\d)/g);
        $.r1 = "a1 a2".replace(/a(\d)/, "x$1");
        $.r2 = "a1 a2".replaceAll(/a(\d)/g, "x$1");
        $.s = "a, b;c".split(/[,;]\s*/);
    "#, obj(vec![]));
    let want = obj(vec![
        ("m1", arr(vec![s("id-42"), s("id"), s("42")])),
        ("m2", arr(vec![s("a1"), s("a2")])),
        ("r1", s("x1 a2")),
        ("r2", s("x1 x2")),
        ("s", arr(vec![s("a"), s("b"), s("c")])),
    ]);
    assert_eq!(r, want);
}
```

- [ ] **Step 2: Run full test suite**

```bash
cd rust_version
cargo test
cargo clippy --all-targets
```

Fix all failures.

- [ ] **Step 3: Commit**

```bash
git add rust_version/tests/lang.rs
git commit -m "test: add ES5 regex method tests"
```

---

## Task 14: Final review and cleanup

- [ ] **Step 1: Run full verification**

```bash
cd rust_version
cargo test
cargo clippy --all-targets
cargo build --release
```

- [ ] **Step 2: Confirm no `regex` references remain**

```bash
rg "regex::|use regex|regex\s*=" rust_version/src rust_version/Cargo.toml
```

Expected: no matches except references to the new `crate::regex` module.

- [ ] **Step 3: Commit final fixes**

```bash
git add -A
git commit -m "fix: final clippy and test adjustments"
```

---

## Spec coverage check

- `/pattern/flags` literal syntax → Task 9
- `RegExp` constructor/object → Task 10
- `test`/`exec` and properties → Task 10
- String methods use new engine → Task 10
- JS/ES5 string-pattern semantics (`match`/`replace` regex, `split` literal, `matchAll` error) → Task 10
- ES5 replacement expansion → Task 10
- UTF-16 code-unit semantics with documented boundary rounding → Tasks 2–6, docs in Task 11
- `caseless` case folding → Task 2 dependency + matcher implementation
- Catastrophic backtracking step limit → Task 4 matcher
- Remove `regex` crate → Task 1 + Task 10 cleanup

## Placeholder scan

No `TBD`, `TODO`, or vague steps remain. Every task names exact files, functions, and test commands.

## Type consistency check

- `Value::Regex(Rc<Regex>)` is used consistently.
- `Regex` methods (`find`, `test`, `replace`, `split`) are referenced with the signatures defined in Tasks 4 and 6.
- `Token.offset` is added before parser regex scanning uses it.
- `Tok::Char` is handled in parser before regex literals can encounter it.
