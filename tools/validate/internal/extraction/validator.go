// Package extraction validates that load-bearing content survived the Epic 2
// Fat-Controller → skill-reference refactor. After command logic was moved into
// skill references, certain critical tokens (output-structure section headers,
// exit-code guards, escalation thresholds, agent names) MUST still appear
// somewhere under the configured search roots. A manifest of invariants is
// kept at tools/validate/extraction-invariants.json; if a refactor drops one of
// these tokens, validation fails — catching the regression class where thinning
// a command silently loses content that lives nowhere else.
package extraction

import (
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"strings"
)

// Result represents a single extraction-completeness finding.
type Result struct {
	File   string   `json:"file"`
	Type   string   `json:"type"`
	Valid  bool     `json:"valid"`
	Errors []string `json:"errors,omitempty"`
}

// Results contains all extraction validation results.
type Results struct {
	Passed []Result
	Failed []Result
}

// invariant is one required token plus the domain/reason it protects.
type invariant struct {
	Domain string `json:"domain"`
	Token  string `json:"token"`
	Reason string `json:"reason"`
}

// manifest mirrors extraction-invariants.json.
type manifest struct {
	SearchRoots []string    `json:"search_roots"`
	Invariants  []invariant `json:"invariants"`
}

const manifestRelPath = "tools/validate/extraction-invariants.json"

// Validate loads the invariant manifest and checks that every required token is
// present in at least one markdown file under the manifest's search roots.
func Validate(repoRoot string) (*Results, error) {
	results := &Results{}

	man, err := loadManifest(filepath.Join(repoRoot, manifestRelPath))
	if err != nil {
		return nil, fmt.Errorf("loading extraction manifest: %w", err)
	}

	corpus, err := loadCorpus(repoRoot, man.SearchRoots)
	if err != nil {
		return nil, fmt.Errorf("loading corpus: %w", err)
	}

	for _, inv := range man.Invariants {
		r := Result{File: manifestRelPath, Type: "extraction-invariant"}
		if containsToken(corpus, inv.Token) {
			r.Valid = true
			results.Passed = append(results.Passed, r)
		} else {
			r.Valid = false
			r.Errors = append(r.Errors, fmt.Sprintf(
				"[%s] required token %q not found under search roots — %s (extraction dropped load-bearing content)",
				inv.Domain, inv.Token, inv.Reason,
			))
			results.Failed = append(results.Failed, r)
		}
	}

	return results, nil
}

func loadManifest(path string) (*manifest, error) {
	content, err := os.ReadFile(path)
	if err != nil {
		return nil, err
	}
	var man manifest
	if err := json.Unmarshal(content, &man); err != nil {
		return nil, fmt.Errorf("invalid manifest JSON: %w", err)
	}
	if len(man.SearchRoots) == 0 {
		return nil, fmt.Errorf("manifest has no search_roots")
	}
	return &man, nil
}

// loadCorpus concatenates every .md file under the given roots into a single
// in-memory string so token presence can be checked with one pass per token.
func loadCorpus(repoRoot string, roots []string) (string, error) {
	var sb strings.Builder
	for _, root := range roots {
		abs := filepath.Join(repoRoot, root)
		err := filepath.Walk(abs, func(p string, info os.FileInfo, err error) error {
			if err != nil {
				return err
			}
			if info.IsDir() || !strings.HasSuffix(p, ".md") {
				return nil
			}
			content, readErr := os.ReadFile(p)
			if readErr != nil {
				return readErr
			}
			sb.Write(content)
			sb.WriteByte('\n')
			return nil
		})
		if err != nil && !os.IsNotExist(err) {
			return "", err
		}
	}
	return sb.String(), nil
}

func containsToken(corpus, token string) bool {
	return strings.Contains(corpus, token)
}
