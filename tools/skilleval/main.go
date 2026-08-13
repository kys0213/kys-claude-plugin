// Command skilleval measures how reliably a skill's description makes the
// skill fire, and which skill leads when several descriptions overlap.
//
// It measures exactly what its arguments name and reports what it observed.
// Choosing which phrasings to test, what counts as good enough, and what to
// change afterwards stays outside the tool.
package main

import (
	"encoding/json"
	"flag"
	"fmt"
	"os"
	"sort"
	"strings"
	"time"

	"github.com/kys0213/kys-claude-plugin/tools/skilleval/internal/evalset"
	"github.com/kys0213/kys-claude-plugin/tools/skilleval/internal/runner"
	"github.com/kys0213/kys-claude-plugin/tools/skilleval/internal/scoring"
	"github.com/kys0213/kys-claude-plugin/tools/skilleval/internal/skill"
)

type skillList []string

func (s *skillList) String() string     { return strings.Join(*s, ",") }
func (s *skillList) Set(v string) error { *s = append(*s, v); return nil }

func main() {
	var skills skillList
	var (
		evalSet  = flag.String("eval-set", "", "Path to eval set JSON")
		runs     = flag.Int("runs", 5, "Runs per case")
		workers  = flag.Int("workers", 4, "Concurrent runs")
		timeout  = flag.Duration("timeout", 3*time.Minute, "Timeout per run")
		model    = flag.String("model", "", "Model for claude -p (default: configured)")
		asJSON   = flag.Bool("json", false, "Emit the report as JSON")
		keepDirs = flag.Bool("keep-dirs", false, "Keep staged project roots for inspection")
	)
	flag.Var(&skills, "skill", "SKILL.md to register, repeatable. First is the target. Form: [name=]path")
	flag.Parse()

	if *evalSet == "" || len(skills) == 0 {
		fmt.Fprintln(os.Stderr, "usage: skilleval --eval-set <json> --skill [name=]<SKILL.md> [--skill ...]")
		flag.PrintDefaults()
		os.Exit(2)
	}

	loader := skill.NewLoader()
	loaded := make([]skill.Skill, 0, len(skills))
	for _, spec := range skills {
		s, err := loader.LoadSpec(spec)
		if err != nil {
			fail(err)
		}
		loaded = append(loaded, s)
	}

	cases, err := evalset.NewLoader().Load(*evalSet)
	if err != nil {
		fail(err)
	}

	rep := scoring.NewScorer(loaded[0].Name, *runs, *workers).
		Score(cases, loaded, runner.NewClaudeRunner(*timeout, *model, *keepDirs))

	if *asJSON {
		enc := json.NewEncoder(os.Stdout)
		enc.SetIndent("", "  ")
		if err := enc.Encode(rep); err != nil {
			fail(err)
		}
		return
	}
	printReport(rep, loaded)
}

func fail(err error) {
	fmt.Fprintf(os.Stderr, "skilleval: %v\n", err)
	os.Exit(1)
}

func printReport(rep scoring.Report, skills []skill.Skill) {
	mode := "solo"
	if len(skills) > 1 {
		others := make([]string, 0, len(skills)-1)
		for _, s := range skills[1:] {
			others = append(others, s.Name)
		}
		mode = "vs " + strings.Join(others, ", ")
	}
	fmt.Printf("target=%s  %s  runs=%d\n\n", rep.Target, mode, rep.RunsPerQuery)

	for _, s := range rep.Scores {
		kind := "+"
		if !s.Expect {
			kind = "-"
		}
		fmt.Printf("  %s %2d/%-2d  %-46s%s\n", kind, s.Correct(), s.Runs, truncate(s.Query, 44), notes(s))
	}

	ph, pt := rep.PositiveTotals()
	nh, nt := rep.NegativeTotals()
	fmt.Println()
	if pt > 0 {
		fmt.Printf("  should fire      %d/%d  (%.0f%%)\n", ph, pt, pct(ph, pt))
	}
	if nt > 0 {
		fmt.Printf("  should not fire  %d/%d  (%.0f%%)\n", nh, nt, pct(nh, nt))
	}
}

func notes(s scoring.QueryScore) string {
	var parts []string
	if len(s.LedByOther) > 0 {
		keys := make([]string, 0, len(s.LedByOther))
		for k := range s.LedByOther {
			keys = append(keys, k)
		}
		sort.Strings(keys)
		led := make([]string, 0, len(keys))
		for _, k := range keys {
			led = append(led, fmt.Sprintf("%s:%d", k, s.LedByOther[k]))
		}
		parts = append(parts, "led "+strings.Join(led, " "))
	}
	if s.WithOthers > 0 {
		parts = append(parts, fmt.Sprintf("with-others %d", s.WithOthers))
	}
	if s.NoFire > 0 {
		parts = append(parts, fmt.Sprintf("no-fire %d", s.NoFire))
	}
	if s.Timeouts > 0 {
		parts = append(parts, fmt.Sprintf("timeout %d", s.Timeouts))
	}
	if len(parts) == 0 {
		return ""
	}
	return "  " + strings.Join(parts, "  ")
}

func truncate(s string, n int) string {
	r := []rune(s)
	if len(r) <= n {
		return s
	}
	return string(r[:n-1]) + "…"
}

func pct(a, b int) float64 {
	if b == 0 {
		return 0
	}
	return float64(a) / float64(b) * 100
}
