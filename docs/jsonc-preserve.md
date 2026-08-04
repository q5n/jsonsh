# JSONC and Source-Structure Preservation

## Output modes

- The default mode applies minimal changes. Nodes with no semantic changes reuse their original source text, preserving property order, whitespace, line endings, comments, string escapes, and number formatting.
- `-p` / `--pretty` reindents the result while preserving comments.
- `-c` / `--compact` emits compact standard JSON and removes comments.
- `--pretty` and `--compact` are mutually exclusive.

## Input syntax

Input uses JSONC: standard JSON extended with `//` line comments, `/* ... */` block comments, and trailing commas. Single-quoted strings, unquoted property names, `NaN`, and `Infinity` are not supported. These restrictions apply to JSONC input only; the scripting language has its own syntax rules.

## Implementation

A JSONC parser written in pure Go converts the input into a syntax tree containing source ranges. Whitespace and comments are retained as trivia between members. The interpreter runs against a copy of the input value. During output, the original nodes are recursively compared with the resulting value: unchanged nodes are reused byte for byte, changed scalars replace only their value text, objects merge added and removed properties while retaining the original property order, and arrays use sequence matching to reuse unchanged elements when their length changes.

When a member is deleted, immediately attached leading comments are deleted with it, while comments belonging to the container are preserved. When a member is added, its style is inferred from the container's existing line endings, indentation, and spacing around colons. New object properties are appended in stable lexicographic order.

In-place output is fully generated before the input file is replaced. Lexical and syntax errors include their input line and column. Failed execution never writes to the destination.

## Acceptance criteria

- When only a scalar changes, all input bytes except the target value remain unchanged.
- LF/CRLF line endings, tab/space indentation, property order, blank lines, and comments are preserved.
- Unchanged string escapes and numeric literal forms remain unchanged.
- Added and deleted object members, array elements, and nested structures are handled correctly.
- Mutations performed through variable aliases and loops are preserved.
- `--pretty` retains comments and produces consistent indentation.
- `--compact` produces strict JSON accepted by Go's standard library.
