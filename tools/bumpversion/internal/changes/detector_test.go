package changes

import (
	"os"
	"path/filepath"
	"testing"
)

func TestGetPluginsOnlyExcludesMissingDirectories(t *testing.T) {
	repoRoot := t.TempDir()

	pluginsDir := filepath.Join(repoRoot, "plugins")
	if err := os.MkdirAll(filepath.Join(pluginsDir, "atelier"), 0o755); err != nil {
		t.Fatalf("failed to create fixture plugin dir: %v", err)
	}
	// "removed-plugin" is intentionally not created on disk, simulating a
	// plugin package detected from stale diff/ref data whose directory has
	// since been deleted from the working tree.

	pkgs := []Package{
		{Name: "atelier", Path: "plugins/atelier", Type: "plugin"},
		{Name: "removed-plugin", Path: "plugins/removed-plugin", Type: "plugin"},
		{Name: "common-thing", Path: "common/thing", Type: "common"},
	}

	got := GetPluginsOnly(pkgs, repoRoot)

	gotNames := make(map[string]bool)
	for _, p := range got {
		gotNames[p.Name] = true
	}

	if !gotNames["atelier"] {
		t.Error("atelier should be included: its directory exists in the working tree")
	}
	if gotNames["removed-plugin"] {
		t.Error("removed-plugin should be excluded: its directory does not exist in the working tree")
	}
	if gotNames["common-thing"] {
		t.Error("common package should not be returned by GetPluginsOnly")
	}
}
