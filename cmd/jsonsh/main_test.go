package main

import (
	"bytes"
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
	if err := run([]string{"-e", `length($.items)`, "-r", "-c"}, strings.NewReader(`{"items":[1,2]}`), &out); err != nil {
		t.Fatal(err)
	}
	if got, want := out.String(), "2\n"; got != want {
		t.Fatalf("got %q want %q", got, want)
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
		if !strings.Contains(out.String(), "用法:") ||
			!strings.Contains(out.String(), "--max-steps") ||
			!strings.Contains(out.String(), "--version") ||
			!strings.Contains(out.String(), "--pretty") ||
			!strings.Contains(out.String(), "JSON/JSONC") ||
			!strings.Contains(out.String(), "$ = value") ||
			!strings.Contains(out.String(), "length(value)") ||
			!strings.Contains(out.String(), "has(value, item)") ||
			!strings.Contains(out.String(), "keys(value)") ||
			!strings.Contains(out.String(), "array.push(value, ...)") {
			t.Fatalf("%s returned incomplete help: %q", option, out.String())
		}
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
