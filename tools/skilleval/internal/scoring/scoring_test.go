package scoring

import (
	"testing"

	"github.com/kys0213/kys-claude-plugin/tools/skilleval/internal/evalset"
	"github.com/kys0213/kys-claude-plugin/tools/skilleval/internal/runner"
	"github.com/kys0213/kys-claude-plugin/tools/skilleval/internal/skill"
)

// scriptedRunner replays fixed results per query, so scoring can be checked
// against known transcripts without spawning an agent.
type scriptedRunner struct {
	byQuery map[string][]runner.Result
	calls   map[string]int
}

func newScripted(m map[string][]runner.Result) *scriptedRunner {
	return &scriptedRunner{byQuery: m, calls: map[string]int{}}
}

func (s *scriptedRunner) Run(c evalset.Case, _ []skill.Skill) runner.Result {
	seq := s.byQuery[c.Query]
	if len(seq) == 0 {
		return runner.Result{Status: runner.StatusOK}
	}
	i := s.calls[c.Query] % len(seq)
	s.calls[c.Query]++
	return seq[i]
}

func fired(names ...string) runner.Result {
	return runner.Result{Fired: names, Status: runner.StatusOK}
}

func TestTargetCountedWhenAnotherSkillLeads(t *testing.T) {
	// The case that produced a wrong conclusion when only the first skill was
	// recorded: git leads on a PR-description prompt and communicate follows.
	// Both are used; neither lost.
	cases := []evalset.Case{{Query: "PR 설명 써줘", Expect: true}}
	r := newScripted(map[string][]runner.Result{
		"PR 설명 써줘": {fired("git", "communicate")},
	})

	rep := NewScorer("communicate", 4, 1).Score(cases, nil, r)

	got := rep.Scores[0]
	if got.TargetHits != 4 {
		t.Errorf("target fired in every run, want TargetHits=4, got %d", got.TargetHits)
	}
	if got.WithOthers != 4 {
		t.Errorf("every run also used another skill, want WithOthers=4, got %d", got.WithOthers)
	}
	if got.LedByOther["git"] != 4 {
		t.Errorf("git led every run, want LedByOther[git]=4, got %d", got.LedByOther["git"])
	}
	if got.Correct() != 4 {
		t.Errorf("positive case fully hit, want Correct=4, got %d", got.Correct())
	}
}

func TestNegativeCaseScoredInverted(t *testing.T) {
	cases := []evalset.Case{{Query: "테스트 추가해줘", Expect: false}}
	r := newScripted(map[string][]runner.Result{
		"테스트 추가해줘": {fired(), fired(), fired("communicate"), fired()},
	})

	rep := NewScorer("communicate", 4, 1).Score(cases, nil, r)

	got := rep.Scores[0]
	if got.TargetHits != 1 {
		t.Fatalf("want TargetHits=1, got %d", got.TargetHits)
	}
	if got.Correct() != 3 {
		t.Errorf("negative case is correct when target stays silent, want 3, got %d", got.Correct())
	}
	if got.NoFire != 3 {
		t.Errorf("want NoFire=3, got %d", got.NoFire)
	}
}

func TestTimeoutIsNotANoFire(t *testing.T) {
	cases := []evalset.Case{{Query: "릴리스 노트", Expect: true}}
	r := newScripted(map[string][]runner.Result{
		"릴리스 노트": {
			fired("communicate"),
			{Fired: nil, Status: runner.StatusTimeout},
		},
	})

	rep := NewScorer("communicate", 2, 1).Score(cases, nil, r)

	got := rep.Scores[0]
	if got.Timeouts != 1 {
		t.Errorf("want Timeouts=1, got %d", got.Timeouts)
	}
	if got.NoFire != 0 {
		t.Errorf("a timed-out run is not a no-fire, want NoFire=0, got %d", got.NoFire)
	}
}

func TestTotalsSplitPositiveAndNegative(t *testing.T) {
	cases := []evalset.Case{
		{Query: "슬랙 정리", Expect: true},
		{Query: "버그 고쳐줘", Expect: false},
	}
	r := newScripted(map[string][]runner.Result{
		"슬랙 정리":  {fired("communicate"), fired()},
		"버그 고쳐줘": {fired(), fired("communicate")},
	})

	rep := NewScorer("communicate", 2, 2).Score(cases, nil, r)

	ph, pt := rep.PositiveTotals()
	if ph != 1 || pt != 2 {
		t.Errorf("want positives 1/2, got %d/%d", ph, pt)
	}
	nh, nt := rep.NegativeTotals()
	if nh != 1 || nt != 2 {
		t.Errorf("want negatives 1/2, got %d/%d", nh, nt)
	}
}

func TestScoresKeepCaseOrder(t *testing.T) {
	cases := []evalset.Case{
		{Query: "a", Expect: true},
		{Query: "b", Expect: true},
		{Query: "c", Expect: false},
	}

	rep := NewScorer("x", 1, 3).Score(cases, nil, newScripted(nil))

	if len(rep.Scores) != 3 {
		t.Fatalf("want 3 scores, got %d", len(rep.Scores))
	}
	for i, want := range []string{"a", "b", "c"} {
		if rep.Scores[i].Query != want {
			t.Errorf("scores must follow eval-set order; index %d = %q, want %q",
				i, rep.Scores[i].Query, want)
		}
	}
}
