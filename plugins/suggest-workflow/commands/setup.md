---
description: Build suggest-workflow Rust CLI
---

# Setup Suggest Workflow CLI

This command sets up the Rust CLI binary for suggest-workflow.
On macOS Apple Silicon, it downloads a pre-built binary from GitHub Releases.
Otherwise, it builds from source.

## Execution

```bash
cd ${CLAUDE_PLUGIN_ROOT}/cli

REPO="kys0213/kys-claude-plugin"
BINARY_NAME="suggest-workflow"
TARGET_DIR="$(pwd)/target/release"
mkdir -p "$TARGET_DIR"

DOWNLOADED=false

# macOS Apple Silicon: try pre-built binary
if [ "$(uname -s)" = "Darwin" ] && [ "$(uname -m)" = "arm64" ]; then
  ASSET="suggest-workflow-darwin-aarch64.tar.gz"
  LATEST_TAG=$(gh release list --repo "$REPO" --limit 1 --json tagName -q '.[0].tagName' 2>/dev/null || echo "")

  if [ -n "$LATEST_TAG" ]; then
    echo "Downloading pre-built binary ($LATEST_TAG)..."
    if gh release download "$LATEST_TAG" --repo "$REPO" --pattern "$ASSET" --dir /tmp 2>/dev/null; then
      tar xzf "/tmp/$ASSET" -C "$TARGET_DIR"
      chmod +x "$TARGET_DIR/$BINARY_NAME"
      rm -f "/tmp/$ASSET"
      DOWNLOADED=true
      echo "Pre-built binary installed!"
    fi
  fi
fi

# Fallback: build from source
if [ "$DOWNLOADED" = "false" ]; then
  echo "Building from source..."

  if ! command -v cargo &> /dev/null; then
    echo "Installing Rust..."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    source $HOME/.cargo/env
  fi

  cargo build --release --features lindera-korean
fi

echo ""
echo "Setup complete!"
echo "Binary: $TARGET_DIR/$BINARY_NAME"
```

## Verification

```bash
./target/release/suggest-workflow --help
```
