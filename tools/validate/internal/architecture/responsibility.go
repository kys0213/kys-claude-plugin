package architecture

import (
	"fmt"
	"regexp"
	"strings"
)

// Responsibility violation patterns

// Anti-pattern: Fat Controller — command with too much business logic
const (
	commandMaxLines     = 500  // commands over this are likely doing too much
	commandWarnLines    = 300  // warning threshold
	skillMinLines       = 10   // skills below this are trivially thin
)

var (
	// Orchestration patterns that should NOT appear in skills
	orchestrationPatterns = []*regexp.Regexp{
		regexp.MustCompile(`(?i)Task\s*\(`),
		regexp.MustCompile(`(?i)subagent_type`),
		regexp.MustCompile(`(?i)(?:spawn|launch|delegate)\s+(?:agent|에이전트|worker)`),
		regexp.MustCompile(`(?i)병렬\s*(?:실행|처리|에이전트)`),
		regexp.MustCompile(`(?i)parallel\s+(?:execution|agents?|workers?)`),
	}

	// User interaction patterns that should NOT appear in agents/skills
	userInteractionPatterns = []*regexp.Regexp{
		regexp.MustCompile(`(?i)(?:사용자|user)\s*(?:입력|input|확인|confirm)`),
		regexp.MustCompile(`(?i)(?:ask|prompt)\s+(?:the\s+)?user`),
		regexp.MustCompile(`(?i)argument-hint`),
		regexp.MustCompile(`(?i)Magic\s+Keyword`),
	}

	// Business logic patterns that should NOT dominate commands
	businessLogicPatterns = []*regexp.Regexp{
		regexp.MustCompile(`(?i)(?:평가|evaluate|분석|analyze|검토|review)\s+(?:기준|criteria|항목|점수|score)`),
		regexp.MustCompile(`(?i)(?:점수|score)\s*[><=]+\s*\d+`),
		regexp.MustCompile(`(?i)(?:if|else|switch|case)\s+.*(?:then|do|→)`),
	}
)

// validateResponsibilities checks for layer responsibility violations
func validateResponsibilities(files []LayerFile, results *Results) {
	for _, file := range files {
		switch file.Layer {
		case LayerSkill:
			checkSkillResponsibility(file, results)
		case LayerAgent:
			checkAgentResponsibility(file, results)
		case LayerCommand:
			checkCommandResponsibility(file, results)
		}
	}
}

// checkSkillResponsibility ensures skills don't contain orchestration or user interaction
func checkSkillResponsibility(file LayerFile, results *Results) {
	var violations []string
	inCodeBlock := false

	for i, line := range file.Lines {
		trimmed := strings.TrimSpace(line)
		if strings.HasPrefix(trimmed, "```") {
			inCodeBlock = !inCodeBlock
			continue
		}
		if inCodeBlock || isDocumentationLine(trimmed) {
			continue
		}

		// Check for orchestration patterns (God Skill anti-pattern)
		for _, p := range orchestrationPatterns {
			if p.MatchString(line) {
				violations = append(violations, fmt.Sprintf(
					"line %d: skill contains orchestration logic (%s) — delegate to agent layer",
					i+1, p.FindString(line),
				))
			}
		}

		// Check for user interaction patterns
		for _, p := range userInteractionPatterns {
			if p.MatchString(line) {
				violations = append(violations, fmt.Sprintf(
					"line %d: skill contains user interaction (%s) — delegate to command layer",
					i+1, p.FindString(line),
				))
			}
		}
	}

	result := Result{
		File:     file.Path,
		Type:     "responsibility",
		Severity: "warning",
	}

	if len(violations) > 0 {
		result.Valid = false
		result.Errors = violations
		results.Warnings = append(results.Warnings, result)
	} else {
		result.Valid = true
		results.Passed = append(results.Passed, result)
	}
}

// checkAgentResponsibility ensures agents don't handle user interaction directly
func checkAgentResponsibility(file LayerFile, results *Results) {
	var violations []string
	inCodeBlock := false

	for i, line := range file.Lines {
		trimmed := strings.TrimSpace(line)
		if strings.HasPrefix(trimmed, "```") {
			inCodeBlock = !inCodeBlock
			continue
		}
		if inCodeBlock || isDocumentationLine(trimmed) {
			continue
		}

		// Check for user interaction patterns (should be in command layer)
		for _, p := range userInteractionPatterns {
			if p.MatchString(line) {
				violations = append(violations, fmt.Sprintf(
					"line %d: agent contains user interaction (%s) — delegate to command layer",
					i+1, p.FindString(line),
				))
			}
		}
	}

	result := Result{
		File:     file.Path,
		Type:     "responsibility",
		Severity: "warning",
	}

	if len(violations) > 0 {
		result.Valid = false
		result.Errors = violations
		results.Warnings = append(results.Warnings, result)
	} else {
		result.Valid = true
		results.Passed = append(results.Passed, result)
	}
}

// checkCommandResponsibility ensures commands don't contain too much business logic (Fat Controller)
func checkCommandResponsibility(file LayerFile, results *Results) {
	var violations []string

	// Check line count
	lineCount := len(file.Lines)
	if lineCount > commandMaxLines {
		violations = append(violations, fmt.Sprintf(
			"command has %d lines (max recommended: %d) — consider extracting logic to agent/skill layers (Fat Controller anti-pattern)",
			lineCount, commandMaxLines,
		))
	} else if lineCount > commandWarnLines {
		violations = append(violations, fmt.Sprintf(
			"command has %d lines (warning threshold: %d) — review if business logic should move to agent/skill layers",
			lineCount, commandWarnLines,
		))
	}

	// Check for excessive business logic patterns
	businessLogicCount := 0
	inCodeBlock := false
	for _, line := range file.Lines {
		trimmed := strings.TrimSpace(line)
		if strings.HasPrefix(trimmed, "```") {
			inCodeBlock = !inCodeBlock
			continue
		}
		if inCodeBlock {
			continue
		}

		for _, p := range businessLogicPatterns {
			if p.MatchString(line) {
				businessLogicCount++
			}
		}
	}

	if businessLogicCount > 10 {
		violations = append(violations, fmt.Sprintf(
			"command contains %d business logic patterns — consider extracting evaluation/analysis logic to agent layer",
			businessLogicCount,
		))
	}

	result := Result{
		File:     file.Path,
		Type:     "responsibility",
		Severity: "warning",
	}

	if len(violations) > 0 {
		result.Valid = false
		result.Errors = violations
		results.Warnings = append(results.Warnings, result)
	} else {
		result.Valid = true
		results.Passed = append(results.Passed, result)
	}
}
