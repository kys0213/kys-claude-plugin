// Package evalset loads the scenarios a measurement runs against.
package evalset

import (
	"encoding/json"
	"fmt"
	"os"
)

// Case is one scenario: a prompt, whether the target skill should fire, and
// the project files that must exist before the prompt is sent.
//
// Files matter more than they look. A skill that works on material already in
// the session (write this up for someone else, explain this simply) cannot
// fire from a cold prompt with nothing to work on — the agent just asks what
// it should be working on and stops. Seeding context is what makes such a
// skill measurable at all.
type Case struct {
	Query  string            `json:"query"`
	Expect bool              `json:"expect"`
	Files  map[string]string `json:"files,omitempty"`
}

// Loader reads eval sets from disk.
type Loader struct{}

// NewLoader creates a Loader.
func NewLoader() *Loader { return &Loader{} }

// Load reads and validates an eval set. An empty or malformed set is an error
// rather than a measurement of nothing.
func (l *Loader) Load(path string) ([]Case, error) {
	b, err := os.ReadFile(path)
	if err != nil {
		return nil, err
	}
	var cases []Case
	if err := json.Unmarshal(b, &cases); err != nil {
		return nil, fmt.Errorf("%s: %w", path, err)
	}
	if len(cases) == 0 {
		return nil, fmt.Errorf("%s: no cases", path)
	}
	for i, c := range cases {
		if c.Query == "" {
			return nil, fmt.Errorf("%s: case %d has no query", path, i)
		}
	}
	return cases, nil
}
