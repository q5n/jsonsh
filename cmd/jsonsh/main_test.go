package main

import (
	"bytes"
	"errors"
	"strings"
	"testing"

	"jsonsh/internal/jsonc"
)

func TestRunMutationAndResult(t *testing.T) {
	var out bytes.Buffer
	if err := run([]string{"-e", `$.n += 2`, "-c"}, strings.NewReader(`{"n":3}`), &out); err != nil {
		t.Fatal(err)
	}
	if got, want := out.String(), "{\"n\":5}\n"; got != want {
		t.Fatalf("got %q want %q", got, want)
	}
	out.Reset()
	if err := run([]string{"-e", `$.items.length`, "-r", "-c"}, strings.NewReader(`{"items":[1,2]}`), &out); err != nil {
		t.Fatal(err)
	}
	if got, want := out.String(), "2\n"; got != want {
		t.Fatalf("got %q want %q", got, want)
	}
}

func TestRunEmptyExpressionDoesNothing(t *testing.T) {
	src := "{\n  // keep this comment\n  \"items\": [1, 2],\n}\n"
	var out bytes.Buffer
	if err := run([]string{"-e", ""}, strings.NewReader(src), &out); err != nil {
		t.Fatal(err)
	}
	if got := out.String(); got != src {
		t.Fatalf("got %q want %q", got, src)
	}

	out.Reset()
	if err := run([]string{"--expression", "", "--compact"}, strings.NewReader(src), &out); err != nil {
		t.Fatal(err)
	}
	if got, want := out.String(), "{\"items\":[1,2]}\n"; got != want {
		t.Fatalf("got %q want %q", got, want)
	}

	out.Reset()
	if err := run([]string{"-e", "", "--result", "--compact"}, strings.NewReader(src), &out); err != nil {
		t.Fatal(err)
	}
	if got, want := out.String(), "null\n"; got != want {
		t.Fatalf("got %q want %q", got, want)
	}
}

type failingReader struct{}

func (failingReader) Read([]byte) (int, error) {
	return 0, errors.New("input should not be read")
}

func TestNullInputInitializesRootWithoutReading(t *testing.T) {
	var out bytes.Buffer
	if err := run([]string{"-n", "-e", `$ = {ready: true}`, "-c"}, failingReader{}, &out); err != nil {
		t.Fatal(err)
	}
	if got, want := out.String(), "{\"ready\":true}\n"; got != want {
		t.Fatalf("got %q want %q", got, want)
	}

	out.Reset()
	if err := run([]string{"--null-input", "-e", `$`, "-r", "-c"}, failingReader{}, &out); err != nil {
		t.Fatal(err)
	}
	if got, want := out.String(), "null\n"; got != want {
		t.Fatalf("got %q want %q", got, want)
	}
}

func TestNullInputCanReturnObjectLiteral(t *testing.T) {
	var out bytes.Buffer
	if err := run([]string{"-n", "-r", "-e", `{age:18}`}, failingReader{}, &out); err != nil {
		t.Fatal(err)
	}
	if got, want := out.String(), "{\n  \"age\": 18\n}\n"; got != want {
		t.Fatalf("got %q want %q", got, want)
	}
}

func TestNullInputRejectsInputFileAndInPlace(t *testing.T) {
	for _, args := range [][]string{
		{"-n", "-e", `$`, "input.json"},
		{"-n", "-e", `$`, "-i"},
	} {
		err := run(args, failingReader{}, &bytes.Buffer{})
		if err == nil || !strings.Contains(err.Error(), "null-input cannot be used") {
			t.Fatalf("run(%v) error = %v", args, err)
		}
	}
}

func TestRunRejectsTrailingJSON(t *testing.T) {
	err := run([]string{"-e", "x=1"}, strings.NewReader(`{} {}`), &bytes.Buffer{})
	if err == nil {
		t.Fatal("expected error")
	}
}

func TestHelp(t *testing.T) {
	for _, option := range []string{"-h", "-help", "--help"} {
		var out bytes.Buffer
		if err := run([]string{option}, strings.NewReader(""), &out); err != nil {
			t.Fatalf("%s returned error: %v", option, err)
		}
		if !strings.Contains(out.String(), "jsonsh "+version+" -") ||
			!strings.Contains(out.String(), "Usage:") ||
			!strings.Contains(out.String(), "--max-steps") ||
			!strings.Contains(out.String(), "--version") ||
			!strings.Contains(out.String(), "--language-help") ||
			!strings.Contains(out.String(), "--pretty") ||
			!strings.Contains(out.String(), "--null-input") ||
			!strings.Contains(out.String(), "JSON/JSONC") ||
			!strings.Contains(out.String(), "$ = value") ||
			!strings.Contains(out.String(), "--language-help") {
			t.Fatalf("%s returned incomplete help: %q", option, out.String())
		}
		for _, hidden := range []string{"Properties:", "Built-in functions:", "String methods:", "Array methods:"} {
			if strings.Contains(out.String(), hidden) {
				t.Errorf("%s unexpectedly includes language section %q", option, hidden)
			}
		}
	}
}

func TestLanguageHelp(t *testing.T) {
	var out bytes.Buffer
	if err := run([]string{"--language-help"}, strings.NewReader("not JSON"), &out); err != nil {
		t.Fatalf("run returned error: %v", err)
	}
	text := out.String()
	for _, want := range []string{
		"jsonsh " + version + " scripting language reference",
		"Values and literals:", "Operators, from lowest", "for (value of array)",
		"typeof(value)", "string.length", "array.length", "toLowerCase()",
		"lastIndexOf(text[, start])", "matchAll(pattern)", "replaceAll(pattern, replacement)",
		"splice(start[, deleteCount, ...items])", "lastIndexOf(value[, start])",
		"Go regular expressions", "typeof(null)",
	} {
		if !strings.Contains(text, want) {
			t.Errorf("language help is missing %q", want)
		}
	}
}

func TestNoArgumentsShowsHelp(t *testing.T) {
	var out bytes.Buffer
	if err := run(nil, strings.NewReader(""), &out); err != nil {
		t.Fatalf("run returned error: %v", err)
	}
	if !strings.Contains(out.String(), "jsonsh "+version+" -") ||
		!strings.Contains(out.String(), "Usage:") {
		t.Fatalf("run returned incomplete help: %q", out.String())
	}
}

func TestVersion(t *testing.T) {
	oldVersion := version
	version = "v1.2.3"
	t.Cleanup(func() { version = oldVersion })

	for _, option := range []string{"-v", "--version"} {
		var out bytes.Buffer
		if err := run([]string{option}, strings.NewReader(""), &out); err != nil {
			t.Fatalf("%s returned error: %v", option, err)
		}
		if got, want := out.String(), "jsonsh v1.2.3\n"; got != want {
			t.Fatalf("%s returned %q, want %q", option, got, want)
		}
	}
}

func TestRunPreservesJSONCStructureByDefault(t *testing.T) {
	src := "{\r\n\t// keep\r\n\t\"price\" : 100,\r\n\t\"name\": \"book\"\r\n}\r\n"
	var out bytes.Buffer
	if err := run([]string{"-e", `$.price = 80`}, strings.NewReader(src), &out); err != nil {
		t.Fatal(err)
	}
	want := strings.Replace(src, "100", "80", 1)
	if out.String() != want {
		t.Fatalf("got %q want %q", out.String(), want)
	}
}

func TestRunJSONCOutputModes(t *testing.T) {
	src := "{/* note */\"a\":1}"
	var out bytes.Buffer
	if err := run([]string{"-e", `$.a = 2`, "--compact"}, strings.NewReader(src), &out); err != nil {
		t.Fatal(err)
	}
	if out.String() != `{"a":2}`+"\n" {
		t.Fatalf("compact=%q", out.String())
	}
	out.Reset()
	if err := run([]string{"-e", `$.a = 2`, "--pretty"}, strings.NewReader(src), &out); err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(out.String(), "/* note */") {
		t.Fatalf("pretty lost comment: %q", out.String())
	}
	if err := run([]string{"-e", `$.a = 2`, "--pretty", "--compact"}, strings.NewReader(src), &bytes.Buffer{}); err == nil {
		t.Fatal("expected option conflict")
	}
}

func TestRunPushPreservesExistingArrayContent(t *testing.T) {
	src := "{\n  \"items\": [\n    1 // existing\n  ]\n}\n"
	var out bytes.Buffer
	if err := run([]string{"-e", `$.items.push(2)`}, strings.NewReader(src), &out); err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(out.String(), "1, // existing") || !strings.Contains(out.String(), "2") {
		t.Fatalf("output=%q", out.String())
	}
	if _, err := jsonc.Parse(out.String()); err != nil {
		t.Fatalf("invalid JSONC: %v\n%s", err, out.String())
	}
}

func TestRunCanReplaceRootAndKeepsOuterTrivia(t *testing.T) {
	src := "// before\n{\"old\":true}\n// after\n"
	var out bytes.Buffer
	if err := run([]string{"-e", `$ = [1, 2]`}, strings.NewReader(src), &out); err != nil {
		t.Fatal(err)
	}
	want := "// before\n[1,2]\n// after\n"
	if out.String() != want {
		t.Fatalf("got %q want %q", out.String(), want)
	}
}
