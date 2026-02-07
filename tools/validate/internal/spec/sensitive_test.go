package spec

import (
	"os"
	"path/filepath"
	"testing"
)

func TestValidateSensitiveData(t *testing.T) {
	tmpDir := t.TempDir()

	tests := []struct {
		name    string
		content string
		wantErr bool
	}{
		{
			name:    "clean script",
			content: "#!/bin/bash\necho 'hello world'\nexit 0\n",
			wantErr: false,
		},
		{
			name:    "password detected",
			content: "#!/bin/bash\npassword = 'mysecret123'\n",
			wantErr: true,
		},
		{
			name:    "api key detected",
			content: "#!/bin/bash\napi_key = 'sk-abc123'\n",
			wantErr: true,
		},
		{
			name:    "token detected",
			content: "#!/bin/bash\ntoken = 'ghp_abc123'\n",
			wantErr: true,
		},
		{
			name:    "secret detected",
			content: "#!/bin/bash\nsecret = 'my-secret'\n",
			wantErr: true,
		},
		{
			name:    "private key detected",
			content: "#!/bin/bash\nprivate_key = '/path/to/key'\n",
			wantErr: true,
		},
		{
			name:    "password in code block skipped",
			content: "# Script docs\n```\npassword = example\n```\necho done\n",
			wantErr: false,
		},
		{
			name:    "comment line skipped",
			content: "#!/bin/bash\n# password = example\necho done\n",
			wantErr: false,
		},
		{
			name:    "case insensitive detection",
			content: "#!/bin/bash\nAPI_KEY = 'test'\n",
			wantErr: true,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			filePath := filepath.Join(tmpDir, "test.sh")
			if err := os.WriteFile(filePath, []byte(tt.content), 0644); err != nil {
				t.Fatalf("Failed to write test file: %v", err)
			}

			result := validateSensitiveData(filePath)

			if tt.wantErr && result.Valid {
				t.Errorf("Expected validation to fail, but it passed")
			}
			if !tt.wantErr && !result.Valid {
				t.Errorf("Expected validation to pass, but it failed: %v", result.Errors)
			}
		})
	}
}
