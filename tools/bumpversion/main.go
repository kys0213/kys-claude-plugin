package main

import (
	"encoding/json"
	"flag"
	"fmt"
	"os"
	"path/filepath"
	"strings"

	"github.com/fatih/color"
	"github.com/kys0213/kys-claude-plugin/tools/bumpversion/internal/bumper"
	"github.com/kys0213/kys-claude-plugin/tools/bumpversion/internal/changes"
)

var (
	baseRef    = flag.String("base", "main", "Base ref to compare against")
	bumpType   = flag.String("type", "", "Bump type: major, minor, patch (or auto-detect from PR title)")
	prTitle    = flag.String("pr-title", "", "PR title for auto-detecting bump type")
	dryRun     = flag.Bool("dry-run", false, "Show what would be changed without making changes")
	jsonOutput = flag.Bool("json", false, "Output results as JSON")
	detectOnly = flag.Bool("detect-only", false, "Only detect changed packages, don't bump versions")
	plugins    = flag.String("plugins", "", "Comma-separated list of plugins to bump (overrides auto-detection)")
	verbose    = flag.Bool("verbose", false, "Verbose output")
	shortV     = flag.Bool("v", false, "Verbose output (short)")
)

func main() {
	flag.Parse()

	isVerbose := *verbose || *shortV

	// Get repo root
	repoRoot := "."
	if flag.NArg() > 0 {
		repoRoot = flag.Arg(0)
	}
	repoRoot, _ = filepath.Abs(repoRoot)

	// Colors
	bold := color.New(color.Bold)
	cyan := color.New(color.FgCyan, color.Bold)
	gray := color.New(color.FgHiBlack)
	green := color.New(color.FgGreen)
	red := color.New(color.FgRed)
	yellow := color.New(color.FgYellow)

	if !*jsonOutput {
		fmt.Println()
		bold.Println("========================================")
		bold.Println("  Version Bump Tool")
		bold.Println("========================================")
		fmt.Println()
		gray.Printf("Repository: %s\n", repoRoot)
		gray.Printf("Base ref: %s\n", *baseRef)
		fmt.Println()
	}

	// Detect or parse plugins to bump
	var pkgsToProcess []changes.Package

	if *plugins != "" {
		// Use explicitly provided plugins
		for _, name := range strings.Split(*plugins, ",") {
			name = strings.TrimSpace(name)
			if name != "" {
				pkgsToProcess = append(pkgsToProcess, changes.Package{
					Name: name,
					Path: "plugins/" + name,
					Type: "plugin",
				})
			}
		}
	} else {
		// Auto-detect changed packages
		if !*jsonOutput {
			cyan.Println("Detecting changed packages...")
		}

		detected, err := changes.DetectChanges(repoRoot, *baseRef)
		if err != nil {
			if *jsonOutput {
				outputJSON(map[string]interface{}{"error": err.Error()})
			} else {
				red.Printf("Error detecting changes: %v\n", err)
			}
			os.Exit(1)
		}

		pkgsToProcess = changes.GetPluginsOnly(detected)
	}

	if len(pkgsToProcess) == 0 {
		if *jsonOutput {
			outputJSON(map[string]interface{}{
				"message":  "No plugins to bump",
				"packages": []string{},
			})
		} else {
			yellow.Println("No plugins to bump")
		}
		os.Exit(0)
	}

	if !*jsonOutput && isVerbose {
		gray.Println("Changed packages:")
		for _, pkg := range pkgsToProcess {
			gray.Printf("  - %s (%s)\n", pkg.Name, pkg.Path)
		}
		fmt.Println()
	}

	// Detect-only mode
	if *detectOnly {
		if *jsonOutput {
			var names []string
			for _, pkg := range pkgsToProcess {
				names = append(names, pkg.Name)
			}
			outputJSON(map[string]interface{}{
				"packages": names,
			})
		} else {
			green.Println("Changed plugins:")
			for _, pkg := range pkgsToProcess {
				fmt.Printf("  %s\n", pkg.Name)
			}
		}
		os.Exit(0)
	}

	// Determine bump type
	bt := bumper.BumpType(*bumpType)
	if bt == "" && *prTitle != "" {
		bt = bumper.GetBumpTypeFromPRTitle(*prTitle)
		if bt != "" && !*jsonOutput {
			gray.Printf("Auto-detected bump type from PR title: %s\n", bt)
		}
	}

	if bt == "" {
		if *jsonOutput {
			outputJSON(map[string]interface{}{"error": "bump type required (use --type or --pr-title)"})
		} else {
			red.Println("Error: bump type required")
			red.Println("Use --type=major|minor|patch or --pr-title='feat: ...'")
		}
		os.Exit(1)
	}

	if !*jsonOutput {
		cyan.Printf("Bumping versions (%s)...\n", bt)
		if *dryRun {
			yellow.Println("[DRY RUN - no changes will be made]")
		}
		fmt.Println()
	}

	// Perform bump
	b := bumper.NewBumper(repoRoot, *dryRun)
	results, err := b.BumpPlugins(pkgsToProcess, bt)
	if err != nil {
		if *jsonOutput {
			outputJSON(map[string]interface{}{"error": err.Error()})
		} else {
			red.Printf("Error: %v\n", err)
		}
		os.Exit(1)
	}

	// Output results
	if *jsonOutput {
		outputJSON(map[string]interface{}{
			"dry_run": *dryRun,
			"results": results,
		})
	} else {
		for _, r := range results {
			green.Printf("  ✓ %s: %s → %s\n", r.Plugin, r.OldVersion, r.NewVersion)
			if isVerbose {
				gray.Printf("    plugin.json: %s\n", r.PluginJSON)
				if r.Marketplace {
					gray.Println("    marketplace.json: updated")
				}
			}
		}
		fmt.Println()

		if *dryRun {
			yellow.Println("Dry run complete - no files were modified")
		} else {
			green.Printf("Successfully bumped %d plugin(s)\n", len(results))
		}
		fmt.Println()
	}
}

func outputJSON(data interface{}) {
	enc := json.NewEncoder(os.Stdout)
	enc.SetIndent("", "  ")
	enc.Encode(data)
}
