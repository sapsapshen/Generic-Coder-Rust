//! Computer Use — Python Bridge implementation (cc-haha aligned).
//!
//! Architecture:
//!   Agent dispatch → tools.rs (thin wrappers) → computer_use.rs (Rust bridge)
//!     → Python subprocess (runtime/{platform}_helper.py) → pyautogui + mss
//!
//! The Rust layer handles:
//!   - Python venv auto-bootstrap (first call only)
//!   - JSON RPC over subprocess stdout (--action + --payload)
//!   - Global mutex (one Computer Use action at a time)
//!   - App allowlist (request_access / list_granted)
//!   - Coordinate validation

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::OnceLock;

use anyhow::{anyhow, Context, Result};
use parking_lot::Mutex;
use serde_json::{json, Value};

// ─── Global state ───────────────────────────────────────────────────────

/// Global mutex: only one Computer Use action at a time.
static CU_MUTEX: Mutex<()> = Mutex::new(());

/// Allowed applications (populated by request_access).
static ALLOWLIST: parking_lot::RwLock<Vec<String>> = parking_lot::RwLock::new(Vec::new());

/// Cached screen dimensions from last screenshot (for coordinate validation).
static SCREEN_DIMS: parking_lot::RwLock<Option<(u64, u64)>> =
    parking_lot::RwLock::new(None);

/// Cached venv Python path (bootstrapped once).
static PYTHON_PATH: OnceLock<PathBuf> = OnceLock::new();

/// Cached project root for resolving runtime/ paths.
static PROJECT_DIR: OnceLock<PathBuf> = OnceLock::new();

// ─── Public init ────────────────────────────────────────────────────────

pub fn set_project_dir(dir: PathBuf) {
    let _ = PROJECT_DIR.set(dir);
}

fn project_dir() -> &'static Path {
    PROJECT_DIR
        .get()
        .map(|p| p.as_path())
        .unwrap_or_else(|| Path::new("."))
}

// ─── Venv bootstrap ─────────────────────────────────────────────────────

fn get_python() -> Result<&'static Path> {
    if let Some(p) = PYTHON_PATH.get() {
        return Ok(p.as_path());
    }
    let path = bootstrap_venv()?;
    // OnceCell doesn't have get_or_try_init in stable Rust
    let p = PYTHON_PATH.get_or_init(|| path);
    Ok(p.as_path())
}

fn bootstrap_venv() -> Result<PathBuf> {
    let runtime_dir = project_dir().join("runtime");
    let venv_python = if cfg!(windows) {
        runtime_dir.join("venv").join("Scripts").join("python.exe")
    } else {
        runtime_dir.join("venv").join("bin").join("python")
    };

    if !venv_python.exists() {
        log::info!(
            "[computer_use] Bootstrapping Python venv in {:?}",
            runtime_dir
        );

        // python -m venv runtime/venv
        let system_python = find_system_python()?;
        let status = Command::new(&system_python)
            .args(["-m", "venv"])
            .arg(runtime_dir.join("venv"))
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .status()
            .context("Failed to create Python virtual environment")?;

        if !status.success() {
            return Err(anyhow!(
                "Failed to create Python venv. Ensure Python 3.8+ is installed."
            ));
        }

        // Install dependencies
        let req_path = runtime_dir.join("requirements.txt");
        let pip = if cfg!(windows) {
            runtime_dir.join("venv").join("Scripts").join("pip.exe")
        } else {
            runtime_dir.join("venv").join("bin").join("pip")
        };

        let status = Command::new(&pip)
            .args(["install", "-r"])
            .arg(&req_path)
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .status()
            .context("Failed to install Python dependencies")?;

        if !status.success() {
            return Err(anyhow!(
                "Failed to install Python Computer Use dependencies (pyautogui, mss, Pillow). \
                 Check network connectivity."
            ));
        }

        log::info!("[computer_use] Python venv ready: {:?}", venv_python);
    }

    Ok(venv_python)
}

fn find_system_python() -> Result<String> {
    for name in &["python3", "python", "py"] {
        if Command::new(name)
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok()
        {
            return Ok(name.to_string());
        }
    }
    Err(anyhow!(
        "Python 3.8+ not found. Install from https://python.org to use Computer Use."
    ))
}

// ─── Platform helper name ───────────────────────────────────────────────

fn platform_helper() -> &'static str {
    if cfg!(target_os = "macos") {
        "mac_helper.py"
    } else if cfg!(target_os = "linux") {
        "linux_helper.py"
    } else {
        "win_helper.py"
    }
}

// ─── JSON RPC call to Python ────────────────────────────────────────────

/// Call the Python helper via subprocess.
/// Protocol: `python helper.py --action <action> --payload '<json>'`
/// Response on stdout: `{"status": "ok", "data": {...}}` or `{"status": "error", "error": "..."}`
fn call_python(action: &str, payload: &Value) -> Result<Value> {
    let py = get_python()?;
    let helper = project_dir().join("runtime").join(platform_helper());
    let payload_str = serde_json::to_string(payload)?;

    let output = Command::new(py)
        .arg(&helper)
        .arg("--action")
        .arg(action)
        .arg("--payload")
        .arg(&payload_str)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .context("Failed to execute Python bridge")?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    if !stderr.is_empty() {
        log::warn!("[computer_use] Python stderr ({}): {}", action, stderr.trim());
    }

    if !output.status.success() {
        return Err(anyhow!(
            "Python bridge exited with status {}: {}",
            output.status,
            stderr.trim()
        ));
    }

    let response: Value = serde_json::from_str(stdout.trim()).context(format!(
        "Failed to parse Python bridge response (action={}): {}",
        action, stdout
    ))?;

    if response.get("status").and_then(|v| v.as_str()) == Some("error") {
        let err_msg = response
            .get("error")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown error");
        return Err(anyhow!("{}", err_msg));
    }

    // Cache screen dimensions from screenshot responses
    if let Some(data) = response.get("data") {
        if let (Some(w), Some(h)) = (
            data.get("physical_width").and_then(|v| v.as_u64()),
            data.get("physical_height").and_then(|v| v.as_u64()),
        ) {
            *SCREEN_DIMS.write() = Some((w, h));
        }
    }

    Ok(response)
}

/// Validate coordinates are within screen bounds (warns only, doesn't block).
fn validate_coords(x: Option<u64>, y: Option<u64>) -> Result<()> {
    if let (Some(x), Some(y)) = (x, y) {
        if let Some((w, h)) = *SCREEN_DIMS.read() {
            if x > w || y > h {
                log::warn!(
                    "[computer_use] Coordinates ({}, {}) outside screen bounds ({}x{}), \
                     continuing anyway",
                    x,
                    y,
                    w,
                    h
                );
            }
        }
    }
    Ok(())
}

// ─── Lock wrapper ───────────────────────────────────────────────────────

/// Execute a Computer Use action under the global mutex.
fn with_lock<F>(f: F) -> Result<Value>
where
    F: FnOnce() -> Result<Value>,
{
    let _guard = CU_MUTEX.lock();
    f()
}

// ─── 24 Tool Wrappers ───────────────────────────────────────────────────

pub fn screenshot(region: Option<&[u64]>, display: Option<u64>) -> Result<Value> {
    with_lock(|| {
        let mut payload = json!({});
        if let Some(r) = region {
            payload["region"] = json!(r);
        }
        if let Some(d) = display {
            payload["display"] = json!(d);
        }
        call_python("screenshot", &payload)
    })
}

pub fn zoom(x0: u64, y0: u64, x1: u64, y1: u64) -> Result<Value> {
    with_lock(|| {
        let payload = json!({"x0": x0, "y0": y0, "x1": x1, "y1": y1});
        call_python("zoom", &payload)
    })
}

pub fn left_click(x: u64, y: u64) -> Result<Value> {
    validate_coords(Some(x), Some(y))?;
    with_lock(|| call_python("left_click", &json!({"x": x, "y": y})))
}

pub fn right_click(x: u64, y: u64) -> Result<Value> {
    validate_coords(Some(x), Some(y))?;
    with_lock(|| call_python("right_click", &json!({"x": x, "y": y})))
}

pub fn middle_click(x: u64, y: u64) -> Result<Value> {
    validate_coords(Some(x), Some(y))?;
    with_lock(|| call_python("middle_click", &json!({"x": x, "y": y})))
}

pub fn double_click(x: u64, y: u64) -> Result<Value> {
    validate_coords(Some(x), Some(y))?;
    with_lock(|| call_python("double_click", &json!({"x": x, "y": y})))
}

pub fn triple_click(x: u64, y: u64) -> Result<Value> {
    validate_coords(Some(x), Some(y))?;
    with_lock(|| call_python("triple_click", &json!({"x": x, "y": y})))
}

pub fn left_click_drag(start_x: u64, start_y: u64, x: u64, y: u64) -> Result<Value> {
    with_lock(|| {
        call_python(
            "left_click_drag",
            &json!({"start_x": start_x, "start_y": start_y, "x": x, "y": y}),
        )
    })
}

pub fn mouse_move(x: u64, y: u64) -> Result<Value> {
    validate_coords(Some(x), Some(y))?;
    with_lock(|| call_python("mouse_move", &json!({"x": x, "y": y})))
}

pub fn left_mouse_down(x: u64, y: u64) -> Result<Value> {
    validate_coords(Some(x), Some(y))?;
    with_lock(|| call_python("left_mouse_down", &json!({"x": x, "y": y})))
}

pub fn left_mouse_up(x: u64, y: u64) -> Result<Value> {
    validate_coords(Some(x), Some(y))?;
    with_lock(|| call_python("left_mouse_up", &json!({"x": x, "y": y})))
}

pub fn cursor_position() -> Result<Value> {
    with_lock(|| call_python("cursor_position", &json!({})))
}

pub fn scroll(x: u64, y: u64, direction: &str, amount: u64) -> Result<Value> {
    validate_coords(Some(x), Some(y))?;
    with_lock(|| {
        call_python(
            "scroll",
            &json!({"x": x, "y": y, "direction": direction, "amount": amount}),
        )
    })
}

pub fn type_text(text: &str) -> Result<Value> {
    with_lock(|| call_python("type", &json!({"text": text})))
}

pub fn key(text: &str) -> Result<Value> {
    with_lock(|| call_python("key", &json!({"text": text})))
}

pub fn hold_key(text: &str, duration: f64) -> Result<Value> {
    with_lock(|| call_python("hold_key", &json!({"text": text, "duration": duration})))
}

pub fn open_application(application: &str, target: Option<&str>) -> Result<Value> {
    // Check allowlist (if non-empty)
    {
        let allowlist = ALLOWLIST.read();
        if !allowlist.is_empty() {
            let app_lower = application.to_lowercase();
            if !allowlist.iter().any(|a| a.to_lowercase() == app_lower) {
                return Err(anyhow!(
                    "Application '{}' is not in the allowlist. \
                     Use computer_request_access to grant permission.",
                    application
                ));
            }
        }
    }
    with_lock(|| {
        let mut payload = json!({"text": application});
        if let Some(t) = target {
            payload["target"] = json!(t);
        }
        call_python("open_application", &payload)
    })
}

pub fn switch_display() -> Result<Value> {
    with_lock(|| call_python("switch_display", &json!({})))
}

pub fn request_access(applications: &[String]) -> Result<Value> {
    {
        let mut allowlist = ALLOWLIST.write();
        for app in applications {
            if !allowlist.contains(app) {
                allowlist.push(app.clone());
            }
        }
    }
    with_lock(|| call_python("request_access", &json!({"applications": applications})))
}

pub fn list_granted_applications() -> Result<Value> {
    with_lock(|| call_python("list_granted_applications", &json!({})))
}

pub fn read_clipboard() -> Result<Value> {
    with_lock(|| call_python("read_clipboard", &json!({})))
}

pub fn write_clipboard(text: &str) -> Result<Value> {
    with_lock(|| call_python("write_clipboard", &json!({"text": text})))
}

pub fn wait(duration: f64) -> Result<Value> {
    let dur = duration.max(0.0).min(100.0);
    std::thread::sleep(std::time::Duration::from_secs_f64(dur));
    Ok(json!({"status": "ok", "waited": dur}))
}

pub fn computer_batch(actions: &Value) -> Result<Value> {
    with_lock(|| call_python("computer_batch", actions))
}

// ─── Backward-compat wrappers (match old tools.rs API) ──────────────────

/// Backward-compat: old `computer_action` unified dispatch.
pub fn computer_action(
    action: &str,
    x: Option<u64>,
    y: Option<u64>,
    text: Option<&str>,
    direction: Option<&str>,
    amount: Option<u64>,
    duration: Option<f64>,
) -> Result<Value> {
    match action {
        "open_app" | "open_application" => {
            let app = text.ok_or_else(|| anyhow!("text (application name) required"))?;
            open_application(app, None)
        }
        "type" => {
            let t = text.unwrap_or("");
            type_text(t)
        }
        "key" => {
            let k = text.unwrap_or("");
            key(k)
        }
        "scroll" => {
            let (cx, cy) = (
                x.ok_or_else(|| anyhow!("x,y required for scroll"))?,
                y.ok_or_else(|| anyhow!("x,y required for scroll"))?,
            );
            let dir = direction.unwrap_or("down");
            let amt = amount.unwrap_or(3);
            scroll(cx, cy, dir, amt)
        }
        "wait" => {
            let dur = duration.unwrap_or(1.0);
            wait(dur)
        }
        _ => {
            // Click/move actions requiring x,y
            let (cx, cy) = (
                x.ok_or_else(|| anyhow!("x,y required for {}", action))?,
                y.ok_or_else(|| anyhow!("x,y required for {}", action))?,
            );
            match action {
                "left_click" => left_click(cx, cy),
                "right_click" => right_click(cx, cy),
                "double_click" => double_click(cx, cy),
                "triple_click" => triple_click(cx, cy),
                "middle_click" => middle_click(cx, cy),
                "mouse_move" => mouse_move(cx, cy),
                "left_mouse_down" => left_mouse_down(cx, cy),
                "left_mouse_up" => left_mouse_up(cx, cy),
                _ => Err(anyhow!("Unknown computer action: {}", action)),
            }
        }
    }
}

/// Backward-compat: old `computer_open` API.
pub fn computer_open(
    application: Option<&str>,
    target: Option<&str>,
    _wait_timeout_ms: Option<u64>,
) -> Result<Value> {
    let app = application.unwrap_or("");
    if app.is_empty() && target.is_some() {
        return open_application(target.unwrap(), None);
    }
    open_application(app, target)
}

// ─── Public utilities ───────────────────────────────────────────────────

/// Quick check whether venv is ready (non-blocking, no bootstrap).
pub fn is_venv_ready() -> bool {
    let runtime_dir = project_dir().join("runtime");
    if cfg!(windows) {
        runtime_dir
            .join("venv")
            .join("Scripts")
            .join("python.exe")
            .exists()
    } else {
        runtime_dir.join("venv").join("bin").join("python").exists()
    }
}

/// Warm up the venv in background (call early to avoid first-call latency).
pub fn warm_venv() {
    std::thread::spawn(|| {
        let _ = get_python();
    });
}

// ─── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_system_python() {
        let _ = find_system_python();
    }

    #[test]
    fn test_validate_coords() {
        *SCREEN_DIMS.write() = Some((1920, 1080));
        assert!(validate_coords(Some(100), Some(200)).is_ok());
        // Out-of-bounds warns but doesn't error
        assert!(validate_coords(Some(3000), Some(3000)).is_ok());
    }

    #[test]
    fn test_platform_helper_exists() {
        let helper = platform_helper();
        assert!(helper.ends_with("_helper.py"));
    }

    #[test]
    fn test_wait() {
        let result = wait(0.01).unwrap();
        assert_eq!(result["status"], "ok");
    }

    #[test]
    fn test_empty_allowlist_allows_any() {
        ALLOWLIST.write().clear();
        // With empty allowlist, all apps are allowed (check happens in open_application)
    }

    #[test]
    fn test_allowlist_blocks_unknown() {
        *ALLOWLIST.write() = vec!["Chrome".to_string()];
        // This would be blocked (not "Chrome"), but we can't test
        // without a real Python bridge running
        assert_eq!(ALLOWLIST.read().len(), 1);
    }
}
