#!/usr/bin/env python3
"""
Windows Computer Use helper.
Usage: python win_helper.py --action <action> --payload '<json>'

All 24 actions implemented using pyautogui + mss + pywin32.
DPI-aware via SetProcessDpiAwareness(PerMonitorV2).
"""

import os
import sys
import time
import subprocess
import ctypes

# ─── DPI Awareness (must be set before any GUI imports) ─────────────────
try:
    ctypes.windll.shcore.SetProcessDpiAwareness(2)  # PerMonitorV2
except Exception:
    try:
        ctypes.windll.user32.SetProcessDPIAware()
    except Exception:
        pass

# Add parent dir for shared module
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

import pyautogui
import mss

pyautogui.FAILSAFE = False

from computer_use import (
    init_scaling, parse_args, read_payload, write_response,
    require_coords, screenshot_base64_result, get_last_screenshot,
)

init_scaling()

# ─── Platform-specific: App & Display Management ────────────────────────

try:
    import win32api
    import win32con
    import win32gui
    import win32process
    HAS_PYWIN32 = True
except ImportError:
    HAS_PYWIN32 = False


def list_displays() -> list:
    displays = []
    if not HAS_PYWIN32:
        return displays

    def _enum_callback(hMonitor, hdc, rect, data):
        mi = win32api.GetMonitorInfo(hMonitor)
        wa = mi.get("Work") or mi.get("Monitor", (0, 0, 0, 0))
        displays.append({
            "id": len(displays) + 1,
            "x": wa[0], "y": wa[1],
            "width": wa[2] - wa[0], "height": wa[3] - wa[1],
        })
        return True

    try:
        ctypes.windll.user32.EnumDisplayMonitors(0, 0, ctypes.WINFUNCTYPE(
            ctypes.c_int, ctypes.c_void_p, ctypes.c_void_p,
            ctypes.c_void_p, ctypes.c_void_p)(_enum_callback), 0)
    except Exception:
        displays.append({
            "id": 1, "x": 0, "y": 0,
            "width": pyautogui.size().width,
            "height": pyautogui.size().height,
        })
    return displays


def frontmost_app() -> dict:
    if not HAS_PYWIN32:
        return {"bundleId": "", "displayName": ""}
    try:
        hwnd = win32gui.GetForegroundWindow()
        _, pid = win32process.GetWindowThreadProcessId(hwnd)
        name = win32gui.GetWindowText(hwnd)
        try:
            handle = win32api.OpenProcess(win32con.PROCESS_QUERY_INFORMATION, False, pid)
            exe = win32process.GetModuleFileNameEx(handle, 0)
            win32api.CloseHandle(handle)
        except Exception:
            exe = ""
        return {"bundleId": exe, "displayName": name or exe}
    except Exception:
        return {"bundleId": "", "displayName": ""}


def list_installed_apps() -> list:
    apps = []
    for base in [os.environ.get("ProgramFiles", "C:\\Program Files"),
                 os.environ.get("ProgramFiles(x86)", "C:\\Program Files (x86)")]:
        if not os.path.isdir(base):
            continue
        try:
            for entry in os.listdir(base):
                p = os.path.join(base, entry)
                if os.path.isdir(p):
                    apps.append({"name": entry, "path": p})
        except PermissionError:
            pass
    return apps


def open_app(app_name: str, target: str = None) -> None:
    if target:
        os.startfile(target)
    else:
        os.startfile(app_name)


def read_clipboard() -> str:
    try:
        import pyperclip
        return pyperclip.paste() or ""
    except Exception:
        return ""


def write_clipboard(text: str) -> None:
    import pyperclip
    pyperclip.copy(text)


# ─── Mouse Actions ──────────────────────────────────────────────────────


def do_screenshot(payload: dict):
    display = payload.get("display")
    region = payload.get("region")

    with mss.mss() as sct:
        idx = int(display) if display and isinstance(display, (int, float)) else 1
        if idx >= len(sct.monitors):
            idx = 1
        monitor = sct.monitors[idx]
        img = sct.grab(monitor)
        from PIL import Image
        pil_img = Image.frombytes("RGB", img.size, img.bgra, "raw", "BGRX")

        if region and len(region) == 4:
            x, y, w, h = region
            pil_img = pil_img.crop((x, y, x + w, y + h))

        result = screenshot_base64_result(pil_img)
        result["displays"] = list_displays()
        write_response("ok", data=result)


def do_zoom(payload: dict):
    x0 = payload.get("x0")
    y0 = payload.get("y0")
    x1 = payload.get("x1")
    y1 = payload.get("y1")
    if x0 is None or y0 is None or x1 is None or y1 is None:
        write_response("error", error="x0, y0, x1, y1 required for zoom")
        return

    last = get_last_screenshot()
    if last is not None:
        try:
            cropped = last.crop((x0, y0, x1, y1))
            from computer_use import image_to_base64
            write_response("ok", data={
                "base64": image_to_base64(cropped),
                "width": cropped.width,
                "height": cropped.height,
                "displayWidth": cropped.width,
                "displayHeight": cropped.height,
                "format": "png",
            })
            return
        except Exception:
            pass

    with mss.mss() as sct:
        region = {"top": y0, "left": x0, "width": x1 - x0, "height": y1 - y0}
        img = sct.grab(region)
        from PIL import Image
        pil_img = Image.frombytes("RGB", img.size, img.bgra, "raw", "BGRX")
        from computer_use import image_to_base64
        write_response("ok", data={
            "base64": image_to_base64(pil_img),
            "width": pil_img.width,
            "height": pil_img.height,
            "displayWidth": pil_img.width,
            "displayHeight": pil_img.height,
            "format": "png",
        })


def do_left_click(payload: dict):
    x, y = require_coords(payload.get("x"), payload.get("y"), "left_click")
    pyautogui.click(x, y)


def do_right_click(payload: dict):
    x, y = require_coords(payload.get("x"), payload.get("y"), "right_click")
    pyautogui.rightClick(x, y)


def do_middle_click(payload: dict):
    x, y = require_coords(payload.get("x"), payload.get("y"), "middle_click")
    pyautogui.middleClick(x, y)


def do_double_click(payload: dict):
    x, y = require_coords(payload.get("x"), payload.get("y"), "double_click")
    pyautogui.doubleClick(x, y)


def do_triple_click(payload: dict):
    x, y = require_coords(payload.get("x"), payload.get("y"), "triple_click")
    pyautogui.tripleClick(x, y)


def do_left_click_drag(payload: dict):
    start_x = payload.get("start_x")
    start_y = payload.get("start_y")
    end_x = payload.get("x")
    end_y = payload.get("y")
    if start_x is None or start_y is None or end_x is None or end_y is None:
        write_response("error", error="start_x, start_y, x, y required for left_click_drag")
        return
    sx, sy = require_coords(start_x, start_y, "drag_start")
    ex, ey = require_coords(end_x, end_y, "drag_end")
    pyautogui.moveTo(sx, sy)
    pyautogui.drag(ex - sx, ey - sy, duration=0.2)


def do_mouse_move(payload: dict):
    x, y = require_coords(payload.get("x"), payload.get("y"), "mouse_move")
    pyautogui.moveTo(x, y)


def do_left_mouse_down(payload: dict):
    x, y = require_coords(payload.get("x"), payload.get("y"), "left_mouse_down")
    pyautogui.moveTo(x, y)
    pyautogui.mouseDown()


def do_left_mouse_up(payload: dict):
    x, y = require_coords(payload.get("x"), payload.get("y"), "left_mouse_up")
    pyautogui.moveTo(x, y)
    pyautogui.mouseUp()


def do_cursor_position(payload: dict):
    pos = pyautogui.position()
    from computer_use import scale_from_screen
    px, py = scale_from_screen(pos.x, pos.y)
    write_response("ok", data={"x": px, "y": py, "logical_x": pos.x, "logical_y": pos.y})


def do_scroll(payload: dict):
    x, y = require_coords(payload.get("x"), payload.get("y"), "scroll")
    direction = payload.get("direction", "down")
    amount = payload.get("amount", 3)
    pyautogui.moveTo(x, y)
    clicks = int(amount)
    if direction == "up":
        pyautogui.scroll(clicks)
    elif direction == "down":
        pyautogui.scroll(-clicks)
    elif direction == "left":
        pyautogui.hscroll(-clicks)
    elif direction == "right":
        pyautogui.hscroll(clicks)
    else:
        pyautogui.scroll(-clicks)


# ─── Keyboard Actions ───────────────────────────────────────────────────

KEY_MAP = {
    "return": "enter", "enter": "enter", "escape": "esc", "esc": "esc",
    "space": "space", "tab": "tab", "backspace": "backspace", "delete": "delete",
    "up": "up", "down": "down", "left": "left", "right": "right",
    "home": "home", "end": "end", "pageup": "pageup", "pagedown": "pagedown",
}


def _map_modifier(m: str) -> str:
    m = m.lower().strip()
    mapping = {
        "cmd": "win", "command": "win", "win": "win",
        "shift": "shift",
        "option": "alt", "alt": "alt",
        "ctrl": "ctrl", "control": "ctrl",
    }
    return mapping.get(m, m)


def do_type(payload: dict):
    text = payload.get("text", "")
    if not text:
        write_response("error", error="text required for type")
        return
    pyautogui.write(text, interval=0.008)


def do_key(payload: dict):
    text = payload.get("text", "")
    if not text:
        write_response("error", error="text required for key")
        return
    parts = [p.strip() for p in text.split("+")]
    if len(parts) == 1:
        key = KEY_MAP.get(parts[0].lower(), parts[0].lower())
        pyautogui.press(key)
    else:
        key = KEY_MAP.get(parts[-1].lower(), parts[-1].lower())
        modifiers = [_map_modifier(p) for p in parts[:-1]]
        pyautogui.hotkey(*modifiers, key)


def do_hold_key(payload: dict):
    text = payload.get("text", "")
    duration = payload.get("duration", 1.0)
    if not text:
        write_response("error", error="text required for hold_key")
        return
    parts = [p.strip() for p in text.split("+")]
    for p in parts:
        pyautogui.keyDown(_map_modifier(p))
    time.sleep(float(duration))
    for p in reversed(parts):
        pyautogui.keyUp(_map_modifier(p))


# ─── App / Display / System ─────────────────────────────────────────────


def do_open_application(payload: dict):
    app_name = payload.get("text", "") or payload.get("application", "")
    target = payload.get("target")
    if not app_name and not target:
        write_response("error", error="text (application name) or target required")
        return
    try:
        if app_name:
            open_app(app_name, target)
        elif target:
            os.startfile(target)
    except Exception as e:
        # Try subprocess as fallback
        try:
            if target:
                subprocess.Popen(["start", "", target], shell=True)
            else:
                subprocess.Popen(["start", "", app_name], shell=True)
        except Exception:
            write_response("error", error=str(e))
            return
    time.sleep(0.5)
    app = frontmost_app()
    write_response("ok", data={"frontmost_app": app})


def do_switch_display(payload: dict):
    displays = list_displays()
    write_response("ok", data={"displays": displays})


def do_request_access(payload: dict):
    apps = payload.get("applications", [])
    if isinstance(apps, str):
        apps = [apps]
    write_response("ok", data={
        "granted": apps,
        "message": "Application access granted for this session",
    })


def do_list_granted_applications(payload: dict):
    apps = list_installed_apps()
    write_response("ok", data={"applications": apps[:50]})


def do_read_clipboard(payload: dict):
    text = read_clipboard()
    write_response("ok", data={"text": text})


def do_write_clipboard(payload: dict):
    text = payload.get("text", "")
    write_clipboard(text)
    write_response("ok", data={"written": True})


def do_wait(payload: dict):
    duration = payload.get("duration", 1.0)
    time.sleep(max(0.0, float(duration)))
    write_response("ok", data={"waited": duration})


def do_computer_batch(payload: dict):
    actions = payload.get("actions", [])
    if not actions:
        write_response("ok", data={"results": []})
        return
    results = []
    for act in actions:
        an = act.get("action")
        if an and an in DISPATCH:
            try:
                DISPATCH[an](act)
                results.append({"action": an, "status": "ok"})
            except Exception as e:
                results.append({"action": an, "status": "error", "error": str(e)})
        else:
            results.append({"action": an, "status": "error", "error": "unknown action"})
    write_response("ok", data={"results": results})


# ─── Dispatch table ─────────────────────────────────────────────────────

DISPATCH = {
    "screenshot": do_screenshot,
    "zoom": do_zoom,
    "left_click": do_left_click,
    "right_click": do_right_click,
    "middle_click": do_middle_click,
    "double_click": do_double_click,
    "triple_click": do_triple_click,
    "left_click_drag": do_left_click_drag,
    "mouse_move": do_mouse_move,
    "left_mouse_down": do_left_mouse_down,
    "left_mouse_up": do_left_mouse_up,
    "cursor_position": do_cursor_position,
    "scroll": do_scroll,
    "type": do_type,
    "key": do_key,
    "hold_key": do_hold_key,
    "open_application": do_open_application,
    "switch_display": do_switch_display,
    "request_access": do_request_access,
    "list_granted_applications": do_list_granted_applications,
    "read_clipboard": do_read_clipboard,
    "write_clipboard": do_write_clipboard,
    "wait": do_wait,
    "computer_batch": do_computer_batch,
}


def main():
    args = parse_args()
    action = args.action
    payload = read_payload()

    if action not in DISPATCH:
        write_response("error", error=f"Unknown action: {action}")
        return

    try:
        DISPATCH[action](payload)
    except Exception as e:
        write_response("error", error=str(e))


if __name__ == "__main__":
    main()
