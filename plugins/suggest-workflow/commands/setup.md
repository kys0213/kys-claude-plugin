---
description: Build suggest-workflow Rust CLI
---

# Setup Suggest Workflow CLI

This command sets up the Rust CLI binary for suggest-workflow.
It first tries to download a pre-built binary from GitHub Releases, then falls back to building from source.

## Steps

1. Detect OS and architecture
2. Try downloading pre-built binary from latest GitHub Release
3. If download fails, build from source with `cargo build --release --features lindera-korean`
4. Report binary location

## Execution

```bash
cd ${CLAUDE_PLUGIN_ROOT}/cli

REPO="kys0213/kys-claude-plugin"
BINARY_NAME="suggest-workflow"
TARGET_DIR="$(pwd)/target/release"
mkdir -p "$TARGET_DIR"

# Detect OS and architecture
OS="$(uname -s | tr '[:upper:]' '[:lower:]')"
ARCH="$(uname -m)"

case "$OS" in
  linux*)  OS_NAME="linux" ;;
  darwin*) OS_NAME="darwin" ;;
  mingw*|msys*|cygwin*) OS_NAME="windows" ;;
  *) OS_NAME="" ;;
esac

case "$ARCH" in
  x86_64|amd64) ARCH_NAME="x86_64" ;;
  aarch64|arm64) ARCH_NAME="aarch64" ;;
  *) ARCH_NAME="" ;;
esac

DOWNLOADED=false

if [ -n "$OS_NAME" ] && [ -n "$ARCH_NAME" ]; then
  ARTIFACT="${BINARY_NAME}-${OS_NAME}-${ARCH_NAME}"

  if [ "$OS_NAME" = "windows" ]; then
    ASSET="${ARTIFACT}.zip"
  else
    ASSET="${ARTIFACT}.tar.gz"
  fi

  echo "Trying to download pre-built binary: $ASSET"

  # Get latest release tag
  LATEST_TAG=$(gh release list --repo "$REPO" --limit 1 --json tagName -q '.[0].tagName' 2>/dev/null || echo "")

  if [ -n "$LATEST_TAG" ]; then
    if gh release download "$LATEST_TAG" --repo "$REPO" --pattern "$ASSET" --dir /tmp 2>/dev/null; then
      echo "Downloaded $ASSET from release $LATEST_TAG"

      if [ "$OS_NAME" = "windows" ]; then
        unzip -o "/tmp/$ASSET" -d "$TARGET_DIR"
      else
        tar xzf "/tmp/$ASSET" -C "$TARGET_DIR"
      fi

      chmod +x "$TARGET_DIR/$BINARY_NAME" 2>/dev/null || true
      rm -f "/tmp/$ASSET"
      DOWNLOADED=true
      echo "Pre-built binary installed successfully!"
    else
      echo "Pre-built binary not found for this platform."
    fi
  else
    echo "Could not determine latest release."
  fi
fi

if [ "$DOWNLOADED" = "false" ]; then
  echo "Falling back to building from source..."

  # Check Rust installation
  if ! command -v cargo &> /dev/null; then
    echo "Installing Rust..."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    source $HOME/.cargo/env
  fi

  echo "Building release binary with Korean support..."
  cargo build --release --features lindera-korean
fi

echo ""
echo "Setup complete!"
echo "Binary location: $TARGET_DIR/$BINARY_NAME"
echo ""
echo "To use globally, add to PATH or create symlink:"
echo "  ln -s $TARGET_DIR/$BINARY_NAME /usr/local/bin/$BINARY_NAME"
```

## Verification

After setup, test the binary:

```bash
./target/release/suggest-workflow --help
```

## Output

The binary will be at:
- `${CLAUDE_PLUGIN_ROOT}/cli/target/release/suggest-workflow`
