package repofs

import (
	"fmt"
	"os"
	"strings"

	"github.com/bmatcuk/doublestar/v4"
)

// Globber matches patterns under a repository root, leaving out directories
// that hold other checkouts of the repository.
type Globber struct {
	RepoRoot         string
	ExcludedPrefixes []string
}

// NewGlobber creates a Globber that excludes `.claude/worktrees/`.
func NewGlobber(repoRoot string) *Globber {
	return &Globber{
		RepoRoot:         repoRoot,
		ExcludedPrefixes: []string{".claude/worktrees/"},
	}
}

// Glob returns the paths, relative to RepoRoot, that match pattern.
func (g *Globber) Glob(pattern string) ([]string, error) {
	matches, err := doublestar.Glob(os.DirFS(g.RepoRoot), pattern)
	if err != nil {
		return nil, fmt.Errorf("glob %q: %w", pattern, err)
	}

	kept := matches[:0]
	for _, m := range matches {
		if !g.isExcluded(m) {
			kept = append(kept, m)
		}
	}
	return kept, nil
}

func (g *Globber) isExcluded(rel string) bool {
	for _, prefix := range g.ExcludedPrefixes {
		if strings.HasPrefix(rel, prefix) {
			return true
		}
	}
	return false
}
