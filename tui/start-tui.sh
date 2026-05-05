#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

echo "=== Generic Coder TUI Launcher (macOS/Linux) ==="
echo ""

# ── Build (release) ────────────────────────────────────────────────
echo "Building TUI..."
cd "$PROJECT_DIR"
cargo build --release -p generic-coder-tui 2>&1
echo ""

BIN="$PROJECT_DIR/target/release/generic-coder-tui"

if [ ! -f "$BIN" ]; then
    echo "ERROR: Build failed — binary not found at $BIN"
    exit 1
fi

# ── Run ────────────────────────────────────────────────────────────
echo "Launching Generic Coder TUI..."
echo "  Ctrl+Q  Quit"
echo "  F1      Work mode"
echo "  F2      Plan mode"
echo "  F3      Review mode"
echo "  Ctrl+S  Settings"
echo "  Ctrl+W  Toggle sidebar"
echo ""

exec "$BIN" "$@"
