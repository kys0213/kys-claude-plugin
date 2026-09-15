package bumper

import (
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/kys0213/kys-claude-plugin/tools/bumpversion/internal/changes"
)

func TestBumpCargoToml_SimplePackage(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "Cargo.toml")

	content := `[package]
name = "autodev"
version = "0.2.3"
edition = "2021"

[dependencies]
clap = { version = "4", features = ["derive"] }
serde = { version = "1.0.228", features = ["derive"] }
`
	if err := os.WriteFile(path, []byte(content), 0644); err != nil {
		t.Fatal(err)
	}

	b := NewBumper(dir, false)
	if err := b.bumpCargoToml(path, "0.9.4"); err != nil {
		t.Fatalf("bumpCargoToml failed: %v", err)
	}

	updated, err := os.ReadFile(path)
	if err != nil {
		t.Fatalf("failed to read updated file: %v", err)
	}
	result := string(updated)

	// [package] version should be updated
	if !strings.Contains(result, `version = "0.9.4"`) {
		t.Error("expected [package] version to be 0.9.4")
	}

	// dependency versions must NOT be modified
	if !strings.Contains(result, `version = "4"`) {
		t.Error("clap dependency version was corrupted")
	}
	if !strings.Contains(result, `version = "1.0.228"`) {
		t.Error("serde dependency version was corrupted")
	}
}

func TestBumpCargoToml_TableStyleDependency(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "Cargo.toml")

	content := `[package]
name = "myapp"
version = "1.0.0"
edition = "2021"

[dependencies.special-dep]
version = "3.2.1"
features = ["foo"]

[dependencies]
serde = "1"
`
	if err := os.WriteFile(path, []byte(content), 0644); err != nil {
		t.Fatal(err)
	}

	b := NewBumper(dir, false)
	if err := b.bumpCargoToml(path, "2.0.0"); err != nil {
		t.Fatalf("bumpCargoToml failed: %v", err)
	}

	updated, err := os.ReadFile(path)
	if err != nil {
		t.Fatalf("failed to read updated file: %v", err)
	}
	result := string(updated)

	// [package] version should be updated
	if !strings.Contains(result, `version = "2.0.0"`) {
		t.Error("expected [package] version to be 2.0.0")
	}

	// [dependencies.special-dep] version must NOT be modified
	if !strings.Contains(result, `version = "3.2.1"`) {
		t.Error("table-style dependency version was corrupted")
	}
}

func TestBumpCargoToml_NoPackageSection(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "Cargo.toml")

	content := `[workspace]
members = ["crates/*"]
`
	if err := os.WriteFile(path, []byte(content), 0644); err != nil {
		t.Fatal(err)
	}

	b := NewBumper(dir, false)
	err := b.bumpCargoToml(path, "1.0.0")
	if err == nil {
		t.Error("expected error for missing [package] section")
	}
	if !strings.Contains(err.Error(), "[package]") {
		t.Errorf("expected error about [package], got: %v", err)
	}
}

func TestBumpCargoToml_NoVersionInPackage(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "Cargo.toml")

	content := `[package]
name = "mylib"
edition = "2021"
`
	if err := os.WriteFile(path, []byte(content), 0644); err != nil {
		t.Fatal(err)
	}

	b := NewBumper(dir, false)
	err := b.bumpCargoToml(path, "1.0.0")
	if err == nil {
		t.Error("expected error for missing version field")
	}
	if !strings.Contains(err.Error(), "no version field") {
		t.Errorf("expected error about version field, got: %v", err)
	}
}

func TestBumpCargoToml_WorkspacePackageVersion(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "Cargo.toml")

	// [workspace.package] has version at line start, but it's NOT in [package]
	content := `[package]
name = "myapp"
version = "1.0.0"
edition = "2021"

[workspace.package]
version = "5.0.0"
`
	if err := os.WriteFile(path, []byte(content), 0644); err != nil {
		t.Fatal(err)
	}

	b := NewBumper(dir, false)
	if err := b.bumpCargoToml(path, "2.0.0"); err != nil {
		t.Fatalf("bumpCargoToml failed: %v", err)
	}

	updated, err := os.ReadFile(path)
	if err != nil {
		t.Fatalf("failed to read updated file: %v", err)
	}
	result := string(updated)

	// [package] version should be updated
	if !strings.Contains(result, "name = \"myapp\"\nversion = \"2.0.0\"") {
		t.Error("expected [package] version to be 2.0.0")
	}

	// [workspace.package] version must NOT be modified
	if !strings.Contains(result, "[workspace.package]\nversion = \"5.0.0\"") {
		t.Error("workspace.package version was corrupted")
	}
}

func TestBumpCargoToml_PrereleaseVersion(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "Cargo.toml")

	content := `[package]
name = "myapp"
version = "1.0.0-beta.1"
edition = "2021"

[dependencies]
serde = "1"
`
	if err := os.WriteFile(path, []byte(content), 0644); err != nil {
		t.Fatal(err)
	}

	b := NewBumper(dir, false)
	if err := b.bumpCargoToml(path, "1.0.0"); err != nil {
		t.Fatalf("bumpCargoToml failed on prerelease version: %v", err)
	}

	updated, err := os.ReadFile(path)
	if err != nil {
		t.Fatalf("failed to read updated file: %v", err)
	}
	result := string(updated)

	if !strings.Contains(result, `version = "1.0.0"`) {
		t.Error("expected prerelease version to be replaced with 1.0.0")
	}
	if strings.Contains(result, "beta") {
		t.Error("prerelease suffix should be removed after bump")
	}
}

func TestParseCargoPackageSection_PrereleaseValid(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "Cargo.toml")

	content := `[package]
name = "myapp"
version = "1.0.0-rc.1"
edition = "2021"
`
	if err := os.WriteFile(path, []byte(content), 0644); err != nil {
		t.Fatal(err)
	}

	info, err := parseCargoPackageSection(path)
	if err != nil {
		t.Fatalf("parseCargoPackageSection should accept prerelease version, got: %v", err)
	}
	if !strings.Contains(info.section, "1.0.0-rc.1") {
		t.Error("section should contain the prerelease version")
	}
}

func TestExtractCargoPackageVersion(t *testing.T) {
	dir := t.TempDir()

	tests := []struct {
		name    string
		content string
		want    string
		wantErr bool
	}{
		{
			name: "simple version",
			content: `[package]
name = "myapp"
version = "1.2.3"
`,
			want: "1.2.3",
		},
		{
			name: "prerelease version",
			content: `[package]
name = "myapp"
version = "1.0.0-beta.1"

[dependencies]
serde = { version = "1.0.0" }
`,
			want: "1.0.0-beta.1",
		},
		{
			name: "no package section",
			content: `[workspace]
members = ["crates/*"]
`,
			wantErr: true,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			path := filepath.Join(dir, tt.name+".toml")
			if err := os.WriteFile(path, []byte(tt.content), 0644); err != nil {
				t.Fatal(err)
			}

			got, err := ExtractCargoPackageVersion(path)
			if tt.wantErr {
				if err == nil {
					t.Error("expected error, got nil")
				}
				return
			}
			if err != nil {
				t.Fatalf("unexpected error: %v", err)
			}
			if got != tt.want {
				t.Errorf("got %q, want %q", got, tt.want)
			}
		})
	}
}

func TestBumpCargoToml_PreservesFileStructure(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "Cargo.toml")

	content := `[package]
name = "autodev"
version = "0.9.4"
edition = "2021"

[lib]
name = "autodev"
path = "src/lib.rs"

[[bin]]
name = "autodev"
path = "src/main.rs"

[dependencies]
clap = { version = "4", features = ["derive"] }
tokio = { version = "1", features = ["full"] }

[dev-dependencies]
assert_cmd = "2"

[profile.release]
opt-level = 3
lto = true
`
	if err := os.WriteFile(path, []byte(content), 0644); err != nil {
		t.Fatal(err)
	}

	b := NewBumper(dir, false)
	if err := b.bumpCargoToml(path, "0.9.5"); err != nil {
		t.Fatalf("bumpCargoToml failed: %v", err)
	}

	updated, err := os.ReadFile(path)
	if err != nil {
		t.Fatalf("failed to read updated file: %v", err)
	}
	result := string(updated)

	// Only version should change
	expected := strings.Replace(content, `version = "0.9.4"`, `version = "0.9.5"`, 1)
	if result != expected {
		t.Errorf("file structure was not preserved.\nExpected:\n%s\nGot:\n%s", expected, result)
	}
}

// Cargo.lock records the workspace-local crate's own version, so a bump that
// touches only Cargo.toml leaves the lockfile behind.

const lockWithLocalPackage = `# This file is automatically @generated by Cargo.
version = 4

[[package]]
name = "atelier"
version = "0.20.1"
dependencies = [
 "clap",
 "serde",
]

[[package]]
name = "clap"
version = "4.6.1"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "1ddb117e43bbf7dacf0a4190fef4d345b9bad68dfc649cb349e7d17d28428e51"
dependencies = [
 "clap_builder",
]
`

func writeLock(t *testing.T, content string) (string, string) {
	t.Helper()
	dir := t.TempDir()
	path := filepath.Join(dir, "Cargo.lock")
	if err := os.WriteFile(path, []byte(content), 0644); err != nil {
		t.Fatal(err)
	}
	return dir, path
}

func TestBumpCargoLock_LocalPackage(t *testing.T) {
	dir, path := writeLock(t, lockWithLocalPackage)

	b := NewBumper(dir, false)
	if err := b.bumpCargoLock(path, "atelier", "0.24.0"); err != nil {
		t.Fatalf("bumpCargoLock failed: %v", err)
	}

	updated, err := os.ReadFile(path)
	if err != nil {
		t.Fatalf("failed to read updated file: %v", err)
	}
	result := string(updated)

	if !strings.Contains(result, "name = \"atelier\"\nversion = \"0.24.0\"") {
		t.Error("expected local package version to be 0.24.0")
	}
	if !strings.Contains(result, "name = \"clap\"\nversion = \"4.6.1\"") {
		t.Error("registry dependency version was corrupted")
	}
	if !strings.Contains(result, `checksum = "1ddb117e43bbf7dacf0a4190fef4d345b9bad68dfc649cb349e7d17d28428e51"`) {
		t.Error("registry checksum was corrupted")
	}
	// The lockfile format header must survive untouched.
	if !strings.HasPrefix(result, "# This file is automatically @generated by Cargo.\nversion = 4\n") {
		t.Error("lockfile header was corrupted")
	}
}

func TestBumpCargoLock_CrateNotFound(t *testing.T) {
	dir, path := writeLock(t, lockWithLocalPackage)

	b := NewBumper(dir, false)
	err := b.bumpCargoLock(path, "nonexistent", "0.24.0")
	if err == nil {
		t.Fatal("expected an error for a crate missing from the lockfile")
	}

	after, readErr := os.ReadFile(path)
	if readErr != nil {
		t.Fatalf("failed to read file: %v", readErr)
	}
	if string(after) != lockWithLocalPackage {
		t.Error("lockfile must stay untouched when the crate is not found")
	}
}

func TestBumpCargoLock_IgnoresRegistryPackageWithSameName(t *testing.T) {
	// A crate published to crates.io under the same name must not be rewritten:
	// only the entry without a `source` field is the workspace-local one.
	content := `# This file is automatically @generated by Cargo.
version = 4

[[package]]
name = "atelier"
version = "9.9.9"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "deadbeef"

[[package]]
name = "atelier"
version = "0.20.1"
dependencies = [
 "clap",
]
`
	dir, path := writeLock(t, content)

	b := NewBumper(dir, false)
	if err := b.bumpCargoLock(path, "atelier", "0.24.0"); err != nil {
		t.Fatalf("bumpCargoLock failed: %v", err)
	}

	result, err := os.ReadFile(path)
	if err != nil {
		t.Fatalf("failed to read updated file: %v", err)
	}
	if !strings.Contains(string(result), `version = "9.9.9"`) {
		t.Error("registry package with the same name must not be rewritten")
	}
	if !strings.Contains(string(result), `version = "0.24.0"`) {
		t.Error("expected the workspace-local package to be bumped")
	}
}

func TestExtractCargoPackageName(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "Cargo.toml")

	content := `[package]
name = "atelier"
version = "0.24.0"
edition = "2021"

[dependencies]
clap = { version = "4", features = ["derive"] }
`
	if err := os.WriteFile(path, []byte(content), 0644); err != nil {
		t.Fatal(err)
	}

	name, err := ExtractCargoPackageName(path)
	if err != nil {
		t.Fatalf("ExtractCargoPackageName failed: %v", err)
	}
	if name != "atelier" {
		t.Errorf("expected crate name atelier, got %s", name)
	}
}

// TestBumpPlugins_UpdatesCargoLock exercises the full bump flow against a fixture
// repository: a bump must leave Cargo.toml and Cargo.lock reporting the same version.
func TestBumpPlugins_UpdatesCargoLock(t *testing.T) {
	root := t.TempDir()

	mustWrite := func(rel, content string) {
		t.Helper()
		path := filepath.Join(root, rel)
		if err := os.MkdirAll(filepath.Dir(path), 0755); err != nil {
			t.Fatal(err)
		}
		if err := os.WriteFile(path, []byte(content), 0644); err != nil {
			t.Fatal(err)
		}
	}

	mustWrite(".claude-plugin/marketplace.json",
		`{"plugins":[{"name":"atelier","version":"0.24.0"}]}`)
	mustWrite("plugins/atelier/.claude-plugin/plugin.json",
		`{"name":"atelier","version":"0.24.0"}`)
	mustWrite("plugins/atelier/cli/Cargo.toml", `[package]
name = "atelier"
version = "0.24.0"
edition = "2021"

[dependencies]
clap = { version = "4", features = ["derive"] }
`)
	mustWrite("plugins/atelier/cli/Cargo.lock", strings.Replace(
		lockWithLocalPackage, `version = "0.20.1"`, `version = "0.24.0"`, 1))

	b := NewBumper(root, false)
	results, err := b.BumpPlugins([]changes.Package{
		{Name: "atelier", Path: "plugins/atelier", Type: "plugin"},
	}, BumpPatch)
	if err != nil {
		t.Fatalf("BumpPlugins failed: %v", err)
	}

	if len(results) != 1 {
		t.Fatalf("expected 1 result, got %d", len(results))
	}
	if results[0].NewVersion != "0.24.1" {
		t.Fatalf("expected 0.24.1, got %s", results[0].NewVersion)
	}
	if !results[0].CargoLock {
		t.Error("expected cargo_lock_updated to be reported")
	}

	tomlVersion, err := ExtractCargoPackageVersion(filepath.Join(root, "plugins/atelier/cli/Cargo.toml"))
	if err != nil {
		t.Fatalf("failed to read Cargo.toml version: %v", err)
	}
	if tomlVersion != "0.24.1" {
		t.Errorf("Cargo.toml version = %s, want 0.24.1", tomlVersion)
	}

	lock, err := os.ReadFile(filepath.Join(root, "plugins/atelier/cli/Cargo.lock"))
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(string(lock), "name = \"atelier\"\nversion = \"0.24.1\"") {
		t.Error("Cargo.lock was not bumped in step with Cargo.toml")
	}
	if !strings.Contains(string(lock), "name = \"clap\"\nversion = \"4.6.1\"") {
		t.Error("dependency entry in Cargo.lock was corrupted")
	}
}
