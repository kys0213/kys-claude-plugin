package spec

import (
	"os"
	"path/filepath"
	"testing"
)

func TestValidateHookMD(t *testing.T) {
	tmpDir := t.TempDir()

	tests := []struct {
		name    string
		content string
		wantErr bool
	}{
		{
			name: "valid hook with all fields",
			content: `---
name: my-hook
description: A test hook
event: PreToolUse
type: prompt
matcher: Bash
---

# My Hook
`,
			wantErr: false,
		},
		{
			name: "valid hook minimal fields",
			content: `---
name: my-hook
description: A test hook
---

# My Hook
`,
			wantErr: false,
		},
		{
			name: "missing name",
			content: `---
description: A test hook
event: PreToolUse
---

# My Hook
`,
			wantErr: true,
		},
		{
			name: "missing description",
			content: `---
name: my-hook
event: PreToolUse
---

# My Hook
`,
			wantErr: true,
		},
		{
			name: "invalid event name",
			content: `---
name: my-hook
description: A test hook
event: InvalidEvent
---

# My Hook
`,
			wantErr: true,
		},
		{
			name: "invalid type",
			content: `---
name: my-hook
description: A test hook
type: invalid
---

# My Hook
`,
			wantErr: true,
		},
		{
			name: "valid SessionStart event",
			content: `---
name: my-hook
description: A test hook
event: SessionStart
type: command
---

# My Hook
`,
			wantErr: false,
		},
		{
			name: "valid Notification event",
			content: `---
name: my-hook
description: A test hook
event: Notification
type: intercept
---

# My Hook
`,
			wantErr: false,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			filePath := filepath.Join(tmpDir, "hook.md")
			if err := os.WriteFile(filePath, []byte(tt.content), 0644); err != nil {
				t.Fatalf("Failed to write test file: %v", err)
			}

			result := validateHookMD(filePath)

			if tt.wantErr && result.Valid {
				t.Errorf("Expected validation to fail, but it passed")
			}
			if !tt.wantErr && !result.Valid {
				t.Errorf("Expected validation to pass, but it failed: %v", result.Errors)
			}
		})
	}
}
