#!/usr/bin/env bash
# ensure-binary.sh — suggest-workflow 바이너리가 없으면 다운로드 또는 빌드
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
CLI_DIR="$(cd "$SCRIPT_DIR/../cli" && pwd)"
BINARY="$CLI_DIR/target/release/suggest-workflow"

if [ -x "$BINARY" ]; then
  exit 0
fi

echo "suggest-workflow binary not found. Setting up..."

REPO="kys0213/kys-claude-plugin"
TARGET_DIR="$CLI_DIR/target/release"
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
      chmod +x "$BINARY"
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
    # shellcheck disable=SC1091
    source "$HOME/.cargo/env"
  fi

  cd "$CLI_DIR"
  cargo build --release --features lindera-korean
fi

echo "suggest-workflow ready: $BINARY"
