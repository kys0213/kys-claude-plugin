package spec

import (
	"encoding/json"
	"os"
	"regexp"

	"github.com/bmatcuk/doublestar/v4"
	"github.com/kys0213/kys-claude-plugin/tools/validate/internal/parser"
)

// Result represents a validation result
type Result struct {
	File   string   `json:"file"`
	Type   string   `json:"type"`
	Valid  bool     `json:"valid"`
	Errors []string `json:"errors,omitempty"`
}

// Results contains all validation results
type Results struct {
	Passed []Result
	Failed []Result
}

var (
	kebabCaseRegex = regexp.MustCompile(`^[a-z0-9]+(-[a-z0-9]+)*$`)
	semverRegex    = regexp.MustCompile(`^\d+\.\d+\.\d+(-[\w.]+)?$`)
	validModels    = []string{"inherit", "sonnet", "opus", "haiku"}
)

// Validate runs all spec validations
func Validate(repoRoot string) (*Results, error) {
	results := &Results{}

	// 1. plugin.json validation
	pluginFiles, _ := doublestar.Glob(os.DirFS(repoRoot), "**/plugin.json")
	for _, file := range pluginFiles {
		result := validatePluginJSON(repoRoot + "/" + file)
		if result.Valid {
			results.Passed = append(results.Passed, result)
		} else {
			results.Failed = append(results.Failed, result)
		}
	}

	// 2. marketplace.json validation
	marketplaceFile := repoRoot + "/.claude-plugin/marketplace.json"
	if _, err := os.Stat(marketplaceFile); err == nil {
		result := validateMarketplaceJSON(marketplaceFile)
		if result.Valid {
			results.Passed = append(results.Passed, result)
		} else {
			results.Failed = append(results.Failed, result)
		}
	}

	// 3. SKILL.md validation
	skillFiles, _ := doublestar.Glob(os.DirFS(repoRoot), "**/skills/*/SKILL.md")
	for _, file := range skillFiles {
		result := validateSkillMD(repoRoot + "/" + file)
		if result.Valid {
			results.Passed = append(results.Passed, result)
		} else {
			results.Failed = append(results.Failed, result)
		}
	}

	// 4. Agent validation
	agentFiles, _ := doublestar.Glob(os.DirFS(repoRoot), "**/agents/*.md")
	for _, file := range agentFiles {
		result := validateAgentMD(repoRoot + "/" + file)
		if result.Valid {
			results.Passed = append(results.Passed, result)
		} else {
			results.Failed = append(results.Failed, result)
		}
	}

	// 5. Command validation
	commandFiles, _ := doublestar.Glob(os.DirFS(repoRoot), "**/commands/*.md")
	for _, file := range commandFiles {
		result := validateCommandMD(repoRoot + "/" + file)
		if result.Valid {
			results.Passed = append(results.Passed, result)
		} else {
			results.Failed = append(results.Failed, result)
		}
	}

	return results, nil
}

func validatePluginJSON(filePath string) Result {
	result := Result{
		File: filePath,
		Type: "plugin.json",
	}

	content, err := os.ReadFile(filePath)
	if err != nil {
		result.Errors = append(result.Errors, "Cannot read file: "+err.Error())
		return result
	}

	var data map[string]interface{}
	if err := json.Unmarshal(content, &data); err != nil {
		result.Errors = append(result.Errors, "Invalid JSON: "+err.Error())
		return result
	}

	// Required field: name
	name, ok := data["name"].(string)
	if !ok || name == "" {
		result.Errors = append(result.Errors, "Missing required field: name")
	} else if !kebabCaseRegex.MatchString(name) {
		result.Errors = append(result.Errors, "Invalid name format: '"+name+"' (must be kebab-case)")
	}

	// Optional field: version (if present, must be semver)
	if version, ok := data["version"].(string); ok && version != "" {
		if !semverRegex.MatchString(version) {
			result.Errors = append(result.Errors, "Invalid version format: '"+version+"' (must be semver)")
		}
	}

	result.Valid = len(result.Errors) == 0
	return result
}

func validateMarketplaceJSON(filePath string) Result {
	result := Result{
		File: filePath,
		Type: "marketplace.json",
	}

	content, err := os.ReadFile(filePath)
	if err != nil {
		result.Errors = append(result.Errors, "Cannot read file: "+err.Error())
		return result
	}

	var data map[string]interface{}
	if err := json.Unmarshal(content, &data); err != nil {
		result.Errors = append(result.Errors, "Invalid JSON: "+err.Error())
		return result
	}

	// Required: name
	if name, ok := data["name"].(string); !ok || name == "" {
		result.Errors = append(result.Errors, "Missing required field: name")
	}

	// Required: owner.name
	if owner, ok := data["owner"].(map[string]interface{}); ok {
		if ownerName, ok := owner["name"].(string); !ok || ownerName == "" {
			result.Errors = append(result.Errors, "Missing required field: owner.name")
		}
	} else {
		result.Errors = append(result.Errors, "Missing required field: owner.name")
	}

	// Required: plugins array
	plugins, ok := data["plugins"].([]interface{})
	if !ok {
		result.Errors = append(result.Errors, "Missing or invalid field: plugins (must be array)")
	} else {
		for i, p := range plugins {
			plugin, ok := p.(map[string]interface{})
			if !ok {
				continue
			}
			if name, ok := plugin["name"].(string); !ok || name == "" {
				result.Errors = append(result.Errors, "plugins["+string(rune('0'+i))+"]: Missing required field 'name'")
			}
			if source, ok := plugin["source"]; !ok || source == nil {
				result.Errors = append(result.Errors, "plugins["+string(rune('0'+i))+"]: Missing required field 'source'")
			}
			if version, ok := plugin["version"].(string); ok && version != "" {
				if !semverRegex.MatchString(version) {
					result.Errors = append(result.Errors, "plugins["+string(rune('0'+i))+"]: Invalid version format '"+version+"'")
				}
			}
		}
	}

	result.Valid = len(result.Errors) == 0
	return result
}

func validateSkillMD(filePath string) Result {
	result := Result{
		File: filePath,
		Type: "skill",
	}

	parsed, err := parser.ParseMarkdown(filePath)
	if err != nil {
		result.Errors = append(result.Errors, "Parse error: "+err.Error())
		return result
	}

	errors := parser.ValidateFrontmatter(parsed.Frontmatter, []string{"name", "description"})
	result.Errors = append(result.Errors, errors...)
	result.Valid = len(result.Errors) == 0
	return result
}

func validateAgentMD(filePath string) Result {
	result := Result{
		File: filePath,
		Type: "agent",
	}

	parsed, err := parser.ParseMarkdown(filePath)
	if err != nil {
		result.Errors = append(result.Errors, "Parse error: "+err.Error())
		return result
	}

	errors := parser.ValidateFrontmatter(parsed.Frontmatter, []string{"name", "description"})
	result.Errors = append(result.Errors, errors...)

	// tools must be array
	if _, ok := parsed.Frontmatter["tools"]; ok {
		if !parsed.Frontmatter.IsArray("tools") {
			result.Errors = append(result.Errors, "'tools' must be an array")
		}
	}

	// model validation
	if model := parsed.Frontmatter.GetString("model"); model != "" {
		valid := false
		for _, m := range validModels {
			if model == m {
				valid = true
				break
			}
		}
		if !valid {
			result.Errors = append(result.Errors, "Invalid model: '"+model+"' (valid: inherit, sonnet, opus, haiku)")
		}
	}

	result.Valid = len(result.Errors) == 0
	return result
}

func validateCommandMD(filePath string) Result {
	result := Result{
		File: filePath,
		Type: "command",
	}

	parsed, err := parser.ParseMarkdown(filePath)
	if err != nil {
		result.Errors = append(result.Errors, "Parse error: "+err.Error())
		return result
	}

	errors := parser.ValidateFrontmatter(parsed.Frontmatter, []string{"name", "description"})
	result.Errors = append(result.Errors, errors...)

	// allowed-tools must be array
	if _, ok := parsed.Frontmatter["allowed-tools"]; ok {
		if !parsed.Frontmatter.IsArray("allowed-tools") {
			result.Errors = append(result.Errors, "'allowed-tools' must be an array")
		}
	}

	result.Valid = len(result.Errors) == 0
	return result
}
