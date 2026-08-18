# jsonsh Language and Command-Line Specification (MVP)

## Goal

`jsonsh` is a dependency-light command-line tool written in Rust. It reads a JSONC value, binds it to the predefined global variable `$`, executes a JavaScript-like script, and outputs either the modified `$` value or the result of the last expression. JSONC input extends standard JSON with line comments, block comments, and trailing commas. The scripting language is intentionally small and does not claim JavaScript compatibility.

## Data types and variables

The runtime supports `null`, `boolean`, `number` (`float64`), `string`, `array`, `object`, and `function`. There are no implicit type conversions except string concatenation with `+`, which converts both operands using the language's `toString()` rules. Assigning to a regular identifier for the first time creates a global variable; reading an undefined variable is an error. There is no block scope. `$` is the predefined root variable. Its members can be modified, and `$ = value` can replace the entire root value. Deleting `$` remains prohibited.

Literals include single- and double-quoted strings, JSON-format numbers, booleans, null, arrays, and objects. Object keys may be strings or identifiers, and script literals may contain trailing commas. Strings support common escape sequences and `\uXXXX`. Scripts support `//` and `/* ... */` comments.

## Expressions

Operator precedence, from lowest to highest, is: assignment `= += -= *= /=` (right-associative), `||`, `&&`, `== !=`, `> >= < <=`, `+ -`, `* /`, unary `! - typeof`, and finally member access and calls.

- Arrow functions create function values: `(a, b) => expression`, `(a, b) => { statements }`, `x => expression`, or `() => expression`. The body is either a single expression whose value is returned, or a braced block that returns via `return` (or `null` if it completes without one). Arrow functions are lexical closures: they capture the variables visible at definition time and share them with other closures and the global scope. Parameters are local to the call; missing arguments become `null` and extra arguments are ignored. A `return` outside a function is an error. Function values are truthy, stringify as `[Function]`, and marshal to `null`. The built-in functions `log`, `env`, and `keys` are ordinary global variables and may be reassigned (shadowed) by user code.

- `+` accepts two numbers for addition or two strings for concatenation. Other arithmetic operators accept numbers only. Division by zero is an error.
- Equality operators do not convert types. Arrays and objects use recursive deep equality.
- Ordered comparisons accept either two numbers or two strings.
- Falsy values are `null`, `false`, numeric zero, and the empty string. Arrays and objects are always truthy. `&&` and `||` short-circuit and return booleans.
- Multi-level access supports `.name`, `[number]`, and `[string-expression]`. Reading a missing object property returns `null`. Array indexes must be integers. Negative indexes count from the end (`-1` is the last element); an index whose resolved position is before the start is an error. Reading an index at or beyond `length` returns `null`. Assigning to such an index grows the array to `index + 1`, filling the gap with `null`. Deleting an index at or beyond `length` is a no-op; deleting a valid negative index removes that element and shifts the rest forward.

An assignment target must be a variable or member expression. Compound assignments follow the rules of their corresponding arithmetic operators.

## Statements

Statements are separated by semicolons or line breaks. A line break separates statements after a complete expression; syntactically incomplete expressions can continue on following lines. The final semicolon in a block, or immediately before `}`, may be omitted. Control-flow bodies may be a braced block or a single statement on the same logical line; a brace-less body ends at a semicolon, a line break, or an `else` keyword.

```js
if (condition) { ... } else if (condition) { ... } else { ... }
if (condition) statement
if (condition) statement else statement
for (key in object) { ... }
for (index in array) statement
for (value of array) statement
for (character of string) statement
for (initializer; condition; update) statement
delete object.member;
break;
continue;
return [value];
```

A brace-less body contains exactly one statement, so control-flow statements may nest as bodies (e.g. `if (a) for (x of xs) { ... }`). A dangling `else` binds to the nearest preceding `if`.

`for..in` iterates over numeric array indexes or object keys in lexicographic order. A key snapshot is created when the loop starts, and members deleted before their turn are skipped. `for..of` iterates over array values or Unicode code points in a string. Its source expression is evaluated once. Array iteration is live: each iteration reads the current length and current element, so `splice`, deletion, and `push` affect later iterations. Loop variables live in global scope. `break` and `continue` apply only to the nearest enclosing loop and are errors outside a loop. Deleting an array element shifts subsequent elements toward the beginning. Deleting a missing member is an error. Regular variables and `$` cannot be deleted. Empty statements are allowed, including repeated semicolons and semicolons following blocks.

The traditional `for (initializer; condition; update) { ... }` loop evaluates the optional initializer once before the loop. Each iteration evaluates the optional condition; an omitted condition is `true`. The body runs only when the condition is truthy. After a normal body completion or `continue`, the optional update expression runs before the next condition check. `break` exits without running the update. The initializer and update are single expressions such as `i = 0` or `i += 1`; block declarations (`let`/`var`), comma expressions, and postfix `++`/`--` are not supported.

## Built-in functions and methods

- `log(value, ...)` writes its arguments to standard output separated by spaces and followed by a newline, using the language's `toString()` rules. It accepts zero or more arguments and returns `null`. Logs remain on standard output when the processed JSON is written with `-o` or `-i`.
- `env(name)` returns the named environment variable as a string, including an empty string when the variable is set but empty. It returns `null` when the variable is not set. The name must be a string.
- Strings and arrays provide a read-only `length` property. String length counts Unicode code points.
- Strings provide `toLowerCase()`, `toUpperCase()`, `substring(start[, end])`, `indexOf(text[, start])`, `lastIndexOf(text[, start])`, `localeCompare(text)`, `padStart(targetLength[, padString])`, `padEnd(targetLength[, padString])`, `split(pattern[, limit])`, `match(pattern)`, `matchAll(pattern)`, `replace(pattern, replacement)`, `replaceAll(pattern, replacement)`, and `trim()`. String lengths and padding operate on Unicode code points. Padding defaults to a space. Pattern arguments use Go regular-expression syntax, not JavaScript regex literals. `match` returns the full first match followed by its capture groups, or `null`; `matchAll` returns an array of those match arrays. Replacement strings use Go expansion syntax such as `$1`.
- Arrays provide `push(value, ...)`, `reverse()`, `splice(start[, deleteCount, ...items])`, `join([separator])`, `indexOf(value[, start])`, and `lastIndexOf(value[, start])`. `reverse()` reverses the array in place and returns the same array reference. Array searches use the language's recursive deep equality.
- `typeof value` is a prefix expression that returns `string`, `array`, `object`, `boolean`, `number`, or `function`. Like JavaScript, `typeof null` returns `object`.
- `keys(v)` returns object keys in lexicographic order or numeric array indexes.
- Every supported value provides `toString()`. Objects are encoded as compact, single-line JSON. Array string conversion uses comma-separated element values.

`+` adds two numbers. If either operand is a string, both operands are converted using the same rules as `toString()` and concatenated.

Array mutations are visible through every variable referencing the same array. Calling a method with an invalid argument count or type, calling an array method on a non-array receiver, or calling an unknown method is a runtime error.

Invalid argument counts or types are runtime errors.

## Command line

```text
jsonsh (-e CODE | -f SCRIPT) [options] [INPUT]

-e, --expression CODE  Execute code
-f, --script FILE      Read code from a UTF-8 file
-r, --result           Output the last expression result (default: output $)
-p, --pretty           Reformat output while preserving comments
-c, --compact          Output compact standard JSON without comments
-o, --output FILE      Write output to a file
-i, --in-place         Safely replace the input file
-n, --no-output        Suppress final output; log output remains visible
    --max-steps N      Maximum interpreter steps (default: 1000000)
```

When `INPUT` is omitted, input is read from standard input if it is redirected or piped; otherwise `$` is initialized to `null`. `-e` and `-f`, `-o` and `-i`, and `-p` and `-c` are mutually exclusive. `-i` requires an input file. The default mode applies minimal changes and preserves the original source structure. `--pretty` reformats while retaining comments. `--compact` emits comment-free standard JSON. In-place writes first create a temporary file in the same directory and replace the input only after the output has been written and closed successfully. See [jsonc-preserve.md](jsonc-preserve.md) for details.

Lexical, syntax, and runtime errors include line and column positions, are written to standard error, and produce a nonzero exit code. Failed execution does not write an output file. An explicitly empty expression (`-e ""`) performs no mutations and outputs the root value in the selected output mode. Named function statements, `while`, increment operators, modulo, ternary expressions, default parameters, rest/spread, slicing, and JavaScript standard objects are not supported yet.
