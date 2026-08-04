package lang

import (
	"reflect"
	"strings"
	"testing"
)

func TestForOfArrayValuesAndControlFlow(t *testing.T) {
	root, _, err := Execute(`
		$.out = [];
		for (item of $.values) {
			if (item == 2) { continue; }
			if (item == 4) { break; }
			$.out.push(item);
		}
	`, map[string]any{"values": []any{float64(1), float64(2), float64(3), float64(4), float64(5)}}, 10000)
	if err != nil {
		t.Fatal(err)
	}
	want := map[string]any{
		"values": []any{float64(1), float64(2), float64(3), float64(4), float64(5)},
		"out":    []any{float64(1), float64(3)},
	}
	if !reflect.DeepEqual(root, want) {
		t.Fatalf("root = %#v, want %#v", root, want)
	}
}

func TestForOfArrayUsesLiveIterator(t *testing.T) {
	root, _, err := Execute(`
		$.seen = [];
		for (item of $.values) {
			$.seen.push(item);
			if (item == 1) { delete $.values[0]; }
			if (item == 3) { $.values.push(4); }
		}
	`, map[string]any{"values": []any{float64(1), float64(2), float64(3)}}, 10000)
	if err != nil {
		t.Fatal(err)
	}
	want := map[string]any{
		"values": []any{float64(2), float64(3), float64(4)},
		"seen":   []any{float64(1), float64(3), float64(4)},
	}
	if !reflect.DeepEqual(root, want) {
		t.Fatalf("root = %#v, want %#v", root, want)
	}
}

func TestForOfStringUsesUnicodeCodePoints(t *testing.T) {
	root, _, err := Execute(`$.out = []; for (ch of "A😀界") { $.out.push(ch); }`, map[string]any{}, 10000)
	if err != nil {
		t.Fatal(err)
	}
	want := map[string]any{"out": []any{"A", "😀", "界"}}
	if !reflect.DeepEqual(root, want) {
		t.Fatalf("root = %#v, want %#v", root, want)
	}
}

func TestForOfRejectsNonIterable(t *testing.T) {
	_, _, err := Execute(`for (item of $) {}`, map[string]any{}, 1000)
	if err == nil || !strings.Contains(err.Error(), "for..of requires array or string") {
		t.Fatalf("err = %v", err)
	}
}

func TestEmptyStatementsAndTrailingBlockSemicolon(t *testing.T) {
	root, _, err := Execute(`;;; { $.a = 1;;; };;;; if (true) { $.b = 2; };;; for (k in $) { break; };;;`, map[string]any{}, 10000)
	if err != nil {
		t.Fatal(err)
	}
	want := map[string]any{"a": float64(1), "b": float64(2)}
	if !reflect.DeepEqual(root, want) {
		t.Fatalf("root = %#v, want %#v", root, want)
	}
}
