#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
UI_DIR="$SCRIPT_DIR"
ASSETS_DIR="$UI_DIR/assets"

echo "========================================"
echo " Generic Coder — macOS PKG Builder"
echo "========================================"
echo ""

# ── Check prerequisites ─────────────────────────────────────────
command -v node >/dev/null 2>&1 || { echo "ERROR: Node.js not found. Install from https://nodejs.org"; exit 1; }
command -v npm >/dev/null 2>&1 || { echo "ERROR: npm not found."; exit 1; }

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

# ── Rebuild Rust backend ────────────────────────────────────────
echo "Building Rust backend..."
cd "$PROJECT_DIR"
cargo build --release 2>&1
echo "  → Backend built"

# Copy binary into electron-builder's reach
mkdir -p "$UI_DIR/bin"
cp target/release/generic-coder "$UI_DIR/bin/generic-coder-backend" 2>/dev/null || true
echo "  → Backend binary staged at ui/bin/"

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
APP_VERSION="$(node -p "require('./package.json').version")"
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
npm run build:macos:x64 2>&1
echo "  Building arm64 PKG..."
npm run build:macos:arm64 2>&1

[ -f "$X64_INSTALLER" ] || { echo "ERROR: Missing x64 installer: $X64_INSTALLER"; exit 1; }
[ -f "$ARM64_INSTALLER" ] || { echo "ERROR: Missing arm64 installer: $ARM64_INSTALLER"; exit 1; }

rm -rf "$UI_DIR/dist/mac" "$UI_DIR/dist/mac-arm64" 2>/dev/null || true
rm -f "$UI_DIR"/dist/*.dmg 2>/dev/null || true
rm -f "$UI_DIR"/dist/*.zip 2>/dev/null || true
rm -f "$UI_DIR"/dist/*.blockmap 2>/dev/null || true
rm -f "$UI_DIR"/dist/*.yml 2>/dev/null || true
rm -f "$UI_DIR"/dist/*.yaml 2>/dev/null || true

echo ""
echo "========================================"
echo " Done!"
echo " Installer output: ui/dist/"
ls -lh "$X64_INSTALLER" "$ARM64_INSTALLER"
echo "========================================"
