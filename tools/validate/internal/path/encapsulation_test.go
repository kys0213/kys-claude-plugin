package path

import (
	"os"
	"path/filepath"
	"testing"
)

func TestValidateStrictEncapsulation(t *testing.T) {
	// Create a temp directory with plugin structure
	tmpDir := t.TempDir()

	t.Run("strict plugin with clean paths passes", func(t *testing.T) {
		pluginDir := filepath.Join(tmpDir, "strict-clean")
		os.MkdirAll(filepath.Join(pluginDir, ".claude-plugin"), 0755)
		os.MkdirAll(filepath.Join(pluginDir, "commands"), 0755)

		// strict: true (explicit) plugin.json
		os.WriteFile(
			filepath.Join(pluginDir, ".claude-plugin", "plugin.json"),
			[]byte(`{"name": "strict-clean", "version": "1.0.0", "strict": true}`),
			0644,
		)

		// Command with clean path
		os.WriteFile(
			filepath.Join(pluginDir, "commands", "test.md"),
			[]byte("---\nname: test\ndescription: test\n---\nUse `${CLAUDE_PLUGIN_ROOT}/scripts/run.sh`\n"),
			0644,
		)

		results := validateStrictEncapsulation(
			filepath.Join(pluginDir, ".claude-plugin", "plugin.json"),
			tmpDir,
		)

		for _, r := range results {
			if !r.Valid {
				t.Errorf("Expected clean strict plugin to pass, got error: %s", r.Error)
			}
		}
	})

	t.Run("strict plugin with parent dir reference fails", func(t *testing.T) {
		pluginDir := filepath.Join(tmpDir, "strict-parent")
		os.MkdirAll(filepath.Join(pluginDir, ".claude-plugin"), 0755)
		os.MkdirAll(filepath.Join(pluginDir, "commands"), 0755)

		os.WriteFile(
			filepath.Join(pluginDir, ".claude-plugin", "plugin.json"),
			[]byte(`{"name": "strict-parent", "version": "1.0.0", "strict": true}`),
			0644,
		)

		// Command with ../ path
		os.WriteFile(
			filepath.Join(pluginDir, "commands", "test.md"),
			[]byte("---\nname: test\ndescription: test\n---\nUse `${CLAUDE_PLUGIN_ROOT}/../other-plugin/script.sh`\n"),
			0644,
		)

		results := validateStrictEncapsulation(
			filepath.Join(pluginDir, ".claude-plugin", "plugin.json"),
			tmpDir,
		)

		hasError := false
		for _, r := range results {
			if !r.Valid {
				hasError = true
			}
		}
		if !hasError {
			t.Errorf("Expected strict plugin with ../ to fail validation")
		}
	})

	t.Run("non-strict plugin with parent dir passes", func(t *testing.T) {
		pluginDir := filepath.Join(tmpDir, "nonstrict")
		os.MkdirAll(filepath.Join(pluginDir, ".claude-plugin"), 0755)
		os.MkdirAll(filepath.Join(pluginDir, "commands"), 0755)

		os.WriteFile(
			filepath.Join(pluginDir, ".claude-plugin", "plugin.json"),
			[]byte(`{"name": "nonstrict", "version": "1.0.0", "strict": false}`),
			0644,
		)

		// Command with ../ path should be OK for non-strict
		os.WriteFile(
			filepath.Join(pluginDir, "commands", "test.md"),
			[]byte("---\nname: test\ndescription: test\n---\nUse `${CLAUDE_PLUGIN_ROOT}/../other-plugin/script.sh`\n"),
			0644,
		)

		results := validateStrictEncapsulation(
			filepath.Join(pluginDir, ".claude-plugin", "plugin.json"),
			tmpDir,
		)

		// Non-strict should return no results (skipped)
		if len(results) > 0 {
			t.Errorf("Expected non-strict plugin to skip encapsulation check, got %d results", len(results))
		}
	})

	t.Run("strict plugin with plugin-relative path passes", func(t *testing.T) {
		pluginDir := filepath.Join(tmpDir, "strict-rel")
		os.MkdirAll(filepath.Join(pluginDir, ".claude-plugin"), 0755)
		os.MkdirAll(filepath.Join(pluginDir, "commands"), 0755)

		os.WriteFile(
			filepath.Join(pluginDir, ".claude-plugin", "plugin.json"),
			[]byte(`{"name": "strict-rel", "version": "1.0.0", "strict": true}`),
			0644,
		)

		// ${CLAUDE_PLUGIN_ROOT}/some/deep/path is valid (relative to plugin root)
		os.WriteFile(
			filepath.Join(pluginDir, "commands", "test.md"),
			[]byte("---\nname: test\ndescription: test\n---\nUse `${CLAUDE_PLUGIN_ROOT}/usr/local/bin/script.sh`\n"),
			0644,
		)

		results := validateStrictEncapsulation(
			filepath.Join(pluginDir, ".claude-plugin", "plugin.json"),
			tmpDir,
		)

		// Paths with ${CLAUDE_PLUGIN_ROOT} prefix are plugin-relative, not absolute
		for _, r := range results {
			if !r.Valid {
				t.Errorf("Expected plugin-relative path to pass, got error: %s", r.Error)
			}
		}
	})

	t.Run("strict plugin with multiple violations", func(t *testing.T) {
		pluginDir := filepath.Join(tmpDir, "strict-multi")
		os.MkdirAll(filepath.Join(pluginDir, ".claude-plugin"), 0755)
		os.MkdirAll(filepath.Join(pluginDir, "commands"), 0755)
		os.MkdirAll(filepath.Join(pluginDir, "agents"), 0755)

		os.WriteFile(
			filepath.Join(pluginDir, ".claude-plugin", "plugin.json"),
			[]byte(`{"name": "strict-multi", "version": "1.0.0", "strict": true}`),
			0644,
		)

		// Command with parent dir
		os.WriteFile(
			filepath.Join(pluginDir, "commands", "test.md"),
			[]byte("---\nname: test\ndescription: test\n---\nUse `${CLAUDE_PLUGIN_ROOT}/../shared/script.sh`\n"),
			0644,
		)

		// Agent with absolute path
		os.WriteFile(
			filepath.Join(pluginDir, "agents", "agent.md"),
			[]byte("Use `${CLAUDE_PLUGIN_ROOT}/bin/tool` for testing.\n"),
			0644,
		)

		results := validateStrictEncapsulation(
			filepath.Join(pluginDir, ".claude-plugin", "plugin.json"),
			tmpDir,
		)

		// Should have at least one error (from parent dir reference)
		errorCount := 0
		for _, r := range results {
			if !r.Valid {
				errorCount++
			}
		}
		if errorCount < 1 {
			t.Errorf("Expected at least 1 validation error, got %d", errorCount)
		}
	})
}

func TestValidateMarkdownLinks(t *testing.T) {
	tmpDir := t.TempDir()

	// Create some target files
	os.MkdirAll(filepath.Join(tmpDir, "scripts"), 0755)
	os.WriteFile(filepath.Join(tmpDir, "scripts", "run.sh"), []byte("#!/bin/bash"), 0644)
	os.WriteFile(filepath.Join(tmpDir, "README.md"), []byte("# Readme"), 0644)

	tests := []struct {
		name      string
		content   string
		wantFails int
	}{
		{
			name:      "valid local link",
			content:   "See [readme](./README.md) for details.\n",
			wantFails: 0,
		},
		{
			name:      "valid script link",
			content:   "Run [script](./scripts/run.sh) first.\n",
			wantFails: 0,
		},
		{
			name:      "broken local link",
			content:   "See [nonexistent](./nonexistent.md) file.\n",
			wantFails: 1,
		},
		{
			name:      "external URL skipped",
			content:   "Visit [Google](https://google.com) for info.\n",
			wantFails: 0,
		},
		{
			name:      "anchor link skipped",
			content:   "See [section](#my-section) below.\n",
			wantFails: 0,
		},
		{
			name:      "plugin root path skipped",
			content:   "Use [script](${CLAUDE_PLUGIN_ROOT}/scripts/run.sh)\n",
			wantFails: 0,
		},
		{
			name:      "link inside code block skipped",
			content:   "```\n[broken](./nonexistent.md)\n```\n",
			wantFails: 0,
		},
		{
			name:      "link with anchor to existing file",
			content:   "See [readme section](./README.md#introduction) for details.\n",
			wantFails: 0,
		},
		{
			name:      "link with anchor to missing file",
			content:   "See [missing](./missing.md#section) for details.\n",
			wantFails: 1,
		},
		{
			name:      "http URL skipped",
			content:   "Visit [example](http://example.com) for info.\n",
			wantFails: 0,
		},
		{
			name:      "multiple links mixed valid and invalid",
			content:   "See [readme](./README.md) and [broken](./broken.md) files.\n",
			wantFails: 1,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			filePath := filepath.Join(tmpDir, "test.md")
			if err := os.WriteFile(filePath, []byte(tt.content), 0644); err != nil {
				t.Fatalf("Failed to write test file: %v", err)
			}

			results := validateMarkdownLinks(filePath)

			failCount := 0
			for _, r := range results {
				if !r.Valid {
					failCount++
				}
			}

			if failCount != tt.wantFails {
				t.Errorf("Expected %d failures, got %d. Results: %+v", tt.wantFails, failCount, results)
			}
		})
	}
}

func TestValidateMarkdownLinks_CodeBlockHandling(t *testing.T) {
	tmpDir := t.TempDir()

	content := `# Documentation

Normal link: [exists](./README.md)

` + "```" + `bash
# This link should be skipped
[broken](./nonexistent.md)
` + "```" + `

Another link: [also broken](./also-nonexistent.md)
`

	os.WriteFile(filepath.Join(tmpDir, "README.md"), []byte("# Readme"), 0644)
	filePath := filepath.Join(tmpDir, "test.md")
	os.WriteFile(filePath, []byte(content), 0644)

	results := validateMarkdownLinks(filePath)

	// Should have 1 valid (./README.md) and 1 invalid (./also-nonexistent.md)
	// The link in the code block should be skipped
	validCount := 0
	invalidCount := 0
	for _, r := range results {
		if r.Valid {
			validCount++
		} else {
			invalidCount++
		}
	}

	if validCount != 1 {
		t.Errorf("Expected 1 valid link, got %d", validCount)
	}
	if invalidCount != 1 {
		t.Errorf("Expected 1 invalid link, got %d", invalidCount)
	}
}
