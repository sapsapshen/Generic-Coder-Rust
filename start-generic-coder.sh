#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")"
export GENERIC_CODER_PROJECT_DIR="$PWD"

HOST="127.0.0.1"
PORT="8765"
BASE_URL="http://${HOST}:${PORT}/"
GENERIC_CODER_PICKER_TOKEN="$(uuidgen | tr '[:upper:]' '[:lower:]' | tr -d '-')"
export GENERIC_CODER_PICKER_TOKEN
OPEN_URL="${BASE_URL}#picker_token=${GENERIC_CODER_PICKER_TOKEN}"
EXE="target/debug/generic-coder"

wait_for_server() {
    for i in $(seq 1 600); do
        if curl -sf --max-time 2 "${BASE_URL}health" >/dev/null 2>&1; then
            open "$OPEN_URL"
            return 0
        fi
        sleep 1
    done
    echo "[Generic Coder] Server did not become ready in time."
    return 1
}

# Prefer cargo run when Rust toolchain is available
if command -v cargo >/dev/null 2>&1; then
    echo "[Generic Coder] Starting from source with cargo run..."
    cargo run -- serve --host "$HOST" --port "$PORT" &
    wait_for_server
    exit $?
fi

# Fall back to pre-built binary
if [ -x "$EXE" ]; then
    echo "[Generic Coder] Starting compiled Rust binary..."
    "$EXE" serve --host "$HOST" --port "$PORT" &
    wait_for_server
    exit $?
fi

echo "[Generic Coder] Neither Cargo nor '$EXE' was found."
echo "Install Rust from https://rustup.rs/ or build once with: cargo build"
exit 1
