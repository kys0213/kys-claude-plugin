package changes

import (
	"bufio"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
)

// Package represents a detected package with changes
type Package struct {
	Name string // e.g., "review", "external-llm"
	Path string // e.g., "plugins/review"
	Type string // "plugin" or "common"
}

// DetectChanges finds all packages affected by changes since baseRef
func DetectChanges(repoRoot, baseRef string) ([]Package, error) {
	changedFiles, err := getChangedFiles(repoRoot, baseRef)
	if err != nil {
		return nil, err
	}

	if len(changedFiles) == 0 {
		return nil, nil
	}

	affected := make(map[string]Package)

	// 1. Direct changes detection
	for _, file := range changedFiles {
		if strings.HasPrefix(file, "plugins/") {
			parts := strings.SplitN(file, "/", 3)
			if len(parts) >= 2 {
				pkgPath := parts[0] + "/" + parts[1]
				affected[pkgPath] = Package{
					Name: parts[1],
					Path: pkgPath,
					Type: "plugin",
				}
			}
		} else if strings.HasPrefix(file, "common/") {
			affected["common"] = Package{
				Name: "common",
				Path: "common",
				Type: "common",
			}
		}
	}

	// 2. If common changed, find plugins that reference it
	if _, hasCommon := affected["common"]; hasCommon {
		commonFiles := filterPrefix(changedFiles, "common/")
		dependentPlugins, err := findDependentPlugins(repoRoot, commonFiles)
		if err == nil {
			for _, pkg := range dependentPlugins {
				affected[pkg.Path] = pkg
			}
		}
	}

	// Convert map to slice
	result := make([]Package, 0, len(affected))
	for _, pkg := range affected {
		result = append(result, pkg)
	}

	return result, nil
}

// getChangedFiles returns list of files changed since baseRef
func getChangedFiles(repoRoot, baseRef string) ([]string, error) {
	// Try three-dot diff first (for branches)
	cmd := exec.Command("git", "diff", "--name-only", baseRef+"...HEAD")
	cmd.Dir = repoRoot
	output, err := cmd.Output()

	if err != nil {
		// Fallback to two-dot diff
		cmd = exec.Command("git", "diff", "--name-only", baseRef)
		cmd.Dir = repoRoot
		output, err = cmd.Output()
		if err != nil {
			return nil, err
		}
	}

	var files []string
	scanner := bufio.NewScanner(strings.NewReader(string(output)))
	for scanner.Scan() {
		line := strings.TrimSpace(scanner.Text())
		if line != "" {
			files = append(files, line)
		}
	}

	return files, nil
}

// filterPrefix returns files that start with given prefix
func filterPrefix(files []string, prefix string) []string {
	var result []string
	for _, f := range files {
		if strings.HasPrefix(f, prefix) {
			result = append(result, f)
		}
	}
	return result
}

// findDependentPlugins finds plugins that reference any of the given common files
func findDependentPlugins(repoRoot string, commonFiles []string) ([]Package, error) {
	pluginsDir := filepath.Join(repoRoot, "plugins")
	if _, err := os.Stat(pluginsDir); os.IsNotExist(err) {
		return nil, nil
	}

	var result []Package
	seen := make(map[string]bool)

	entries, err := os.ReadDir(pluginsDir)
	if err != nil {
		return nil, err
	}

	for _, entry := range entries {
		if !entry.IsDir() {
			continue
		}

		pluginPath := filepath.Join(pluginsDir, entry.Name())

		// Check if any file in this plugin references a common file
		for _, commonFile := range commonFiles {
			if hasReference(pluginPath, commonFile) && !seen[entry.Name()] {
				seen[entry.Name()] = true
				result = append(result, Package{
					Name: entry.Name(),
					Path: "plugins/" + entry.Name(),
					Type: "plugin",
				})
				break
			}
		}
	}

	return result, nil
}

// hasReference checks if any file in dir references the given file
func hasReference(dir, targetFile string) bool {
	cmd := exec.Command("grep", "-rl", targetFile, dir)
	output, _ := cmd.Output()
	return len(output) > 0
}

// GetPluginsOnly filters and returns only plugin packages
func GetPluginsOnly(packages []Package) []Package {
	var plugins []Package
	for _, pkg := range packages {
		if pkg.Type == "plugin" {
			plugins = append(plugins, pkg)
		}
	}
	return plugins
}
