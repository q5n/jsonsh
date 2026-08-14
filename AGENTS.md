# AGENTS.md

`jsonsh` is a CLI that runs a small JS-like language over JSONC and outputs the result. Two behaviorally identical implementations live side by side:

- **`rust_version/`** — the primary implementation. Single Cargo crate (`jsonsh`), dependencies: `regex` + `unicode-general-category` only.
- **`go_version/`** — the original pure-Go implementation (zero dependencies), kept for reference.

## Commands

Rust (primary):

```bash
cd rust_version
cargo test                  # all tests
cargo build --release       # build to rust_version/target/release/jsonsh(.exe)
cargo clippy --all-targets  # lint
```

Go (reference):

```bash
cd go_version
go build ./cmd/jsonsh
go test ./...
go vet ./...
bash build-go.sh            # test + build to go_version/dist/ (Git Bash/MSYS on Windows)
```

Windows shell is PowerShell 7 (`Get-ChildItem`, not `ls -la`). Bash scripts (`release.sh`, `go_version/build-go.sh`) run under Git Bash/MSYS — invoke with `bash -lc '...'` so `sed`/`grep` are on PATH. `.test.bat` and `.tips.txt` are personal scratch files — ignore them.

## Layout

Rust (`rust_version/src/`):

- `value.rs` — the unified `Value` enum (`Null/Bool/Number/String/Array/Object`); arrays and objects are `Rc<RefCell<...>>` for reference semantics. `deep_clone()` gives parse→runtime isolation. Also holds `is_letter`/`is_digit`/`is_print` (mirror Go's `unicode.Is*`).
- `jsonc/mod.rs` — JSONC parser + source-preserving renderer (`Document::preserve`/`render` reuse original source bytes for unchanged nodes).
- `jsonc/encode.rs` — `marshal`/`append_json_string`/`format_float` (mirrors Go's `encoding/json` escaping + float formatting).
- `lang/` — interpreter (`lexer.rs`, `parser.rs`, `ast.rs`, `eval.rs`, `token.rs`). Root variable is `$`; numbers are `f64`.
- `cli.rs` + `main.rs` — CLI entry (`expand_short_options`, output dispatch `-r/-c/-p/-n`, `replace_file`).
- `tests/{jsonc,lang,cli}.rs` — integration tests ported from the Go `*_test.go` suite.

Go (`go_version/`): `cmd/jsonsh/main.go`, `internal/jsonc`, `internal/lang`, `docs/`. Same architecture (lang → jsonc → stdlib).

Dependency direction is one-way: `lang` → `jsonc` → stdlib. No cycles.

## Critical gotchas

- **Never use `encoding/json.Marshal` (Go) or `serde_json` (Rust) for output.** All output goes through the custom marshal (`jsonc.Marshal` / `jsonc::marshal`) so non-printable runes (e.g. `\uee63` private-use) stay `\uXXXX` escapes. In Rust, `is_print` must classify only L/M/N/P/S as printable (Go `unicode.IsPrint`).
- **Default output mode is byte-preserving.** Tests assert byte-for-byte round-trips of unchanged JSONC (comments, CRLF, tabs, `\uXXXX` escapes). Don't alter the renderer's unchanged-node reuse lightly.
- **All numbers are `float64`/`f64`.** `format_float` mirrors `encoding/json`'s ES6-ish formatting (fixed notation, switching to `e` for |f|<1e-6 or ≥1e21).
- **Reference semantics:** array/object mutations are visible through every alias. In Rust this is `Rc<RefCell<...>>`; keep borrows scoped (never hold a `borrow()` across a call that may mutate).
- **UTF-8 slicing:** the Go strings are byte-indexed; in Rust, slice `as_bytes()` (not `&str`) anywhere an offset may land mid-rune. The lexer's two-char operator lookahead and the JSONC parser both operate on bytes.
- **`toUpperCase`/`toLowerCase` use Go's *simple* (1:1) Unicode case mapping**, not Rust's full mapping (`ß`→`ß`, `İ`→`i`). See `simple_to_uppercase`/`simple_to_lowercase` in `lang/eval.rs`.
- **Regex `\d`/`\s`/`\w`/`\b` are Unicode in Rust but ASCII in Go.** Go's `regexp` matches only `[0-9]`, `[ \t\n\f\r]`, `[0-9A-Za-z_]` while Rust's `regex` (with the `unicode-perl` feature) matches the full Unicode sets. Known, accepted divergence (e.g. `"１２３".match("\\d")` → Go `null`, Rust `["１"]`); do not "fix" by dropping `unicode-perl` — that turns `\d`/`\s`/`\w` into *parse errors* (`UnicodePerlClassNotFound`). The `regex` dependency is trimmed to `std`/`perf`/`unicode-gencat`/`unicode-script`/`unicode-case`/`unicode-perl` (dropping `unicode-bool`/`unicode-age`/`unicode-segment`, which Go's `regexp` also rejects).

## Testing conventions

- Rust: table-driven tests in `tests/{jsonc,lang,cli}.rs`. `cli.rs` calls `run(args, Input, stdout)` directly (no subprocess); `lang.rs` uses a `run(code, root)` helper and `execute`/`execute_with_output`.
- Go: `*_test.go` next to source. `cmd/jsonsh/main_test.go` calls `run(args, stdin, stdout)` directly.
- When adding features, port the Go test case to Rust and keep both green (they must stay behaviorally identical).

## Releases

`release.sh` (Bash) computes the next semver from `+001`/`+010`/`+100`, bumps `rust_version/Cargo.toml`'s `version` field (and syncs `Cargo.lock`), commits, tags `vX.Y.Z`, pushes, and prunes old tags. The binary version comes from `env!("CARGO_PKG_VERSION")` (the Cargo.toml `version`). CI (`.github/workflows/release.yml`) triggers on `v*` tags and builds `cargo build --release` for Windows + Linux x64.

Docs: `go_version/docs/spec.md` (language semantics), `go_version/docs/jsonc-preserve.md` (preservation/escaping rules). A `.codegraph/` index exists for symbol-level exploration.
