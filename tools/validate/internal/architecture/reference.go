package architecture

import (
	"fmt"
	"path/filepath"
	"strings"
)

// validateSkillReferences checks that skills declared in agent frontmatter actually exist
func validateSkillReferences(files []LayerFile, repoRoot string, results *Results) {
	// Build a registry of existing skills per plugin
	// Key: "plugin/skill-name", Value: full path
	skillRegistry := buildSkillRegistry(files)

	for _, file := range files {
		if file.Layer != LayerAgent {
			continue
		}

		if file.Parsed == nil || file.Parsed.Frontmatter == nil {
			continue
		}

		declaredSkills := file.Parsed.Frontmatter.GetStringSlice("skills")
		if len(declaredSkills) == 0 {
			continue
		}

		var errors []string

		for _, skillName := range declaredSkills {
			skillName = strings.TrimSpace(skillName)
			if skillName == "" {
				continue
			}

			// Check if skill exists in the same plugin
			key := file.Plugin + "/" + skillName
			if _, exists := skillRegistry[key]; !exists {
				// Try to find it in other plugins for a helpful message
				suggestion := findSkillSuggestion(skillName, skillRegistry)
				msg := fmt.Sprintf(
					"agent declares skill %q but it does not exist at plugins/%s/skills/%s/SKILL.md",
					skillName, file.Plugin, skillName,
				)
				if suggestion != "" {
					msg += fmt.Sprintf(" (found in %s)", suggestion)
				}
				errors = append(errors, msg)
			}
		}

		result := Result{
			File:     file.Path,
			Type:     "skill-reference",
			Severity: "error",
		}

		if len(errors) > 0 {
			result.Valid = false
			result.Errors = errors
			results.Failed = append(results.Failed, result)
		} else {
			result.Valid = true
			results.Passed = append(results.Passed, result)
		}
	}
}

// buildSkillRegistry builds a map of existing skills from collected files
func buildSkillRegistry(files []LayerFile) map[string]string {
	registry := make(map[string]string)

	for _, file := range files {
		if file.Layer != LayerSkill {
			continue
		}

		// Extract skill name from path
		skillName := extractSkillName(file.Path)
		if skillName != "" {
			key := file.Plugin + "/" + skillName
			registry[key] = file.Path
		}
	}

	return registry
}

// extractSkillName extracts the skill name from a skill file path
// "plugins/develop-workflow/skills/feedback-routing/SKILL.md" → "feedback-routing"
// "plugins/git-utils/skills/SKILL.md" → "git" (uses plugin name or directory)
func extractSkillName(path string) string {
	normalized := filepath.ToSlash(path)
	parts := strings.Split(normalized, "/")

	for i, part := range parts {
		if part == "skills" && i+2 < len(parts) && parts[i+2] == "SKILL.md" {
			// Pattern: skills/{name}/SKILL.md
			return parts[i+1]
		}
		if part == "skills" && i+1 < len(parts) && parts[i+1] == "SKILL.md" {
			// Pattern: skills/SKILL.md — use parent plugin name
			if i >= 2 && parts[i-2] == "plugins" {
				return parts[i-1]
			}
		}
	}

	return ""
}

// findSkillSuggestion searches for a skill name across all plugins
func findSkillSuggestion(skillName string, registry map[string]string) string {
	for key := range registry {
		parts := strings.SplitN(key, "/", 2)
		if len(parts) == 2 && parts[1] == skillName {
			return "plugins/" + parts[0]
		}
	}
	return ""
}

