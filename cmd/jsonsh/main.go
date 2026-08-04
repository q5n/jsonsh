package main

import (
	"encoding/json"
	"errors"
	"flag"
	"fmt"
	"io"
	"os"
	"path/filepath"

	"jsonsh/internal/jsonc"
	"jsonsh/internal/lang"
)

type options struct {
	expr, script, output             string
	result, compact, pretty, inPlace bool
	showVersion                      bool
	maxSteps                         int
}

var version = "dev"

func main() {
	if err := run(os.Args[1:], os.Stdin, os.Stdout); err != nil {
		fmt.Fprintln(os.Stderr, "jsonsh:", err)
		os.Exit(1)
	}
}
func run(args []string, stdin io.Reader, stdout io.Writer) error {
	var o options
	fs := flag.NewFlagSet("jsonsh", flag.ContinueOnError)
	fs.SetOutput(stdout)
	fs.Usage = func() {
		fmt.Fprintf(stdout, `jsonsh %s - process JSON/JSONC with JavaScript-like expressions

Usage:
  jsonsh (-e CODE | -f SCRIPT) [options] [INPUT]

If INPUT is omitted, input is read from standard input. Line comments,
block comments, and trailing commas are supported. By default, only changed
values are replaced, preserving the original formatting and comments.

Scripts:
  -e, --expression CODE  Execute the specified code
  -f, --script FILE      Read code from a UTF-8 file

Root variable:
  $                       Current JSON root value
  $ = value               Replace the entire JSON root value

Output:
  -r, --result            Print the last expression result (default: modified $)
  -p, --pretty            Pretty-print output while preserving comments
  -c, --compact           Print compact standard JSON without comments
  -o, --output FILE       Write output to a file
  -i, --in-place          Safely replace the input file

Built-in functions:
  length(value)           Return the length of an array, object, or string
  has(value, item)        Check for an array item, object key, or substring
  keys(value)             Return ordered object keys or numeric array indexes

Built-in function examples:
  length($.users)
  has($.tags, "go")
  keys($.user)

Array methods:
  array.push(value, ...)  Append values to an array and return its new length

Array method example:
  $.users.push({name: "Tom"})

Other:
      --max-steps N       Maximum execution steps (default: 1000000)
  -v, --version           Show version
  -h, -help, --help       Show help

Examples:
  jsonsh -e "$.price *= 0.8" input.json
  jsonsh -e "length($.users)" -r input.json
  jsonsh -f update.js -i input.json
`, version)
	}
	fs.StringVar(&o.expr, "e", "", "script expression")
	fs.StringVar(&o.expr, "expression", "", "script expression")
	fs.StringVar(&o.script, "f", "", "script file")
	fs.StringVar(&o.script, "script", "", "script file")
	fs.BoolVar(&o.result, "r", false, "output last result")
	fs.BoolVar(&o.result, "result", false, "output last result")
	fs.BoolVar(&o.compact, "c", false, "compact JSON")
	fs.BoolVar(&o.compact, "compact", false, "compact JSON")
	fs.BoolVar(&o.pretty, "p", false, "pretty JSONC")
	fs.BoolVar(&o.pretty, "pretty", false, "pretty JSONC")
	fs.StringVar(&o.output, "o", "", "output file")
	fs.StringVar(&o.output, "output", "", "output file")
	fs.BoolVar(&o.inPlace, "i", false, "replace input file")
	fs.BoolVar(&o.inPlace, "in-place", false, "replace input file")
	fs.BoolVar(&o.showVersion, "v", false, "show version")
	fs.BoolVar(&o.showVersion, "version", false, "show version")
	fs.IntVar(&o.maxSteps, "max-steps", 1000000, "maximum execution steps")
	if err := fs.Parse(args); err != nil {
		if errors.Is(err, flag.ErrHelp) {
			return nil
		}
		return err
	}
	if len(args) == 0 {
		fs.Usage()
		return nil
	}
	if o.showVersion {
		_, err := fmt.Fprintf(stdout, "jsonsh %s\n", version)
		return err
	}
	if (o.expr == "") == (o.script == "") {
		return errors.New("exactly one of -e or -f is required")
	}
	if o.output != "" && o.inPlace {
		return errors.New("-o and -i are mutually exclusive")
	}
	if o.compact && o.pretty {
		return errors.New("--compact and --pretty are mutually exclusive")
	}
	if fs.NArg() > 1 {
		return errors.New("only one input file is supported")
	}
	input := ""
	if fs.NArg() == 1 {
		input = fs.Arg(0)
	}
	if o.inPlace && input == "" {
		return errors.New("-i requires an input file")
	}
	if o.maxSteps <= 0 {
		return errors.New("--max-steps must be positive")
	}
	code := o.expr
	if o.script != "" {
		b, e := os.ReadFile(o.script)
		if e != nil {
			return fmt.Errorf("read script: %w", e)
		}
		code = string(b)
	}
	var rd io.Reader = stdin
	if input != "" {
		f, e := os.Open(input)
		if e != nil {
			return fmt.Errorf("open input: %w", e)
		}
		defer f.Close()
		rd = f
	}
	raw, e := io.ReadAll(rd)
	if e != nil {
		return fmt.Errorf("read input: %w", e)
	}
	doc, e := jsonc.Parse(string(raw))
	if e != nil {
		return e
	}
	root := jsonc.Clone(doc.Root.Value)
	root, last, e := lang.Execute(code, root, o.maxSteps)
	if e != nil {
		return e
	}
	var output string
	if o.result {
		var data []byte
		if o.compact {
			data, e = json.Marshal(last)
		} else {
			data, e = json.MarshalIndent(last, "", "  ")
		}
		output = string(data) + "\n"
	} else if o.compact {
		output, e = jsonc.Compact(root)
		output += "\n"
	} else {
		output, e = doc.Preserve(root)
		if e == nil && o.pretty {
			output, e = jsonc.PrettyPreserve(output, "  ")
			output += "\n"
		}
	}
	if e != nil {
		return fmt.Errorf("encode output: %w", e)
	}
	data := []byte(output)
	if o.inPlace {
		return replaceFile(input, data)
	}
	if o.output != "" {
		return os.WriteFile(o.output, data, 0644)
	}
	_, e = stdout.Write(data)
	return e
}
func replaceFile(path string, data []byte) error {
	dir := filepath.Dir(path)
	f, e := os.CreateTemp(dir, ".jsonsh-*")
	if e != nil {
		return e
	}
	tmp := f.Name()
	ok := false
	defer func() {
		if !ok {
			_ = os.Remove(tmp)
		}
	}()
	if _, e = f.Write(data); e != nil {
		f.Close()
		return e
	}
	if e = f.Sync(); e != nil {
		f.Close()
		return e
	}
	if e = f.Close(); e != nil {
		return e
	}
	backup, e := os.CreateTemp(dir, ".jsonsh-backup-*")
	if e != nil {
		return e
	}
	backupPath := backup.Name()
	if e = backup.Close(); e != nil {
		return e
	}
	if e = os.Remove(backupPath); e != nil {
		return e
	}
	if e = os.Rename(path, backupPath); e != nil {
		return e
	}
	if e = os.Rename(tmp, path); e != nil {
		_ = os.Rename(backupPath, path)
		return e
	}
	_ = os.Remove(backupPath)
	ok = true
	return nil
}
