package evalset

import (
	"os"
	"path/filepath"
	"testing"
)

func write(t *testing.T, content string) string {
	t.Helper()
	p := filepath.Join(t.TempDir(), "set.json")
	if err := os.WriteFile(p, []byte(content), 0o644); err != nil {
		t.Fatal(err)
	}
	return p
}

func TestLoadReadsCasesWithFiles(t *testing.T) {
	p := write(t, `[
	  {"query": "슬랙에 정리해줘", "expect": true, "files": {"notes/a.md": "작업 로그"}},
	  {"query": "버그 고쳐줘", "expect": false}
	]`)

	cases, err := NewLoader().Load(p)
	if err != nil {
		t.Fatal(err)
	}
	if len(cases) != 2 {
		t.Fatalf("want 2 cases, got %d", len(cases))
	}
	if cases[0].Files["notes/a.md"] != "작업 로그" {
		t.Errorf("seed file lost, got %#v", cases[0].Files)
	}
	if cases[1].Expect {
		t.Error("second case is a negative")
	}
}

func TestLoadRejectsEmptySet(t *testing.T) {
	if _, err := NewLoader().Load(write(t, `[]`)); err == nil {
		t.Error("an empty set must error rather than report a vacuous pass")
	}
}

func TestLoadRejectsCaseWithoutQuery(t *testing.T) {
	if _, err := NewLoader().Load(write(t, `[{"expect": true}]`)); err == nil {
		t.Error("a case with no query must error")
	}
}

func TestLoadRejectsMalformedJSON(t *testing.T) {
	if _, err := NewLoader().Load(write(t, `{`)); err == nil {
		t.Error("malformed JSON must error")
	}
}
