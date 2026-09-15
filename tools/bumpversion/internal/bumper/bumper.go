package bumper

import (
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"regexp"
	"strconv"
	"strings"

	"github.com/kys0213/kys-claude-plugin/tools/bumpversion/internal/changes"
)

// BumpType represents the type of version bump
type BumpType string

const (
	BumpMajor BumpType = "major"
	BumpMinor BumpType = "minor"
	BumpPatch BumpType = "patch"
)

// Pre-compiled regexes for Cargo.toml parsing
var (
	cargoPackageRe      = regexp.MustCompile(`(?m)^\[package\]\s*$`)
	cargoSectionRe      = regexp.MustCompile(`(?m)^\[`)
	cargoVersionRe      = regexp.MustCompile(`(?m)^(version\s*=\s*")\d+\.\d+\.\d+(-[\w.]+)?(")\s*$`)
	cargoVersionMatchRe = regexp.MustCompile(`(?m)^version\s*=\s*"\d+\.\d+\.\d+(-[\w.]+)?"\s*$`)
	cargoVersionExtract = regexp.MustCompile(`(?m)^version\s*=\s*"(\d+\.\d+\.\d+(?:-[\w.]+)?)"\s*$`)
	cargoNameExtract    = regexp.MustCompile(`(?m)^name\s*=\s*"([^"]+)"\s*$`)
)

// Pre-compiled regexes for Cargo.lock parsing
var (
	cargoLockPackageRe = regexp.MustCompile(`(?m)^\[\[package\]\]\s*$`)
	cargoLockSourceRe  = regexp.MustCompile(`(?m)^source\s*=\s*"`)
)

// BumpResult represents the result of a version bump operation
type BumpResult struct {
	Plugin      string `json:"plugin"`
	OldVersion  string `json:"old_version"`
	NewVersion  string `json:"new_version"`
	PluginJSON  string `json:"plugin_json"`
	Marketplace bool   `json:"marketplace_updated"`
	CargoToml   bool   `json:"cargo_toml_updated"`
	CargoLock   bool   `json:"cargo_lock_updated"`
}

// Bumper handles version bumping operations
type Bumper struct {
	RepoRoot        string
	MarketplacePath string
	DryRun          bool
}

// NewBumper creates a new Bumper instance
func NewBumper(repoRoot string, dryRun bool) *Bumper {
	return &Bumper{
		RepoRoot:        repoRoot,
		MarketplacePath: filepath.Join(repoRoot, ".claude-plugin", "marketplace.json"),
		DryRun:          dryRun,
	}
}

// BumpPlugins bumps versions for the given plugins
func (b *Bumper) BumpPlugins(plugins []changes.Package, bumpType BumpType) ([]BumpResult, error) {
	var results []BumpResult

	// Load marketplace.json
	marketplace, err := b.loadMarketplace()
	if err != nil {
		return nil, fmt.Errorf("failed to load marketplace.json: %w", err)
	}

	for _, pkg := range plugins {
		if pkg.Type != "plugin" {
			continue
		}

		result, err := b.bumpPlugin(pkg, bumpType, marketplace)
		if err != nil {
			return nil, fmt.Errorf("failed to bump %s: %w", pkg.Name, err)
		}

		results = append(results, result)
	}

	// Save marketplace.json if any plugins were bumped
	if len(results) > 0 && !b.DryRun {
		if err := b.saveMarketplace(marketplace); err != nil {
			return nil, fmt.Errorf("failed to save marketplace.json: %w", err)
		}
	}

	return results, nil
}

func (b *Bumper) bumpPlugin(pkg changes.Package, bumpType BumpType, marketplace map[string]interface{}) (BumpResult, error) {
	result := BumpResult{
		Plugin: pkg.Name,
	}

	// Find plugin.json path
	pluginJSONPath := filepath.Join(b.RepoRoot, pkg.Path, ".claude-plugin", "plugin.json")
	result.PluginJSON = pluginJSONPath

	var currentVersion string
	var pluginData map[string]interface{}
	pluginJSONExists := false

	// Try to read plugin.json if it exists
	if _, err := os.Stat(pluginJSONPath); err == nil {
		pluginJSONExists = true
		pluginData, err = b.loadJSON(pluginJSONPath)
		if err != nil {
			return result, fmt.Errorf("failed to load plugin.json: %w", err)
		}
		currentVersion, _ = pluginData["version"].(string)
	}

	// If plugin.json doesn't exist or has no version, try marketplace.json
	if currentVersion == "" {
		currentVersion = b.getMarketplacePluginVersion(marketplace, pkg.Name)
	}

	if currentVersion == "" {
		currentVersion = "0.0.0"
	}
	result.OldVersion = currentVersion

	// Calculate new version
	newVersion, err := BumpVersion(currentVersion, bumpType)
	if err != nil {
		return result, err
	}
	result.NewVersion = newVersion

	if !b.DryRun {
		// Validate Cargo.toml first (before any writes) to avoid partial updates
		cargoTomlPath := filepath.Join(b.RepoRoot, pkg.Path, "cli", "Cargo.toml")
		cargoLockPath := filepath.Join(b.RepoRoot, pkg.Path, "cli", "Cargo.lock")
		hasCargoToml := false
		hasCargoLock := false
		crateName := ""
		if _, err := os.Stat(cargoTomlPath); err == nil {
			hasCargoToml = true
			if _, err := parseCargoPackageSection(cargoTomlPath); err != nil {
				return result, fmt.Errorf("Cargo.toml validation failed (no files modified): %w", err)
			}

			// Cargo.lock records the crate's own version, so it needs the same bump.
			// Resolving the crate name from Cargo.toml keeps the lookup exact when the
			// plugin directory and the crate are named differently.
			crateName, err = ExtractCargoPackageName(cargoTomlPath)
			if err != nil {
				return result, fmt.Errorf("Cargo.toml validation failed (no files modified): %w", err)
			}
			if _, err := os.Stat(cargoLockPath); err == nil {
				hasCargoLock = true
				if _, err := parseCargoLockLocalPackage(cargoLockPath, crateName); err != nil {
					return result, fmt.Errorf("Cargo.lock validation failed (no files modified): %w", err)
				}
			}
		}

		// All validations passed — now perform writes
		if pluginJSONExists {
			pluginData["version"] = newVersion
			if err := b.saveJSON(pluginJSONPath, pluginData); err != nil {
				return result, fmt.Errorf("failed to save plugin.json: %w", err)
			}
		}

		// Update marketplace.json (in-memory, saved later in BumpPlugins)
		if err := b.updateMarketplacePlugin(marketplace, pkg.Name, newVersion); err != nil {
			return result, fmt.Errorf("failed to update marketplace: %w", err)
		}
		result.Marketplace = true

		// Update Cargo.toml (already validated above)
		if hasCargoToml {
			if err := b.bumpCargoToml(cargoTomlPath, newVersion); err != nil {
				return result, fmt.Errorf("Cargo.toml update failed: %w", err)
			}
			result.CargoToml = true
		}

		// Update Cargo.lock (already validated above)
		if hasCargoLock {
			if err := b.bumpCargoLock(cargoLockPath, crateName, newVersion); err != nil {
				return result, fmt.Errorf("Cargo.lock update failed: %w", err)
			}
			result.CargoLock = true
		}
	}

	return result, nil
}

// cargoPackageInfo holds the parsed [package] section of a Cargo.toml file.
type cargoPackageInfo struct {
	fullText     string // entire file content
	sectionStart int    // byte offset where [package] header ends
	sectionEnd   int    // byte offset where the next section starts (or EOF)
	section      string // content between [package] and next section
}

// parseCargoPackageSection reads a Cargo.toml file and extracts the [package] section boundaries.
// It validates that exactly one version field exists in the section.
func parseCargoPackageSection(path string) (*cargoPackageInfo, error) {
	content, err := os.ReadFile(path)
	if err != nil {
		return nil, err
	}

	text := string(content)

	pkgLoc := cargoPackageRe.FindStringIndex(text)
	if pkgLoc == nil {
		return nil, fmt.Errorf("no [package] section found in %s", path)
	}

	pkgEnd := len(text)
	remaining := text[pkgLoc[1]:]
	if nextSection := cargoSectionRe.FindStringIndex(remaining); nextSection != nil {
		pkgEnd = pkgLoc[1] + nextSection[0]
	}

	section := text[pkgLoc[1]:pkgEnd]

	matches := cargoVersionMatchRe.FindAllStringIndex(section, -1)
	if len(matches) == 0 {
		return nil, fmt.Errorf("no version field found in [package] section of %s", path)
	}
	if len(matches) > 1 {
		return nil, fmt.Errorf("multiple version fields found in [package] section of %s", path)
	}

	return &cargoPackageInfo{
		fullText:     text,
		sectionStart: pkgLoc[1],
		sectionEnd:   pkgEnd,
		section:      section,
	}, nil
}

// ExtractCargoPackageVersion reads a Cargo.toml and returns the version from the [package] section.
func ExtractCargoPackageVersion(path string) (string, error) {
	info, err := parseCargoPackageSection(path)
	if err != nil {
		return "", err
	}

	match := cargoVersionExtract.FindStringSubmatch(info.section)
	if match == nil {
		return "", fmt.Errorf("no version field found in [package] section of %s", path)
	}
	return match[1], nil
}

// bumpCargoToml updates the version field in the [package] section of a Cargo.toml file.
// It only replaces the version within the [package] section to avoid corrupting dependency versions.
func (b *Bumper) bumpCargoToml(path, newVersion string) error {
	info, err := parseCargoPackageSection(path)
	if err != nil {
		return err
	}

	updatedSection := cargoVersionRe.ReplaceAllString(info.section, "${1}"+newVersion+"${3}")
	updated := info.fullText[:info.sectionStart] + updatedSection + info.fullText[info.sectionEnd:]

	return os.WriteFile(path, []byte(updated), 0644)
}

// ExtractCargoPackageName reads a Cargo.toml and returns the crate name from the [package] section.
func ExtractCargoPackageName(path string) (string, error) {
	info, err := parseCargoPackageSection(path)
	if err != nil {
		return "", err
	}

	match := cargoNameExtract.FindStringSubmatch(info.section)
	if match == nil {
		return "", fmt.Errorf("no name field found in [package] section of %s", path)
	}
	return match[1], nil
}

// cargoLockPackageInfo holds the located [[package]] block of a Cargo.lock file.
type cargoLockPackageInfo struct {
	fullText   string // entire file content
	blockStart int    // byte offset where the [[package]] header ends
	blockEnd   int    // byte offset where the next [[package]] starts (or EOF)
	block      string // content of the located block
}

// parseCargoLockLocalPackage locates the [[package]] entry for crateName in a Cargo.lock file.
//
// Cargo.lock records the workspace-local crate's own version, so a bump must update it too.
// The local entry is the one WITHOUT a `source` field — registry and git entries carry a
// source (and a checksum) and must never be rewritten, even when they share the crate name.
func parseCargoLockLocalPackage(path, crateName string) (*cargoLockPackageInfo, error) {
	content, err := os.ReadFile(path)
	if err != nil {
		return nil, err
	}

	text := string(content)

	headers := cargoLockPackageRe.FindAllStringIndex(text, -1)
	if len(headers) == 0 {
		return nil, fmt.Errorf("no [[package]] entries found in %s", path)
	}

	var found []*cargoLockPackageInfo
	for i, header := range headers {
		blockEnd := len(text)
		if i+1 < len(headers) {
			blockEnd = headers[i+1][0]
		}
		block := text[header[1]:blockEnd]

		nameMatch := cargoNameExtract.FindStringSubmatch(block)
		if nameMatch == nil || nameMatch[1] != crateName {
			continue
		}
		if cargoLockSourceRe.MatchString(block) {
			continue
		}

		matches := cargoVersionMatchRe.FindAllStringIndex(block, -1)
		if len(matches) == 0 {
			return nil, fmt.Errorf("no version field found in [[package]] %q of %s", crateName, path)
		}
		if len(matches) > 1 {
			return nil, fmt.Errorf("multiple version fields found in [[package]] %q of %s", crateName, path)
		}

		found = append(found, &cargoLockPackageInfo{
			fullText:   text,
			blockStart: header[1],
			blockEnd:   blockEnd,
			block:      block,
		})
	}

	if len(found) == 0 {
		return nil, fmt.Errorf("no workspace-local [[package]] entry named %q found in %s", crateName, path)
	}
	if len(found) > 1 {
		return nil, fmt.Errorf("multiple workspace-local [[package]] entries named %q found in %s", crateName, path)
	}

	return found[0], nil
}

// bumpCargoLock updates the version of the workspace-local package entry in a Cargo.lock file.
// Only that entry's version line is rewritten, so dependency versions and checksums are left intact.
func (b *Bumper) bumpCargoLock(path, crateName, newVersion string) error {
	info, err := parseCargoLockLocalPackage(path, crateName)
	if err != nil {
		return err
	}

	updatedBlock := cargoVersionRe.ReplaceAllString(info.block, "${1}"+newVersion+"${3}")
	updated := info.fullText[:info.blockStart] + updatedBlock + info.fullText[info.blockEnd:]

	return os.WriteFile(path, []byte(updated), 0644)
}

// getMarketplacePluginVersion gets the version of a plugin from marketplace.json
func (b *Bumper) getMarketplacePluginVersion(marketplace map[string]interface{}, pluginName string) string {
	plugins, ok := marketplace["plugins"].([]interface{})
	if !ok {
		return ""
	}

	for _, p := range plugins {
		plugin, ok := p.(map[string]interface{})
		if !ok {
			continue
		}

		name, _ := plugin["name"].(string)
		if name == pluginName {
			version, _ := plugin["version"].(string)
			return version
		}
	}

	return ""
}

func (b *Bumper) loadMarketplace() (map[string]interface{}, error) {
	return b.loadJSON(b.MarketplacePath)
}

func (b *Bumper) saveMarketplace(data map[string]interface{}) error {
	return b.saveJSON(b.MarketplacePath, data)
}

func (b *Bumper) loadJSON(path string) (map[string]interface{}, error) {
	content, err := os.ReadFile(path)
	if err != nil {
		return nil, err
	}

	var data map[string]interface{}
	if err := json.Unmarshal(content, &data); err != nil {
		return nil, err
	}

	return data, nil
}

func (b *Bumper) saveJSON(path string, data map[string]interface{}) error {
	content, err := json.MarshalIndent(data, "", "  ")
	if err != nil {
		return err
	}

	// Add trailing newline
	content = append(content, '\n')

	return os.WriteFile(path, content, 0644)
}

func (b *Bumper) updateMarketplacePlugin(marketplace map[string]interface{}, pluginName, newVersion string) error {
	plugins, ok := marketplace["plugins"].([]interface{})
	if !ok {
		return fmt.Errorf("marketplace.json has no plugins array")
	}

	for i, p := range plugins {
		plugin, ok := p.(map[string]interface{})
		if !ok {
			continue
		}

		name, _ := plugin["name"].(string)
		if name == pluginName {
			plugin["version"] = newVersion
			plugins[i] = plugin
			marketplace["plugins"] = plugins
			return nil
		}
	}

	return fmt.Errorf("plugin %s not found in marketplace.json", pluginName)
}

// BumpVersion calculates a new version based on bump type
func BumpVersion(currentVersion string, bumpType BumpType) (string, error) {
	re := regexp.MustCompile(`^(\d+)\.(\d+)\.(\d+)`)
	match := re.FindStringSubmatch(currentVersion)
	if match == nil {
		return "", fmt.Errorf("invalid version format: %s", currentVersion)
	}

	major, _ := strconv.Atoi(match[1])
	minor, _ := strconv.Atoi(match[2])
	patch, _ := strconv.Atoi(match[3])

	switch bumpType {
	case BumpMajor:
		return fmt.Sprintf("%d.0.0", major+1), nil
	case BumpMinor:
		return fmt.Sprintf("%d.%d.0", major, minor+1), nil
	case BumpPatch:
		return fmt.Sprintf("%d.%d.%d", major, minor, patch+1), nil
	default:
		return "", fmt.Errorf("invalid bump type: %s", bumpType)
	}
}

// ParseBumpType converts a string to BumpType
func ParseBumpType(s string) (BumpType, error) {
	switch strings.ToLower(s) {
	case "major":
		return BumpMajor, nil
	case "minor":
		return BumpMinor, nil
	case "patch":
		return BumpPatch, nil
	default:
		return "", fmt.Errorf("invalid bump type: %s (expected: major, minor, patch)", s)
	}
}

// GetBumpTypeFromPRTitle determines bump type from conventional commit PR title
func GetBumpTypeFromPRTitle(title string) BumpType {
	title = strings.ToLower(title)

	if matched, _ := regexp.MatchString(`^major(\(.+\))?:`, title); matched {
		return BumpMajor
	}
	if matched, _ := regexp.MatchString(`^feat(\(.+\))?:`, title); matched {
		return BumpMinor
	}
	if matched, _ := regexp.MatchString(`^(fix|refactor|perf|style|docs|test|chore)(\(.+\))?:`, title); matched {
		return BumpPatch
	}

	return ""
}

// GetScopeFromPRTitle extracts the scope from a conventional commit PR title
// e.g., "feat(atelier): add branch detection" returns "atelier"
func GetScopeFromPRTitle(title string) string {
	re := regexp.MustCompile(`^\w+\(([^)]+)\):`)
	match := re.FindStringSubmatch(title)
	if len(match) >= 2 {
		return match[1]
	}
	return ""
}
