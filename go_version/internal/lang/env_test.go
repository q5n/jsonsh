package lang

import (
	"reflect"
	"strings"
	"testing"
)

func TestEnvReadsEnvironmentVariables(t *testing.T) {
	t.Setenv("JSONSH_ENV_TEST", "available")
	t.Setenv("JSONSH_EMPTY_ENV_TEST", "")

	root, _, err := Execute(`
		$ = {
			value: env("JSONSH_ENV_TEST"),
			empty: env("JSONSH_EMPTY_ENV_TEST"),
			missing: env("JSONSH_MISSING_ENV_TEST_7B3D2A"),
		};
	`, nil, 1000)
	if err != nil {
		t.Fatal(err)
	}
	want := map[string]any{"value": "available", "empty": "", "missing": nil}
	if !reflect.DeepEqual(root, want) {
		t.Fatalf("root=%#v, want %#v", root, want)
	}
}

func TestEnvRejectsInvalidArguments(t *testing.T) {
	for _, tc := range []struct {
		code, want string
	}{
		{`env();`, "expects 1 argument"},
		{`env("A", "B");`, "expects 1 argument"},
		{`env(1);`, "requires a string argument"},
	} {
		_, _, err := Execute(tc.code, nil, 100)
		if err == nil || !strings.Contains(err.Error(), tc.want) {
			t.Errorf("%q error=%v, want %q", tc.code, err, tc.want)
		}
	}
}
