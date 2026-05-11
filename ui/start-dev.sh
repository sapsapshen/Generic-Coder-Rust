#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
WORKSPACE_BACKEND="$PROJECT_DIR/target/release/generic-coder"
STAGED_BACKEND="$SCRIPT_DIR/bin/generic-coder-backend"

echo "=== Generic Coder — Dev Mode ==="
echo ""

command -v node >/dev/null 2>&1 || {
  echo "[Generic Coder] Node.js was not found. Install it from https://nodejs.org/"
  exit 1
}

command -v npm >/dev/null 2>&1 || {
  echo "[Generic Coder] npm was not found. Install Node.js from https://nodejs.org/"
  exit 1
}

# ── Check/Install dependencies ──────────────────────────────────
cd "$SCRIPT_DIR"
if [ ! -d "node_modules" ]; then
  echo "Installing Electron dependencies..."
  npm install
  echo ""
fi

# ── Prepare backend (release for proper linking) ────────────────
cd "$PROJECT_DIR"
if command -v cargo >/dev/null 2>&1; then
  echo "Building Rust backend..."
  cargo build --release -q
elif [ -x "$WORKSPACE_BACKEND" ] || [ -x "$STAGED_BACKEND" ]; then
  echo "Cargo not found. Using prebuilt backend binary."
else
  echo "[Generic Coder] Cargo was not found and no prebuilt backend binary is available."
  echo "Install Rust from https://rustup.rs/ or build once with: cargo build --release"
  exit 1
fi
echo ""

# ── Launch Electron ─────────────────────────────────────────────
echo "Launching Electron dev mode..."
echo "  App starts its own backend on the first available localhost port"
echo ""

cd "$SCRIPT_DIR"
npm start
