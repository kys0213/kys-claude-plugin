---
description: Build suggest-workflow Rust CLI
---

# Setup Suggest Workflow CLI

This command builds the Rust CLI binary for suggest-workflow.

## Prerequisites

- Rust toolchain (rustc, cargo)
- lindera Korean dictionary support

## Steps

1. Check if Rust is installed
2. Install Rust if needed
3. Build release binary
4. Report binary location

## Execution

```bash
cd ${CLAUDE_PLUGIN_ROOT}/cli

# Check Rust installation
if ! command -v cargo &> /dev/null; then
  echo "Installing Rust..."
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
  source $HOME/.cargo/env
fi

echo "Building release binary..."
cargo build --release

echo ""
echo "Build complete!"
echo "Binary location: $(pwd)/target/release/suggest-workflow"
echo ""
echo "To use globally, add to PATH or create symlink:"
echo "  ln -s $(pwd)/target/release/suggest-workflow /usr/local/bin/suggest-workflow"
```

## Verification

After building, test the binary:

```bash
./target/release/suggest-workflow --help
```

## Output

The compiled binary will be at:
- `${CLAUDE_PLUGIN_ROOT}/cli/target/release/suggest-workflow`

Binary size: ~5-10MB (optimized with LTO and strip)
