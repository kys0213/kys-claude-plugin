package bumper

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
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

	updated, _ := os.ReadFile(path)
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

	updated, _ := os.ReadFile(path)
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

	updated, _ := os.ReadFile(path)
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

	updated, _ := os.ReadFile(path)
	result := string(updated)

	// Only version should change
	expected := strings.Replace(content, `version = "0.9.4"`, `version = "0.9.5"`, 1)
	if result != expected {
		t.Errorf("file structure was not preserved.\nExpected:\n%s\nGot:\n%s", expected, result)
	}
}
