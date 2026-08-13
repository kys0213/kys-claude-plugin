// Package skill loads a skill's registered identity from its SKILL.md.
package skill

import (
	"bufio"
	"fmt"
	"os"
	"path/filepath"
	"strings"
)

// Skill is a candidate registered for a measurement run.
type Skill struct {
	Name string `json:"name"`
	Desc string `json:"description"`
}

// Loader reads skills off disk.
type Loader struct {
	MaxLineBytes int
}

// NewLoader creates a Loader with a line budget large enough for the
// single-line descriptions this repo uses.
func NewLoader() *Loader {
	return &Loader{MaxLineBytes: 1024 * 1024}
}

// Load reads name and description straight out of the frontmatter, so a
// measurement always reflects the committed description rather than a copy
// that drifts from it.
func (l *Loader) Load(path string) (Skill, error) {
	f, err := os.Open(path)
	if err != nil {
		return Skill{}, err
	}
	defer f.Close()

	var (
		sc       = bufio.NewScanner(f)
		inFront  bool
		name     string
		desc     []string
		inBlock  bool
		seenDesc bool
	)
	sc.Buffer(make([]byte, 0, 64*1024), l.MaxLineBytes)

	for sc.Scan() {
		line := sc.Text()
		if !inFront {
			if strings.TrimSpace(line) == "---" {
				inFront = true
			}
			continue
		}
		if strings.TrimSpace(line) == "---" {
			break
		}
		if inBlock {
			// A block scalar continues while its lines stay indented.
			if strings.HasPrefix(line, "  ") {
				desc = append(desc, strings.TrimSpace(line))
				continue
			}
			inBlock = false
		}
		switch {
		case strings.HasPrefix(line, "name:"):
			name = strings.TrimSpace(strings.TrimPrefix(line, "name:"))
		case strings.HasPrefix(line, "description:"):
			seenDesc = true
			v := strings.TrimSpace(strings.TrimPrefix(line, "description:"))
			if v == "|" || v == ">" || v == "|-" || v == ">-" {
				inBlock = true
				continue
			}
			desc = append(desc, v)
		}
	}
	if err := sc.Err(); err != nil {
		return Skill{}, err
	}

	body := strings.TrimSpace(strings.Join(desc, "\n"))
	if !seenDesc || body == "" {
		return Skill{}, fmt.Errorf("%s: frontmatter has no description", path)
	}
	if name == "" {
		name = filepath.Base(filepath.Dir(path))
	}
	return Skill{Name: name, Desc: body}, nil
}

// LoadSpec accepts "name=path" or a bare path, with an explicit name winning
// over the one in the file.
func (l *Loader) LoadSpec(spec string) (Skill, error) {
	name, path := "", spec
	if i := strings.Index(spec, "="); i > 0 {
		name, path = spec[:i], spec[i+1:]
	}
	s, err := l.Load(path)
	if err != nil {
		return Skill{}, err
	}
	if name != "" {
		s.Name = name
	}
	return s, nil
}
