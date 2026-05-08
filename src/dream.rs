//! Dream Memory Consolidation
//!
//! After each meaningful session ends, automatically extracts key facts
//! (files changed, commands run, errors hit, user intent) and writes a
//! lightweight JSON snapshot to `memory/dreams/dream_<ts>.json`.
//!
//! On the next session, `recent_context()` returns a concise summary string
//! that can be prepended to the system prompt — giving the agent "memory"
//! of what happened in previous sessions without consuming extra tokens.
//!
//! Design constraints:
//! - Pure rule-based extraction, zero LLM calls, zero extra token cost.
//! - Only records sessions where actual code files were changed (≥1 file_write
//!   or file_patch), or sessions lasting ≥3 turns — skips trivial Q&A chat.
//! - Max 20 dream files per project (oldest pruned automatically).
//! - Injection budget: at most 5 recent entries × ~250 chars ≈ ~1 250 chars.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::Value;

const MAX_DREAMS_PER_PROJECT: usize = 20;

/// A single condensed memory of one agent session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DreamEntry {
    /// ISO 8601 timestamp of when the session ended.
    pub timestamp: String,
    /// Truncated user intent (first user message, max 200 chars).
    pub intent: String,
    /// Unique file paths that were written or patched during the session.
    pub files_changed: Vec<String>,
    /// Up to 3 representative shell/code commands (max 100 chars each).
    pub commands_run: Vec<String>,
    /// Number of agent turns in this session.
    pub turns: usize,
    /// Number of tool calls that returned `status: error`.
    pub errors_encountered: usize,
    /// Condensed session outcome (last assistant message, max 300 chars).
    pub outcome: String,
}

/// Manages dream files for a single project directory.
pub struct DreamStore {
    dreams_dir: PathBuf,
}

impl DreamStore {
    pub fn new(project_dir: &Path) -> Self {
        Self {
            dreams_dir: project_dir.join("memory").join("dreams"),
        }
    }

    fn ensure_dir(&self) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.dreams_dir)
    }

    /// Persist a dream entry to disk.
    pub fn save(&self, entry: &DreamEntry) -> Result<(), String> {
        self.ensure_dir()
            .map_err(|e| format!("dream dir create failed: {e}"))?;

        // Build a safe filename from the timestamp, e.g. dream_20250101_120000.json
        let ts_safe: String = entry
            .timestamp
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
            .take(19)
            .collect::<String>()
            .replace('-', "")
            .chars()
            .enumerate()
            .map(|(i, c)| if i == 8 { '_' } else { c })
            .collect();
        let filename = format!("dream_{}.json", ts_safe);
        let path = self.dreams_dir.join(&filename);

        let data =
            serde_json::to_string_pretty(entry).map_err(|e| format!("serialize dream: {e}"))?;
        std::fs::write(&path, data).map_err(|e| format!("write dream: {e}"))?;

        self.prune();
        Ok(())
    }

    /// Remove oldest dreams beyond the per-project cap.
    fn prune(&self) {
        let Ok(mut paths) = self.list_paths() else {
            return;
        };
        if paths.len() > MAX_DREAMS_PER_PROJECT {
            paths.sort(); // lexicographic = chronological for our filenames
            let excess = paths.len() - MAX_DREAMS_PER_PROJECT;
            for path in paths.into_iter().take(excess) {
                let _ = std::fs::remove_file(path);
            }
        }
    }

    fn list_paths(&self) -> Result<Vec<PathBuf>, String> {
        if !self.dreams_dir.exists() {
            return Ok(vec![]);
        }
        let paths: Vec<PathBuf> = std::fs::read_dir(&self.dreams_dir)
            .map_err(|e| e.to_string())?
            .filter_map(|e| e.ok())
            .filter(|e| {
                let name = e.file_name();
                let s = name.to_string_lossy();
                s.starts_with("dream_") && s.ends_with(".json")
            })
            .map(|e| e.path())
            .collect();
        Ok(paths)
    }

    /// Return the `max` most-recent dream entries (newest first).
    pub fn recent(&self, max: usize) -> Vec<DreamEntry> {
        let Ok(mut paths) = self.list_paths() else {
            return vec![];
        };
        paths.sort();
        paths.reverse(); // newest first
        paths
            .into_iter()
            .take(max)
            .filter_map(|p| {
                std::fs::read_to_string(&p)
                    .ok()
                    .and_then(|s| serde_json::from_str(&s).ok())
            })
            .collect()
    }
}

/// Extract key events from a session's tool log and persist a dream file.
///
/// `tool_events` is a flat list of `(tool_name, args, result)` tuples
/// accumulated across all turns of the session.
///
/// This function is designed to be called from `tokio::spawn` (fire-and-forget)
/// so it never blocks the session exit path.
pub fn consolidate(
    project_dir: &Path,
    user_intent: &str,
    tool_events: &[(String, Value, Value)],
    turns: usize,
    outcome: &str,
) {
    // --- significance gate ---------------------------------------------------
    // Count file-writing events
    let write_count = tool_events
        .iter()
        .filter(|(name, _, _)| name == "file_write" || name == "file_patch")
        .count();

    // Skip trivial sessions: no file writes AND fewer than 3 turns
    if write_count == 0 && turns < 3 {
        return;
    }

    // --- extract files changed -----------------------------------------------
    let files_changed: Vec<String> = {
        let mut seen: HashSet<String> = HashSet::new();
        tool_events
            .iter()
            .filter(|(name, _, _)| name == "file_write" || name == "file_patch")
            .filter_map(|(_, args, _)| {
                args.get("path")
                    .or_else(|| args.get("file_path"))
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
            })
            .filter(|s| seen.insert(s.clone()))
            .collect()
    };

    // --- extract representative commands ------------------------------------
    let commands_run: Vec<String> = tool_events
        .iter()
        .filter(|(name, _, _)| name == "code_run")
        .filter_map(|(_, args, _)| {
            args.get("command")
                .or_else(|| args.get("code"))
                .and_then(|v| v.as_str())
                .map(|s| {
                    // Trim to first line and cap at 100 chars
                    let first_line = s.lines().next().unwrap_or(s);
                    let truncated: String = first_line.chars().take(100).collect();
                    truncated
                })
        })
        .take(3)
        .collect();

    // --- count errors --------------------------------------------------------
    let errors_encountered = tool_events
        .iter()
        .filter(|(_, _, result)| {
            result
                .get("status")
                .and_then(|v| v.as_str())
                .map(|s| s == "error")
                .unwrap_or(false)
        })
        .count();

    // --- build and save entry ------------------------------------------------
    let entry = DreamEntry {
        timestamp: Utc::now().to_rfc3339(),
        intent: user_intent.chars().take(200).collect(),
        files_changed,
        commands_run,
        turns,
        errors_encountered,
        outcome: outcome.chars().take(300).collect(),
    };

    let store = DreamStore::new(project_dir);
    match store.save(&entry) {
        Ok(()) => log::info!(
            "[Dream] Session memory saved: {} file(s) changed, {} turn(s)",
            entry.files_changed.len(),
            entry.turns
        ),
        Err(e) => log::warn!("[Dream] Failed to save session memory: {e}"),
    }
}

/// Return a formatted string summarising recent sessions for system prompt injection.
///
/// Returns an empty string if no dreams exist (no overhead).
/// Budget: ≤5 entries × ~250 chars ≈ ~1 250 chars.
pub fn recent_context(project_dir: &Path, max_entries: usize) -> String {
    let store = DreamStore::new(project_dir);
    let dreams = store.recent(max_entries);
    if dreams.is_empty() {
        return String::new();
    }

    let mut lines = vec!["\n## Recent Session Memory\n".to_string()];
    for d in &dreams {
        let date = if d.timestamp.len() >= 10 {
            &d.timestamp[..10]
        } else {
            &d.timestamp
        };
        lines.push(format!("**[{}]** {}", date, d.intent));
        if !d.files_changed.is_empty() {
            lines.push(format!("  Changed: {}", d.files_changed.join(", ")));
        }
        if !d.commands_run.is_empty() {
            lines.push(format!("  Ran: {}", d.commands_run.join(" | ")));
        }
        if !d.outcome.is_empty() {
            lines.push(format!("  Outcome: {}", d.outcome));
        }
    }
    lines.push(String::new());
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_consolidate_trivial_session_skipped() {
        let dir = std::env::temp_dir().join("gc_dream_trivial");
        let _ = std::fs::remove_dir_all(&dir);
        consolidate(&dir, "hello", &[], 1, "");
        assert!(!dir.join("memory").join("dreams").exists());
    }

    #[test]
    fn test_consolidate_writes_dream_for_file_write() {
        let dir = std::env::temp_dir().join("gc_dream_fw");
        let _ = std::fs::remove_dir_all(&dir);
        let events = vec![(
            "file_write".to_string(),
            json!({"path": "src/lib.rs", "content": "..."}),
            json!({"status": "ok"}),
        )];
        consolidate(&dir, "add a new function", &events, 2, "Done");
        let dreams_dir = dir.join("memory").join("dreams");
        let files: Vec<_> = std::fs::read_dir(&dreams_dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .collect();
        assert_eq!(files.len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_consolidate_long_session_without_writes() {
        let dir = std::env::temp_dir().join("gc_dream_long");
        let _ = std::fs::remove_dir_all(&dir);
        // 3 turns, no file writes — should still be saved
        consolidate(&dir, "explain rust lifetimes", &[], 3, "Explained");
        let dreams_dir = dir.join("memory").join("dreams");
        assert!(dreams_dir.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_recent_context_empty_when_no_dreams() {
        let dir = std::env::temp_dir().join("gc_dream_empty");
        let _ = std::fs::remove_dir_all(&dir);
        let ctx = recent_context(&dir, 5);
        assert!(ctx.is_empty());
    }

    #[test]
    fn test_recent_context_includes_intent() {
        let dir = std::env::temp_dir().join("gc_dream_ctx");
        let _ = std::fs::remove_dir_all(&dir);
        let events = vec![(
            "file_write".to_string(),
            json!({"path": "src/main.rs"}),
            json!({"status": "ok"}),
        )];
        consolidate(&dir, "refactor the main module", &events, 4, "Refactored OK");
        let ctx = recent_context(&dir, 5);
        assert!(ctx.contains("refactor the main module"));
        assert!(ctx.contains("src/main.rs"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_prune_keeps_max_entries() {
        let dir = std::env::temp_dir().join("gc_dream_prune");
        let _ = std::fs::remove_dir_all(&dir);
        let store = DreamStore::new(&dir);
        // Write 25 entries
        for i in 0..25u32 {
            let ts = format!("2025-01-{:02}T12:00:00Z", (i % 28) + 1);
            let entry = DreamEntry {
                timestamp: ts,
                intent: format!("intent {i}"),
                files_changed: vec![],
                commands_run: vec![],
                turns: 2,
                errors_encountered: 0,
                outcome: String::new(),
            };
            store.save(&entry).unwrap();
        }
        let dreams = store.recent(100);
        assert!(dreams.len() <= MAX_DREAMS_PER_PROJECT);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
