package version

import (
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"regexp"
	"strconv"
	"strings"

	"github.com/bmatcuk/doublestar/v4"
)

// Result represents a version validation result
type Result struct {
	File    string   `json:"file"`
	Type    string   `json:"type"`
	Plugin  string   `json:"plugin,omitempty"`
	Valid   bool     `json:"valid"`
	Errors  []string `json:"errors,omitempty"`
	Error   string   `json:"error,omitempty"`
}

// Results contains all validation results
type Results struct {
	Passed []Result
	Failed []Result
}

var semverRegex = regexp.MustCompile(`^\d+\.\d+\.\d+(-[\w.]+)?$`)

// Validate runs all version validations
func Validate(repoRoot string) (*Results, error) {
	results := &Results{}
	marketplaceFile := filepath.Join(repoRoot, ".claude-plugin", "marketplace.json")

	// 1. Individual plugin.json version format validation
	pluginFiles, _ := doublestar.Glob(os.DirFS(repoRoot), "**/plugin.json")
	for _, file := range pluginFiles {
		fullPath := filepath.Join(repoRoot, file)
		result := validateVersionFormat(fullPath)
		if result.Valid {
			results.Passed = append(results.Passed, result)
		} else {
			results.Failed = append(results.Failed, result)
		}
	}

	// 2. marketplace.json version format validation
	if _, err := os.Stat(marketplaceFile); err == nil {
		result := validateMarketplaceVersions(marketplaceFile)
		if result.Valid {
			results.Passed = append(results.Passed, result)
		} else {
			results.Failed = append(results.Failed, result)
		}

		// 3. Version consistency check
		consistencyResults := validateVersionConsistency(marketplaceFile, repoRoot)
		for _, r := range consistencyResults {
			if r.Valid {
				results.Passed = append(results.Passed, r)
			} else {
				results.Failed = append(results.Failed, r)
			}
		}
	}

	return results, nil
}

func validateVersionFormat(filePath string) Result {
	result := Result{
		File: filePath,
		Type: "version-format",
	}

	content, err := os.ReadFile(filePath)
	if err != nil {
		result.Errors = append(result.Errors, "Cannot read file: "+err.Error())
		return result
	}

	var data map[string]interface{}
	if err := json.Unmarshal(content, &data); err != nil {
		result.Errors = append(result.Errors, "Parse error: "+err.Error())
		return result
	}

	if version, ok := data["version"].(string); ok && version != "" {
		if !semverRegex.MatchString(version) {
			result.Errors = append(result.Errors, fmt.Sprintf("Invalid version format: '%s' (expected: MAJOR.MINOR.PATCH)", version))
		}
	}

	result.Valid = len(result.Errors) == 0
	return result
}

func validateMarketplaceVersions(filePath string) Result {
	result := Result{
		File: filePath,
		Type: "marketplace-versions",
	}

	content, err := os.ReadFile(filePath)
	if err != nil {
		result.Errors = append(result.Errors, "Cannot read file: "+err.Error())
		return result
	}

	var data map[string]interface{}
	if err := json.Unmarshal(content, &data); err != nil {
		result.Errors = append(result.Errors, "Parse error: "+err.Error())
		return result
	}

	// metadata.version validation
	if metadata, ok := data["metadata"].(map[string]interface{}); ok {
		if version, ok := metadata["version"].(string); ok && version != "" {
			if !semverRegex.MatchString(version) {
				result.Errors = append(result.Errors, fmt.Sprintf("Invalid metadata.version: '%s'", version))
			}
		}
	}

	// Each plugin version validation
	if plugins, ok := data["plugins"].([]interface{}); ok {
		for i, p := range plugins {
			if plugin, ok := p.(map[string]interface{}); ok {
				if version, ok := plugin["version"].(string); ok && version != "" {
					if !semverRegex.MatchString(version) {
						result.Errors = append(result.Errors, fmt.Sprintf("Invalid plugins[%d].version: '%s'", i, version))
					}
				}
			}
		}
	}

	result.Valid = len(result.Errors) == 0
	return result
}

func validateVersionConsistency(marketplacePath string, repoRoot string) []Result {
	var results []Result

	content, err := os.ReadFile(marketplacePath)
	if err != nil {
		return results
	}

	var marketplace map[string]interface{}
	if err := json.Unmarshal(content, &marketplace); err != nil {
		return results
	}

	plugins, ok := marketplace["plugins"].([]interface{})
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

		pluginPath := filepath.Join(repoRoot, strings.TrimPrefix(source, "./"))
		pluginJSONPath := filepath.Join(pluginPath, ".claude-plugin", "plugin.json")

		if _, err := os.Stat(pluginJSONPath); err != nil {
			continue
		}

		pluginContent, err := os.ReadFile(pluginJSONPath)
		if err != nil {
			results = append(results, Result{
				File:   pluginJSONPath,
				Type:   "version-consistency",
				Plugin: plugin["name"].(string),
				Valid:  false,
				Error:  "Cannot read file: " + err.Error(),
			})
			continue
		}

		var pluginJSON map[string]interface{}
		if err := json.Unmarshal(pluginContent, &pluginJSON); err != nil {
			results = append(results, Result{
				File:   pluginJSONPath,
				Type:   "version-consistency",
				Plugin: plugin["name"].(string),
				Valid:  false,
				Error:  "Parse error: " + err.Error(),
			})
			continue
		}

		marketplaceVersion, _ := plugin["version"].(string)
		pluginVersion, _ := pluginJSON["version"].(string)

		// Only check consistency if both have versions
		if marketplaceVersion != "" && pluginVersion != "" {
			if marketplaceVersion != pluginVersion {
				results = append(results, Result{
					File:   pluginJSONPath,
					Type:   "version-consistency",
					Plugin: plugin["name"].(string),
					Valid:  false,
					Error:  fmt.Sprintf("Version mismatch: marketplace.json has '%s', plugin.json has '%s'", marketplaceVersion, pluginVersion),
				})
			} else {
				results = append(results, Result{
					File:   pluginJSONPath,
					Type:   "version-consistency",
					Plugin: plugin["name"].(string),
					Valid:  true,
				})
			}
		}
	}

	return results
}

// BumpVersion calculates a new version based on bump type
func BumpVersion(currentVersion string, bumpType string) (string, error) {
	match := regexp.MustCompile(`^(\d+)\.(\d+)\.(\d+)`).FindStringSubmatch(currentVersion)
	if match == nil {
		return "", fmt.Errorf("invalid version: %s", currentVersion)
	}

	major, _ := strconv.Atoi(match[1])
	minor, _ := strconv.Atoi(match[2])
	patch, _ := strconv.Atoi(match[3])

	switch bumpType {
	case "major":
		return fmt.Sprintf("%d.0.0", major+1), nil
	case "minor":
		return fmt.Sprintf("%d.%d.0", major, minor+1), nil
	case "patch":
		return fmt.Sprintf("%d.%d.%d", major, minor, patch+1), nil
	default:
		return "", fmt.Errorf("invalid bump type: %s", bumpType)
	}
}

// GetBumpTypeFromPRTitle determines bump type from PR title
func GetBumpTypeFromPRTitle(title string) string {
	if matched, _ := regexp.MatchString(`^major(\(.+\))?:`, title); matched {
		return "major"
	}
	if matched, _ := regexp.MatchString(`^feat(\(.+\))?:`, title); matched {
		return "minor"
	}
	if matched, _ := regexp.MatchString(`^fix(\(.+\))?:`, title); matched {
		return "patch"
	}
	return ""
}
