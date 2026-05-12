#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
UI_DIR="$SCRIPT_DIR"
ASSETS_DIR="$UI_DIR/assets"
BIN_DIR="$UI_DIR/bin"

echo "========================================"
echo " Generic Coder — macOS PKG Builder"
echo "========================================"
echo ""

# ── Check prerequisites ─────────────────────────────────────────
command -v node >/dev/null 2>&1 || { echo "ERROR: Node.js not found. Install from https://nodejs.org"; exit 1; }
command -v npm >/dev/null 2>&1 || { echo "ERROR: npm not found."; exit 1; }
command -v cargo >/dev/null 2>&1 || { echo "ERROR: Cargo not found. Install Rust from https://rustup.rs"; exit 1; }

ensure_rust_target() {
  local target="$1"
  if rustup target list --installed 2>/dev/null | grep -qx "$target"; then
    return
  fi
  if command -v rustup >/dev/null 2>&1; then
    echo "Installing Rust target: $target"
    rustup target add "$target"
  else
    echo "ERROR: Rust target $target is not installed and rustup was not found."
    exit 1
  fi
}

stage_backend_for_arch() {
  local arch="$1"
  local target="$2"

  echo "Building Rust backend for macOS $arch ($target)..."
  ensure_rust_target "$target"
  cd "$PROJECT_DIR"
  cargo build --release --target "$target" 2>&1

  mkdir -p "$BIN_DIR"
  cp "$PROJECT_DIR/target/$target/release/generic-coder" "$BIN_DIR/generic-coder-backend"
  chmod +x "$BIN_DIR/generic-coder-backend"

  echo "  → Backend staged for $arch:"
  file "$BIN_DIR/generic-coder-backend"
}

# ── Generate icons from SVG ─────────────────────────────────────
echo "Generating app icon..."

FORCE_ICONS=false
if [ "${1:-}" = "--force-icons" ]; then
  FORCE_ICONS=true
fi

ICONS_STALE=false
if [ ! -f "$ASSETS_DIR/icon.png" ] || [ ! -f "$ASSETS_DIR/icon.icns" ] || [ ! -f "$ASSETS_DIR/icon.ico" ]; then
  ICONS_STALE=true
elif [ "$ASSETS_DIR/icon.svg" -nt "$ASSETS_DIR/icon.png" ] || [ "$ASSETS_DIR/icon.svg" -nt "$ASSETS_DIR/icon.icns" ] || [ "$ASSETS_DIR/icon.svg" -nt "$ASSETS_DIR/icon.ico" ]; then
  ICONS_STALE=true
fi

if [ "$FORCE_ICONS" = false ] && [ "$ICONS_STALE" = false ]; then
  echo "  Icons are up to date. Skipping (use --force-icons to regenerate anyway)."
else
  cd "$ASSETS_DIR"

  # Use rsvg-convert (librsvg from brew) for SVG→PNG, fallback to ImageMagick
  if command -v rsvg-convert >/dev/null 2>&1; then
    echo "  Using rsvg-convert..."
    # Generate 1024x1024 PNG base
    rsvg-convert -w 1024 -h 1024 icon.svg -o icon.png
    echo "  → icon.png (1024x1024)"

    # Generate ICNS via iconutil
    mkdir -p icon.iconset
    for s in 16 32 64 128 256 512; do
      rsvg-convert -w $s -h $s icon.svg -o "icon.iconset/icon_${s}x${s}.png"
      rsvg-convert -w $((s*2)) -h $((s*2)) icon.svg -o "icon.iconset/icon_${s}x${s}@2x.png"
    done
    iconutil -c icns icon.iconset -o icon.icns
    rm -rf icon.iconset
    echo "  → icon.icns"

    # Generate ICO (256x256 PNG-based, electron-builder handles multires)
    rsvg-convert -w 256 -h 256 icon.svg -o icon.ico
    echo "  → icon.ico"

  elif command -v convert >/dev/null 2>&1; then
    echo "  Using ImageMagick..."
    convert -background none -resize 1024x1024 icon.svg icon.png
    echo "  → icon.png (1024x1024)"

    mkdir -p icon.iconset
    convert -resize 16x16 icon.png icon.iconset/icon_16x16.png
    convert -resize 32x32 icon.png icon.iconset/icon_16x16@2x.png
    convert -resize 32x32 icon.png icon.iconset/icon_32x32.png
    convert -resize 64x64 icon.png icon.iconset/icon_32x32@2x.png
    convert -resize 128x128 icon.png icon.iconset/icon_128x128.png
    convert -resize 256x256 icon.png icon.iconset/icon_128x128@2x.png
    convert -resize 256x256 icon.png icon.iconset/icon_256x256.png
    convert -resize 512x512 icon.png icon.iconset/icon_256x256@2x.png
    convert -resize 512x512 icon.png icon.iconset/icon_512x512.png
    convert -resize 1024x1024 icon.png icon.iconset/icon_512x512@2x.png
    iconutil -c icns icon.iconset -o icon.icns
    rm -rf icon.iconset
    echo "  → icon.icns"

    convert -resize 256x256 icon.png -define icon:auto-resize=256,128,64,48,32,16 icon.ico
    echo "  → icon.ico"
  else
    echo "  WARNING: Neither rsvg-convert nor ImageMagick found."
    echo "  Install librsvg: brew install librsvg"
    echo "  Or ImageMagick: brew install imagemagick"
    echo "  Using existing placeholders if any."
  fi

  cd "$UI_DIR"
fi

echo ""

# Copy backend assets into ui/assets/ for bundling
echo "  Copying backend assets..."
mkdir -p "$UI_DIR/assets"
cp -a "$PROJECT_DIR/assets"/*.txt "$UI_DIR/assets/" 2>/dev/null || true
cp -a "$PROJECT_DIR/assets"/*.json "$UI_DIR/assets/" 2>/dev/null || true
# Copy preset skills for bundling
rm -rf "$UI_DIR/assets/skills" 2>/dev/null || true
if [ -d "$PROJECT_DIR/skills" ]; then
  cp -a "$PROJECT_DIR/skills" "$UI_DIR/assets/skills"
  echo "  → Skills staged at ui/assets/skills/"
fi
echo "  → Assets staged at ui/assets/"
echo ""

# ── Install JS dependencies ─────────────────────────────────────
echo "Installing JS dependencies..."
cd "$UI_DIR"
npm install 2>&1
echo ""

# ── Build Electron installers (PKG, per-arch) ──────────────────
APP_VERSION="$(node scripts/resolve-app-version.cjs)"
X64_INSTALLER="$UI_DIR/dist/Generic Coder-${APP_VERSION}-x64-installer.pkg"
ARM64_INSTALLER="$UI_DIR/dist/Generic Coder-${APP_VERSION}-arm64-installer.pkg"

echo "Building macOS installer packages..."
rm -f "$UI_DIR"/dist/*.pkg 2>/dev/null || true
rm -f "$UI_DIR"/dist/*.dmg 2>/dev/null || true
rm -f "$UI_DIR"/dist/*.zip 2>/dev/null || true
rm -f "$UI_DIR"/dist/*.blockmap 2>/dev/null || true
rm -f "$UI_DIR"/dist/*.yml 2>/dev/null || true
rm -f "$UI_DIR"/dist/*.yaml 2>/dev/null || true

echo "  Building x64 PKG..."
stage_backend_for_arch "x64" "x86_64-apple-darwin"
cd "$UI_DIR"
npm run build:macos:x64 2>&1
echo "  Building arm64 PKG..."
stage_backend_for_arch "arm64" "aarch64-apple-darwin"
cd "$UI_DIR"
npm run build:macos:arm64 2>&1

[ -f "$X64_INSTALLER" ] || { echo "ERROR: Missing x64 installer: $X64_INSTALLER"; exit 1; }
[ -f "$ARM64_INSTALLER" ] || { echo "ERROR: Missing arm64 installer: $ARM64_INSTALLER"; exit 1; }

rm -rf "$UI_DIR/dist/mac" "$UI_DIR/dist/mac-arm64" 2>/dev/null || true

echo ""
echo "========================================"
echo " Done!"
echo " Installer output: ui/dist/"
ls -lh "$X64_INSTALLER" "$ARM64_INSTALLER"
find "$UI_DIR/dist" -maxdepth 1 \( -name '*.zip' -o -name '*.yml' -o -name '*.yaml' -o -name '*.blockmap' \) -print
echo "========================================"
