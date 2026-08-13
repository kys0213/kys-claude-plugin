// Package runner performs one measured agent invocation per run.
package runner

import (
	"github.com/kys0213/kys-claude-plugin/tools/skilleval/internal/evalset"
	"github.com/kys0213/kys-claude-plugin/tools/skilleval/internal/skill"
)

// Status distinguishes a real no-fire from a run that never got to answer.
type Status string

const (
	StatusOK      Status = "ok"
	StatusTimeout Status = "timeout"
	StatusError   Status = "error"
)

// Result records every skill that fired, in order.
//
// Recording only the first one conflates "another skill won" with "another
// skill led and ours followed" — opposite conclusions drawn from identical
// transcripts.
type Result struct {
	Fired  []string `json:"fired"`
	Tools  []string `json:"tools,omitempty"`
	Status Status   `json:"status"`
}

// Led reports the skill that fired first, or "" when none did.
func (r Result) Led() string {
	if len(r.Fired) == 0 {
		return ""
	}
	return r.Fired[0]
}

// DidFire reports whether name fired at any point in the run.
func (r Result) DidFire(name string) bool {
	for _, f := range r.Fired {
		if f == name {
			return true
		}
	}
	return false
}

// Runner is the seam between deterministic scoring and the non-deterministic
// agent it measures, so scoring can be tested without spawning anything.
type Runner interface {
	Run(c evalset.Case, skills []skill.Skill) Result
}
