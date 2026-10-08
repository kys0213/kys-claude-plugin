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

func TestGlob(t *testing.T) {
	tests := []struct {
		name    string
		files   []string
		pattern string
		want    []string
	}{
		{
			name: "leaves out claude worktrees",
			files: []string{
				"plugins/a/.claude-plugin/plugin.json",
				".claude/worktrees/x/plugins/a/.claude-plugin/plugin.json",
			},
			pattern: "**/plugin.json",
			want:    []string{"plugins/a/.claude-plugin/plugin.json"},
		},
		{
			name:    "keeps other claude directories",
			files:   []string{".claude/rules/a.md"},
			pattern: "**/*.md",
			want:    []string{".claude/rules/a.md"},
		},
		{
			name:    "keeps nested claude worktrees",
			files:   []string{"a/.claude/worktrees/x/p.json"},
			pattern: "**/p.json",
			want:    []string{"a/.claude/worktrees/x/p.json"},
		},
		{
			name:    "keeps directory with similar name",
			files:   []string{".claude/worktrees-x/y/p.json"},
			pattern: "**/p.json",
			want:    []string{".claude/worktrees-x/y/p.json"},
		},
		{
			name:    "returns nothing when no file matches",
			files:   []string{"plugins/a/readme.txt"},
			pattern: "**/p.json",
			want:    nil,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			root := t.TempDir()
			for _, f := range tt.files {
				writeFile(t, root, f)
			}

			got, err := NewGlobber(root).Glob(tt.pattern)
			if err != nil {
				t.Fatal(err)
			}
			if len(got) != len(tt.want) || (len(got) > 0 && !reflect.DeepEqual(got, tt.want)) {
				t.Errorf("Glob() = %v, want %v", got, tt.want)
			}
		})
	}
}

func TestGlobUsesInjectedExcludedPrefixes(t *testing.T) {
	root := t.TempDir()
	writeFile(t, root, "keep/p.json")
	writeFile(t, root, "skip/p.json")

	g := &Globber{RepoRoot: root, ExcludedPrefixes: []string{"skip/"}}
	got, err := g.Glob("**/p.json")
	if err != nil {
		t.Fatal(err)
	}

	want := []string{"keep/p.json"}
	if !reflect.DeepEqual(got, want) {
		t.Errorf("Glob() = %v, want %v", got, want)
	}
}

func TestGlobReturnsErrorForMalformedPattern(t *testing.T) {
	if _, err := NewGlobber(t.TempDir()).Glob("[a"); err == nil {
		t.Error("expected error for malformed pattern")
	}
}
