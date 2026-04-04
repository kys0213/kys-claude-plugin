#!/usr/bin/env bash
# ensure-binary.sh — autopilot CLI 바이너리 버전 확인 및 조건부 빌드/설치
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PLUGIN_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
CLI_DIR="$PLUGIN_DIR/cli"
PLUGIN_JSON="$PLUGIN_DIR/.claude-plugin/plugin.json"
INSTALL_DIR="$HOME/.local/bin"
BINARY_NAME="autopilot"
BINARY_PATH="$INSTALL_DIR/$BINARY_NAME"

# --- Helper: SemVer 비교 ---
# Returns: 0 if a == b, 1 if a > b, 2 if a < b
compare_semver() {
  local a="$1" b="$2"
  a="${a#v}"; b="${b#v}"

  local a_core="${a%%-*}" b_core="${b%%-*}"
  IFS='.' read -r a_major a_minor a_patch <<< "$a_core"
  IFS='.' read -r b_major b_minor b_patch <<< "$b_core"

  a_major=${a_major:-0}; a_minor=${a_minor:-0}; a_patch=${a_patch:-0}
  b_major=${b_major:-0}; b_minor=${b_minor:-0}; b_patch=${b_patch:-0}

  for field in major minor patch; do
    local va="a_$field" vb="b_$field"
    if (( ${!va} < ${!vb} )); then return 2; fi
    if (( ${!va} > ${!vb} )); then return 1; fi
  done
  return 0
}

# --- 1. plugin.json에서 버전 읽기 ---
if [ ! -f "$PLUGIN_JSON" ]; then
  echo "ERROR: plugin.json not found: $PLUGIN_JSON" >&2
  exit 1
fi

PLUGIN_VERSION=$(grep -o '"version"[[:space:]]*:[[:space:]]*"[^"]*"' "$PLUGIN_JSON" | head -1 | sed 's/.*"version"[[:space:]]*:[[:space:]]*"\([^"]*\)"/\1/')

if [ -z "$PLUGIN_VERSION" ]; then
  echo "ERROR: Failed to read version from plugin.json" >&2
  exit 1
fi

# --- 2. 설치된 바이너리 버전 확인 ---
INSTALLED_VERSION=""
if command -v "$BINARY_NAME" &> /dev/null; then
  INSTALLED_VERSION=$("$BINARY_NAME" --version 2>/dev/null | awk '{print $NF}' || echo "")
fi

# --- 3. 버전 비교 → 액션 결정 ---
ACTION=""
if [ -z "$INSTALLED_VERSION" ]; then
  ACTION="install"
  echo "autopilot CLI not found. Installing v${PLUGIN_VERSION}..."
else
  set +e
  compare_semver "$INSTALLED_VERSION" "$PLUGIN_VERSION"
  CMP=$?
  set -e

  if [ "$CMP" -eq 2 ]; then
    ACTION="update"
    echo "autopilot CLI outdated: v${INSTALLED_VERSION} → v${PLUGIN_VERSION}. Rebuilding..."
  else
    echo "autopilot CLI is up to date (v${INSTALLED_VERSION})."
    exit 0
  fi
fi

# --- 4. Rust 툴체인 확인 ---
if ! command -v cargo &> /dev/null; then
  echo "ERROR: Rust toolchain not found." >&2
  echo "Install from: https://rustup.rs/" >&2
  exit 1
fi

# --- 5. 빌드 ---
echo "Building autopilot CLI..."
cargo build --release --manifest-path "$CLI_DIR/Cargo.toml"

# --- 6. 설치 ---
mkdir -p "$INSTALL_DIR"
cp "$CLI_DIR/target/release/autopilot" "$BINARY_PATH"
chmod +x "$BINARY_PATH"

# --- 7. 설치 확인 ---
if "$BINARY_PATH" --version &> /dev/null; then
  NEW_VERSION=$("$BINARY_PATH" --version 2>/dev/null | awk '{print $NF}')
  if [ "$ACTION" = "install" ]; then
    echo "autopilot CLI installed successfully (v${NEW_VERSION})."
  else
    echo "autopilot CLI updated successfully: v${INSTALLED_VERSION} → v${NEW_VERSION}."
  fi
else
  echo "ERROR: Installation verification failed." >&2
  exit 1
fi
