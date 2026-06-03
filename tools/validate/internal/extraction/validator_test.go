package extraction

import (
	"os"
	"path/filepath"
	"testing"
)

// writes a temp repo with a manifest + corpus and returns its root.
func setupRepo(t *testing.T, manifestJSON string, files map[string]string) string {
	t.Helper()
	root := t.TempDir()

	manifestPath := filepath.Join(root, manifestRelPath)
	if err := os.MkdirAll(filepath.Dir(manifestPath), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(manifestPath, []byte(manifestJSON), 0o644); err != nil {
		t.Fatal(err)
	}
	for rel, content := range files {
		p := filepath.Join(root, rel)
		if err := os.MkdirAll(filepath.Dir(p), 0o755); err != nil {
			t.Fatal(err)
		}
		if err := os.WriteFile(p, []byte(content), 0o644); err != nil {
			t.Fatal(err)
		}
	}
	return root
}

const manifestTwo = `{
  "search_roots": ["plugins/atelier/skills"],
  "invariants": [
    {"domain": "a", "token": "## OCP 확장점", "reason": "design output"},
    {"domain": "b", "token": "diff-filter=U", "reason": "rebase guard"}
  ]
}`

func TestAllInvariantsPresent(t *testing.T) {
	root := setupRepo(t, manifestTwo, map[string]string{
		"plugins/atelier/skills/spec/references/design-protocol.md": "## OCP 확장점\n내용",
		"plugins/atelier/skills/git/references/conflict.md":         "git diff --diff-filter=U 검사",
	})
	res, err := Validate(root)
	if err != nil {
		t.Fatalf("Validate error: %v", err)
	}
	if len(res.Failed) != 0 {
		t.Errorf("expected 0 failures, got %d: %+v", len(res.Failed), res.Failed)
	}
	if len(res.Passed) != 2 {
		t.Errorf("expected 2 passes, got %d", len(res.Passed))
	}
}

func TestDroppedInvariantFails(t *testing.T) {
	// design-protocol drops the "## OCP 확장점" section — the exact #1 regression.
	root := setupRepo(t, manifestTwo, map[string]string{
		"plugins/atelier/skills/spec/references/design-protocol.md": "## 핑퐁 루프\n핑퐁만 있고 출력구조 없음",
		"plugins/atelier/skills/git/references/conflict.md":         "git diff --diff-filter=U 검사",
	})
	res, err := Validate(root)
	if err != nil {
		t.Fatalf("Validate error: %v", err)
	}
	if len(res.Failed) != 1 {
		t.Fatalf("expected 1 failure for dropped token, got %d", len(res.Failed))
	}
	if got := res.Failed[0].Errors[0]; !contains(got, "## OCP 확장점") {
		t.Errorf("failure should name the missing token, got: %s", got)
	}
}

func TestTokenInAnyFileCounts(t *testing.T) {
	// token may live in a surviving command, not only a reference.
	man := `{
      "search_roots": ["plugins/atelier/skills", "plugins/atelier/commands"],
      "invariants": [{"domain": "x", "token": "#643", "reason": "merge guard"}]
    }`
	root := setupRepo(t, man, map[string]string{
		"plugins/atelier/commands/autopilot/autopilot.md": "머지 가드 #643 참조",
	})
	res, err := Validate(root)
	if err != nil {
		t.Fatalf("Validate error: %v", err)
	}
	if len(res.Failed) != 0 {
		t.Errorf("token present in a command file should pass; got failures: %+v", res.Failed)
	}
}

func TestMissingSearchRootIsNotFatal(t *testing.T) {
	// a non-existent search root must not error; tokens simply won't be found.
	man := `{
      "search_roots": ["plugins/atelier/skills", "plugins/atelier/nonexistent"],
      "invariants": [{"domain": "x", "token": "present", "reason": "r"}]
    }`
	root := setupRepo(t, man, map[string]string{
		"plugins/atelier/skills/s/SKILL.md": "present token here",
	})
	res, err := Validate(root)
	if err != nil {
		t.Fatalf("missing search root should not error: %v", err)
	}
	if len(res.Failed) != 0 {
		t.Errorf("expected pass, got %+v", res.Failed)
	}
}

func contains(s, sub string) bool {
	return len(s) >= len(sub) && (func() bool {
		for i := 0; i+len(sub) <= len(s); i++ {
			if s[i:i+len(sub)] == sub {
				return true
			}
		}
		return false
	})()
}
