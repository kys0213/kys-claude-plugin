package skill

import (
	"os"
	"path/filepath"
	"testing"
)

func writeTemp(t *testing.T, content string) string {
	t.Helper()
	p := filepath.Join(t.TempDir(), "SKILL.md")
	if err := os.WriteFile(p, []byte(content), 0o644); err != nil {
		t.Fatal(err)
	}
	return p
}

func TestLoadReadsSingleLineDescription(t *testing.T) {
	p := writeTemp(t, `---
name: communicate
description: 다른 사람이 읽을 글을 쓰거나 다듬을 때의 작문 기준입니다 — 슬랙, PR 설명.
version: 2.1.0
---

# communicate
본문은 description 이 아니다.
`)

	s, err := NewLoader().Load(p)
	if err != nil {
		t.Fatal(err)
	}
	if s.Name != "communicate" {
		t.Errorf("want name from frontmatter, got %q", s.Name)
	}
	want := "다른 사람이 읽을 글을 쓰거나 다듬을 때의 작문 기준입니다 — 슬랙, PR 설명."
	if s.Desc != want {
		t.Errorf("desc mismatch:\n got %q\nwant %q", s.Desc, want)
	}
}

func TestLoadReadsBlockScalarDescription(t *testing.T) {
	p := writeTemp(t, `---
name: git
description: |
  첫 줄.
  둘째 줄.
version: 1.0.0
---
`)

	s, err := NewLoader().Load(p)
	if err != nil {
		t.Fatal(err)
	}
	if s.Desc != "첫 줄.\n둘째 줄." {
		t.Errorf("block scalar not joined, got %q", s.Desc)
	}
}

func TestLoadStopsAtFrontmatterEnd(t *testing.T) {
	// A body line that looks like frontmatter must not be picked up.
	p := writeTemp(t, `---
name: x
description: real
---

description: not this one
`)

	s, err := NewLoader().Load(p)
	if err != nil {
		t.Fatal(err)
	}
	if s.Desc != "real" {
		t.Errorf("read past frontmatter, got %q", s.Desc)
	}
}

func TestLoadErrorsWithoutDescription(t *testing.T) {
	p := writeTemp(t, "---\nname: x\n---\n")
	if _, err := NewLoader().Load(p); err == nil {
		t.Error("a SKILL.md with no description must be an error, not an empty measurement")
	}
}

func TestLoadFallsBackToDirectoryName(t *testing.T) {
	dir := filepath.Join(t.TempDir(), "myskill")
	if err := os.MkdirAll(dir, 0o755); err != nil {
		t.Fatal(err)
	}
	p := filepath.Join(dir, "SKILL.md")
	if err := os.WriteFile(p, []byte("---\ndescription: d\n---\n"), 0o644); err != nil {
		t.Fatal(err)
	}

	s, err := NewLoader().Load(p)
	if err != nil {
		t.Fatal(err)
	}
	if s.Name != "myskill" {
		t.Errorf("want directory name as fallback, got %q", s.Name)
	}
}

func TestLoadSpecOverridesName(t *testing.T) {
	p := writeTemp(t, "---\nname: infile\ndescription: d\n---\n")

	s, err := NewLoader().LoadSpec("override=" + p)
	if err != nil {
		t.Fatal(err)
	}
	if s.Name != "override" {
		t.Errorf("explicit name must win, got %q", s.Name)
	}
}
