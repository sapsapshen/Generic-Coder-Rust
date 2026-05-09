#!/usr/bin/env python3
"""
Shared computer-use bridge module.
- JSON RPC protocol (action + payload → JSON response on stdout)
- Coordinate scaling (physical screenshot pixels ↔ logical screen coords)
- Base64 image encoding
- Screen dimension caching

Protocol:
    python {platform}_helper.py --action <action> --payload '<json>'
    stdout: {"status": "ok", "data": {...}}  or  {"status": "error", "error": "..."}
"""

import json
import sys
import argparse
import base64
import io
from typing import Any, Optional

# ─── DPI / Coordinate Scaling ───────────────────────────────────────────
# mss returns physical pixels (e.g., 2880×1800 on Retina 1440×900 logical)
# pyautogui uses logical coordinates.
# We compute scale_factor = logical / physical and apply to input coords.

_scale_factor: float = 1.0
_screen_width: int = 0
_screen_height: int = 0
_physical_screen_width: int = 0
_physical_screen_height: int = 0

_last_screenshot: Optional[Any] = None  # PIL Image, for zoom cropping


def init_scaling():
    """Compute DPI scale factor. Call once after importing pyautogui + mss."""
    global _scale_factor, _screen_width, _screen_height
    global _physical_screen_width, _physical_screen_height

    import pyautogui

    _screen_width, _screen_height = pyautogui.size()

    import mss

    with mss.mss() as sct:
        monitor = sct.monitors[1]  # primary
        _physical_screen_width = monitor["width"]
        _physical_screen_height = monitor["height"]

    if _physical_screen_width > 0:
        _scale_factor = _screen_width / _physical_screen_width


def scale_to_screen(x: float, y: float) -> tuple[int, int]:
    """Convert screenshot (physical pixel) coords to logical screen coords for pyautogui."""
    return int(x * _scale_factor), int(y * _scale_factor)


def scale_from_screen(x: float, y: float) -> tuple[int, int]:
    """Convert logical screen coords to physical pixel coords."""
    if _scale_factor > 0:
        return int(x / _scale_factor), int(y / _scale_factor)
    return int(x), int(y)


# ─── JSON RPC protocol ──────────────────────────────────────────────────


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--action", required=True)
    parser.add_argument("--payload", default="{}")
    return parser.parse_args()


def read_payload() -> dict:
    args = parse_args()
    return json.loads(args.payload)


def write_response(status: str, data: Any = None, error: str = None):
    response = {"status": status}
    if data is not None:
        response["data"] = data
    if error is not None:
        response["error"] = error
    print(json.dumps(response, default=str))
    sys.stdout.flush()


# ─── Image utilities ────────────────────────────────────────────────────


def image_to_base64(img) -> str:
    """Convert PIL Image to base64-encoded PNG string."""
    buf = io.BytesIO()
    img.save(buf, format="PNG")
    return base64.b64encode(buf.getvalue()).decode("ascii")


def save_last_screenshot(img):
    global _last_screenshot
    _last_screenshot = img


def get_last_screenshot():
    return _last_screenshot


def screenshot_base64_result(img) -> dict:
    """Return the standard screenshot result dict."""
    save_last_screenshot(img)
    return {
        "base64": image_to_base64(img),
        "width": img.width,
        "height": img.height,
        "physical_width": _physical_screen_width,
        "physical_height": _physical_screen_height,
        "screen_width": _screen_width,
        "screen_height": _screen_height,
        "scale_factor": _scale_factor,
        "format": "png",
    }


# ─── Error helpers ──────────────────────────────────────────────────────


def require_coords(x, y, label: str = "action") -> tuple[int, int]:
    """Extract and validate x, y from payload. Apply coordinate scaling."""
    if x is None or y is None:
        write_response("error", error=f"x, y required for {label}")
        sys.exit(0)
    px, py = scale_to_screen(x, y)
    return px, py
