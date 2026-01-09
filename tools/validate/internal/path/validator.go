package path

import (
	"encoding/json"
	"os"
	"path/filepath"
	"regexp"
	"strings"

	"github.com/bmatcuk/doublestar/v4"
	"github.com/kys0213/kys-claude-plugin/tools/validate/internal/parser"
)

// Result represents a path validation result
type Result struct {
	File           string `json:"file"`
	Type           string `json:"type"`
	ReferencedPath string `json:"referencedPath,omitempty"`
	ResolvedPath   string `json:"resolvedPath,omitempty"`
	Valid          bool   `json:"valid"`
	Error          string `json:"error,omitempty"`
}

// Results contains all validation results
type Results struct {
	Passed []Result
	Failed []Result
}

var pluginRootPattern = regexp.MustCompile(`\$\{CLAUDE_PLUGIN_ROOT\}`)

// Validate runs all path validations
func Validate(repoRoot string) (*Results, error) {
	results := &Results{}

	// 1. Skill reference paths
	skillFiles, _ := doublestar.Glob(os.DirFS(repoRoot), "**/skills/*/SKILL.md")
	for _, file := range skillFiles {
		fullPath := filepath.Join(repoRoot, file)
		pathResults := validateSkillPaths(fullPath)
		for _, r := range pathResults {
			if r.Valid {
				results.Passed = append(results.Passed, r)
			} else {
				results.Failed = append(results.Failed, r)
			}
		}
	}

	// 2. Agent reference paths
	agentFiles, _ := doublestar.Glob(os.DirFS(repoRoot), "**/agents/*.md")
	for _, file := range agentFiles {
		fullPath := filepath.Join(repoRoot, file)
		pathResults := validateDocumentPaths(fullPath, repoRoot)
		for _, r := range pathResults {
			if r.Valid {
				results.Passed = append(results.Passed, r)
			} else {
				results.Failed = append(results.Failed, r)
			}
		}
	}

	// 3. Command reference paths
	commandFiles, _ := doublestar.Glob(os.DirFS(repoRoot), "**/commands/*.md")
	for _, file := range commandFiles {
		fullPath := filepath.Join(repoRoot, file)
		pathResults := validateDocumentPaths(fullPath, repoRoot)
		for _, r := range pathResults {
			if r.Valid {
				results.Passed = append(results.Passed, r)
			} else {
				results.Failed = append(results.Failed, r)
			}
		}
	}

	// 4. marketplace.json source paths
	marketplaceFile := filepath.Join(repoRoot, ".claude-plugin", "marketplace.json")
	if _, err := os.Stat(marketplaceFile); err == nil {
		sourceResults := validateMarketplaceSources(marketplaceFile, repoRoot)
		for _, r := range sourceResults {
			if r.Valid {
				results.Passed = append(results.Passed, r)
			} else {
				results.Failed = append(results.Failed, r)
			}
		}
	}

	return results, nil
}

func validateSkillPaths(filePath string) []Result {
	var results []Result
	skillDir := filepath.Dir(filePath)

	paths, err := parser.ExtractPluginRootPaths(filePath)
	if err != nil {
		results = append(results, Result{
			File:  filePath,
			Type:  "skill-reference",
			Valid: false,
			Error: "Parse error: " + err.Error(),
		})
		return results
	}

	for _, p := range paths {
		// Skip ${CLAUDE_PLUGIN_ROOT} paths (handled in validateDocumentPaths)
		if strings.Contains(p.Value, "${CLAUDE_PLUGIN_ROOT}") {
			continue
		}

		// Only check references/ and examples/ paths
		if !strings.HasPrefix(p.Value, "references/") && !strings.HasPrefix(p.Value, "examples/") {
			continue
		}

		fullPath := filepath.Join(skillDir, p.Value)
		exists := fileExists(fullPath)

		results = append(results, Result{
			File:           filePath,
			Type:           "skill-reference",
			ReferencedPath: p.Value,
			ResolvedPath:   fullPath,
			Valid:          exists,
			Error:          errorIfNotExists(exists, "Referenced file not found: "+p.Value),
		})
	}

	return results
}

func validateDocumentPaths(filePath string, repoRoot string) []Result {
	var results []Result
	fileDir := filepath.Dir(filePath)
	pluginRoot := findPluginRoot(fileDir)

	paths, err := parser.ExtractPluginRootPaths(filePath)
	if err != nil {
		results = append(results, Result{
			File:  filePath,
			Type:  "document-path",
			Valid: false,
			Error: "Parse error: " + err.Error(),
		})
		return results
	}

	for _, p := range paths {
		if !strings.Contains(p.Value, "${CLAUDE_PLUGIN_ROOT}") {
			continue
		}

		result := validatePluginRootPath(p.Value, pluginRoot, filePath)
		results = append(results, result)
	}

	return results
}

func validatePluginRootPath(pathStr string, pluginRoot string, sourceFile string) Result {
	// Replace ${CLAUDE_PLUGIN_ROOT} with actual path
	resolvedPath := pluginRootPattern.ReplaceAllString(pathStr, pluginRoot)

	// Normalize path (handle ../)
	resolvedPath = filepath.Clean(resolvedPath)

	exists := fileExists(resolvedPath)

	return Result{
		File:           sourceFile,
		Type:           "plugin-root-path",
		ReferencedPath: pathStr,
		ResolvedPath:   resolvedPath,
		Valid:          exists,
		Error:          errorIfNotExists(exists, "Script/file not found: "+pathStr+" (resolved to: "+resolvedPath+")"),
	}
}

func validateMarketplaceSources(marketplacePath string, repoRoot string) []Result {
	var results []Result

	content, err := os.ReadFile(marketplacePath)
	if err != nil {
		results = append(results, Result{
			File:  marketplacePath,
			Type:  "marketplace-source",
			Valid: false,
			Error: "Cannot read file: " + err.Error(),
		})
		return results
	}

	var data map[string]interface{}
	if err := json.Unmarshal(content, &data); err != nil {
		results = append(results, Result{
			File:  marketplacePath,
			Type:  "marketplace-source",
			Valid: false,
			Error: "Parse error: " + err.Error(),
		})
		return results
	}

	plugins, ok := data["plugins"].([]interface{})
	if !ok {
		return results
	}

	for _, p := range plugins {
		plugin, ok := p.(map[string]interface{})
		if !ok {
			continue
		}

		var source string
		switch s := plugin["source"].(type) {
		case string:
			source = s
		case map[string]interface{}:
			if path, ok := s["path"].(string); ok {
				source = path
			}
		}

		if source == "" {
			continue
		}

		// Check if strict: false (plugin.json not required)
		strict := true // default is true
		if strictVal, ok := plugin["strict"].(bool); ok {
			strict = strictVal
		}

		// ./plugins/review -> absolute path
		pluginPath := filepath.Join(repoRoot, strings.TrimPrefix(source, "./"))
		pluginJSON := filepath.Join(pluginPath, ".claude-plugin", "plugin.json")

		dirExists := dirExists(pluginPath)
		jsonExists := fileExists(pluginJSON)

		if !dirExists {
			results = append(results, Result{
				File:           marketplacePath,
				Type:           "marketplace-source",
				ReferencedPath: source,
				ResolvedPath:   pluginPath,
				Valid:          false,
				Error:          "Plugin directory not found: " + source,
			})
		} else if !jsonExists && strict {
			// Only require plugin.json when strict is true (default)
			results = append(results, Result{
				File:           marketplacePath,
				Type:           "marketplace-source",
				ReferencedPath: source,
				ResolvedPath:   pluginJSON,
				Valid:          false,
				Error:          "plugin.json not found for: " + source,
			})
		} else {
			results = append(results, Result{
				File:           marketplacePath,
				Type:           "marketplace-source",
				ReferencedPath: source,
				Valid:          true,
			})
		}
	}

	return results
}

func findPluginRoot(startDir string) string {
	current := startDir

	for current != "/" && current != "." {
		if dirExists(filepath.Join(current, ".claude-plugin")) {
			return current
		}
		current = filepath.Dir(current)
	}

	return startDir
}

func fileExists(path string) bool {
	info, err := os.Stat(path)
	return err == nil && !info.IsDir()
}

func dirExists(path string) bool {
	info, err := os.Stat(path)
	return err == nil && info.IsDir()
}

func errorIfNotExists(exists bool, errMsg string) string {
	if exists {
		return ""
	}
	return errMsg
}
