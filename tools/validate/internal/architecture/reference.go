package architecture

import (
	"fmt"
	"os"
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
// "plugins/team-claude/skills/feedback-routing/SKILL.md" → "feedback-routing"
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

// validateSkillCoverage checks if agents that use skills from the same plugin
// have declared them in their frontmatter (advisory/info level)
func validateSkillCoverage(files []LayerFile, repoRoot string, results *Results) {
	// Group agents by plugin
	agentsByPlugin := make(map[string][]LayerFile)
	for _, f := range files {
		if f.Layer == LayerAgent {
			agentsByPlugin[f.Plugin] = append(agentsByPlugin[f.Plugin], f)
		}
	}

	// Get skill count per plugin
	skillsByPlugin := make(map[string][]string)
	for _, f := range files {
		if f.Layer == LayerSkill {
			name := extractSkillName(f.Path)
			if name != "" {
				skillsByPlugin[f.Plugin] = append(skillsByPlugin[f.Plugin], name)
			}
		}
	}

	// For each plugin that has both agents AND skills, check coverage
	for plugin, agents := range agentsByPlugin {
		skills, hasSkills := skillsByPlugin[plugin]
		if !hasSkills || len(skills) == 0 {
			continue
		}

		// Check how many agents declare skills
		agentsWithSkills := 0
		for _, agent := range agents {
			if agent.Parsed != nil && agent.Parsed.Frontmatter != nil {
				declared := agent.Parsed.Frontmatter.GetStringSlice("skills")
				if len(declared) > 0 {
					agentsWithSkills++
				}
			}
		}

		// If plugin has skills but no agents declare them, flag as warning
		if agentsWithSkills == 0 && len(agents) > 0 {
			// Build list of agents missing skill declarations
			var agentNames []string
			for _, a := range agents {
				base := filepath.Base(a.Path)
				agentNames = append(agentNames, strings.TrimSuffix(base, ".md"))
			}

			// Check if any skill directory actually exists on disk (not just collected files)
			skillDirs := findSkillDirs(repoRoot, plugin)
			if len(skillDirs) == 0 {
				continue
			}

			results.Failed = append(results.Failed, Result{
				File:     fmt.Sprintf("plugins/%s/agents/", plugin),
				Type:     "skill-coverage",
				Valid:    false,
				Severity: "warning",
				Errors: []string{
					fmt.Sprintf(
						"plugin has %d skill(s) [%s] but none of %d agent(s) [%s] declare skills in frontmatter — consider adding skills: [...] to agent YAML",
						len(skills), strings.Join(skills, ", "),
						len(agents), strings.Join(agentNames, ", "),
					),
				},
			})
		}
	}
}

// findSkillDirs finds skill directories for a plugin
func findSkillDirs(repoRoot, plugin string) []string {
	skillsDir := filepath.Join(repoRoot, "plugins", plugin, "skills")
	entries, err := os.ReadDir(skillsDir)
	if err != nil {
		return nil
	}

	var dirs []string
	for _, e := range entries {
		if e.IsDir() {
			dirs = append(dirs, e.Name())
		}
	}
	return dirs
}
