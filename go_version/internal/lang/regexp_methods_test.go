package lang

import (
	"reflect"
	"strings"
	"testing"
)

func TestAdditionalStringMethods(t *testing.T) {
	root, _ := run(t, `
		$.last = "a😀a😀".lastIndexOf("😀");
		$.lastFrom = "a😀a😀".lastIndexOf("😀", 2);
		$.compare = ["a".localeCompare("b"), "b".localeCompare("b"), "c".localeCompare("b")];
		$.split = "a, b;c".split("[,;]\\s*");
		$.limited = "a,b,c".split(",", 2);
	`, map[string]any{})
	want := map[string]any{
		"last": float64(3), "lastFrom": float64(1),
		"compare": []any{float64(-1), float64(0), float64(1)},
		"split":   []any{"a", "b", "c"}, "limited": []any{"a", "b"},
	}
	if !reflect.DeepEqual(root, want) {
		t.Fatalf("root=%#v, want %#v", root, want)
	}
}

func TestRegexpMatchAndReplaceMethods(t *testing.T) {
	root, _ := run(t, `
		$.match = "id-42".match("([a-z]+)-(\\d+)");
		$.missing = "abc".match("\\d+");
		$.optional = "b".match("(a)?b");
		$.all = "a1 b22".matchAll("([a-z])(\\d+)");
		$.first = "a1 a2".replace("a(\\d)", "x$1");
		$.every = "a1 a2".replaceAll("a(\\d)", "x$1");
	`, map[string]any{})
	want := map[string]any{
		"match": []any{"id-42", "id", "42"}, "missing": nil,
		"optional": []any{"b", nil},
		"all":      []any{[]any{"a1", "a", "1"}, []any{"b22", "b", "22"}},
		"first":    "x1 a2", "every": "x1 x2",
	}
	if !reflect.DeepEqual(root, want) {
		t.Fatalf("root=%#v, want %#v", root, want)
	}
}

func TestArrayIndexMethods(t *testing.T) {
	root, _ := run(t, `
		a = [1, {name:"x"}, 1, 2];
		$.first = a.indexOf(1);
		$.from = a.indexOf(1, 1);
		$.negative = a.indexOf(1, -2);
		$.object = a.indexOf({name:"x"});
		$.last = a.lastIndexOf(1);
		$.lastFrom = a.lastIndexOf(1, 1);
		$.missing = a.lastIndexOf(9);
	`, map[string]any{})
	want := map[string]any{
		"first": float64(0), "from": float64(2), "negative": float64(2),
		"object": float64(1), "last": float64(2), "lastFrom": float64(0),
		"missing": float64(-1),
	}
	if !reflect.DeepEqual(root, want) {
		t.Fatalf("root=%#v, want %#v", root, want)
	}
}

func TestRegexpMethodErrors(t *testing.T) {
	cases := []struct {
		code, want string
	}{
		{`"x".match("[");`, "invalid regular expression"},
		{`"x".split(1);`, "pattern must be a string"},
		{`"x".split("x", -1);`, "non-negative integer"},
		{`"x".replace("x", 1);`, "replacement must be a string"},
		{`"x".localeCompare(1);`, "string argument"},
		{`[1].indexOf();`, "1 or 2 arguments"},
	}
	for _, tc := range cases {
		_, _, err := Execute(tc.code, map[string]any{}, 100)
		if err == nil || !strings.Contains(err.Error(), tc.want) {
			t.Errorf("%q error=%v", tc.code, err)
		}
	}
}
