#!/bin/bash

# install-server.sh
# Installs dependencies and starts the Team Claude coordination server
#
# Usage: install-server.sh [--start] [--dev]
#   --start: Start the server after installation
#   --dev:   Start in development mode with hot reload

set -e

SCRIPT_DIR="$(dirname "$0")"
SERVER_DIR="${SCRIPT_DIR}/../server"
PORT="${PORT:-3847}"

# Parse arguments
START_SERVER=false
DEV_MODE=false

for arg in "$@"; do
    case $arg in
        --start)
            START_SERVER=true
            ;;
        --dev)
            DEV_MODE=true
            START_SERVER=true
            ;;
    esac
done

echo "╔══════════════════════════════════════════════════════════════╗"
echo "║      Team Claude Coordination Server Setup                   ║"
echo "╚══════════════════════════════════════════════════════════════╝"
echo ""

# Check for bun
if ! command -v bun &>/dev/null; then
    echo "[install] Bun not found. Installing..."
    curl -fsSL https://bun.sh/install | bash
    export PATH="$HOME/.bun/bin:$PATH"
fi

echo "[install] Bun version: $(bun --version)"

# Navigate to server directory
cd "$SERVER_DIR"

# Install dependencies
echo "[install] Installing dependencies..."
bun install

echo ""
echo "[install] Installation complete!"
echo ""

if [[ "$START_SERVER" == "true" ]]; then
    echo "[install] Starting server on port $PORT..."
    echo ""

    if [[ "$DEV_MODE" == "true" ]]; then
        echo "[install] Running in development mode (hot reload enabled)"
        bun run dev
    else
        bun run start
    fi
else
    echo "To start the server, run:"
    echo ""
    echo "  cd $SERVER_DIR && bun run start"
    echo ""
    echo "Or run in development mode:"
    echo ""
    echo "  cd $SERVER_DIR && bun run dev"
    echo ""
fi
