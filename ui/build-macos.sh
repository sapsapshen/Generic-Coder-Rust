#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
UI_DIR="$SCRIPT_DIR"
ASSETS_DIR="$UI_DIR/assets"

echo "========================================"
echo " Generic Coder — macOS DMG Builder"
echo "========================================"
echo ""

# ── Check prerequisites ─────────────────────────────────────────
command -v node >/dev/null 2>&1 || { echo "ERROR: Node.js not found. Install from https://nodejs.org"; exit 1; }
command -v npm >/dev/null 2>&1 || { echo "ERROR: npm not found."; exit 1; }

# electron-builder needs `python` (not just python3) for blockmap generation
if ! command -v python >/dev/null 2>&1; then
  if command -v python3 >/dev/null 2>&1; then
    PYTHON3_PATH="$(command -v python3)"
    sudo ln -sf "$PYTHON3_PATH" "${PYTHON3_PATH%/python3}/python" 2>/dev/null || {
      echo "WARNING: Cannot create python symlink. Blockmap generation may fail."
      echo "  Run: sudo ln -sf \$(which python3) /opt/homebrew/bin/python"
    }
  fi
fi

# ── Generate icons from SVG ─────────────────────────────────────
echo "Generating app icon..."

if [ -f "$ASSETS_DIR/icon.icns" ] && [ "${1:-}" != "--force-icons" ]; then
  echo "  Icons already exist. Skipping (use --force-icons to regenerate)."
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

# ── Build Electron app (ZIP only, per-arch) ────────────────────
echo "Building macOS .app bundles..."
echo "  Building x64 (ZIP)..."
npm run build:macos:x64 2>&1
echo "  Building arm64 (ZIP)..."
npm run build:macos:arm64 2>&1

# ── Create DMGs manually ────────────────────────────────────────
echo ""
echo "Creating DMG files..."

create_dmg() {
  local ARCH="$1"
  local APP_DIR="$2"
  local DMG_OUT="$3"
  local TMP_DMG="/tmp/gc-dmg-${ARCH}-$$.dmg"
  local TMP_MOUNT="/tmp/gc-mount-${ARCH}-$$"

  echo "  Creating ${ARCH} DMG..."

  # Create temporary DMG with the .app inside
  rm -f "$TMP_DMG"
  mkdir -p "$TMP_MOUNT"

  hdiutil create \
    -srcfolder "$APP_DIR" \
    -volname "Generic Coder" \
    -anyowners \
    -nospotlight \
    -format UDRW \
    -fs APFS \
    -quiet \
    "$TMP_DMG" || { echo "  ERROR: Failed to create temp DMG"; return 1; }

  # Mount it
  hdiutil attach -readwrite -noverify -noautoopen -mountpoint "$TMP_MOUNT" "$TMP_DMG" -quiet || {
    echo "  ERROR: Failed to mount temp DMG"
    rm -f "$TMP_DMG"
    return 1
  }

  # Create Applications symlink
  ln -sf /Applications "$TMP_MOUNT/Applications" 2>/dev/null || true

  # Set icon positions using AppleScript (optional, cosmetic)
  for i in 1 2 3; do
    osascript -e "
      tell application \"Finder\"
        try
          tell disk \"Generic Coder\"
            open
            set current view of container window to icon view
            set toolbar visible of container window to false
            set statusbar visible of container window to false
            set the bounds of container window to {400, 100, 900, 450}
            set theViewOptions to the icon view options of container window
            set arrangement of theViewOptions to not arranged
            set icon size of theViewOptions to 72
            set position of item \"Generic Coder.app\" of container window to {160, 200}
            set position of item \"Applications\" of container window to {480, 200}
            close
            update
          end tell
        end try
      end tell
    " 2>/dev/null && break
    sleep 0.5
  done

  # Make sure Finder writes its metadata before detaching
  sleep 1

  # Detach
  hdiutil detach "$TMP_MOUNT" -quiet 2>/dev/null || {
    hdiutil detach "$TMP_MOUNT" -force 2>/dev/null || true
  }

  # Convert to compressed read-only DMG
  rm -f "$DMG_OUT"
  hdiutil convert "$TMP_DMG" -format UDZO -imagekey zlib-level=9 -o "$DMG_OUT" -quiet || {
    echo "  ERROR: Failed to convert DMG"
    rm -f "$TMP_DMG"
    return 1
  }

  rm -f "$TMP_DMG"
  echo "  → $(basename "$DMG_OUT") ($(du -sh "$DMG_OUT" | cut -f1))"
}

# Create x64 DMG
create_dmg "x64" \
  "$UI_DIR/dist/mac/Generic Coder.app" \
  "$UI_DIR/dist/Generic Coder-1.0.0-x64.dmg"

# Create arm64 DMG
create_dmg "arm64" \
  "$UI_DIR/dist/mac-arm64/Generic Coder.app" \
  "$UI_DIR/dist/Generic Coder-1.0.0-arm64.dmg"

# ── Clean up non-arch outputs ─────────────────────────────────
# Remove the generic (non-arch) files if electron-builder created them anyway
rm -f "$UI_DIR/dist/Generic Coder-1.0.0-mac.zip" 2>/dev/null || true
rm -f "$UI_DIR/dist/Generic Coder-1.0.0.dmg" 2>/dev/null || true
# Remove x64-mac.zip since electron-builder produces Generic Coder-1.0.0-x64-mac.zip
rm -f "$UI_DIR/dist/Generic Coder-1.0.0-x64-mac.zip" 2>/dev/null || true
# Rename x64 zip to match expected naming: x64-mac.zip
if [ -f "$UI_DIR/dist/Generic Coder-1.0.0-x64.zip" ]; then
  mv "$UI_DIR/dist/Generic Coder-1.0.0-x64.zip" "$UI_DIR/dist/Generic Coder-1.0.0-x64-mac.zip"
fi

echo ""
echo "========================================"
echo " Done!"
echo " DMG & ZIP output: ui/dist/"
ls -lh "$UI_DIR/dist/" 2>/dev/null | grep -E '\.(dmg|zip)$' || echo "  (check dist/ for output)"
echo "========================================"
