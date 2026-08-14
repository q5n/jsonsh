package lang

import (
	"bytes"
	"reflect"
	"testing"
)

func TestLogWritesValuesAndReturnsNull(t *testing.T) {
	var output bytes.Buffer
	root := map[string]any{"unchanged": true}
	gotRoot, last, err := ExecuteWithOutput(`
		log("hello", 2, true, null, {a:1}, [1,2]);
		log();
	`, root, 1000, &output)
	if err != nil {
		t.Fatal(err)
	}
	if want := "hello 2 true null {\"a\":1} 1,2\n\n"; output.String() != want {
		t.Fatalf("output=%q, want %q", output.String(), want)
	}
	if !reflect.DeepEqual(gotRoot, root) || last != nil {
		t.Fatalf("root=%#v last=%#v", gotRoot, last)
	}
}

func TestExecuteDiscardsLogOutputByDefault(t *testing.T) {
	if _, _, err := Execute(`log("hidden");`, nil, 100); err != nil {
		t.Fatal(err)
	}
}

func TestLogPreservesUnicodeEscapes(t *testing.T) {
	var output bytes.Buffer
	_, _, err := ExecuteWithOutput(`log({icon: "\uee63"});`, nil, 1000, &output)
	if err != nil {
		t.Fatal(err)
	}
	if got, want := output.String(), "{\"icon\":\"\\uee63\"}\n"; got != want {
		t.Fatalf("output=%q want %q", got, want)
	}
}
