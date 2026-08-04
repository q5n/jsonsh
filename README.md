# jsonsh

`jsonsh` is a lightweight JSON/JSONC scripting tool written in pure Go. The root value is available through `$`. It has no third-party dependencies and does not embed or invoke a JavaScript engine. Input files may contain `//` line comments, `/* ... */` block comments, and trailing commas.

```powershell
go run ./cmd/jsonsh -e '$.price *= 0.8' input.json
go run ./cmd/jsonsh -e 'length($.users)' -r input.json
Get-Content input.json | go run ./cmd/jsonsh -e 'delete $.password'
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

## Build and test

```powershell
go build ./cmd/jsonsh
go test ./...
```

On Windows, you can also double-click `build.bat` in the project root. The script runs the test suite and builds `dist\jsonsh.exe`. To skip the tests from a terminal, run:

```powershell
.\build.ps1 -SkipTests
```

To view the complete command-line help:

```powershell
.\dist\jsonsh.exe --help
```

See [docs/spec.md](docs/spec.md) for the complete language semantics, built-in functions, and command-line options.
