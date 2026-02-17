package architecture

import (
	"fmt"
	"os"
	"path/filepath"
	"strings"

	"github.com/bmatcuk/doublestar/v4"
	"github.com/kys0213/kys-claude-plugin/tools/validate/internal/parser"
)

// Layer represents a layer in the layered architecture
type Layer int

const (
	LayerCommand Layer = iota // Controller layer - user entry point
	LayerAgent                // Service layer - orchestration
	LayerSkill                // Domain layer - single responsibility
	LayerUnknown
)

// LayerOrder defines the allowed dependency direction (lower = higher layer)
// Commands (0) → Agents (1) → Skills (2): higher layer can depend on lower, not reverse
var layerNames = map[Layer]string{
	LayerCommand: "command",
	LayerAgent:   "agent",
	LayerSkill:   "skill",
	LayerUnknown: "unknown",
}

func (l Layer) String() string {
	return layerNames[l]
}

// Result represents a single architecture validation finding
type Result struct {
	File     string   `json:"file"`
	Type     string   `json:"type"`
	Valid    bool     `json:"valid"`
	Severity string   `json:"severity"` // "error", "warning"
	Errors   []string `json:"errors,omitempty"`
}

// Results contains all architecture validation results
type Results struct {
	Passed   []Result
	Failed   []Result
	Warnings []Result
}

// LayerFile represents a parsed file with its detected layer
type LayerFile struct {
	Path    string
	Layer   Layer
	Plugin  string // plugin name (directory name)
	Body    string // markdown body (after frontmatter)
	Lines   []string
	Parsed  *parser.ParseResult
}

// Validate runs all architecture validations
func Validate(repoRoot string) (*Results, error) {
	results := &Results{}

	// 1. Collect and classify all layer files
	files, err := collectLayerFiles(repoRoot)
	if err != nil {
		return nil, fmt.Errorf("collecting layer files: %w", err)
	}

	if len(files) == 0 {
		return results, nil
	}

	// 2. Check layer dependency direction (circular reference detection)
	validateLayerDependencies(files, results)

	// 3. Check content similarity across layers within each plugin
	validateContentSimilarity(files, results)

	// 4. Check responsibility violations
	validateResponsibilities(files, results)

	// 5. Check skill references (agent → skill existence)
	validateSkillReferences(files, repoRoot, results)

	return results, nil
}

// collectLayerFiles discovers and parses all command, agent, and skill files
func collectLayerFiles(repoRoot string) ([]LayerFile, error) {
	var files []LayerFile

	patterns := map[string]Layer{
		"plugins/*/commands/*.md":      LayerCommand,
		"plugins/*/skills/*/SKILL.md":  LayerSkill,
		"plugins/*/skills/SKILL.md":    LayerSkill,
		"plugins/*/agents/*.md":        LayerAgent,
	}

	for pattern, layer := range patterns {
		matches, _ := doublestar.Glob(os.DirFS(repoRoot), pattern)
		for _, match := range matches {
			fullPath := filepath.Join(repoRoot, match)

			parsed, err := parser.ParseMarkdown(fullPath)
			if err != nil {
				continue
			}

			plugin := extractPluginName(match)
			body := parsed.Body
			if body == "" {
				body = parsed.Content
			}

			files = append(files, LayerFile{
				Path:   fullPath,
				Layer:  layer,
				Plugin: plugin,
				Body:   body,
				Lines:  strings.Split(body, "\n"),
				Parsed: parsed,
			})
		}
	}

	return files, nil
}

// extractPluginName extracts plugin name from a relative path like "plugins/develop-workflow/commands/flow.md"
func extractPluginName(relPath string) string {
	parts := strings.Split(filepath.ToSlash(relPath), "/")
	if len(parts) >= 2 && parts[0] == "plugins" {
		return parts[1]
	}
	return "unknown"
}

// shortPath returns a display-friendly relative path
func shortPath(fullPath, repoRoot string) string {
	rel, err := filepath.Rel(repoRoot, fullPath)
	if err != nil {
		return fullPath
	}
	return rel
}
