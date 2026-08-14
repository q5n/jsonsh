package lang

import (
	"reflect"
	"strings"
	"testing"
)

func TestStringPropertiesAndMethods(t *testing.T) {
	root, _ := run(t, `
		$.length = " A中b ".length;
		$.lower = "Go语言".toLowerCase();
		$.upper = "Go语言".toUpperCase();
		$.trimmed = "  hello \n".trim();
		$.substring = "A中BC".substring(3, 1);
		$.index = "A中BC中".indexOf("中", 2);
		$.missing = "abc".indexOf("z");
		$.padStart = "中x".padStart(5, "ab");
		$.padEnd = "中x".padEnd(5, "😀文");
		$.defaultPad = "x".padStart(3);
		$.emptyPad = "x".padEnd(3, "");
		$.noPad = "hello".padStart(3, "0");
	`, map[string]any{})
	want := map[string]any{
		"length": float64(5), "lower": "go语言", "upper": "GO语言",
		"trimmed": "hello", "substring": "中B", "index": float64(4), "missing": float64(-1),
		"padStart": "aba中x", "padEnd": "中x😀文😀", "defaultPad": "  x",
		"emptyPad": "x", "noPad": "hello",
	}
	if !reflect.DeepEqual(root, want) {
		t.Fatalf("root=%#v, want %#v", root, want)
	}
}

func TestArrayLengthSpliceAndJoin(t *testing.T) {
	root, last := run(t, `
		a = $.items;
		$.before = a.length;
		$.removed = a.splice(-3, 2, "x", "y");
		$.joined = a.join("|");
		a.length;
	`, map[string]any{"items": []any{float64(1), float64(2), float64(3), float64(4)}})
	want := map[string]any{
		"items":  []any{float64(1), "x", "y", float64(4)},
		"before": float64(4), "removed": []any{float64(2), float64(3)}, "joined": "1|x|y|4",
	}
	if !reflect.DeepEqual(root, want) || last != float64(4) {
		t.Fatalf("root=%#v last=%#v", root, last)
	}
}

func TestStringAndArrayMethodEdgeCases(t *testing.T) {
	root, _ := run(t, `
		$.clamped = "A😀BC".substring(-10, 99);
		$.unicodeIndex = "A😀BC".indexOf("B");
		$.defaultJoin = [1, null, "x"].join();
		a = [1, 4];
		$.none = a.splice(1, 0, 2, 3);
		$.tail = a.splice(2);
		$.remaining = a;
	`, map[string]any{})
	want := map[string]any{
		"clamped": "A😀BC", "unicodeIndex": float64(2), "defaultJoin": "1,,x",
		"none": []any{}, "tail": []any{float64(3), float64(4)},
		"remaining": []any{float64(1), float64(2)},
	}
	if !reflect.DeepEqual(root, want) {
		t.Fatalf("root=%#v, want %#v", root, want)
	}
}

func TestTypeofAndToString(t *testing.T) {
	root, _ := run(t, `
		$.types = [typeof("x"), typeof([]), typeof({}), typeof(true), typeof(1), typeof(null)];
		$.strings = ["x".toString(), [1,"x",null].toString(), {b:2,a:1}.toString(), true.toString(), (12.5).toString()];
	`, map[string]any{})
	want := map[string]any{
		"types":   []any{"string", "array", "object", "boolean", "number", "object"},
		"strings": []any{"x", "1,x,", `{"a":1,"b":2}`, "true", "12.5"},
	}
	if !reflect.DeepEqual(root, want) {
		t.Fatalf("root=%#v, want %#v", root, want)
	}
}

func TestPlusUsesToStringWhenEitherOperandIsString(t *testing.T) {
	root, _ := run(t, `
		$.number = "value=" + 2;
		$.boolean = false + "!";
		$.array = "items=" + [1,2];
		$.object = {b:2,a:1} + "";
		$.sum = 2 + 3;
	`, map[string]any{})
	want := map[string]any{
		"number": "value=2", "boolean": "false!", "array": "items=1,2",
		"object": `{"a":1,"b":2}`, "sum": float64(5),
	}
	if !reflect.DeepEqual(root, want) {
		t.Fatalf("root=%#v, want %#v", root, want)
	}
}

func TestRemovedLengthAndHasBuiltins(t *testing.T) {
	for _, code := range []string{`length("x");`, `has([], 1);`} {
		_, _, err := Execute(code, map[string]any{}, 100)
		if err == nil || !strings.Contains(err.Error(), "unknown function") {
			t.Errorf("%q error=%v", code, err)
		}
	}
}

func TestNewMethodArgumentErrors(t *testing.T) {
	cases := []struct {
		code, want string
	}{
		{`"x".substring();`, "1 or 2 arguments"},
		{`"x".indexOf(1);`, "string needle"},
		{`"x".padStart();`, "1 or 2 arguments"},
		{`"x".padEnd(-1);`, "non-negative integer"},
		{`"x".padStart(3, 1);`, "padding must be a string"},
		{`[1].join(2);`, "separator must be a string"},
		{`[1].splice();`, "at least 1 argument"},
		{`typeof();`, "expects 1 argument"},
		{`true.toString(1);`, "expects no arguments"},
	}
	for _, tc := range cases {
		_, _, err := Execute(tc.code, map[string]any{}, 100)
		if err == nil || !strings.Contains(err.Error(), tc.want) {
			t.Errorf("%q error=%v", tc.code, err)
		}
	}
}
