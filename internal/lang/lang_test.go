package lang

import (
	"reflect"
	"strings"
	"testing"
)

func run(t *testing.T, code string, root any) (any, any) {
	t.Helper()
	r, last, err := Execute(code, root, 10000)
	if err != nil {
		t.Fatalf("Execute() error: %v", err)
	}
	return r, last
}

func TestLiteralsOperatorsAndBuiltins(t *testing.T) {
	root := map[string]any{}
	r, _ := run(t, `
		$.text = 'go' + "lang";
		$.math = 1 + 2 * 3;
		$.logic = 0 || (2 > 1 && !false);
		$.array = [1, {name: "x"}, true, null,];
		$.len = "中文a".length;
		$.has = $.text.indexOf("lang") >= 0;
		$.keys = keys({b: 1, a: 2});
	`, root)
	want := map[string]any{"text": "golang", "math": float64(7), "logic": true, "array": []any{float64(1), map[string]any{"name": "x"}, true, nil}, "len": float64(3), "has": true, "keys": []any{"a", "b"}}
	if !reflect.DeepEqual(r, want) {
		t.Fatalf("got %#v, want %#v", r, want)
	}
}

func TestControlFlowMutationAndDelete(t *testing.T) {
	root := map[string]any{"users": []any{
		map[string]any{"score": float64(70), "tags": []any{"go"}, "secret": true},
		map[string]any{"score": float64(20), "tags": []any{"blocked"}, "secret": true},
		map[string]any{"score": float64(90), "tags": []any{"go"}, "secret": true},
	}}
	r, _ := run(t, `
		total = 0;
		for (i in $.users) {
			u = $.users[i];
			if (u.tags[0] == "blocked") { delete u.secret; continue; }
			total += u.score;
			if (total > 100) { break; }
		}
		$.total = total;
		delete $.users[1];
	`, root)
	o := r.(map[string]any)
	if o["total"] != float64(160) {
		t.Fatalf("total=%v", o["total"])
	}
	users := o["users"].([]any)
	if len(users) != 2 {
		t.Fatalf("users length=%d", len(users))
	}
}

func TestShortCircuitAndErrors(t *testing.T) {
	_, _ = run(t, `x = false && missing.value; y = true || missing.value;`, map[string]any{})
	cases := []struct{ code, contains string }{
		{`x = 1 / 0;`, "division by zero"},
		{`x = true - 1;`, "incompatible operand types"},
		{`break;`, "outside loop"},
		{`x = 1 y = 2;`, "expected ';'"},
	}
	for _, tc := range cases {
		_, _, e := Execute(tc.code, map[string]any{}, 100)
		if e == nil || !strings.Contains(e.Error(), tc.contains) {
			t.Errorf("%q error=%v", tc.code, e)
		}
	}
}

func TestMissingObjectPropertyReturnsNull(t *testing.T) {
	root, last := run(t, `
		$.dot = $.missing;
		$.bracket = $["alsoMissing"];
		$.compound = {};
		$.compound.value += "suffix";
		$.dot;
	`, map[string]any{})
	want := map[string]any{
		"dot": nil, "bracket": nil,
		"compound": map[string]any{"value": "nullsuffix"},
	}
	if !reflect.DeepEqual(root, want) || last != nil {
		t.Fatalf("root=%#v last=%#v, want root=%#v last=nil", root, last, want)
	}
}

func TestDeepEqualityAndDynamicAccess(t *testing.T) {
	r, _ := run(t, `key="item"; $.same = [1,{a:true}] == [1,{a:true}]; $[key] = 3; $[key] *= 2; $.first = keys({b:1,a:2})[0];`, map[string]any{})
	o := r.(map[string]any)
	if o["same"] != true || o["item"] != float64(6) || o["first"] != "a" {
		t.Fatalf("got %#v", o)
	}
}

func TestArrayPushMutatesSharedArrayAndReturnsLength(t *testing.T) {
	r, last := run(t, `items=$.items; size=items.push(2, {name:"three"}); size;`, map[string]any{"items": []any{float64(1)}})
	o := r.(map[string]any)
	items := o["items"].([]any)
	if len(items) != 3 || items[1] != float64(2) || items[2].(map[string]any)["name"] != "three" {
		t.Fatalf("items=%#v", items)
	}
	if last != float64(3) {
		t.Fatalf("last=%#v", last)
	}
}

func TestArrayPushErrors(t *testing.T) {
	for _, tc := range []struct{ code, want string }{{`$.items.push();`, "at least 1"}, {`$.name.push(1);`, "array receiver"}, {`$.items.unknown(1);`, "unknown method"}} {
		_, _, err := Execute(tc.code, map[string]any{"items": []any{}, "name": "x"}, 100)
		if err == nil || !strings.Contains(err.Error(), tc.want) {
			t.Errorf("%q error=%v", tc.code, err)
		}
	}
}

func TestRootCanBeAssignedDirectly(t *testing.T) {
	r, last := run(t, `$ = {items: [1]}; $.items.push(2); $;`, map[string]any{"old": true})
	want := map[string]any{"items": []any{float64(1), float64(2)}}
	if !reflect.DeepEqual(r, want) || !reflect.DeepEqual(last, want) {
		t.Fatalf("root=%#v last=%#v", r, last)
	}
	r, _ = run(t, `$ = 5; $ += 2;`, nil)
	if r != float64(7) {
		t.Fatalf("compound root=%#v", r)
	}
}

func TestTopLevelObjectLiteralIsAnExpression(t *testing.T) {
	root, last := run(t, `{age: 18}`, nil)
	if root != nil {
		t.Fatalf("root=%#v, want nil", root)
	}
	want := map[string]any{"age": float64(18)}
	if !reflect.DeepEqual(last, want) {
		t.Fatalf("last=%#v, want %#v", last, want)
	}

	_, empty := run(t, `{}`, nil)
	if !reflect.DeepEqual(empty, map[string]any{}) {
		t.Fatalf("empty object=%#v", empty)
	}
}
