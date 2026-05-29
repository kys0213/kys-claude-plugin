package changes

import "testing"

func TestGetPluginsOnlyExcludesFrozen(t *testing.T) {
	pkgs := []Package{
		{Name: "atelier", Path: "plugins/atelier", Type: "plugin"},
		{Name: "git-utils", Path: "plugins/git-utils", Type: "plugin"},
		{Name: "github-autopilot", Path: "plugins/github-autopilot", Type: "plugin"},
		{Name: "spec-kit", Path: "plugins/spec-kit", Type: "plugin"},
		{Name: "workflow-guide", Path: "plugins/workflow-guide", Type: "plugin"},
		{Name: "coding-style", Path: "plugins/coding-style", Type: "plugin"},
		{Name: "orchestrator", Path: "plugins/orchestrator", Type: "plugin"},
		{Name: "autodev", Path: "plugins/autodev", Type: "plugin"},
		{Name: "common-thing", Path: "common/thing", Type: "common"},
	}

	got := GetPluginsOnly(pkgs)

	gotNames := make(map[string]bool)
	for _, p := range got {
		gotNames[p.Name] = true
	}

	// frozen plugins must be excluded
	for _, frozen := range []string{
		"git-utils", "github-autopilot", "spec-kit",
		"workflow-guide", "coding-style", "orchestrator",
	} {
		if gotNames[frozen] {
			t.Errorf("frozen plugin %q should be excluded from bump candidates", frozen)
		}
	}

	// active plugins must remain
	if !gotNames["atelier"] {
		t.Error("atelier should be included")
	}
	if !gotNames["autodev"] {
		t.Error("autodev should be included")
	}

	// common packages are not plugins
	if gotNames["common-thing"] {
		t.Error("common package should not be returned by GetPluginsOnly")
	}
}

func TestIsFrozen(t *testing.T) {
	cases := map[string]bool{
		"git-utils":        true,
		"github-autopilot": true,
		"spec-kit":         true,
		"workflow-guide":   true,
		"coding-style":     true,
		"orchestrator":     true,
		"atelier":          false,
		"autodev":          false,
		"suggest-workflow": false,
	}
	for name, want := range cases {
		if got := IsFrozen(name); got != want {
			t.Errorf("IsFrozen(%q) = %v, want %v", name, got, want)
		}
	}
}
