# AGENTS.md

`jsonsh` is a CLI that runs a small JS-like language over JSONC and outputs the result. It is implemented in Rust as a single Cargo crate (`jsonsh`) in `rust_version/`, depending only on `regex` and `unicode-general-category`.

## Commands

```bash
cd rust_version
cargo test                  # all tests
cargo build --release       # build to rust_version/target/release/jsonsh(.exe)
cargo clippy --all-targets  # lint
```

Windows shell is PowerShell 7 (`Get-ChildItem`, not `ls -la`). Bash scripts (`release.sh`) run under Git Bash/MSYS — invoke with `bash -lc '...'` so `sed`/`grep` are on PATH. `.test.bat` and `.tips.txt` are personal scratch files — ignore them.

## Layout

Rust (`rust_version/src/`):

- `value.rs` — the unified `Value` enum (`Null/Bool/Number/String/Array/Object/Function/Builtin`); arrays and objects are `Rc<RefCell<...>>` for reference semantics. `Function(Rc<Function>)` holds `Function::Closure` (AST body + captured env) or `Function::Native` (builtins like `log`/`keys`/`env`); both stored as `Rc<dyn Any>` to keep `value` independent of `lang`. `deep_clone()` shares functions (immutable). Also holds `is_letter`/`is_digit`/`is_print` (ASCII letter/digit plus a printability check classifying L/M/N/P/S as printable, mirroring Go's `unicode.IsPrint`).
- `jsonc/mod.rs` — JSONC parser + source-preserving renderer (`Document::preserve`/`render` reuse original source bytes for unchanged nodes).
- `jsonc/encode.rs` — `marshal`/`append_json_string`/`format_float` (custom JSON escaping + float formatting that switches to `e` notation for |f|<1e-6 or ≥1e21). Functions marshal to `null` (not a JSON type).
- `lang/` — interpreter (`lexer.rs`, `parser.rs`, `ast.rs`, `eval.rs`, `token.rs`). Root variable is `$`; numbers are `f64`. Supports arrow functions (`(a,b) => expr` / `(a,b) => { ... return ... }` / `x => expr` / `() => expr`) with lexical closures, `return`, and a `MAX_CALL_DEPTH = 64` recursion guard. `typeof` is a prefix unary expression (not a builtin function); `log`/`env`/`keys` remain shadowable builtin globals. Only bare names and `obj.method()` are callable (no IIFE, no `f(1)(2)`).
- `cli.rs` + `main.rs` — CLI entry (`expand_short_options`, output dispatch `-r/-c/-p/-n`, `replace_file`).
- `tests/{jsonc,lang,cli}.rs` — integration tests.

Dependency direction is one-way: `lang` → `jsonc` → `value`. No cycles. Functions break the would-be cycle (`value` can't name `lang` types) via `Rc<dyn Any>` downcasting in `eval::invoke_closure`.

## Critical gotchas

- **Never use `serde_json` for output.** All output goes through the custom marshal (`jsonc::marshal`) so non-printable runes (e.g. `\uee63` private-use) stay `\uXXXX` escapes. `is_print` must classify only L/M/N/P/S as printable.
- **Default output mode is byte-preserving.** Tests assert byte-for-byte round-trips of unchanged JSONC (comments, CRLF, tabs, `\uXXXX` escapes). Don't alter the renderer's unchanged-node reuse lightly.
- **All numbers are `f64`.** `format_float` produces ES6-ish formatting (fixed notation, switching to `e` for |f|<1e-6 or ≥1e21).
- **Reference semantics:** array/object mutations are visible through every alias. In Rust this is `Rc<RefCell<...>>`; keep borrows scoped (never hold a `borrow()` across a call that may mutate).
- **UTF-8 slicing:** slice `as_bytes()` (not `&str`) anywhere an offset may land mid-rune. The lexer's two-char operator lookahead and the JSONC parser both operate on bytes.
- **`toUpperCase`/`toLowerCase` use the simple (1:1) Unicode case mapping**, not Rust's full mapping (`ß`→`ß`, `İ`→`i`). See `simple_to_uppercase`/`simple_to_lowercase` in `lang/eval.rs`.
- **Regex `\d`/`\s`/`\w`/`\b` are Unicode** under the `regex` crate's `unicode-perl` feature (full Unicode sets, not ASCII). This is the intended behavior. Do not "fix" by dropping `unicode-perl` — that turns `\d`/`\s`/`\w` into *parse errors* (`UnicodePerlClassNotFound`). The `regex` dependency is trimmed to `std`/`perf`/`unicode-gencat`/`unicode-script`/`unicode-case`/`unicode-perl`.

## Testing conventions

- Table-driven tests in `tests/{jsonc,lang,cli}.rs`. `cli.rs` calls `run(args, Input, stdout)` directly (no subprocess); `lang.rs` uses a `run(code, root)` helper and `execute`/`execute_with_output`.

## Releases

`release.sh` (Bash) computes the next semver from `+001`/`+010`/`+100`, bumps `rust_version/Cargo.toml`'s `version` field (and syncs `Cargo.lock`), commits, tags `vX.Y.Z`, pushes, and prunes old tags. The binary version comes from `env!("CARGO_PKG_VERSION")` (the Cargo.toml `version`). CI (`.github/workflows/release.yml`) triggers on `v*` tags and builds `cargo build --release` for Windows + Linux x64.

Docs: `docs/spec.md` (language semantics), `docs/jsonc-preserve.md` (preservation/escaping rules). A `.codegraph/` index exists for symbol-level exploration.
