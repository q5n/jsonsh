package jsonc

import (
	"reflect"
	"strings"
	"testing"
)

func TestPreserveUnchangedExactly(t *testing.T) {
	src := "\xEF\xBB\xBF// header\r\n{\r\n\t\"n\" : 1e2, // number\r\n\t\"s\": \"a\\u0062\"\r\n}\r\n"
	doc, err := Parse(src)
	if err != nil {
		t.Fatal(err)
	}
	got, err := doc.Preserve(Clone(doc.Root.Value))
	if err != nil {
		t.Fatal(err)
	}
	if got != src {
		t.Fatalf("source changed:\n%q\nwant:\n%q", got, src)
	}
}

func TestDeletingMemberRemovesItsInlineComment(t *testing.T) {
	doc, err := Parse("{\n  \"a\": 1 // owned\n}")
	if err != nil {
		t.Fatal(err)
	}
	got, err := doc.Preserve(map[string]any{})
	if err != nil {
		t.Fatal(err)
	}
	if strings.Contains(got, "owned") {
		t.Fatalf("comment was not removed: %q", got)
	}
	if _, err = Parse(got); err != nil {
		t.Fatalf("invalid result %q: %v", got, err)
	}
}

func TestPreserveOnlyChangedScalar(t *testing.T) {
	src := "{\n    // 商品价格\n    \"price\" : 100,\n\n    \"name\": \"book\"\n}\n"
	doc, err := Parse(src)
	if err != nil {
		t.Fatal(err)
	}
	v := Clone(doc.Root.Value).(map[string]any)
	v["price"] = float64(80)
	got, err := doc.Preserve(v)
	if err != nil {
		t.Fatal(err)
	}
	want := strings.Replace(src, "100", "80", 1)
	if got != want {
		t.Fatalf("got:\n%s\nwant:\n%s", got, want)
	}
}

func TestAddAndDeleteMembersKeepStyle(t *testing.T) {
	src := "{\n  \"a\": 1,\n  // remove with b\n  \"b\": 2\n}\n"
	doc, err := Parse(src)
	if err != nil {
		t.Fatal(err)
	}
	v := Clone(doc.Root.Value).(map[string]any)
	delete(v, "b")
	v["c"] = float64(3)
	got, err := doc.Preserve(v)
	if err != nil {
		t.Fatal(err)
	}
	want := "{\n  \"a\": 1,\n  \"c\": 3\n}\n"
	if got != want {
		t.Fatalf("got:\n%s\nwant:\n%s", got, want)
	}
}

func TestAddingToEmptyContainerKeepsSingleLineStyle(t *testing.T) {
	doc, err := Parse(`{"object":{},"array":[ ]}`)
	if err != nil {
		t.Fatal(err)
	}
	v := Clone(doc.Root.Value).(map[string]any)
	v["object"].(map[string]any)["a"] = float64(1)
	v["array"] = []any{true}
	got, err := doc.Preserve(v)
	if err != nil {
		t.Fatal(err)
	}
	if got != `{"object":{"a": 1},"array":[ true ]}` {
		t.Fatalf("got %q", got)
	}
}

func TestArrayDeletionReusesRemainingNodes(t *testing.T) {
	src := "[\n  1,\n  // remove\n  2,\n  /* keep */ 3\n]"
	doc, err := Parse(src)
	if err != nil {
		t.Fatal(err)
	}
	v := []any{float64(1), float64(3)}
	got, err := doc.Preserve(v)
	if err != nil {
		t.Fatal(err)
	}
	if strings.Contains(got, "remove") || !strings.Contains(got, "keep") {
		t.Fatalf("unexpected comments:\n%s", got)
	}
	reparsed, err := Parse(got)
	if err != nil {
		t.Fatal(err)
	}
	if !reflect.DeepEqual(reparsed.Root.Value, v) {
		t.Fatalf("value=%#v", reparsed.Root.Value)
	}
}

func TestPrettyPreservesComments(t *testing.T) {
	src := `{"a":1,// note
"empty":{},"items":[true,false]}`
	got, err := PrettyPreserve(src, "  ")
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(got, "// note") || !strings.Contains(got, "\n  \"items\": [") {
		t.Fatalf("got:\n%s", got)
	}
	if _, err = Parse(got); err != nil {
		t.Fatalf("pretty output invalid: %v\n%s", err, got)
	}
}

func TestJSONCErrors(t *testing.T) {
	for _, src := range []string{`{"a":01}`, `{/* broken`, `{"a":1} extra`, `{"a":1,"a":2}`} {
		if _, err := Parse(src); err == nil {
			t.Errorf("expected error for %q", src)
		}
	}
}
