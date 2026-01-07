package version

import (
	"os"
	"path/filepath"
	"testing"
)

func TestValidateVersionFormat(t *testing.T) {
	tmpDir := t.TempDir()

	tests := []struct {
		name    string
		content string
		wantErr bool
	}{
		{
			name:    "valid semver",
			content: `{"name": "test", "version": "1.0.0"}`,
			wantErr: false,
		},
		{
			name:    "valid semver with prerelease",
			content: `{"name": "test", "version": "1.0.0-beta.1"}`,
			wantErr: false,
		},
		{
			name:    "no version field (ok)",
			content: `{"name": "test"}`,
			wantErr: false,
		},
		{
			name:    "invalid semver - missing patch",
			content: `{"name": "test", "version": "1.0"}`,
			wantErr: true,
		},
		{
			name:    "invalid semver - letters",
			content: `{"name": "test", "version": "v1.0.0"}`,
			wantErr: true,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			filePath := filepath.Join(tmpDir, "plugin.json")
			if err := os.WriteFile(filePath, []byte(tt.content), 0644); err != nil {
				t.Fatalf("Failed to write test file: %v", err)
			}

			result := validateVersionFormat(filePath)

			if tt.wantErr && result.Valid {
				t.Errorf("Expected validation to fail, but it passed")
			}
			if !tt.wantErr && !result.Valid {
				t.Errorf("Expected validation to pass, but it failed: %v", result.Errors)
			}
		})
	}
}

func TestBumpVersion(t *testing.T) {
	tests := []struct {
		name     string
		version  string
		bumpType string
		want     string
		wantErr  bool
	}{
		{
			name:     "patch bump",
			version:  "1.0.0",
			bumpType: "patch",
			want:     "1.0.1",
		},
		{
			name:     "minor bump",
			version:  "1.0.0",
			bumpType: "minor",
			want:     "1.1.0",
		},
		{
			name:     "major bump",
			version:  "1.0.0",
			bumpType: "major",
			want:     "2.0.0",
		},
		{
			name:     "patch bump with existing numbers",
			version:  "2.5.3",
			bumpType: "patch",
			want:     "2.5.4",
		},
		{
			name:     "minor bump resets patch",
			version:  "2.5.3",
			bumpType: "minor",
			want:     "2.6.0",
		},
		{
			name:     "major bump resets minor and patch",
			version:  "2.5.3",
			bumpType: "major",
			want:     "3.0.0",
		},
		{
			name:     "invalid version",
			version:  "invalid",
			bumpType: "patch",
			wantErr:  true,
		},
		{
			name:     "invalid bump type",
			version:  "1.0.0",
			bumpType: "invalid",
			wantErr:  true,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got, err := BumpVersion(tt.version, tt.bumpType)

			if tt.wantErr {
				if err == nil {
					t.Errorf("Expected error, but got none")
				}
				return
			}

			if err != nil {
				t.Errorf("Unexpected error: %v", err)
				return
			}

			if got != tt.want {
				t.Errorf("BumpVersion(%q, %q) = %q, want %q", tt.version, tt.bumpType, got, tt.want)
			}
		})
	}
}

func TestGetBumpTypeFromPRTitle(t *testing.T) {
	tests := []struct {
		title string
		want  string
	}{
		{"feat: add new feature", "minor"},
		{"feat(scope): add new feature", "minor"},
		{"fix: bug fix", "patch"},
		{"fix(scope): bug fix", "patch"},
		{"major: breaking change", "major"},
		{"major(scope): breaking change", "major"},
		{"docs: update readme", ""},
		{"chore: cleanup", ""},
		{"refactor: code improvement", ""},
	}

	for _, tt := range tests {
		t.Run(tt.title, func(t *testing.T) {
			got := GetBumpTypeFromPRTitle(tt.title)
			if got != tt.want {
				t.Errorf("GetBumpTypeFromPRTitle(%q) = %q, want %q", tt.title, got, tt.want)
			}
		})
	}
}
