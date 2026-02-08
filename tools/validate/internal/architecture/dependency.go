package architecture

import (
	"fmt"
	"regexp"
	"strings"
)

// Reference represents a detected cross-layer reference
type Reference struct {
	TargetLayer Layer
	Pattern     string // the matched text
	Line        int
	Type        string // "path", "slash-command", "agent-invocation", "task-call"
}

// dependency detection patterns
var (
	// Path references to other layers (require kebab-case names, typical of actual file references)
	agentPathPattern   = regexp.MustCompile(`(?:\./?)?agents/[a-z][a-z0-9-]+`)
	commandPathPattern = regexp.MustCompile(`(?:\./?)?commands/[a-z][a-z0-9-]+`)
	skillPathPattern   = regexp.MustCompile(`(?:\./?)?skills/[a-z][a-z0-9-]+`)

	// Slash command references: /plugin:command
	slashCommandPattern = regexp.MustCompile(`/[a-z][\w-]+:[a-z][\w-]+`)

	// Task/subagent invocation patterns (indicates orchestration)
	taskCallPattern  = regexp.MustCompile(`(?i)Task\s*\(.*subagent`)
	subagentPattern  = regexp.MustCompile(`(?i)subagent_type\s*[=:]\s*["']?\w+`)
	agentCallPattern = regexp.MustCompile(`(?i)(?:에이전트|agent)\s+(?:호출|실행|call|invoke|spawn|launch)`)

	// Inline ignore comment: <!-- arch-ignore --> on a line suppresses checking
	archIgnorePattern = regexp.MustCompile(`<!--\s*arch-ignore\s*-->`)
)

// validateLayerDependencies checks that layer references follow the allowed direction:
// Command → Agent → Skill (downward only)
// Violations: Skill → Agent, Skill → Command, Agent → Command
func validateLayerDependencies(files []LayerFile, results *Results) {
	for _, file := range files {
		refs := detectReferences(file)
		var violations []string

		for _, ref := range refs {
			if isViolation(file.Layer, ref.TargetLayer) {
				violations = append(violations, fmt.Sprintf(
					"line %d: %s references %s layer (%s) — violates %s → %s direction",
					ref.Line, file.Layer, ref.TargetLayer, ref.Pattern,
					ref.TargetLayer, file.Layer,
				))
			}
		}

		result := Result{
			File:     file.Path,
			Type:     "layer-dependency",
			Severity: "error",
		}

		if len(violations) > 0 {
			result.Valid = false
			result.Errors = violations
			results.Failed = append(results.Failed, result)
		} else {
			result.Valid = true
			results.Passed = append(results.Passed, result)
		}
	}
}

// isViolation checks if a reference from sourceLayer to targetLayer violates the architecture
func isViolation(sourceLayer, targetLayer Layer) bool {
	// Allowed: Command(0) → Agent(1), Command(0) → Skill(2), Agent(1) → Skill(2)
	// Violation: higher number referencing lower number (upward dependency)
	// Skill(2) → Agent(1): violation
	// Skill(2) → Command(0): violation
	// Agent(1) → Command(0): violation
	return sourceLayer > targetLayer
}

// detectReferences scans file content for cross-layer references
func detectReferences(file LayerFile) []Reference {
	var refs []Reference

	inCodeBlock := false
	inExampleBlock := false

	for i, line := range file.Lines {
		lineNum := i + 1
		trimmed := strings.TrimSpace(line)

		// Track code blocks — skip references inside code examples
		if strings.HasPrefix(trimmed, "```") {
			inCodeBlock = !inCodeBlock
			// Detect example/documentation blocks
			lower := strings.ToLower(trimmed)
			if inCodeBlock && (strings.Contains(lower, "example") || strings.Contains(lower, "예시")) {
				inExampleBlock = true
			}
			if !inCodeBlock {
				inExampleBlock = false
			}
			continue
		}
		if inCodeBlock {
			continue
		}

		// Skip pure comment/documentation lines that describe architecture
		if isDocumentationLine(trimmed) {
			continue
		}

		// Support inline ignore: <!-- arch-ignore -->
		if archIgnorePattern.MatchString(line) {
			continue
		}

		// Skip lines inside example sections (marked by headers like "### 예시", "### Example")
		if inExampleBlock {
			continue
		}

		// Detect references based on the source file's layer
		switch file.Layer {
		case LayerSkill:
			// Skills should NOT reference agents or commands
			refs = append(refs, detectUpwardRefs(line, lineNum, LayerSkill)...)

		case LayerAgent:
			// Agents should NOT reference commands
			refs = append(refs, detectUpwardRefs(line, lineNum, LayerAgent)...)

		case LayerCommand:
			// Commands can reference anything below — no violations to detect
		}
	}

	return refs
}

// detectUpwardRefs detects references from a given layer to layers above it
func detectUpwardRefs(line string, lineNum int, sourceLayer Layer) []Reference {
	var refs []Reference

	switch sourceLayer {
	case LayerSkill:
		// Skill → Command (violation)
		if matches := slashCommandPattern.FindAllString(line, -1); len(matches) > 0 {
			for _, m := range matches {
				refs = append(refs, Reference{
					TargetLayer: LayerCommand,
					Pattern:     m,
					Line:        lineNum,
					Type:        "slash-command",
				})
			}
		}
		if commandPathPattern.MatchString(line) {
			refs = append(refs, Reference{
				TargetLayer: LayerCommand,
				Pattern:     commandPathPattern.FindString(line),
				Line:        lineNum,
				Type:        "path",
			})
		}

		// Skill → Agent (violation)
		if taskCallPattern.MatchString(line) {
			refs = append(refs, Reference{
				TargetLayer: LayerAgent,
				Pattern:     taskCallPattern.FindString(line),
				Line:        lineNum,
				Type:        "task-call",
			})
		}
		if subagentPattern.MatchString(line) {
			refs = append(refs, Reference{
				TargetLayer: LayerAgent,
				Pattern:     subagentPattern.FindString(line),
				Line:        lineNum,
				Type:        "agent-invocation",
			})
		}
		if agentCallPattern.MatchString(line) {
			refs = append(refs, Reference{
				TargetLayer: LayerAgent,
				Pattern:     agentCallPattern.FindString(line),
				Line:        lineNum,
				Type:        "agent-invocation",
			})
		}
		// Check agent path references in skills
		if agentPathPattern.MatchString(line) {
			refs = append(refs, Reference{
				TargetLayer: LayerAgent,
				Pattern:     agentPathPattern.FindString(line),
				Line:        lineNum,
				Type:        "path",
			})
		}

	case LayerAgent:
		// Agent → Command (violation)
		if matches := slashCommandPattern.FindAllString(line, -1); len(matches) > 0 {
			for _, m := range matches {
				refs = append(refs, Reference{
					TargetLayer: LayerCommand,
					Pattern:     m,
					Line:        lineNum,
					Type:        "slash-command",
				})
			}
		}
		if commandPathPattern.MatchString(line) {
			refs = append(refs, Reference{
				TargetLayer: LayerCommand,
				Pattern:     commandPathPattern.FindString(line),
				Line:        lineNum,
				Type:        "path",
			})
		}
	}

	return refs
}

// isDocumentationLine checks if a line is purely descriptive/architectural documentation
func isDocumentationLine(line string) bool {
	// Skip diagram lines (ASCII art)
	if strings.Contains(line, "───") || strings.Contains(line, "│") ||
		strings.Contains(line, "┌") || strings.Contains(line, "└") ||
		strings.Contains(line, "├") || strings.Contains(line, "→") {
		return true
	}

	// Skip markdown table rows
	if strings.HasPrefix(line, "|") && strings.HasSuffix(line, "|") {
		return true
	}

	// Skip markdown comments
	if strings.HasPrefix(line, "<!--") || strings.HasPrefix(line, "-->") {
		return true
	}

	return false
}
