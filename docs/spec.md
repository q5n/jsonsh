# jsonsh Language and Command-Line Specification (MVP)

## Goal

`jsonsh` is a dependency-light command-line tool written in Rust. It reads a JSONC value, binds it to the predefined global variable `$`, executes a JavaScript-like script, and outputs either the modified `$` value or the result of the last expression. JSONC input extends standard JSON with line comments, block comments, and trailing commas. The scripting language is intentionally small and does not claim JavaScript compatibility.

## Data types and variables

The runtime supports `null`, `boolean`, `number` (`float64`), `string`, `array`, `object`, `function`, and `date` (a number value with `Date` semantics). There are no implicit type conversions except string concatenation with `+`, which converts both operands using the language's `toString()` rules. Assigning to a regular identifier for the first time creates a global variable; reading an undefined variable is an error. There is no block scope. `$` is the predefined root variable. Its members can be modified, and `$ = value` can replace the entire root value. Deleting `$` remains prohibited.

A number can be `NaN` or infinite; these marshal to `null` (matching `JSON.stringify`) and stringify as `"NaN"`, `"Infinity"`, or `"-Infinity"`. `NaN` is falsy and compares unequal to itself.

Literals include single- and double-quoted strings, JSON-format numbers, booleans, null, arrays, and objects. Object keys may be strings or identifiers, and script literals may contain trailing commas. Strings support common escape sequences and `\uXXXX`. Scripts support `//` and `/* ... */` comments.

## Expressions

Operator precedence, from lowest to highest, is: assignment `= += -= *= /= %=` (right-associative), the conditional `?:` (right-associative), `||`, `&&`, `|`, `^`, `&`, `== !=`, `> >= < <=`, `<< >> >>>`, `+ -`, `* / %`, unary `! - ~ typeof`, prefix `++ --`, and finally member access, calls, postfix `++ --`, and optional chaining `?.`.

- Arrow functions create function values: `(a, b) => expression`, `(a, b) => { statements }`, `x => expression`, or `() => expression`. The body is either a single expression whose value is returned, or a braced block that returns via `return` (or `null` if it completes without one). Arrow functions are lexical closures: they capture the variables visible at definition time and share them with other closures and the global scope. Parameters are local to the call; missing arguments become `null` and extra arguments are ignored. A `return` outside a function is an error. Function values are truthy, stringify as `[Function]`, and marshal to `null`. The built-in functions `log`, `env`, and `keys` are ordinary global variables and may be reassigned (shadowed) by user code.

- `+` accepts two numbers for addition or two strings for concatenation. Other arithmetic operators accept numbers only. Division and modulo by zero are errors. `%` yields the remainder with the sign of the dividend. The bitwise operators `& | ^ << >> >>> ~` convert operands to 32-bit integers using ECMAScript `ToInt32`/`ToUint32` semantics (operating modulo 2³²); `>>>` yields an unsigned 32-bit result, `>>` shifts sign-preserving, and shift counts are taken modulo 32.
- Equality operators do not convert types. Arrays and objects use recursive deep equality.
- Ordered comparisons accept either two numbers or two strings.
- Falsy values are `null`, `false`, numeric zero (including `NaN`), and the empty string. Arrays and objects are always truthy. `&&` and `||` short-circuit and return booleans. The conditional `condition ? ifTrue : ifFalse` evaluates only the chosen branch.
- `i++`/`i--` return the old value and then add/subtract one; `++i`/`--i` add/subtract one and return the new value. The operand must be a variable or member that holds a number.
- Multi-level access supports `.name`, `[number]`, and `[string-expression]`. Reading a missing object property returns `null`. Array indexes must be integers. Negative indexes count from the end (`-1` is the last element); an index whose resolved position is before the start is an error. Reading an index at or beyond `length` returns `null`. Assigning to such an index grows the array to `index + 1`, filling the gap with `null`. Deleting an index at or beyond `length` is a no-op; deleting a valid negative index removes that element and shifts the rest forward.
- Optional chaining short-circuits the rest of a property/method chain when a receiver is `null`, yielding `null` instead of an error: `a?.b`, `a?.b()`, `a?.[i]`, and chained forms such as `a?.b.c` or `a?.b?.c`. Calling the receiver itself (`a?.(...)`) is not supported.

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
- Strings provide `toLowerCase()`, `toUpperCase()`, `charAt(index)`, `charCodeAt(index)`, `concat(value, ...)`, `includes(text[, start])`, `startsWith(text[, start])`, `endsWith(text[, end])`, `slice(start[, end])`, `repeat(count)`, `substring(start[, end])`, `indexOf(text[, start])`, `lastIndexOf(text[, start])`, `localeCompare(text)`, `padStart(targetLength[, padString])`, `padEnd(targetLength[, padString])`, `split(pattern[, limit])`, `match(pattern)`, `matchAll(pattern)`, `replace(pattern, replacement)`, `replaceAll(pattern, replacement)`, and `trim()`. String lengths, padding, `charAt`, `charCodeAt`, `slice`, and `repeat` operate on Unicode code points. `charAt` returns `""` and `charCodeAt` returns `NaN` for an out-of-range index. `slice` and `substring` accept negative indexes counted from the end. Padding defaults to a space. String patterns follow JavaScript/ES5 semantics: `match`/`replace` convert a string to `RegExp(pattern)`, `replaceAll` converts a string to `RegExp(pattern, "g")`, `split` treats a string as a literal separator, and `matchAll` requires a RegExp. `match` without the `g` flag returns the full match followed by its capture groups, or `null`; with `g` it returns an array of full-match strings. `matchAll` returns an array of those match arrays and requires the `g` flag. Replacement strings use ES5 expansion syntax: `$$`, `$&`, ``$` ``, `$'`, and `$n` capture references.
- Arrays provide `push(value, ...)`, `reverse()`, `splice(start[, deleteCount, ...items])`, `join([separator])`, `concat(value, ...)`, `slice(start[, end])`, `includes(value[, start])`, `indexOf(value[, start])`, `lastIndexOf(value[, start])`, `map(fn)`, `filter(fn)`, `reduce(fn[, initial])`, `forEach(fn)`, `find(fn)`, `some(fn)`, `every(fn)`, and `sort([compareFn])`. `reverse()` and `sort()` mutate the array in place and return the same array reference. `concat` flattens array arguments one level. Array searches and `includes` use the language's recursive deep equality. The iteration callbacks (`map`, `filter`, `reduce`, `forEach`, `find`, `some`, `every`, and `sort`'s comparator) are invoked as `fn(element, index)`, except `reduce`, which is invoked as `fn(accumulator, element, index)`. `find` returns `null` when no element matches. Without a comparator, `sort` compares elements by their `toString()` values.
- Numbers provide `toFixed([digits])`, `toString([radix])`, and `valueOf()`. `toString(radix)` formats the integer part in base `radix` (2–36). `toFixed` rounds half toward positive infinity (`(1.25).toFixed(1)` is `"1.3"`) and uses exponential notation for magnitudes of `10²¹` or more. Values whose exact binary representation sits just below a decimal half-step (e.g. `2.55`) round down, as in JavaScript; exact decimal rounding of such boundaries is not attempted.
- Objects provide `hasOwnProperty(key)`. Every value also provides `valueOf()`, which returns the receiver.
- `typeof value` is a prefix expression that returns `string`, `array`, `object`, `boolean`, `number`, or `function`. Like JavaScript, `typeof null` returns `object`. `typeof` a constructor (`Object`, `Array`, ...) returns `"function"` and `typeof` a `Date` returns `"object"`.
- `keys(v)` returns object keys in lexicographic order or numeric array indexes.
- Every supported value provides `toString()`. Objects are encoded as compact, single-line JSON. Array string conversion uses comma-separated element values. Dates convert to their ISO-8601 UTC string.
- Global functions: `parseInt(string[, radix])`, `parseFloat(string)`, `encodeURI(string)`, `decodeURI(string)`, `encodeURIComponent(string)`, and `decodeURIComponent(string)`. `parseInt` accepts an optional radix (2–36, or `0` to auto-detect `0x`); it returns `NaN` when no digits are parsed. `parseFloat` parses a leading decimal, optionally in scientific notation, and understands `"Infinity"`/`"-Infinity"`. URI functions operate on UTF-8 bytes; `encodeURI` preserves the reserved characters `; / ? : @ & = + $ , #` while `encodeURIComponent` encodes them.

## Constructors

`Object`, `Array`, `String`, `Number`, `Boolean`, and `Date` are global constructor values. They are callable with or without the `new` keyword and may be reassigned (shadowed) like any other global. Constructors return plain values rather than boxed objects: `new String(x)` returns a string value (`typeof` is still `"string"`), `new Number(x)` a number, and `new Boolean(x)` a boolean. `Array(n)` with a single numeric argument returns an array of length `n` filled with `null`; otherwise arguments become elements. `Object(x)` returns `x` when `x` is an object or array, and `{}` otherwise.

Static methods: `Object.keys(v)`, `Object.values(v)`, `Object.entries(v)`, `Object.assign(target, ...sources)`, `Array.isArray(v)`, `String.fromCharCode(code, ...)`, `Number.isInteger(v)`, `Number.isNaN(v)`, `Number.isFinite(v)`, `Number.parseInt(s[, radix])`, `Number.parseFloat(s)`, `Date.now()`, `Date.parse(s)`, and `Date.UTC(year, month[, day[, hours[, minutes[, seconds[, ms]]]]])`. `Object.keys`/`values`/`entries` sort object keys lexicographically. `Object.assign` mutates and returns its target. `Date.UTC` and the `Date` multi-argument constructor take a zero-based month and default `day` to `1`.

`Date` represents an instant as milliseconds since the Unix epoch (a number with reference semantics). All date calculations use UTC; there is no local-timezone support, so `getFullYear`/`getUTCFullYear` and their siblings are identical. A `Date` has `getTime()`, `getFullYear()`, `getMonth()` (0–11), `getDate()`, `getDay()` (0 = Sunday), `getHours()`, `getMinutes()`, `getSeconds()`, `getMilliseconds()`, their `getUTC*` aliases, `toISOString()`, `toString()`, and `valueOf()`. `Date.parse` and `new Date(string)` accept a strict UTC ISO-8601 timestamp (`YYYY-MM-DDTHH:MM:SS[.sss]Z`); anything else yields an invalid date. An invalid date stringifies as `"Invalid Date"` and marshals to `null`. `Date` values marshal as their ISO string, matching `JSON.stringify`.

## Math

`Math` is a global object (not a constructor) exposing mathematical constants and functions. Constants are `PI`, `E`, `LN2`, `LN10`, `LOG2E`, `LOG10E`, `SQRT2`, and `SQRT1_2`. Functions are `abs`, `floor`, `ceil`, `round`, `trunc`, `sign`, `max(...)`, `min(...)`, `pow(x, y)`, `sqrt`, `cbrt`, `exp`, `log`, `log2`, `log10`, `sin`, `cos`, `tan`, `asin`, `acos`, `atan`, `atan2(y, x)`, `hypot(...)`, and `random()`. All numeric functions accept and return `float64` and coerce their arguments with the language's number conversion rules; `max`/`min` accept any number of arguments and return `-Infinity`/`Infinity` respectively when called with none; `hypot` accepts any number of arguments (zero yields `0`). `round` rounds half toward positive infinity (`Math.round(-2.5)` is `-2`). `random()` returns a pseudo-random number in `[0, 1)`. As an ordinary object, `Math` may be shadowed or extended by user code.

## Regular expressions

Regular expressions follow the ECMAScript 5 grammar and are implemented by a built-in engine (the `regex` crate is not used). They support character classes, ranges, quantifiers (`*`, `+`, `?`, `{n}`, `{n,}`, `{n,m}`), capturing and non-capturing groups (`(...)`, `(?:...)`), backreferences (`\1`..`\9`), anchors (`^`, `$`, `\b`, `\B`), alternation (`|`), the dot (`.`), and the `g`/`i`/`m` flags. Predefined classes `\d \D \s \S \w \W` use ASCII semantics; `i` uses Unicode case folding.

- A regex literal is written `/pattern/flags` (for example `/^(\d+)-(\d+)$/g`). A `/` is parsed as a regex literal only where a primary expression is expected, so division such as `10 / 2` is unaffected.
- `RegExp(pattern, flags?)` constructs a regex from string arguments. A regex value has read-only properties `source`, `flags`, `global`, `ignoreCase`, and `multiline`, plus methods `test(str)` (returns a boolean) and `exec(str)` (returns an object with numeric keys `"0"`, `"1"`, ... for the full match and captures, plus `index` and `input`, or `null`). `typeof /re/` returns `"object"` and regex values marshal to `null`.

Regex matching uses UTF-16 code-unit semantics. Because Rust strings are UTF-8 and cannot contain lone surrogates, a match boundary that would split a surrogate pair is rounded to the nearest valid UTF-8 character boundary. This is observable only for supplementary (non-BMP) characters matched by a single code-unit atom such as `.`.

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
