# AGENTS.md

Pure-Go, zero-dependency CLI (`jsonsh`) that runs a small JS-like language over JSONC and outputs the result. Module name is `jsonsh` (no VCS path). Do not add third-party dependencies — `go.mod` has no `require` block.

## Commands

```bash
go build ./cmd/jsonsh      # build
go test ./...              # all tests
go test ./internal/jsonc -run TestMarshal   # single test
go vet ./...
bash build.sh              # test + build to dist/ (Git Bash/MSYS on Windows); injects version via -ldflags "-X main.version=..."
```

Windows shell is PowerShell 7 (`Get-ChildItem`, not `ls -la`). Bash scripts (`build.sh`, `release.sh`) run under Git Bash/MSYS. `.test.bat` and `.tips.txt` are personal scratch files — ignore them.

## Layout & data flow

- `cmd/jsonsh/main.go` — CLI entry. `expandShortOptions` handles grouped short flags (e.g. `-re`, value flag last). Output dispatch: `-r` result, `-c` compact, `-p` pretty, else `Document.Preserve`.
- `internal/jsonc` — JSONC parser + source-preserving renderer. `Document.Preserve`/`render` reuse original source bytes for unchanged nodes; `internal/jsonc/encode.go` holds custom `Marshal`/`appendJSONString`.
- `internal/lang` — interpreter (lexer/parser/eval). Root variable is `$`. Runtime values are `nil`, `bool`, `float64`, `string`, `map[string]any`, and internal `*arrayValue` (exported to `[]any` via `exportValue`).
- Dependency direction is one-way: `lang` → `jsonc` → stdlib only. No cycles.

## Critical gotchas

- **Never use `encoding/json.Marshal` for output.** All output/re-encoding must go through `jsonc.Marshal` so that non-printable runes (e.g. `\uee63` private-use) stay `\uXXXX` escapes instead of raw UTF-8 bytes. `encoding/json` is only for `json.Unmarshal` (JSONC string decode in the parser) and `json.Indent` (pretty `-r` in main.go).
- **Default output mode is byte-preserving.** Tests assert byte-for-byte round-trips of unchanged JSONC (comments, CRLF, tabs, `\uXXXX` escapes). Don't alter `render`'s `reflect.DeepEqual` reuse logic casually.
- **All numbers are `float64`.** `jsonc/encode.go` `appendFloat` deliberately mirrors `encoding/json`'s ES6-ish float formatting — keep it in sync if touched.
- **Output-mode semantics differ**: default = preserve source; `-p` = preserve comments + reindent; `-c` = compact, comments removed; `-r` = re-encode last result (no source preservation).

## Testing conventions

Table-driven tests live in `*_test.go` next to source. `cmd/jsonsh/main_test.go` calls `run(args, stdin, stdout)` directly (no subprocess). `internal/lang` tests use the `run(t, code, root)` helper in `lang_test.go` and `Execute`/`ExecuteWithOutput` for stdout capture.

## Releases

`release.sh` (Bash) computes the next semver from `+001`/`+010`/`+100`, commits, tags `vX.Y.Z`, pushes, and prunes old tags. CI (`.github/workflows/release.yml`) triggers on `v*` tags, builds Windows+Linux x64 with `CGO_ENABLED=0`, and injects `main.version` via `-ldflags`.

Docs: `docs/spec.md` (language semantics), `docs/jsonc-preserve.md` (preservation/escaping rules). A `.codegraph/` index exists for symbol-level exploration.
