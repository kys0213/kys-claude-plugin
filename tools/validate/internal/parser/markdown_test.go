package parser

import (
	"os"
	"path/filepath"
	"testing"
)

func TestParseMarkdown(t *testing.T) {
	tmpDir := t.TempDir()

	tests := []struct {
		name        string
		content     string
		wantFM      bool
		wantFMField string
		wantFMValue string
	}{
		{
			name: "with frontmatter",
			content: `---
name: test
description: test description
---

# Content
`,
			wantFM:      true,
			wantFMField: "name",
			wantFMValue: "test",
		},
		{
			name: "without frontmatter",
			content: `# Just Content

No frontmatter here.
`,
			wantFM: false,
		},
		{
			name: "empty file",
			content: ``,
			wantFM:  false,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			filePath := filepath.Join(tmpDir, "test.md")
			if err := os.WriteFile(filePath, []byte(tt.content), 0644); err != nil {
				t.Fatalf("Failed to write test file: %v", err)
			}

			result, err := ParseMarkdown(filePath)
			if err != nil {
				t.Fatalf("ParseMarkdown failed: %v", err)
			}

			if tt.wantFM && result.Frontmatter == nil {
				t.Error("Expected frontmatter, but got nil")
			}
			if !tt.wantFM && result.Frontmatter != nil {
				t.Error("Expected no frontmatter, but got some")
			}

			if tt.wantFMField != "" {
				val := result.Frontmatter.GetString(tt.wantFMField)
				if val != tt.wantFMValue {
					t.Errorf("Expected frontmatter[%s] = %q, got %q", tt.wantFMField, tt.wantFMValue, val)
				}
			}
		})
	}
}

func TestValidateFrontmatter(t *testing.T) {
	tests := []struct {
		name           string
		fm             Frontmatter
		requiredFields []string
		wantErrCount   int
	}{
		{
			name:           "all required fields present",
			fm:             Frontmatter{"name": "test", "description": "desc"},
			requiredFields: []string{"name", "description"},
			wantErrCount:   0,
		},
		{
			name:           "missing one required field",
			fm:             Frontmatter{"name": "test"},
			requiredFields: []string{"name", "description"},
			wantErrCount:   1,
		},
		{
			name:           "missing all required fields",
			fm:             Frontmatter{},
			requiredFields: []string{"name", "description"},
			wantErrCount:   2,
		},
		{
			name:           "nil frontmatter",
			fm:             nil,
			requiredFields: []string{"name"},
			wantErrCount:   1,
		},
		{
			name:           "empty string value",
			fm:             Frontmatter{"name": ""},
			requiredFields: []string{"name"},
			wantErrCount:   1,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			errors := ValidateFrontmatter(tt.fm, tt.requiredFields)
			if len(errors) != tt.wantErrCount {
				t.Errorf("Expected %d errors, got %d: %v", tt.wantErrCount, len(errors), errors)
			}
		})
	}
}

func TestExtractPluginRootPaths(t *testing.T) {
	tmpDir := t.TempDir()

	content := `# Test

Some text with a path: ${CLAUDE_PLUGIN_ROOT}/scripts/test.sh

And another: ${CLAUDE_PLUGIN_ROOT}/../../common/scripts/call.sh

No path here.
`

	filePath := filepath.Join(tmpDir, "test.md")
	if err := os.WriteFile(filePath, []byte(content), 0644); err != nil {
		t.Fatalf("Failed to write test file: %v", err)
	}

	paths, err := ExtractPluginRootPaths(filePath)
	if err != nil {
		t.Fatalf("ExtractPluginRootPaths failed: %v", err)
	}

	if len(paths) != 2 {
		t.Errorf("Expected 2 paths, got %d", len(paths))
	}

	expectedPaths := []string{
		"${CLAUDE_PLUGIN_ROOT}/scripts/test.sh",
		"${CLAUDE_PLUGIN_ROOT}/../../common/scripts/call.sh",
	}

	for i, expected := range expectedPaths {
		if i < len(paths) && paths[i].Value != expected {
			t.Errorf("Expected path %d to be %q, got %q", i, expected, paths[i].Value)
		}
	}
}

func TestFrontmatterGetStringSlice(t *testing.T) {
	fm := Frontmatter{
		"tools": []interface{}{"Read", "Write", "Bash"},
	}

	got := fm.GetStringSlice("tools")
	want := []string{"Read", "Write", "Bash"}

	if len(got) != len(want) {
		t.Errorf("Expected %d items, got %d", len(want), len(got))
		return
	}

	for i, v := range want {
		if got[i] != v {
			t.Errorf("Expected item %d to be %q, got %q", i, v, got[i])
		}
	}
}

func TestFrontmatterIsArray(t *testing.T) {
	fm := Frontmatter{
		"tools":  []interface{}{"Read", "Write"},
		"name":   "test",
		"number": 42,
	}

	if !fm.IsArray("tools") {
		t.Error("Expected 'tools' to be an array")
	}
	if fm.IsArray("name") {
		t.Error("Expected 'name' to not be an array")
	}
	if fm.IsArray("nonexistent") {
		t.Error("Expected 'nonexistent' to not be an array")
	}
}
