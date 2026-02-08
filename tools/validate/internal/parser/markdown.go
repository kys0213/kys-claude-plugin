package parser

import (
	"bufio"
	"os"
	"regexp"
	"strings"

	"gopkg.in/yaml.v3"
)

// Frontmatter represents parsed YAML frontmatter
type Frontmatter map[string]interface{}

// ParseResult contains parsed markdown data
type ParseResult struct {
	Content     string
	Frontmatter Frontmatter
	Body        string
}

// PathInfo represents an extracted path reference
type PathInfo struct {
	Type     string // "inlineCode", "pluginRootPath", "codeBlockPath"
	Value    string
	Line     int
}

// ParseMarkdown parses a markdown file and extracts frontmatter
func ParseMarkdown(filePath string) (*ParseResult, error) {
	content, err := os.ReadFile(filePath)
	if err != nil {
		return nil, err
	}

	result := &ParseResult{
		Content: string(content),
	}

	// Extract frontmatter
	lines := strings.Split(string(content), "\n")
	if len(lines) > 0 && strings.TrimSpace(lines[0]) == "---" {
		endIndex := -1
		for i := 1; i < len(lines); i++ {
			if strings.TrimSpace(lines[i]) == "---" {
				endIndex = i
				break
			}
		}

		if endIndex > 0 {
			frontmatterYaml := strings.Join(lines[1:endIndex], "\n")
			var fm Frontmatter
			if err := yaml.Unmarshal([]byte(frontmatterYaml), &fm); err != nil {
				fm = Frontmatter{"_parseError": err.Error()}
			}
			result.Frontmatter = fm
			result.Body = strings.Join(lines[endIndex+1:], "\n")
		}
	}

	return result, nil
}

// ExtractPluginRootPaths extracts ${CLAUDE_PLUGIN_ROOT} paths from all content
func ExtractPluginRootPaths(filePath string) ([]PathInfo, error) {
	return extractPluginRootPaths(filePath, false)
}

// ExtractPluginRootPathsSkipCode extracts ${CLAUDE_PLUGIN_ROOT} paths, skipping code blocks and inline code
func ExtractPluginRootPathsSkipCode(filePath string) ([]PathInfo, error) {
	return extractPluginRootPaths(filePath, true)
}

func extractPluginRootPaths(filePath string, skipCode bool) ([]PathInfo, error) {
	file, err := os.Open(filePath)
	if err != nil {
		return nil, err
	}
	defer file.Close()

	var paths []PathInfo
	pluginRootPattern := regexp.MustCompile(`\$\{CLAUDE_PLUGIN_ROOT\}[^\s"')\` + "`" + `]+`)
	inlineCodePattern := regexp.MustCompile("`[^`]+`")

	scanner := bufio.NewScanner(file)
	lineNum := 0
	inCodeBlock := false
	for scanner.Scan() {
		lineNum++
		line := scanner.Text()

		if skipCode {
			// Skip paths inside code blocks (e.g. bash examples referencing build artifacts)
			if strings.HasPrefix(strings.TrimSpace(line), "```") {
				inCodeBlock = !inCodeBlock
				continue
			}
			if inCodeBlock {
				continue
			}
		}

		lineForMatch := line
		if skipCode {
			// Strip inline code (backtick-enclosed) before matching — these are documentation references
			lineForMatch = inlineCodePattern.ReplaceAllString(line, "")
		}

		matches := pluginRootPattern.FindAllString(lineForMatch, -1)
		for _, match := range matches {
			paths = append(paths, PathInfo{
				Type:  "pluginRootPath",
				Value: match,
				Line:  lineNum,
			})
		}
	}

	return paths, scanner.Err()
}

// ValidateFrontmatter checks required fields in frontmatter
func ValidateFrontmatter(fm Frontmatter, requiredFields []string) []string {
	var errors []string

	if fm == nil {
		return []string{"Missing YAML frontmatter"}
	}

	if parseErr, ok := fm["_parseError"].(string); ok {
		return []string{"Invalid YAML: " + parseErr}
	}

	for _, field := range requiredFields {
		if _, ok := fm[field]; !ok {
			errors = append(errors, "Missing required field: "+field)
		} else if fm[field] == nil || fm[field] == "" {
			errors = append(errors, "Missing required field: "+field)
		}
	}

	return errors
}

// GetString safely gets a string value from frontmatter
func (fm Frontmatter) GetString(key string) string {
	if v, ok := fm[key].(string); ok {
		return v
	}
	return ""
}

// GetStringSlice safely gets a string slice from frontmatter
func (fm Frontmatter) GetStringSlice(key string) []string {
	if v, ok := fm[key].([]interface{}); ok {
		result := make([]string, 0, len(v))
		for _, item := range v {
			if s, ok := item.(string); ok {
				result = append(result, s)
			}
		}
		return result
	}
	return nil
}

// IsArray checks if a frontmatter value is an array
func (fm Frontmatter) IsArray(key string) bool {
	_, ok := fm[key].([]interface{})
	return ok
}
