---
description: Build suggest-workflow Rust CLI
---

# Setup Suggest Workflow CLI

This command sets up the Rust CLI binary for suggest-workflow.
On macOS Apple Silicon, it downloads a pre-built binary from GitHub Releases.
Otherwise, it builds from source.

## Execution

```bash
bash ${CLAUDE_PLUGIN_ROOT}/scripts/ensure-binary.sh
${CLAUDE_PLUGIN_ROOT}/cli/target/release/suggest-workflow --help
```
