package main

import (
	"flag"
	"fmt"
	"os"
	"path/filepath"
	"strings"

	"github.com/fatih/color"
	"github.com/kys0213/kys-claude-plugin/tools/validate/internal/architecture"
	"github.com/kys0213/kys-claude-plugin/tools/validate/internal/path"
	"github.com/kys0213/kys-claude-plugin/tools/validate/internal/spec"
	"github.com/kys0213/kys-claude-plugin/tools/validate/internal/version"
)

var (
	specsOnly    = flag.Bool("specs-only", false, "Run only spec validation")
	pathsOnly    = flag.Bool("paths-only", false, "Run only path validation")
	versionsOnly = flag.Bool("versions-only", false, "Run only version validation")
	archOnly     = flag.Bool("arch-only", false, "Run only architecture validation")
	skipVersions = flag.Bool("skip-versions", false, "Skip version validation (for CI)")
	verbose      = flag.Bool("verbose", false, "Verbose output")
	shortVerbose = flag.Bool("v", false, "Verbose output (short)")
)

func main() {
	flag.Parse()

	isVerbose := *verbose || *shortVerbose
	runAll := !*specsOnly && !*pathsOnly && !*versionsOnly && !*archOnly

	// Get repo root
	repoRoot := "."
	if flag.NArg() > 0 {
		repoRoot = flag.Arg(0)
	}
	repoRoot, _ = filepath.Abs(repoRoot)

	// Print header
	bold := color.New(color.Bold)
	cyan := color.New(color.FgCyan, color.Bold)
	gray := color.New(color.FgHiBlack)
	green := color.New(color.FgGreen)
	yellow := color.New(color.FgYellow)
	red := color.New(color.FgRed)

	fmt.Println()
	bold.Println("========================================")
	bold.Println("  Claude Code Plugin Validator")
	bold.Println("========================================")
	fmt.Println()
	gray.Printf("Repository: %s\n\n", repoRoot)

	var totalPassed, totalFailed, totalWarnings int

	// 1. Spec validation
	if runAll || *specsOnly {
		cyan.Println("📋 [1/4] Validating Specs...")
		gray.Println("    Checking plugin.json, SKILL.md, agents, commands")
		fmt.Println()

		results, err := spec.Validate(repoRoot)
		if err != nil {
			red.Printf("Error: %v\n", err)
			os.Exit(1)
		}

		printResults(results.Passed, results.Failed, repoRoot, isVerbose)
		totalPassed += len(results.Passed)
		totalFailed += len(results.Failed)
		fmt.Println()
	}

	// 2. Path validation
	if runAll || *pathsOnly {
		cyan.Println("📁 [2/4] Validating Paths...")
		gray.Println("    Checking file references, script paths")
		fmt.Println()

		results, err := path.Validate(repoRoot)
		if err != nil {
			red.Printf("Error: %v\n", err)
			os.Exit(1)
		}

		printPathResults(results.Passed, results.Failed, repoRoot, isVerbose)
		totalPassed += len(results.Passed)
		totalFailed += len(results.Failed)
		fmt.Println()
	}

	// 3. Version validation (skip in CI - versions are auto-bumped on merge)
	if (runAll || *versionsOnly) && !*skipVersions {
		cyan.Println("🏷️  [3/4] Validating Versions...")
		gray.Println("    Checking semver format, consistency")
		fmt.Println()

		results, err := version.Validate(repoRoot)
		if err != nil {
			red.Printf("Error: %v\n", err)
			os.Exit(1)
		}

		printVersionResults(results.Passed, results.Failed, repoRoot, isVerbose)
		totalPassed += len(results.Passed)
		totalFailed += len(results.Failed)
		fmt.Println()
	}

	// 4. Architecture validation
	if runAll || *archOnly {
		cyan.Println("🏗️  [4/4] Validating Architecture...")
		gray.Println("    Checking layer dependencies, content similarity, responsibilities")
		fmt.Println()

		results, err := architecture.Validate(repoRoot)
		if err != nil {
			red.Printf("Error: %v\n", err)
			os.Exit(1)
		}

		printArchResults(results.Passed, results.Failed, results.Warnings, repoRoot, isVerbose)
		totalPassed += len(results.Passed)
		totalFailed += len(results.Failed)
		totalWarnings += len(results.Warnings)
		fmt.Println()
	}

	// Summary
	bold.Println("========================================")
	bold.Println("  Summary")
	bold.Println("========================================")
	green.Printf("  ✓ Passed: %d\n", totalPassed)
	if totalWarnings > 0 {
		yellow.Printf("  ⚠ Warnings: %d\n", totalWarnings)
	}
	red.Printf("  ✗ Failed: %d\n", totalFailed)
	fmt.Println()

	if totalFailed > 0 {
		red.Println("❌ Validation FAILED")
		fmt.Println()
		os.Exit(1)
	} else {
		green.Println("✅ Validation PASSED")
		fmt.Println()
		os.Exit(0)
	}
}

func printResults(passed []spec.Result, failed []spec.Result, repoRoot string, verbose bool) {
	red := color.New(color.FgRed)
	green := color.New(color.FgGreen)
	gray := color.New(color.FgHiBlack)

	// Print failures
	for _, result := range failed {
		shortPath := strings.Replace(result.File, repoRoot, ".", 1)
		red.Printf("  ✗ %s\n", shortPath)
		gray.Printf("    Type: %s\n", result.Type)
		for _, err := range result.Errors {
			red.Printf("    → %s\n", err)
		}
		fmt.Println()
	}

	// Print passes (verbose only)
	if verbose {
		for _, result := range passed {
			shortPath := strings.Replace(result.File, repoRoot, ".", 1)
			green.Printf("  ✓ %s\n", shortPath)
			gray.Printf("    Type: %s\n", result.Type)
		}
	} else if len(passed) > 0 {
		green.Printf("  ✓ %d checks passed\n", len(passed))
	}
}

func printPathResults(passed []path.Result, failed []path.Result, repoRoot string, verbose bool) {
	red := color.New(color.FgRed)
	green := color.New(color.FgGreen)
	gray := color.New(color.FgHiBlack)

	// Print failures
	for _, result := range failed {
		shortPath := strings.Replace(result.File, repoRoot, ".", 1)
		red.Printf("  ✗ %s\n", shortPath)
		gray.Printf("    Type: %s\n", result.Type)
		if result.Error != "" {
			red.Printf("    → %s\n", result.Error)
		}
		if result.ReferencedPath != "" {
			gray.Printf("    Referenced: %s\n", result.ReferencedPath)
		}
		if result.ResolvedPath != "" && verbose {
			gray.Printf("    Resolved: %s\n", result.ResolvedPath)
		}
		fmt.Println()
	}

	// Print passes (verbose only)
	if verbose {
		for _, result := range passed {
			shortPath := strings.Replace(result.File, repoRoot, ".", 1)
			green.Printf("  ✓ %s\n", shortPath)
			gray.Printf("    Type: %s\n", result.Type)
			if result.ReferencedPath != "" {
				gray.Printf("    Referenced: %s\n", result.ReferencedPath)
			}
		}
	} else if len(passed) > 0 {
		green.Printf("  ✓ %d checks passed\n", len(passed))
	}
}

func printArchResults(passed []architecture.Result, failed []architecture.Result, warnings []architecture.Result, repoRoot string, verbose bool) {
	red := color.New(color.FgRed)
	yellow := color.New(color.FgYellow)
	green := color.New(color.FgGreen)
	gray := color.New(color.FgHiBlack)

	// Print errors (hard failures)
	for _, result := range failed {
		shortPath := strings.Replace(result.File, repoRoot, ".", 1)
		red.Printf("  ✗ %s\n", shortPath)
		gray.Printf("    Type: %s\n", result.Type)
		for _, err := range result.Errors {
			red.Printf("    → %s\n", err)
		}
		fmt.Println()
	}

	// Print warnings (non-blocking)
	for _, result := range warnings {
		shortPath := strings.Replace(result.File, repoRoot, ".", 1)
		yellow.Printf("  ⚠ %s\n", shortPath)
		gray.Printf("    Type: %s\n", result.Type)
		for _, err := range result.Errors {
			yellow.Printf("    → %s\n", err)
		}
		fmt.Println()
	}

	// Print passes (verbose only)
	if verbose {
		for _, result := range passed {
			shortPath := strings.Replace(result.File, repoRoot, ".", 1)
			green.Printf("  ✓ %s\n", shortPath)
			gray.Printf("    Type: %s\n", result.Type)
		}
	} else if len(passed) > 0 {
		green.Printf("  ✓ %d checks passed\n", len(passed))
	}
}

func printVersionResults(passed []version.Result, failed []version.Result, repoRoot string, verbose bool) {
	red := color.New(color.FgRed)
	green := color.New(color.FgGreen)
	gray := color.New(color.FgHiBlack)

	// Print failures
	for _, result := range failed {
		shortPath := strings.Replace(result.File, repoRoot, ".", 1)
		red.Printf("  ✗ %s\n", shortPath)
		gray.Printf("    Type: %s\n", result.Type)
		if result.Plugin != "" {
			gray.Printf("    Plugin: %s\n", result.Plugin)
		}
		for _, err := range result.Errors {
			red.Printf("    → %s\n", err)
		}
		if result.Error != "" {
			red.Printf("    → %s\n", result.Error)
		}
		fmt.Println()
	}

	// Print passes (verbose only)
	if verbose {
		for _, result := range passed {
			shortPath := strings.Replace(result.File, repoRoot, ".", 1)
			green.Printf("  ✓ %s\n", shortPath)
			gray.Printf("    Type: %s\n", result.Type)
		}
	} else if len(passed) > 0 {
		green.Printf("  ✓ %d checks passed\n", len(passed))
	}
}
