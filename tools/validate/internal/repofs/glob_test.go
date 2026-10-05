package repofs

import (
	"os"
	"path/filepath"
	"reflect"
	"testing"
)

func writeFile(t *testing.T, root, rel string) {
	t.Helper()
	path := filepath.Join(root, rel)
	if err := os.MkdirAll(filepath.Dir(path), 0755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(path, []byte("{}"), 0644); err != nil {
		t.Fatal(err)
	}
}

func TestGlobLeavesOutClaudeWorktrees(t *testing.T) {
	root := t.TempDir()
	writeFile(t, root, "plugins/a/.claude-plugin/plugin.json")
	writeFile(t, root, ".claude/worktrees/x/plugins/a/.claude-plugin/plugin.json")

	got := Glob(root, "**/plugin.json")

	want := []string{"plugins/a/.claude-plugin/plugin.json"}
	if !reflect.DeepEqual(got, want) {
		t.Errorf("Glob() = %v, want %v", got, want)
	}
}

func TestGlobKeepsOtherClaudeDirectories(t *testing.T) {
	root := t.TempDir()
	writeFile(t, root, ".claude/rules/a.md")

	got := Glob(root, "**/*.md")

	want := []string{".claude/rules/a.md"}
	if !reflect.DeepEqual(got, want) {
		t.Errorf("Glob() = %v, want %v", got, want)
	}
}
