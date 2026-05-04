use anyhow::{anyhow, Context, Result};
use regex::Regex;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::workspace;

pub type JsonResult = Value;

lazy_static::lazy_static! {
    static ref TIMESTAMP_RE: Regex = Regex::new(r"\d{8}_\d{6}").unwrap();
    static ref FILE_REF_RE: Regex = Regex::new(r"\{\{file:([^:}]+)(?::(\d+))?(?::(\d+))?\}\}").unwrap();
    static ref RG_JSON_LINE_RE: Regex = Regex::new(
        r#"\{"type":"match","data":\{"path":\{"text":"(?P<file>[^"]+)"\},"lines":\{"text":"(?P<line>[^"]*)"\},"line_number":(?P<ln>\d+)\}\}"#
    ).unwrap();
}

const MAX_LINE_LEN: usize = 8000;
const BACKUP_DIR: &str = "temp/backups";

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

fn timestamp() -> String {
    chrono::Local::now().format("%Y%m%d_%H%M%S").to_string()
}

fn ensure_backup_dir() -> Result<PathBuf> {
    let dir = PathBuf::from(BACKUP_DIR);
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

fn access_root() -> Result<PathBuf> {
    let active = workspace::get_active_workspace();
    if active.get("status").and_then(|value| value.as_str()) == Some("success") {
        let root = active
            .get("path")
            .and_then(|value| value.as_str())
            .ok_or_else(|| anyhow!("Active workspace path is missing"))?;
        return fs::canonicalize(root)
            .with_context(|| format!("Cannot resolve active workspace root: {root}"));
    }

    fs::canonicalize(".").context("Cannot resolve current working directory")
}

fn canonicalize_for_access(path: &Path, allow_missing: bool) -> Result<PathBuf> {
    if !allow_missing || path.exists() {
        return fs::canonicalize(path)
            .with_context(|| format!("Cannot resolve path: {}", path.display()));
    }

    let parent = path
        .parent()
        .ok_or_else(|| anyhow!("Cannot resolve parent directory for {}", path.display()))?;
    let canonical_parent = fs::canonicalize(parent)
        .with_context(|| format!("Cannot resolve parent directory: {}", parent.display()))?;
    let file_name = path
        .file_name()
        .ok_or_else(|| anyhow!("Path is missing a file name: {}", path.display()))?;
    Ok(canonical_parent.join(file_name))
}

fn resolve_local_path(path: &str, allow_missing: bool) -> Result<PathBuf> {
    let requested = if Path::new(path).is_absolute() {
        PathBuf::from(path)
    } else {
        std::env::current_dir()
            .context("Cannot resolve current working directory")?
            .join(path)
    };

    let root = access_root()?;
    let resolved = canonicalize_for_access(&requested, allow_missing)?;
    if !resolved.starts_with(&root) {
        return Err(anyhow!(
            "Path is outside the active workspace: {}",
            requested.display()
        ));
    }
    Ok(resolved)
}

fn resolve_search_root(path: &str) -> Result<PathBuf> {
    let requested = if path.trim().is_empty() {
        access_root()?
    } else {
        resolve_local_path(path, false)?
    };

    if !requested.is_dir() {
        return Err(anyhow!("Not a directory: {}", requested.display()));
    }
    Ok(requested)
}

fn backup_path(file_path: &str, task_id: Option<&str>) -> Result<PathBuf> {
    let dir = ensure_backup_dir()?;
    // encode path: replace \ and / with _FS_
    let encoded = file_path
        .replace('\\', "_FS_")
        .replace('/', "_FS_")
        .replace(':', "_COLON_");
    let tid = task_id.unwrap_or("global");
    Ok(dir.join(format!("{}_{}_{}", encoded, tid, timestamp())))
}

fn which(cmd: &str) -> Option<PathBuf> {
    let mut c = Command::new(if cfg!(windows) { "where" } else { "which" })
        .arg(cmd)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let mut out = String::new();
    c.stdout.as_mut()?.read_to_string(&mut out).ok()?;
    let line = out.lines().next()?;
    let p = PathBuf::from(line.trim());
    if p.exists() {
        Some(p)
    } else {
        None
    }
}

fn find_python() -> Option<String> {
    for name in &["python3", "python"] {
        if which(name).is_some() {
            return Some(name.to_string());
        }
    }
    None
}

/// Truncate a line that exceeds max_len, inserting an omission marker.
fn truncate_line(line: &str, max_len: usize) -> String {
    if line.len() <= max_len {
        return line.to_string();
    }
    let head = &line[..max_len / 2];
    let tail = &line[line.len() - max_len / 2..];
    format!("{}…[TRUNCATED {}→{}]…{}", head, max_len, line.len(), tail)
}

// ---------------------------------------------------------------------------
// sequence matcher for fuzzy file-path matching
// ---------------------------------------------------------------------------

struct SequenceMatcher<'a> {
    a: &'a str,
    b: &'a str,
}

impl<'a> SequenceMatcher<'a> {
    fn ratio(s1: &str, s2: &str) -> f64 {
        let m = SequenceMatcher { a: s1, b: s2 };
        let matches = m.lcs_len() as f64;
        let total = (s1.chars().count() + s2.chars().count()) as f64;
        if total == 0.0 {
            1.0
        } else {
            2.0 * matches / total
        }
    }

    fn lcs_len(&self) -> usize {
        let a: Vec<char> = self.a.chars().collect();
        let b: Vec<char> = self.b.chars().collect();
        let n = a.len();
        let m = b.len();
        let mut prev = vec![0usize; m + 1];
        for i in 1..=n {
            let mut curr = vec![0usize; m + 1];
            for j in 1..=m {
                if a[i - 1] == b[j - 1] {
                    curr[j] = prev[j - 1] + 1;
                } else {
                    curr[j] = prev[j].max(curr[j - 1]);
                }
            }
            prev = curr;
        }
        prev[m]
    }
}

fn fuzzy_match_file(needle: &str) -> Option<PathBuf> {
    let needle_path = Path::new(needle);
    let filename = needle_path.file_name()?.to_str()?;
    let search_dir = needle_path.parent().unwrap_or(Path::new("."));

    let mut best: Option<(f64, PathBuf)> = None;

    if let Ok(entries) = fs::read_dir(search_dir) {
        for entry in entries.flatten() {
            let fname = entry.file_name();
            let fname_str = fname.to_string_lossy();
            let r = SequenceMatcher::ratio(filename, &fname_str);
            let full = search_dir.join(&fname);
            if r > 0.4 {
                if best.as_ref().map_or(true, |b| r > b.0) {
                    best = Some((r, full));
                }
            }
        }
    }

    // also try glob
    if best.is_none() {
        // try glob with prefix followed by wildcard
        let prefix = if filename.len() > 3 {
            &filename[..3]
        } else {
            filename
        };
        let pattern = format!("{}/*{}*", search_dir.display(), prefix);
        if let Ok(paths) = glob::glob(&pattern) {
            for p in paths.flatten() {
                let fns = p
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();
                let r = SequenceMatcher::ratio(filename, &fns);
                if r > 0.4 && best.as_ref().map_or(true, |b| r > b.0) {
                    best = Some((r, p));
                }
            }
        }
    }

    best.map(|(_, p)| p)
}

// ---------------------------------------------------------------------------
// 1. code_run
// ---------------------------------------------------------------------------

pub fn code_run(
    code: &str,
    code_type: &str,
    timeout: Option<u64>,
    cwd: Option<&str>,
    code_cwd: Option<&str>,
    stop_signal: Option<Arc<AtomicBool>>,
) -> Result<JsonResult> {
    let cwd = resolve_search_root(cwd.unwrap_or("."))?;
    let cwd_string = cwd.display().to_string();
    let code_cwd = resolve_search_root(code_cwd.unwrap_or(&cwd_string))?;
    let timeout_dur = timeout.map(Duration::from_secs);

    let (cmd, args, write_file) = match code_type.to_lowercase().as_str() {
        "python" | "py" => {
            let dir = code_cwd.clone();
            fs::create_dir_all(&dir).ok();
            let tmp = dir.join(format!("_gc_tmp_{}.py", timestamp()));
            fs::write(&tmp, code.as_bytes())?;
            let py = find_python().unwrap_or_else(|| "python".into());
            (py, vec![tmp.to_string_lossy().to_string()], Some(tmp))
        }
        "powershell" | "ps1" => (
            "powershell".into(),
            vec!["-Command".into(), code.to_string()],
            None,
        ),
        "bash" | "sh" => ("bash".into(), vec!["-c".into(), code.to_string()], None),
        other => return Err(anyhow!("Unsupported code type: {}", other)),
    };

    let proc_path = which(&cmd).unwrap_or_else(|| PathBuf::from(&cmd));

    let mut child = Command::new(proc_path)
        .args(&args)
        .current_dir(&cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("Failed to spawn {}", cmd))?;

    let start = Instant::now();
    loop {
        if let Some(sig) = stop_signal.as_ref() {
            if sig.load(Ordering::Relaxed) {
                let _ = child.kill();
                let output = child.wait_with_output()?;
                if let Some(tmp) = &write_file {
                    let _ = fs::remove_file(tmp);
                }
                let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                let combined = if stdout.is_empty() {
                    stderr
                } else if stderr.is_empty() {
                    stdout
                } else {
                    format!("{stdout}\n{stderr}")
                };
                return Ok(json!({
                    "status": "interrupted",
                    "stdout": combined,
                    "exit_code": -1
                }));
            }
        }

        if let Some(d) = timeout_dur {
            if start.elapsed() > d {
                let _ = child.kill();
                let output = child.wait_with_output()?;
                if let Some(tmp) = &write_file {
                    let _ = fs::remove_file(tmp);
                }
                let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                let combined = if stdout.is_empty() {
                    stderr
                } else if stderr.is_empty() {
                    stdout
                } else {
                    format!("{stdout}\n{stderr}")
                };
                return Ok(json!({
                    "status": "timeout",
                    "stdout": combined,
                    "exit_code": -1
                }));
            }
        }

        if child.try_wait()?.is_some() {
            let output = child.wait_with_output()?;
            if let Some(tmp) = &write_file {
                let _ = fs::remove_file(tmp);
            }
            let exit_code = output.status.code().unwrap_or(-1);
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            let combined = if stdout.is_empty() {
                stderr
            } else if stderr.is_empty() {
                stdout
            } else {
                format!("{stdout}\n{stderr}")
            };

            return Ok(json!({
                "status": "completed",
                "stdout": combined,
                "exit_code": exit_code
            }));
        }

        std::thread::sleep(Duration::from_millis(50));
    }
}

// ---------------------------------------------------------------------------
// 2. ask_user
// ---------------------------------------------------------------------------

pub fn ask_user(_question: &str, _candidates: Option<&[String]>) -> JsonResult {
    json!({
        "status": "INTERRUPT",
        "message": "ask_user requires user interaction"
    })
}

// ---------------------------------------------------------------------------
// 3. file_read
// ---------------------------------------------------------------------------

pub fn file_read(
    path: &str,
    start: Option<usize>,
    keyword: Option<&str>,
    count: Option<usize>,
    show_linenos: Option<bool>,
) -> Result<String> {
    let file_path = Path::new(path);
    let real_path = if file_path.exists() {
        resolve_local_path(path, false)?
    } else if let Some(found) = fuzzy_match_file(path) {
        resolve_local_path(&found.display().to_string(), false)?
    } else {
        return Err(anyhow!("File not found: {}", path));
    };

    let content = fs::read_to_string(&real_path)
        .with_context(|| format!("Cannot read {}", real_path.display()))?;
    let all_lines: Vec<&str> = content.lines().collect();
    let total = all_lines.len();
    let show_linenos = show_linenos.unwrap_or(true);

    let selected: Vec<(usize, String)> = if let Some(kw) = keyword {
        let kw_lower = kw.to_lowercase();
        all_lines
            .iter()
            .enumerate()
            .filter(|(_, l)| l.to_lowercase().contains(&kw_lower))
            .map(|(i, l)| (i + 1, truncate_line(l, MAX_LINE_LEN)))
            .collect()
    } else {
        if total == 0 {
            Vec::new()
        } else {
            let s = start.unwrap_or(1).max(1).min(total);
            let available = total.saturating_sub(s) + 1;
            let c = count.unwrap_or(available).min(available);
            all_lines[s - 1..s - 1 + c]
                .iter()
                .enumerate()
                .map(|(i, l)| (s + i, truncate_line(l, MAX_LINE_LEN)))
                .collect()
        }
    };

    let showing = selected.len();
    let header = if keyword.is_some() && showing < total {
        format!(
            "[FILE] {} — {} lines total | {} matching \"{}\"\n",
            real_path.display(),
            total,
            showing,
            keyword.unwrap()
        )
    } else if showing < total {
        format!(
            "[FILE] {} — {} lines | PARTIAL showing {}\n",
            real_path.display(),
            total,
            showing
        )
    } else {
        format!("[FILE] {} — {} lines\n", real_path.display(), total)
    };

    let mut out = String::with_capacity(header.len() + selected.len() * 100);
    out.push_str(&header);
    out.push('\n');
    for (lineno, text) in &selected {
        if show_linenos {
            out.push_str(&format!("{:6}: {}\n", lineno, text));
        } else {
            out.push_str(text);
            out.push('\n');
        }
    }
    if !out.ends_with('\n') {
        out.push('\n');
    }

    Ok(out)
}

// ---------------------------------------------------------------------------
// 4. file_patch
// ---------------------------------------------------------------------------

pub fn file_patch(path: &str, old_content: &str, new_content: &str) -> Result<JsonResult> {
    let file_path = resolve_local_path(path, false)?;
    let content = fs::read_to_string(&file_path)
        .with_context(|| format!("Cannot read {}", file_path.display()))?;

    let count = content.matches(old_content).count();
    if count == 0 {
        return Ok(json!({ "status": "error", "message": "Old content not found in file" }));
    }
    if count > 1 {
        return Ok(json!({
            "status": "error",
            "message": format!("Old content found {} times (expected exactly 1)", count)
        }));
    }

    // auto-backup
    file_backup(&file_path.display().to_string(), None);

    let new = content.replacen(old_content, new_content, 1);
    fs::write(&file_path, &new).with_context(|| format!("Cannot write {}", file_path.display()))?;

    Ok(json!({ "status": "ok", "message": "Patched successfully" }))
}

// ---------------------------------------------------------------------------
// 5. file_write
// ---------------------------------------------------------------------------

pub fn file_write(path: &str, content: &str, mode: Option<&str>) -> Result<JsonResult> {
    let mode = mode.unwrap_or("overwrite");
    let file_path = resolve_local_path(path, true)?;

    // auto-backup
    file_backup(&file_path.display().to_string(), None);

    match mode {
        "append" => {
            let mut f = fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&file_path)?;
            f.write_all(content.as_bytes())?;
        }
        "prepend" => {
            let existing = if file_path.exists() {
                fs::read_to_string(&file_path).unwrap_or_default()
            } else {
                String::new()
            };
            let mut f = fs::File::create(&file_path)?;
            f.write_all(content.as_bytes())?;
            if !existing.is_empty() {
                f.write_all(b"\n")?;
                f.write_all(existing.as_bytes())?;
            }
        }
        _ => {
            // overwrite
            if let Some(parent) = file_path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(&file_path, content.as_bytes())?;
        }
    }

    Ok(json!({ "status": "ok", "message": format!("Written {} bytes", content.len()) }))
}

// ---------------------------------------------------------------------------
// 6. content_search
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct SearchMatch {
    file: String,
    line: usize,
    content: String,
    context_before: Vec<String>,
    context_after: Vec<String>,
}

fn rg_search(
    pattern: &str,
    path: &str,
    glob_pat: Option<&str>,
    max_results: Option<usize>,
    case_sensitive: Option<bool>,
) -> Result<Vec<SearchMatch>> {
    let mut cmd = Command::new("rg");
    cmd.arg("--json")
        .arg("--no-heading")
        .arg("--with-filename")
        .arg("--line-number");

    if !case_sensitive.unwrap_or(false) {
        cmd.arg("--ignore-case");
    }
    if let Some(ref gp) = glob_pat {
        cmd.arg("--glob").arg(gp);
    }
    cmd.arg(pattern).arg(path);

    let output = cmd.output()?;
    if !output.status.success() && output.stdout.is_empty() {
        return Ok(Vec::new());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut matches = Vec::new();

    for line in stdout.lines() {
        if let Ok(v) = serde_json::from_str::<Value>(line) {
            if v.get("type").and_then(|t| t.as_str()) == Some("match") {
                if let Some(data) = v.get("data") {
                    let file = data
                        .get("path")
                        .and_then(|p| p.get("text"))
                        .and_then(|t| t.as_str())
                        .unwrap_or("")
                        .to_string();
                    let ln = data
                        .get("line_number")
                        .and_then(|n| n.as_u64())
                        .unwrap_or(1) as usize;
                    let content = data
                        .get("lines")
                        .and_then(|l| l.get("text"))
                        .and_then(|t| t.as_str())
                        .unwrap_or("")
                        .trim_end()
                        .to_string();

                    matches.push(SearchMatch {
                        file,
                        line: ln,
                        content,
                        context_before: Vec::new(),
                        context_after: Vec::new(),
                    });
                }
            }
        }
    }

    if let Some(limit) = max_results {
        matches.truncate(limit);
    }

    Ok(matches)
}

fn rust_search(
    pattern: &str,
    path: &str,
    glob_pat: Option<&str>,
    max_results: Option<usize>,
    case_sensitive: Option<bool>,
) -> Result<Vec<SearchMatch>> {
    let case_insensitive = !case_sensitive.unwrap_or(false);
    let re = if case_insensitive {
        Regex::new(&format!("(?i){}", pattern))?
    } else {
        Regex::new(pattern)?
    };

    let glob_str = glob_pat.unwrap_or("**/*");
    let full_glob = format!("{}/{}", path, glob_str);
    let mut matches = Vec::new();

    if let Ok(paths) = glob::glob(&full_glob) {
        for entry in paths.flatten() {
            if !entry.is_file() {
                continue;
            }
            let file_content = match fs::read_to_string(&entry) {
                Ok(c) => c,
                Err(_) => continue,
            };
            for (i, line) in file_content.lines().enumerate() {
                if re.is_match(line) {
                    matches.push(SearchMatch {
                        file: entry.to_string_lossy().to_string(),
                        line: i + 1,
                        content: truncate_line(line, MAX_LINE_LEN),
                        context_before: Vec::new(),
                        context_after: Vec::new(),
                    });
                    if let Some(limit) = max_results {
                        if matches.len() >= limit {
                            break;
                        }
                    }
                }
            }
            if let Some(limit) = max_results {
                if matches.len() >= limit {
                    break;
                }
            }
        }
    }

    Ok(matches)
}

fn add_context(matches: &mut [SearchMatch], context_lines: usize) {
    // For each match, read the file and collect context lines around the match
    let mut file_cache: HashMap<String, Vec<String>> = HashMap::new();
    for m in matches.iter_mut() {
        let lines = file_cache.entry(m.file.clone()).or_insert_with(|| {
            fs::read_to_string(&m.file)
                .map(|c| c.lines().map(String::from).collect())
                .unwrap_or_default()
        });

        let start = m.line.saturating_sub(context_lines + 1);
        let end = (m.line + context_lines).min(lines.len());
        m.context_before = lines[start..m.line.saturating_sub(1)].to_vec();
        m.context_after = lines[m.line..end].to_vec();
    }
}

pub fn content_search(
    pattern: &str,
    path: &str,
    glob_pat: Option<&str>,
    context_lines: Option<usize>,
    max_results: Option<usize>,
    case_sensitive: Option<bool>,
) -> Result<JsonResult> {
    let ctxt = context_lines.unwrap_or(0);
    let root = resolve_search_root(path)?;
    let root_str = root.display().to_string();

    let mut matches = match rg_search(pattern, &root_str, glob_pat, max_results, case_sensitive) {
        Ok(m) if !m.is_empty() => m,
        _ => rust_search(pattern, &root_str, glob_pat, max_results, case_sensitive)?,
    };

    if ctxt > 0 {
        add_context(&mut matches, ctxt);
    }

    let results: Vec<Value> = matches
        .into_iter()
        .map(|m| {
            json!({
                "file": m.file,
                "line": m.line,
                "content": m.content,
                "context_before": m.context_before,
                "context_after": m.context_after,
            })
        })
        .collect();

    Ok(json!({
        "status": "ok",
        "count": results.len(),
        "results": results
    }))
}

// ---------------------------------------------------------------------------
// 7. git_status
// ---------------------------------------------------------------------------

pub fn git_status(path: Option<&str>) -> Result<JsonResult> {
    let output = Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(path.unwrap_or("."))
        .output()
        .with_context(|| "Failed to run git status")?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let files: Vec<Value> = stdout
        .lines()
        .filter(|l| !l.is_empty())
        .map(|l| {
            let status = &l[0..2];
            let file = l[3..].trim();
            json!({
                "status": status,
                "file": file
            })
        })
        .collect();

    Ok(json!({
        "status": "ok",
        "count": files.len(),
        "files": files
    }))
}

// ---------------------------------------------------------------------------
// 8. git_diff
// ---------------------------------------------------------------------------

pub fn git_diff(
    staged: Option<bool>,
    path: Option<&str>,
    path_repo: Option<&str>,
) -> Result<JsonResult> {
    let repo = path_repo.unwrap_or(".");
    let mut args = vec!["diff"];
    if staged.unwrap_or(false) {
        args = vec!["diff", "--cached"];
    }
    if let Some(p) = path {
        args.push(p);
    }

    let output = Command::new("git")
        .args(&args)
        .current_dir(repo)
        .output()
        .with_context(|| "Failed to run git diff")?;

    let diff = String::from_utf8_lossy(&output.stdout).to_string();

    Ok(json!({
        "status": "ok",
        "diff": diff
    }))
}

// ---------------------------------------------------------------------------
// 9. git_log
// ---------------------------------------------------------------------------

pub fn git_log(count: Option<usize>, path_repo: Option<&str>) -> Result<JsonResult> {
    let repo = path_repo.unwrap_or(".");
    let n = count.unwrap_or(10).to_string();

    let output = Command::new("git")
        .args(["log", "--oneline", "-n", &n])
        .current_dir(repo)
        .output()
        .with_context(|| "Failed to run git log")?;

    let log_text = String::from_utf8_lossy(&output.stdout).to_string();
    let entries: Vec<Value> = log_text
        .lines()
        .filter(|l| !l.is_empty())
        .map(|l| {
            let parts: Vec<&str> = l.splitn(2, ' ').collect();
            json!({
                "hash": parts.first().unwrap_or(&""),
                "message": parts.get(1).unwrap_or(&"").trim(),
            })
        })
        .collect();

    Ok(json!({
        "status": "ok",
        "count": entries.len(),
        "entries": entries
    }))
}

// ---------------------------------------------------------------------------
// 10. file_backup
// ---------------------------------------------------------------------------

pub fn file_backup(file_path: &str, task_id: Option<&str>) -> Option<String> {
    let resolved = resolve_local_path(file_path, false).ok()?;
    let src = resolved.as_path();
    if !src.exists() {
        return None;
    }
    let content = fs::read(src).ok()?;
    let dst = backup_path(&resolved.display().to_string(), task_id).ok()?;
    fs::write(&dst, content).ok()?;
    Some(dst.to_string_lossy().to_string())
}

// ---------------------------------------------------------------------------
// 11. file_revert
// ---------------------------------------------------------------------------

pub fn file_revert(file_path: &str, task_id: Option<&str>) -> Result<JsonResult> {
    let resolved_target = resolve_local_path(file_path, true)?;
    let dir = ensure_backup_dir()?;
    let tid = task_id.unwrap_or("global");
    let encoded = resolved_target
        .display()
        .to_string()
        .replace('\\', "_FS_")
        .replace('/', "_FS_")
        .replace(':', "_COLON_");
    let prefix = format!("{}_{}", encoded, tid);

    // find most recent backup with this prefix
    let mut best: Option<(String, PathBuf)> = None;
    for entry in fs::read_dir(&dir)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with(&prefix) {
            let ts = name.split('_').rev().take(2).collect::<Vec<_>>();
            if ts.len() >= 2 {
                let ts_str = format!("{}_{}", ts[1], ts[0]);
                if best.as_ref().map_or(true, |b| ts_str > b.0) {
                    best = Some((ts_str, entry.path()));
                }
            } else if best.is_none() {
                best = Some((String::new(), entry.path()));
            }
        }
    }

    match best {
        Some((_, bp)) => {
            let content = fs::read(&bp)?;
            if let Some(parent) = resolved_target.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(&resolved_target, content)?;
            Ok(json!({ "status": "ok", "message": format!("Restored from {}", bp.display()) }))
        }
        None => Ok(json!({ "status": "error", "message": "No backup found" })),
    }
}

// ---------------------------------------------------------------------------
// 12. expand_file_refs
// ---------------------------------------------------------------------------

pub fn expand_file_refs(text: &str, base_dir: Option<&str>) -> Result<String> {
    let base = base_dir.unwrap_or(".");
    let base_path = Path::new(base);

    let mut result = text.to_string();
    let mut replacements: Vec<(usize, usize, String)> = Vec::new();

    for cap in FILE_REF_RE.captures_iter(text) {
        let full_match = cap.get(0).unwrap();
        let file_path = cap.get(1).unwrap().as_str();
        let start_line: Option<usize> = cap.get(2).map(|m| m.as_str().parse().unwrap_or(1));
        let end_line: Option<usize> = cap.get(3).map(|m| m.as_str().parse().unwrap_or(usize::MAX));

        let full_path = if Path::new(file_path).is_absolute() {
            PathBuf::from(file_path)
        } else {
            base_path.join(file_path)
        };

        let resolved_path = match resolve_local_path(&full_path.display().to_string(), false) {
            Ok(path) => path,
            Err(_) => continue,
        };

        let content = match fs::read_to_string(&resolved_path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        let replaced = if start_line.is_some() || end_line.is_some() {
            let lines: Vec<&str> = content.lines().collect();
            if lines.is_empty() {
                String::new()
            } else {
                let s = start_line.unwrap_or(1).max(1).min(lines.len() + 1);
                let e = end_line.unwrap_or(lines.len()).min(lines.len());
                if s > e || s > lines.len() {
                    String::new()
                } else {
                    lines[s - 1..e].join("\n")
                }
            }
        } else {
            content
        };

        replacements.push((full_match.start(), full_match.end(), replaced));
    }

    // apply in reverse order to preserve indices
    replacements.sort_by_key(|(start, _, _)| *start);
    replacements.reverse();
    for (start, end, repl) in replacements {
        result.replace_range(start..end, &repl);
    }

    Ok(result)
}

// ---------------------------------------------------------------------------
// 13. smart_format
// ---------------------------------------------------------------------------

pub fn smart_format(data: &Value, max_str_len: Option<usize>, omit_str: Option<&str>) -> String {
    let max_len = max_str_len.unwrap_or(MAX_LINE_LEN);
    let omit = omit_str.unwrap_or("…[TRUNCATED]…");
    do_format(data, max_len, omit, 0)
}

fn do_format(data: &Value, max_len: usize, omit: &str, depth: usize) -> String {
    if depth > 20 {
        return "[…]".to_string();
    }
    match data {
        Value::Null => "null".to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        Value::String(s) => {
            if s.len() > max_len {
                let half = max_len / 2;
                format!("{}…{}…{}", &s[..half], omit, &s[s.len() - half..])
            } else {
                s.clone()
            }
        }
        Value::Array(arr) => {
            let items: Vec<String> = arr
                .iter()
                .take(50)
                .map(|v| do_format(v, max_len, omit, depth + 1))
                .collect();
            let _tail = if arr.len() > 50 { ", …" } else { "" };
            format!("[{}]", items.join(", "))
        }
        Value::Object(obj) => {
            let items: Vec<String> = obj
                .iter()
                .take(50)
                .map(|(k, v)| format!("{}: {}", k, do_format(v, max_len, omit, depth + 1)))
                .collect();
            let _tail = if obj.len() > 50 { ", …" } else { "" };
            format!("{{{}}}", items.join(", "))
        }
    }
}

// ---------------------------------------------------------------------------
// 14. consume_file
// ---------------------------------------------------------------------------

pub fn consume_file(dir: &str, filename: &str) -> Option<String> {
    let path = resolve_local_path(&Path::new(dir).join(filename).display().to_string(), false).ok()?;
    let content = fs::read_to_string(&path).ok()?;
    let _ = fs::remove_file(&path);
    Some(content)
}

// ---------------------------------------------------------------------------
// 15. get_global_memory
// ---------------------------------------------------------------------------

pub fn get_global_memory() -> String {
    let mut out = String::new();
    // read ~/.gcmem or project-level .gcmem
    let candidates = [
        dirs::home_dir().map(|p| p.join(".gcmem")),
        Some(PathBuf::from("temp/gcmem")),
        Some(PathBuf::from(".gcmem")),
    ];

    for cand in candidates.iter().flatten() {
        if let Ok(content) = fs::read_to_string(cand) {
            if !content.trim().is_empty() {
                out.push_str(&format!("# {}\n", cand.display()));
                out.push_str(&content);
                out.push('\n');
            }
        }
    }

    if out.is_empty() {
        out = "No global memory found.".to_string();
    }
    out
}

// ---------------------------------------------------------------------------
// 16. format_error
// ---------------------------------------------------------------------------

pub fn format_error(e: &anyhow::Error) -> String {
    let chain: Vec<String> = e.chain().map(|c| c.to_string()).collect();
    if chain.len() > 1 {
        chain.join("\n  caused by: ")
    } else {
        e.to_string()
    }
}

// ---------------------------------------------------------------------------
// 17. web_scan (stub)
// ---------------------------------------------------------------------------

pub fn web_scan(
    tabs_only: Option<bool>,
    _switch_tab_id: Option<&str>,
    _text_only: Option<bool>,
) -> Result<JsonResult> {
    // delegates to webdriver module
    Ok(json!({
        "status": "ok",
        "tabs": [],
        "message": if tabs_only.unwrap_or(false) { "tab scan" } else { "page scan" }
    }))
}

// ---------------------------------------------------------------------------
// 18. web_execute_js (stub)
// ---------------------------------------------------------------------------

pub fn web_execute_js(
    _script: &str,
    _switch_tab_id: Option<&str>,
    _no_monitor: Option<bool>,
) -> Result<JsonResult> {
    // delegates to webdriver module
    Ok(json!({
        "status": "ok",
        "result": null,
        "message": "web_execute_js stub — wire to webdriver module"
    }))
}

// ---------------------------------------------------------------------------
// tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sequence_matcher_ratio() {
        let r = SequenceMatcher::ratio("hello_world.txt", "hello_world.rs");
        assert!(r > 0.6);
        let r2 = SequenceMatcher::ratio("foo", "bar");
        assert!(r2 < 0.5);
    }

    #[test]
    fn test_smart_format_truncates() {
        let long = "a".repeat(9000);
        let data = json!({ "key": long });
        let out = smart_format(&data, Some(8000), None);
        assert!(out.contains("TRUNCATED"));
    }

    #[test]
    fn test_expand_file_refs() {
        // just test no-panic on missing file
        let result = expand_file_refs("{{file:/nonexistent_path_xyz}}", None);
        // missing files are silently skipped
        assert!(result.is_ok());
    }

    #[test]
    fn test_expand_file_refs_handles_empty_file_range() {
        let tmp = "temp/test_expand_empty.txt";
        let _ = file_write(tmp, "", Some("overwrite"));
        let result = expand_file_refs(&format!("{{{{file:{tmp}:2:4}}}}"), None).unwrap();
        assert_eq!(result, "");
        let _ = fs::remove_file(tmp);
    }

    #[test]
    fn test_file_write_and_read() {
        let tmp = "temp/test_tools_write.txt";
        let _ = file_write(tmp, "hello world", Some("overwrite"));
        let out = file_read(tmp, None, None, None, Some(false)).unwrap();
        assert!(out.contains("hello world"));
        let _ = fs::remove_file(tmp);
    }

    #[test]
    fn test_file_patch_unique() {
        let tmp = "temp/test_tools_patch.txt";
        let _ = file_write(tmp, "aaa\nbbb\nccc", Some("overwrite"));
        let r = file_patch(tmp, "bbb", "BBB").unwrap();
        assert_eq!(r["status"], "ok");
        let out = file_read(tmp, None, None, None, Some(false)).unwrap();
        assert!(out.contains("BBB"));
        let _ = fs::remove_file(tmp);
    }

    #[test]
    fn test_file_patch_nonunique() {
        let tmp = "temp/test_tools_patch2.txt";
        let _ = file_write(tmp, "aaa\naaa\nbbb", Some("overwrite"));
        let r = file_patch(tmp, "aaa", "AAA").unwrap();
        assert_eq!(r["status"], "error");
        assert!(r["message"].as_str().unwrap().contains("2 times"));
        let _ = fs::remove_file(tmp);
    }

    #[test]
    fn test_consume_file() {
        let dir = "temp";
        let file = "test_consume.txt";
        let _ = file_write("temp/test_consume.txt", "consumed", Some("overwrite"));
        let content = consume_file(dir, file);
        assert_eq!(content, Some("consumed".to_string()));
        assert!(!Path::new("temp/test_consume.txt").exists());
    }

    #[test]
    fn test_ask_user() {
        let r = ask_user("Are you sure?", None);
        assert_eq!(r["status"], "INTERRUPT");
    }

    #[test]
    fn test_format_error() {
        let e = anyhow!("outer").context("inner");
        let s = format_error(&e);
        assert!(s.contains("outer"));
        assert!(s.contains("inner"));
    }

    #[test]
    fn test_smart_format_deep_nested() {
        let mut v = json!(null);
        for _ in 0..21 {
            v = json!({ "nested": v });
        }
        let out = smart_format(&v, Some(1000), None);
        assert!(out.contains("[…]"));
    }

    #[test]
    fn test_web_stubs() {
        let r = web_scan(Some(true), None, None).unwrap();
        assert_eq!(r["status"], "ok");
        let r = web_execute_js("console.log(1)", None, None).unwrap();
        assert_eq!(r["status"], "ok");
    }

    #[test]
    fn test_git_status_no_repo() {
        // Should not panic, may return error or empty
        let r = git_status(Some("nonexistent_dir"));
        // Expect error since dir doesn't exist
        assert!(r.is_err());
    }

    #[test]
    fn test_file_read_keyword() {
        let tmp = "temp/test_tools_kw.txt";
        let _ = file_write(tmp, "line one\nline two\nline three\n", Some("overwrite"));
        let out = file_read(tmp, None, Some("two"), None, Some(true)).unwrap();
        assert!(out.contains("line two"));
        assert!(!out.contains("line one"));
        let _ = fs::remove_file(tmp);
    }

    #[test]
    fn test_file_read_blocks_outside_root() {
        let external = std::env::temp_dir().join("generic_coder_tools_outside_read.txt");
        let _ = fs::write(&external, "outside");
        let err = file_read(&external.display().to_string(), None, None, None, Some(false))
            .unwrap_err()
            .to_string();
        assert!(err.contains("outside the active workspace"));
        let _ = fs::remove_file(external);
    }

    #[test]
    fn test_file_write_blocks_outside_root() {
        let external = std::env::temp_dir().join("generic_coder_tools_outside_write.txt");
        let err = file_write(&external.display().to_string(), "outside", Some("overwrite"))
            .unwrap_err()
            .to_string();
        assert!(err.contains("outside the active workspace"));
    }

    #[test]
    fn test_file_read_empty_file_and_large_start() {
        let tmp = "temp/test_tools_empty.txt";
        let _ = file_write(tmp, "", Some("overwrite"));
        let out = file_read(tmp, Some(999), None, Some(20), Some(true)).unwrap();
        assert!(out.contains("0 lines"));
        let _ = fs::remove_file(tmp);
    }

    #[test]
    fn test_file_revert_none() {
        let r = file_revert("nonexistent_file_xyz_123", None).unwrap();
        assert_eq!(r["status"], "error");
    }

    #[test]
    fn test_backup_path_encoding() {
        let bp = backup_path("src\\main.rs", Some("task1")).unwrap();
        let s = bp.to_string_lossy();
        assert!(s.contains("_FS_"));
        assert!(s.contains("task1"));
    }
}
