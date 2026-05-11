#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")"
export GENERIC_CODER_PROJECT_DIR="$PWD"

if [ ! -f "ui/start-dev.sh" ]; then
    echo "[Generic Coder] Missing ui/start-dev.sh"
    exit 1
fi

echo "[Generic Coder] Launching desktop app..."
exec bash ./ui/start-dev.sh
