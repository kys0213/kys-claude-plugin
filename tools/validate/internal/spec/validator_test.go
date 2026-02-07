package spec

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func TestValidatePluginJSON(t *testing.T) {
	// 임시 디렉토리 생성
	tmpDir := t.TempDir()

	tests := []struct {
		name     string
		content  string
		wantErr  bool
		errCount int
	}{
		{
			name:    "valid plugin.json",
			content: `{"name": "my-plugin", "version": "1.0.0"}`,
			wantErr: false,
		},
		{
			name:     "missing name",
			content:  `{"version": "1.0.0"}`,
			wantErr:  true,
			errCount: 1,
		},
		{
			name:     "invalid name format (not kebab-case)",
			content:  `{"name": "MyPlugin", "version": "1.0.0"}`,
			wantErr:  true,
			errCount: 1,
		},
		{
			name:     "invalid version format",
			content:  `{"name": "my-plugin", "version": "1.0"}`,
			wantErr:  true,
			errCount: 1,
		},
		{
			name:     "invalid JSON",
			content:  `{invalid}`,
			wantErr:  true,
			errCount: 1,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			// 임시 파일 생성
			filePath := filepath.Join(tmpDir, "plugin.json")
			if err := os.WriteFile(filePath, []byte(tt.content), 0644); err != nil {
				t.Fatalf("Failed to write test file: %v", err)
			}

			result := validatePluginJSON(filePath)

			if tt.wantErr && result.Valid {
				t.Errorf("Expected validation to fail, but it passed")
			}
			if !tt.wantErr && !result.Valid {
				t.Errorf("Expected validation to pass, but it failed: %v", result.Errors)
			}
			if tt.wantErr && len(result.Errors) != tt.errCount {
				t.Errorf("Expected %d errors, got %d: %v", tt.errCount, len(result.Errors), result.Errors)
			}
		})
	}
}

func TestValidateAgentMD(t *testing.T) {
	tmpDir := t.TempDir()

	tests := []struct {
		name    string
		content string
		wantErr bool
	}{
		{
			name: "valid agent",
			content: `---
name: my-agent
description: A test agent
model: sonnet
tools: ["Read", "Write"]
---

# My Agent
`,
			wantErr: false,
		},
		{
			name: "agent without name",
			content: `---
description: A test agent
---

# My Agent
`,
			wantErr: false,
		},
		{
			name: "invalid model",
			content: `---
name: my-agent
description: A test agent
model: invalid-model
---

# My Agent
`,
			wantErr: true,
		},
		{
			name: "tools not array",
			content: `---
name: my-agent
description: A test agent
tools: "Read"
---

# My Agent
`,
			wantErr: true,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			filePath := filepath.Join(tmpDir, "agent.md")
			if err := os.WriteFile(filePath, []byte(tt.content), 0644); err != nil {
				t.Fatalf("Failed to write test file: %v", err)
			}

			result := validateAgentMD(filePath)

			if tt.wantErr && result.Valid {
				t.Errorf("Expected validation to fail, but it passed")
			}
			if !tt.wantErr && !result.Valid {
				t.Errorf("Expected validation to pass, but it failed: %v", result.Errors)
			}
		})
	}
}

func TestValidateCommandMD(t *testing.T) {
	tmpDir := t.TempDir()

	tests := []struct {
		name    string
		content string
		wantErr bool
	}{
		{
			name: "valid command",
			content: `---
name: my-command
description: A test command
allowed-tools: ["Task", "Glob"]
---

# My Command
`,
			wantErr: false,
		},
		{
			name: "missing description",
			content: `---
name: my-command
---

# My Command
`,
			wantErr: true,
		},
		{
			name: "valid command without name",
			content: `---
description: A test command
---

# My Command
`,
			wantErr: false,
		},
		{
			name: "command with qualified name",
			content: `---
name: plugin:my-command
description: A test command
---

# My Command
`,
			wantErr: false,
		},
		{
			name: "allowed-tools not array",
			content: `---
name: my-command
description: A test command
allowed-tools: "Task"
---

# My Command
`,
			wantErr: true,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			filePath := filepath.Join(tmpDir, "command.md")
			if err := os.WriteFile(filePath, []byte(tt.content), 0644); err != nil {
				t.Fatalf("Failed to write test file: %v", err)
			}

			result := validateCommandMD(filePath)

			if tt.wantErr && result.Valid {
				t.Errorf("Expected validation to fail, but it passed")
			}
			if !tt.wantErr && !result.Valid {
				t.Errorf("Expected validation to pass, but it failed: %v", result.Errors)
			}
		})
	}
}

func TestValidateSkillMD(t *testing.T) {
	tmpDir := t.TempDir()

	tests := []struct {
		name    string
		content string
		wantErr bool
	}{
		{
			name: "skill without name still fails",
			content: `---
description: A test skill
---

` + strings.Repeat("line\n", 60),
			wantErr: true,
		},
		{
			name: "valid skill with name",
			content: `---
name: my-skill
description: A test skill
---

` + strings.Repeat("line\n", 60),
			wantErr: false,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			filePath := filepath.Join(tmpDir, "SKILL.md")
			if err := os.WriteFile(filePath, []byte(tt.content), 0644); err != nil {
				t.Fatalf("Failed to write test file: %v", err)
			}

			results := &Results{}
			result := validateSkillMD(filePath, results)

			if tt.wantErr && result.Valid {
				t.Errorf("Expected validation to fail, but it passed")
			}
			if !tt.wantErr && !result.Valid {
				t.Errorf("Expected validation to pass, but it failed: %v", result.Errors)
			}
		})
	}
}
