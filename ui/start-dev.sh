#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

echo "=== Generic Coder — Dev Mode ==="
echo ""

# ── Check/Install dependencies ──────────────────────────────────
cd "$SCRIPT_DIR"
if [ ! -d "node_modules" ]; then
  echo "Installing Electron dependencies..."
  npm install
  echo ""
fi

# ── Build backend (release for proper linking) ──────────────────
echo "Building Rust backend..."
cd "$PROJECT_DIR"
cargo build --release -q 2>&1
echo ""

# ── Launch Electron ─────────────────────────────────────────────
echo "Launching Electron dev mode..."
echo "  App connects to backend at http://localhost:8765"
echo "  Make sure the backend is running: cargo run --release"
echo ""

cd "$SCRIPT_DIR"
npx electron .
