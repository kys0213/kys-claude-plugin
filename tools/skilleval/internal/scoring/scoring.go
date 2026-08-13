// Package scoring turns run outcomes into a report.
package scoring

import (
	"sync"

	"github.com/kys0213/kys-claude-plugin/tools/skilleval/internal/evalset"
	"github.com/kys0213/kys-claude-plugin/tools/skilleval/internal/runner"
	"github.com/kys0213/kys-claude-plugin/tools/skilleval/internal/skill"
)

// QueryScore is the outcome of every run of one case.
type QueryScore struct {
	Query      string         `json:"query"`
	Expect     bool           `json:"expect"`
	Runs       int            `json:"runs"`
	TargetHits int            `json:"target_hits"`
	LedByOther map[string]int `json:"led_by_other,omitempty"`
	WithOthers int            `json:"with_others"`
	NoFire     int            `json:"no_fire"`
	Timeouts   int            `json:"timeouts"`
}

// Correct counts runs that came out the way the case says they should. For a
// negative case that is the runs where the target stayed silent, so a
// description that fires on everything cannot score well by firing a lot.
func (q QueryScore) Correct() int {
	if q.Expect {
		return q.TargetHits
	}
	return q.Runs - q.TargetHits
}

// Report is the whole measurement.
type Report struct {
	Target       string       `json:"target"`
	RunsPerQuery int          `json:"runs_per_query"`
	Scores       []QueryScore `json:"scores"`
}

func (r Report) totals(expect bool) (correct, total int) {
	for _, s := range r.Scores {
		if s.Expect != expect {
			continue
		}
		correct += s.Correct()
		total += s.Runs
	}
	return correct, total
}

// PositiveTotals counts runs of cases the target is supposed to handle.
func (r Report) PositiveTotals() (int, int) { return r.totals(true) }

// NegativeTotals counts runs of cases the target is supposed to stay out of.
func (r Report) NegativeTotals() (int, int) { return r.totals(false) }

// Scorer executes an eval set and scores the outcome.
type Scorer struct {
	Target  string
	Runs    int
	Workers int
}

// NewScorer creates a Scorer, clamping the counts to usable minimums.
func NewScorer(target string, runs, workers int) *Scorer {
	if runs < 1 {
		runs = 1
	}
	if workers < 1 {
		workers = 1
	}
	return &Scorer{Target: target, Runs: runs, Workers: workers}
}

// Score runs every case Runs times through r. Cases keep their eval-set order
// in the report regardless of the order results arrive in.
func (s *Scorer) Score(cases []evalset.Case, skills []skill.Skill, r runner.Runner) Report {
	results := make([][]runner.Result, len(cases))
	for i := range results {
		results[i] = make([]runner.Result, 0, s.Runs)
	}

	jobs := make(chan int)
	var mu sync.Mutex
	var wg sync.WaitGroup

	for w := 0; w < s.Workers; w++ {
		wg.Add(1)
		go func() {
			defer wg.Done()
			for idx := range jobs {
				res := r.Run(cases[idx], skills)
				mu.Lock()
				results[idx] = append(results[idx], res)
				mu.Unlock()
			}
		}()
	}
	for i := range cases {
		for n := 0; n < s.Runs; n++ {
			jobs <- i
		}
	}
	close(jobs)
	wg.Wait()

	rep := Report{Target: s.Target, RunsPerQuery: s.Runs, Scores: make([]QueryScore, len(cases))}
	for i, c := range cases {
		rep.Scores[i] = s.tally(c, results[i])
	}
	return rep
}

func (s *Scorer) tally(c evalset.Case, results []runner.Result) QueryScore {
	q := QueryScore{Query: c.Query, Expect: c.Expect, Runs: len(results)}
	for _, res := range results {
		if res.Status == runner.StatusTimeout {
			q.Timeouts++
		}
		if res.DidFire(s.Target) {
			q.TargetHits++
			if len(res.Fired) > 1 {
				q.WithOthers++
			}
		}
		led := res.Led()
		switch {
		case led == "":
			// A timed-out run is not evidence the skill would have stayed
			// silent, so it must not be counted as a no-fire.
			if res.Status != runner.StatusTimeout {
				q.NoFire++
			}
		case led != s.Target:
			if q.LedByOther == nil {
				q.LedByOther = map[string]int{}
			}
			q.LedByOther[led]++
		}
	}
	return q
}
