package runner

import (
	"bufio"
	"context"
	"encoding/json"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"sync/atomic"
	"time"

	"github.com/kys0213/kys-claude-plugin/tools/skilleval/internal/evalset"
	"github.com/kys0213/kys-claude-plugin/tools/skilleval/internal/skill"
)

// stageSeq keeps registered ids distinct across concurrent runs.
var stageSeq uint64

// ClaudeRunner measures real triggering by registering the skills under test
// as commands in a throwaway project root and sending one prompt to `claude`.
//
// Each run gets its own root. Sharing one would let concurrent runs see each
// other's registered skills, quietly turning a solo measurement into a
// competitive one.
type ClaudeRunner struct {
	Timeout time.Duration
	Model   string
	Keep    bool // leave staged roots on disk for inspection
}

// NewClaudeRunner creates a ClaudeRunner with a usable default timeout.
func NewClaudeRunner(timeout time.Duration, model string, keep bool) *ClaudeRunner {
	if timeout <= 0 {
		timeout = 3 * time.Minute
	}
	return &ClaudeRunner{Timeout: timeout, Model: model, Keep: keep}
}

type streamEvent struct {
	Type    string `json:"type"`
	Message struct {
		Content []struct {
			Type  string          `json:"type"`
			Name  string          `json:"name"`
			Input json.RawMessage `json:"input"`
		} `json:"content"`
	} `json:"message"`
}

// Run stages a project root, sends the prompt, and reports which skills fired.
func (cr *ClaudeRunner) Run(c evalset.Case, skills []skill.Skill) Result {
	root, ids, err := stage(c, skills)
	if err != nil {
		return Result{Status: StatusError}
	}
	if !cr.Keep {
		defer os.RemoveAll(root)
	}

	ctx, cancel := context.WithTimeout(context.Background(), cr.Timeout)
	defer cancel()

	args := []string{"-p", c.Query, "--output-format", "stream-json", "--verbose"}
	if cr.Model != "" {
		args = append(args, "--model", cr.Model)
	}
	cmd := exec.CommandContext(ctx, "claude", args...)
	cmd.Dir = root
	// CLAUDECODE marks an interactive session and blocks nesting; a measured
	// subprocess is not the case that guard is for.
	cmd.Env = filterEnv(os.Environ(), "CLAUDECODE")

	stdout, err := cmd.StdoutPipe()
	if err != nil {
		return Result{Status: StatusError}
	}
	if err := cmd.Start(); err != nil {
		return Result{Status: StatusError}
	}

	res := Result{Status: StatusOK}
	seen := map[string]bool{}
	sc := bufio.NewScanner(stdout)
	sc.Buffer(make([]byte, 0, 64*1024), 8*1024*1024)
	for sc.Scan() {
		var ev streamEvent
		if err := json.Unmarshal(sc.Bytes(), &ev); err != nil {
			continue
		}
		if ev.Type != "assistant" {
			continue
		}
		for _, item := range ev.Message.Content {
			if item.Type != "tool_use" {
				continue
			}
			res.Tools = append(res.Tools, item.Name)
			// Match the registered id anywhere in the tool input: the skill may
			// be reached as a command or as a direct file read, and either way
			// the description did its job.
			blob := string(item.Input)
			for id, name := range ids {
				if !seen[name] && strings.Contains(blob, id) {
					seen[name] = true
					res.Fired = append(res.Fired, name)
				}
			}
		}
	}
	_ = cmd.Wait()
	if ctx.Err() == context.DeadlineExceeded {
		res.Status = StatusTimeout
	}
	return res
}

// stage builds the throwaway project root and returns registered-id -> name.
func stage(c evalset.Case, skills []skill.Skill) (string, map[string]string, error) {
	root, err := os.MkdirTemp("", "skilleval-")
	if err != nil {
		return "", nil, err
	}
	cmds := filepath.Join(root, ".claude", "commands")
	if err := os.MkdirAll(cmds, 0o755); err != nil {
		return "", nil, err
	}

	ids := make(map[string]string, len(skills))
	for _, s := range skills {
		id := fmt.Sprintf("%s-skill-%d", s.Name, atomic.AddUint64(&stageSeq, 1))
		ids[id] = s.Name
		indented := strings.ReplaceAll(s.Desc, "\n", "\n  ")
		body := fmt.Sprintf("---\ndescription: |\n  %s\n---\n\n# %s\n\nThis skill handles: %s\n",
			indented, s.Name, s.Desc)
		if err := os.WriteFile(filepath.Join(cmds, id+".md"), []byte(body), 0o644); err != nil {
			return "", nil, err
		}
	}

	for rel, content := range c.Files {
		p := filepath.Join(root, filepath.Clean(rel))
		if err := os.MkdirAll(filepath.Dir(p), 0o755); err != nil {
			return "", nil, err
		}
		if err := os.WriteFile(p, []byte(content), 0o644); err != nil {
			return "", nil, err
		}
	}
	return root, ids, nil
}

func filterEnv(env []string, drop string) []string {
	out := make([]string, 0, len(env))
	for _, kv := range env {
		if strings.HasPrefix(kv, drop+"=") {
			continue
		}
		out = append(out, kv)
	}
	return out
}
