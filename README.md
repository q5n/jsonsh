# jsonsh

`jsonsh` is a lightweight JSON/JSONC scripting tool. The root value is available through `$`. It does not embed or invoke a JavaScript engine. Input files may contain `//` line comments, `/* ... */` block comments, and trailing commas.

Two behaviorally identical implementations are provided:

- **Go** — pure Go, zero third-party dependencies, in [`go_version/`](go_version/).
- **Rust** — in [`rust_version/`](rust_version/), depending only on `regex` and `unicode-general-category`.

```bash
jsonsh -e '$.price *= 0.8' input.json
jsonsh -e '$.users.length' -r input.json
cat input.json | jsonsh -e 'delete $.password'
jsonsh -e '$ = {status: "ok"}'
```

When no input file is given and standard input is not redirected or piped, the
root value `$` is initialized to `null`. This is useful when a script creates its
root value from scratch.

Boolean short options can be grouped. A value-taking short option can appear at
the end of a group, so the previous example can also be written as:

```bash
jsonsh -re "{status: 'ok'}"
```

Use `-n` or `--no-output` to suppress the final processed JSON while keeping
`log(...)` output visible:

```bash
jsonsh -ne "log('done')"
```

By default, `jsonsh` rewrites only values that actually change. Existing indentation, line endings, property order, comments, string escapes, and number formatting remain untouched. Use `--pretty` to reformat the document while preserving comments, or `--compact` to emit compact, comment-free standard JSON.

Use `push` to append one or more elements to an array. It returns the new array length:

```js
$.users.push({name: "Tom"});
newLength = $.tags.push("go", "json");
```

You can also replace the entire JSON root value:

```js
$ = {status: "ok", items: []};
```

Script files passed with `-f` may separate statements with line breaks instead
of semicolons. Incomplete expressions can continue on the next line:

```js
$.count += 1
$.label = $.label
  .padEnd(10, ".")
$.ready = true
```

## Build and test

### Rust version (`rust_version/`)

```bash
cd rust_version
cargo test                   # run all tests
cargo build --release        # build to target/release/jsonsh(.exe)
```

The release profile strips symbols and enables LTO (`strip = true`, `lto = true`,
`codegen-units = 1`, `panic = "abort"`), producing a small, dependency-free binary.
The version comes from the `version` field in `Cargo.toml` (baked in at compile time
via `CARGO_PKG_VERSION`).

### Go version (`go_version/`)

```bash
cd go_version
go build ./cmd/jsonsh        # build to go_version/jsonsh(.exe)
go test ./...                # run all tests
go vet ./...                 # static analysis
```

On Linux Bash or Windows Git Bash/MSYS, use the build script. It runs the test
suite and builds to `go_version/dist/jsonsh` (or `.exe`), injecting the version
via `-ldflags "-X main.version=..."` (derived from Git by default):

```bash
cd go_version
bash ./build-go.sh
bash ./build-go.sh --skip-tests
bash ./build-go.sh --version v0.2.1
```

To view the complete command-line help:

```bash
jsonsh --help
```

To display the build version:

```bash
jsonsh -v
```

To display the built-in scripting language reference:

```bash
jsonsh --syntax
```

See [go_version/docs/spec.md](go_version/docs/spec.md) for the complete language semantics, built-in functions, and command-line options.
