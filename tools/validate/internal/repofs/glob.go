package repofs

import (
	"os"
	"strings"

	"github.com/bmatcuk/doublestar/v4"
)

var excludedPrefixes = []string{".claude/worktrees/"}

// Glob matches pattern under repoRoot, leaving out directories that hold
// other checkouts of the repository.
func Glob(repoRoot, pattern string) []string {
	matches, _ := doublestar.Glob(os.DirFS(repoRoot), pattern)

	kept := matches[:0]
	for _, m := range matches {
		if !isExcluded(m) {
			kept = append(kept, m)
		}
	}
	return kept
}

func isExcluded(rel string) bool {
	for _, prefix := range excludedPrefixes {
		if strings.HasPrefix(rel, prefix) {
			return true
		}
	}
	return false
}
