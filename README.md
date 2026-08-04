# jsonsh

`jsonsh` is a lightweight JSON/JSONC scripting tool written in pure Go. The root value is available through `$`. It has no third-party dependencies and does not embed or invoke a JavaScript engine. Input files may contain `//` line comments, `/* ... */` block comments, and trailing commas.

```powershell
go run ./cmd/jsonsh -e '$.price *= 0.8' input.json
go run ./cmd/jsonsh -e '$.users.length' -r input.json
Get-Content input.json | go run ./cmd/jsonsh -e 'delete $.password'
go run ./cmd/jsonsh -n -e '$ = {status: "ok"}'
```

Use `-n` or `--null-input` to skip standard input and input files and initialize
the root value `$` to `null`. This is useful when a script creates its root value
from scratch.

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

## Build and test

```powershell
go build ./cmd/jsonsh
go test ./...
```

On Linux Bash or Windows Git Bash/MSYS, use the build script. It runs the test
suite and builds for the current Go target. The output is `dist/jsonsh` on Linux
or `dist/jsonsh` on Windows:

```bash
bash ./build.sh
bash ./build.sh --skip-tests
```

To view the complete command-line help:

```powershell
.\dist\jsonsh --help
```

To display the build version:

```powershell
.\dist\jsonsh -v
```

To display the built-in scripting language reference:

```powershell
.\dist\jsonsh --language-help
```

The version is injected at build time. `build.sh` derives it from Git by default,
or accepts an explicit value with `--version v0.2.1`.

See [docs/spec.md](docs/spec.md) for the complete language semantics, built-in functions, and command-line options.
