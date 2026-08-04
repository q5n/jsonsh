# jsonsh Language and Command-Line Specification (MVP)

## Goal

`jsonsh` is a dependency-free command-line tool written in Go. It reads a JSONC value, binds it to the predefined global variable `$`, executes a JavaScript-like script, and outputs either the modified `$` value or the result of the last expression. JSONC input extends standard JSON with line comments, block comments, and trailing commas. The scripting language is intentionally small and does not claim JavaScript compatibility.

## Data types and variables

The runtime supports only `null`, `boolean`, `number` (`float64`), `string`, `array`, and `object`. There are no implicit type conversions. Assigning to a regular identifier for the first time creates a global variable; reading an undefined variable is an error. There is no block scope. `$` is the predefined root variable. Its members can be modified, and `$ = value` can replace the entire root value. Deleting `$` remains prohibited.

Literals include single- and double-quoted strings, JSON-format numbers, booleans, null, arrays, and objects. Object keys may be strings or identifiers, and script literals may contain trailing commas. Strings support common escape sequences and `\uXXXX`. Scripts support `//` and `/* ... */` comments.

## Expressions

Operator precedence, from lowest to highest, is: assignment `= += -= *= /=` (right-associative), `||`, `&&`, `== !=`, `> >= < <=`, `+ -`, `* /`, unary `! -`, and finally member access and calls.

- `+` accepts two numbers for addition or two strings for concatenation. Other arithmetic operators accept numbers only. Division by zero is an error.
- Equality operators do not convert types. Arrays and objects use recursive deep equality.
- Ordered comparisons accept either two numbers or two strings.
- Falsy values are `null`, `false`, numeric zero, and the empty string. Arrays and objects are always truthy. `&&` and `||` short-circuit and return booleans.
- Multi-level access supports `.name`, `[number]`, and `[string-expression]`. Missing properties, invalid types, non-integer indexes, negative indexes, and out-of-range indexes are errors.

An assignment target must be a variable or member expression. Compound assignments follow the rules of their corresponding arithmetic operators.

## Statements

Statements are separated by semicolons. The final semicolon in a block, or immediately before `}`, may be omitted. Control-flow bodies must use braces.

```js
if (condition) { ... } else if (condition) { ... } else { ... }
for (key in object) { ... }
for (index in array) { ... }
delete object.member;
break;
continue;
```

`for..in` iterates over numeric array indexes or object keys in lexicographic order. A key snapshot is created when the loop starts, and members deleted before their turn are skipped. Loop variables live in global scope. `break` and `continue` apply only to the nearest enclosing loop and are errors outside a loop. Deleting an array element shifts subsequent elements toward the beginning. Deleting a missing member is an error. Regular variables and `$` cannot be deleted.

## Built-in functions and methods

- `length(v)` returns the number of array elements, object properties, or Unicode characters in a string.
- `has(container, value)` searches an array using deep equality, checks an object for a string key, or checks a string for a substring.
- `keys(v)` returns object keys in lexicographic order or numeric array indexes.

Arrays provide the built-in method `array.push(value, ...)`. It accepts one or more arguments, appends them in order, mutates the array in place, and returns the new length. Mutations are visible through every variable referencing the same array. Calling it without arguments, calling it on a non-array receiver, or calling an unknown method is a runtime error.

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
    --max-steps N      Maximum interpreter steps (default: 1000000)
```

When `INPUT` is omitted, input is read from standard input. `-e` and `-f`, `-o` and `-i`, and `-p` and `-c` are mutually exclusive. `-i` requires an input file. The default mode applies minimal changes and preserves the original source structure. `--pretty` reformats while retaining comments. `--compact` emits comment-free standard JSON. In-place writes first create a temporary file in the same directory and replace the input only after the output has been written and closed successfully. See [jsonc-preserve.md](jsonc-preserve.md) for details.

Lexical, syntax, and runtime errors include line and column positions, are written to standard error, and produce a nonzero exit code. Failed execution does not write an output file. Function definitions, conversions, `return`, `while`, traditional `for`, increment operators, modulo, ternary expressions, slicing, and JavaScript standard objects are not supported yet.
