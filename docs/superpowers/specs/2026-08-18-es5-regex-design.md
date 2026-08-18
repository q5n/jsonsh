# jsonsh ES5 Regular Expression Support Design

## Goal

Replace the Rust `regex` crate dependency with a self-contained ES5-regular-expression engine, add `/pattern/flags` literal syntax, and expose a first-class `RegExp` object. String methods that currently rely on `regex` must switch to the new engine while following JavaScript/ES5 semantics as closely as possible.

## Decisions

| Topic | Decision |
|---|---|
| Engine architecture | AST backtracking interpreter (simplest path to captures, backreferences, and greedy/lazy quantifiers). |
| ES5 coverage | Near-full ES5: character classes, quantifiers, capturing/non-capturing groups, backreferences `\1..\9`, anchors `^ $ \b \B`, line terminators, flags `g/i/m`, escapes `\cX \xHH \uHHHH \0 \n \r \t \v \f`, and predefined classes `\d \D \s \S \w \W`. |
| Unicode/UTF-16 | Regex matching uses UTF-16 code-unit semantics. `Value::String` remains Rust UTF-8. If a match boundary falls inside a surrogate pair, it is rounded to the nearest valid UTF-8 character boundary; this deviation is documented in `docs/spec.md` and the CLI `--syntax` help. |
| Case-insensitive matching | Use the `caseless` crate for Unicode case folding. Multi-character folds that do not align with UTF-16 code units are approximated. |
| `/` ambiguity | JavaScript-style contextual rule: a `/` token parsed in primary-expression position starts a regex literal; otherwise it is the division operator. |
| String pattern handling | `match`/`replace` convert a string to `new RegExp(pattern)` (no flags). `replaceAll` converts a string to `new RegExp(pattern, "g")`. `split` treats a string as a literal separator. `matchAll` requires a RegExp argument and rejects strings. |
| Replacement expansion | ES5-style: `$$`, `$&`, `$'`, `$``, and `$n` capture references. Named capture groups are not supported. |
| `RegExp.exec` | Returns an object with numeric keys `"0"`, `"1"`, ... for captures, plus `"index"` and `"input"`. Returns `null` on no match. |
| Catastrophic backtracking | Each regex match has a step budget; exceeding it raises a runtime error. |

## Module Layout

```
rust_version/src/
  regex.rs          # New regex engine: Flags, AST, parser, backtracking matcher
  value.rs          # Add Value::Regex(Rc<Regex>); typeof returns "object"
  lang/
    token.rs        # Add Token.offset
    lexer.rs        # Record offset; add Tok::Char fallback token
    parser.rs       # Keep original source; parse /pattern/flags in primary()
    eval.rs         # Remove regex crate; route string/regex methods through new engine
  cli.rs            # Update --syntax help
```

Dependency changes:

- Remove `regex` from `Cargo.toml`.
- Add `caseless` for Unicode case folding.

## Regex Engine

### Internal representation

The AST is linearized into a `Pattern` (a sequence of items) to make backtracking straightforward:

```rust
pub type Pattern = Vec<Item>;

pub enum Item {
    Atom(Atom),
    Group(u32, Pattern),        // capturing group
    NonCapture(Pattern),
    Alt(Vec<Pattern>),          // alternation
    Quant(Box<Pattern>, u32, u32, bool), // min, max, greedy
}

pub enum Atom {
    Literal(u16),               // UTF-16 code unit
    Any,                        // . excluding line terminators
    Class(Vec<(u16, u16)>),     // inclusive code-unit ranges
    NegatedClass(Vec<(u16, u16)>),
    Anchor(Anchor),
    Backref(u32),
}

pub enum Anchor {
    StartOfString,
    EndOfString,
    StartOfLine,
    EndOfLine,
    WordBoundary,
    NonWordBoundary,
}
```

### Matching

- Input is converted to `Vec<u16>`.
- `Regex::find(input, start)` tries every start position from `start` to the end.
- The matcher walks `Pattern`, saving/restoring `(position, captures)` at choice points.
- Quantifiers record the state after each successful repetition so greedy can back off from max to min and lazy can advance from min to max. A zero-width match stops repetition to avoid infinite loops.
- Anchors inspect the current position and, when the `m` flag is set, treat line terminators as line boundaries.
- A step counter is incremented for every atom match, quantifier loop, and alternation retry. Hitting the budget returns an error.

### Character classes and case folding

- Ranges such as `a-z` are stored as inclusive `(u16, u16)` ranges and normalized.
- Predefined classes use ASCII semantics: `\d` `[0-9]`, `\w` `[A-Za-z0-9_]`, `\s` common whitespace plus BOM/NBSP.
- With the `i` flag, literal atoms and backreferences compare case-folded code units at match time. Character classes are expanded at compile time using `caseless` case folding. Multi-character folds that do not fit a single UTF-16 code unit are approximated.
- Inside a class, `\b` means the backspace code unit `0x08`.

### Escapes

Supported escapes include:

- `\. \/ \* \+ \? \( \) \{ \} \[ \] \| \^ \$` for literals.
- `\d \D \s \S \w \W` inside and outside classes.
- `\n \r \t \v \f \b`.
- `\cX` control characters.
- `\xHH` two-digit hex.
- `\uHHHH` four-digit Unicode.
- `\0` NUL.
- `\1..\9` backreferences.

## Regex Literal Syntax

### Lexer changes

- `Token` gains an `offset: usize` field so the parser can locate the literal in the original source.
- Unrecognized characters are emitted as `Tok::Char(c)` instead of causing a lexer error. This lets regex bodies contain `? | ^ % \` and other meta-characters without breaking tokenization; the parser reports a syntax error if such a token appears outside a regex literal.

### Parser changes

- `Parser` stores the original source string.
- In `primary()`, when the current token is `Tok::Slash`, the parser scans the source from the slash's byte offset:
  - Skip `\x` escapes and `[...]` character classes.
  - Find the closing unescaped `/`.
  - Read trailing flag letters `g`, `i`, `m`.
  - Skip all tokens whose offset falls inside the scanned span.
  - Compile the pattern and flags into a `Value::Regex` literal.
- The same contextual rule as JavaScript applies: a slash parsed in primary-expression position is a regex start; a slash parsed after a complete expression is division.

## `Value::Regex` and the `RegExp` Object

- Add `Value::Regex(Rc<Regex>)` to `value.rs`.
- `typeof /re/` returns `"object"`.
- `toString()` returns `/source/flags`.
- JSON marshaling outputs `null`.
- Add `Builtin::RegExp` so `RegExp(pattern, flags?)` is callable as a global function.
- Regex methods/properties:
  - `re.test(str)` → `bool`
  - `re.exec(str)` → object or `null`
  - `re.source` → pattern string
  - `re.flags` → flags string
  - `re.global`, `re.ignoreCase`, `re.multiline` → booleans
- `RegExp` called with a regex object may clone/override flags (optional enhancement).

## String Methods

All string methods route through the new regex engine.

- `match(pattern)`
  - RegExp without `g`: returns `[full, cap1, ...]` or `null`.
  - RegExp with `g`: returns array of full-match strings.
  - String: converted to `new RegExp(pattern)`.
- `matchAll(pattern)`
  - Requires a RegExp. Without `g`, errors. Returns array of `[full, cap1, ...]` arrays.
  - String argument is rejected.
- `replace(pattern, replacement)`
  - RegExp with `g`: replace all. Without `g`: replace first.
  - String: converted to `new RegExp(pattern)`.
  - Replacement uses ES5 expansion: `$$`, `$&`, `$'`, `$``, `$n`.
- `replaceAll(pattern, replacement)`
  - RegExp must have `g`; otherwise error.
  - String: converted to `new RegExp(pattern, "g")`.
- `split(pattern[, limit])`
  - String pattern is treated as a literal separator.
  - RegExp pattern splits at each match; captured groups are inserted into the result array (ES5 behavior).
  - Empty/zero-width matches advance one code unit to avoid infinite loops.

## Error Handling

- Invalid regex syntax is reported at parse time or `RegExp` construction time with a line and column.
- Regex match step limit exceeded is a runtime error.
- `matchAll` with a non-RegExp or a RegExp without `g` is a runtime error.
- `replaceAll` with a RegExp without `g` is a runtime error.

## Testing

Add or update tests in `rust_version/tests/lang.rs`:

- Literal parsing: `/a/g`, `/\s*\W+/gi`, `/^(\d+)-(\d+)$/m`.
- Division disambiguation: `1/2`, `x=/a/g`, `(1)/a`, `if(x)/a/g`, `x /= /a/g`.
- `RegExp` constructor and properties.
- `test` and `exec`.
- `match`/`matchAll`/`replace`/`replaceAll`/`split` with both literals and strings.
- Case-insensitive and multiline behavior.
- Split with regex captures.
- Replacement expansion `$&`/`$'`/`$``/`$n`.
- Invalid patterns and step-limit errors.

Existing tests that pass a regex-like string to `split` must be updated to use a regex literal or `RegExp(...)`, because `split` now treats strings as literal separators.

Run:

```bash
cd rust_version
cargo test
cargo clippy --all-targets
```

## Documentation Updates

- `docs/spec.md`: update the "Built-in functions and methods" section to describe regex literals, `RegExp`, and the new string-method semantics. Add a note about the UTF-16/Rust-String boundary compromise.
- `src/cli.rs` `--syntax` help: add regex literal examples, flag descriptions, replacement syntax, and the same UTF-16 boundary note.

## Risks and Known Limitations

1. **UTF-16 boundary rounding**: Strict ES5 can produce lone surrogate substrings; Rust `String` cannot represent them. This is documented and accepted.
2. **Case folding**: Multi-character folds are approximated because matching operates on UTF-16 code units.
3. **Backtracking performance**: The AST interpreter can be exponential on pathological input; the step budget limits the damage.
4. **No `lastIndex` stateful iteration**: `RegExp.exec` is implemented statelessly. The `lastIndex` property is not supported.
