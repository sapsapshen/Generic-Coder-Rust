use std::collections::HashMap;
use std::path::{Path, PathBuf};

use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Severity level of a recorded error
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ErrorSeverity {
    #[serde(rename = "critical")]
    Critical, // agent loop failure, LLM crash, unrecoverable
    #[serde(rename = "tool")]
    Tool, // tool execution failure (file_read, code_run, etc.)
    #[serde(rename = "system")]
    System, // filesystem, IO, config — not agent's fault
    #[serde(rename = "validation")]
    Validation, // user input validation, invalid args
    #[serde(rename = "unknown")]
    Unknown,
}

/// A single recorded error experience
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorRecord {
    /// Stable fingerprint: tool_name:error_category (e.g., "file_read:not_found")
    pub fingerprint: String,
    /// Human-readable error summary
    pub summary: String,
    /// Which tool produced the error (or "agent_loop", "llm_call")
    pub tool: String,
    /// The error message (truncated to 500 chars)
    pub message: String,
    /// Arguments or context at time of error
    #[serde(default)]
    pub context: Value,
    /// Severity
    pub severity: ErrorSeverity,
    /// How many times this fingerprint has been seen
    #[serde(default = "default_one")]
    pub count: usize,
    /// When first seen (ISO 8601)
    pub first_seen: String,
    /// When last seen (ISO 8601)
    pub last_seen: String,
    /// The LLM model that was active when the error occurred
    #[serde(default)]
    pub model: String,
    /// The turn number when the error occurred
    #[serde(default)]
    pub turn: usize,
    /// Suggested avoidance hint — injected into system prompt
    #[serde(default)]
    pub avoidance_hint: String,
}

fn default_one() -> usize {
    1
}

impl ErrorRecord {
    /// Build a stable fingerprint from tool name and error category
    pub fn fingerprint(tool: &str, category: &str) -> String {
        format!("{}:{}", tool, category)
    }

    /// Classify an error message into a category for fingerprinting
    pub fn classify(message: &str) -> &str {
        let lower = message.to_lowercase();
        if lower.contains("not found") || lower.contains("no such file") {
            "not_found"
        } else if lower.contains("permission denied") || lower.contains("access denied") {
            "permission"
        } else if lower.contains("timeout") || lower.contains("timed out") {
            "timeout"
        } else if lower.contains("connection") || lower.contains("refused") {
            "connection"
        } else if lower.contains("parse")
            || lower.contains("invalid")
            || lower.contains("malformed")
        {
            "parse_error"
        } else if lower.contains("http") && lower.contains("error") {
            "http_error"
        } else if lower.contains("rate limit") || lower.contains("429") {
            "rate_limit"
        } else if lower.contains("out of memory") || lower.contains("oom") {
            "oom"
        } else if lower.contains("panic") || lower.contains("unwrapped") {
            "panic"
        } else {
            "general"
        }
    }

    /// Generate an avoidance hint based on the error fingerprint
    pub fn avoidance_hint(tool: &str, category: &str, message: &str) -> String {
        match (tool, category) {
            ("file_read", "not_found") => {
                "Before file_read, verify the path exists using workspace_list or check parent directory first.".into()
            }
            ("file_patch", "not_found") => {
                "Before file_patch, always file_read the target file to get current line numbers and verify content.".into()
            }
            ("file_write", "permission") => {
                "Check write permissions on the target directory. Use workspace_list to verify the path is writable.".into()
            }
            ("code_run", "timeout") => {
                "Code timed out. Consider breaking into smaller steps, increasing timeout, or running with inline_eval for quick checks.".into()
            }
            ("code_run", "general") => {
                let _ = message;
                "Code execution failed. Read the error output carefully — check imports, syntax, and runtime environment before retrying.".into()
            }
            (_, "not_found") => {
                format!("Double-check that all file paths and resource references exist before calling {tool}.")
            }
            (_, "permission") => {
                format!("Verify access permissions before calling {tool}. Use file_read on the target directory if needed.")
            }
            (_, "connection") => {
                "Check network connectivity and service availability. Consider using web_scan to verify the service is reachable.".into()
            }
            (_, "timeout") => {
                format!("{tool} timed out. Increase the timeout parameter or split the work into smaller units.")
            }
            (_, "parse_error") => {
                format!("{tool} received malformed input. Verify arguments match the schema — check required fields and types.")
            }
            _ => {
                format!("Previous {tool} call with similar signature failed. Consider an alternative approach or tool.")
            }
        }
    }
}

/// Persisted error memory store
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ErrorStore {
    /// Fingerprint → record
    pub records: HashMap<String, ErrorRecord>,
}

/// Manages error memory: recording, loading, querying
#[derive(Debug, Clone)]
pub struct ErrorMemory {
    pub store_dir: PathBuf,
    pub store_path: PathBuf,
}

impl ErrorMemory {
    pub fn new(project_dir: &Path) -> Self {
        let store_dir = project_dir.join("memory").join("errors");
        let store_path = store_dir.join("error_log.json");
        Self {
            store_dir,
            store_path,
        }
    }

    fn ensure_dirs(&self) -> Result<(), String> {
        std::fs::create_dir_all(&self.store_dir)
            .map_err(|e| format!("Failed to create error memory dir: {e}"))
    }

    fn load_store(&self) -> Result<ErrorStore, String> {
        self.ensure_dirs()?;
        if !self.store_path.exists() {
            return Ok(ErrorStore::default());
        }
        let data = std::fs::read_to_string(&self.store_path)
            .map_err(|e| format!("Failed to read error log: {e}"))?;
        Ok(serde_json::from_str(&data).unwrap_or_else(|_| ErrorStore::default()))
    }

    fn save_store(&self, store: &ErrorStore) -> Result<(), String> {
        self.ensure_dirs()?;
        let data = serde_json::to_string_pretty(store)
            .map_err(|e| format!("Failed to serialize error log: {e}"))?;
        std::fs::write(&self.store_path, data)
            .map_err(|e| format!("Failed to write error log: {e}"))
    }

    /// Record an error. Returns true if this is a new fingerprint (first occurrence).
    pub fn record(
        &self,
        tool: &str,
        message: &str,
        severity: ErrorSeverity,
        context: Value,
        model: &str,
        turn: usize,
    ) -> Result<bool, String> {
        let category = ErrorRecord::classify(message);
        let fingerprint = ErrorRecord::fingerprint(tool, category);
        let now = Utc::now().to_rfc3339();

        let mut store = self.load_store()?;
        let is_new = !store.records.contains_key(&fingerprint);

        if let Some(existing) = store.records.get_mut(&fingerprint) {
            existing.count += 1;
            existing.last_seen = now.clone();
            existing.message = message.chars().take(500).collect();
            existing.context = context;
            existing.model = format_model(model);
            existing.turn = turn;
            // Update hint if the new message has a better category
            let new_hint = ErrorRecord::avoidance_hint(tool, category, message);
            if !new_hint.is_empty() {
                existing.avoidance_hint = new_hint;
            }
        } else {
            let record = ErrorRecord {
                fingerprint: fingerprint.clone(),
                summary: format!("{} ({})", tool, category),
                tool: tool.to_string(),
                message: message.chars().take(500).collect(),
                context,
                severity,
                count: 1,
                first_seen: now.clone(),
                last_seen: now,
                model: format_model(model),
                turn,
                avoidance_hint: ErrorRecord::avoidance_hint(tool, category, message),
            };
            store.records.insert(fingerprint.clone(), record);
            log::warn!("[ErrorMemory] New error pattern: {fingerprint}");
        }

        self.save_store(&store)?;
        Ok(is_new)
    }

    /// Get all records, sorted by count descending (most frequent first)
    pub fn list_records(&self) -> Result<Vec<ErrorRecord>, String> {
        let store = self.load_store()?;
        let mut records: Vec<ErrorRecord> = store.records.into_values().collect();
        records.sort_by(|a, b| b.count.cmp(&a.count));
        Ok(records)
    }

    /// Get records that should inform the agent (recurring errors with avoidance hints)
    pub fn active_warnings(&self) -> Result<Vec<ErrorRecord>, String> {
        let records = self.list_records()?;
        Ok(records
            .into_iter()
            .filter(|r| r.count >= 2 && !r.avoidance_hint.is_empty())
            .collect())
    }

    /// Build an error avoidance section for injection into the system prompt
    pub fn avoidance_summary(&self) -> String {
        let warnings = match self.active_warnings() {
            Ok(w) => w,
            Err(_) => return String::new(),
        };

        if warnings.is_empty() {
            return String::new();
        }

        let mut lines = vec![
            "\n## Error Experience (auto-avoid)\n".to_string(),
            "These errors have occurred repeatedly across sessions. Avoid the broken approaches listed below.\n".to_string(),
        ];

        for w in &warnings {
            lines.push(format!(
                "- **{}** (×{}, last: {}): {}",
                w.summary,
                w.count,
                &w.last_seen[..10],
                w.avoidance_hint
            ));
        }

        lines.push(String::new());
        lines.join("\n")
    }

    /// Clear all error records
    pub fn clear(&self) -> Result<(), String> {
        self.save_store(&ErrorStore::default())
    }

    /// Delete a specific error fingerprint
    pub fn forget(&self, fingerprint: &str) -> Result<(), String> {
        let mut store = self.load_store()?;
        store.records.remove(fingerprint);
        self.save_store(&store)
    }

    /// Total error count across all records
    pub fn total_errors(&self) -> Result<usize, String> {
        let store = self.load_store()?;
        Ok(store.records.values().map(|r| r.count).sum())
    }
}

fn format_model(model: &str) -> String {
    if model.is_empty() {
        "unknown".into()
    } else if model.len() > 80 {
        format!("{}...", &model[..80])
    } else {
        model.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fingerprint() {
        let fp = ErrorRecord::fingerprint("file_read", "not_found");
        assert_eq!(fp, "file_read:not_found");
    }

    #[test]
    fn test_classify_not_found() {
        assert_eq!(ErrorRecord::classify("File not found: /tmp/x"), "not_found");
        assert_eq!(
            ErrorRecord::classify("No such file or directory"),
            "not_found"
        );
    }

    #[test]
    fn test_classify_permission() {
        assert_eq!(
            ErrorRecord::classify("Permission denied: /etc/shadow"),
            "permission"
        );
    }

    #[test]
    fn test_classify_timeout() {
        assert_eq!(ErrorRecord::classify("Operation timed out"), "timeout");
    }

    #[test]
    fn test_record_and_list() {
        let temp = std::env::temp_dir().join("gc_err_test");
        let _ = std::fs::remove_dir_all(&temp);
        let em = ErrorMemory::new(&temp);

        em.record(
            "file_read",
            "File not found: /nonexistent/path.txt",
            ErrorSeverity::Tool,
            serde_json::json!({"path": "/nonexistent/path.txt"}),
            "test-model",
            3,
        )
        .unwrap();

        em.record(
            "file_read",
            "File not found: /another/missing.txt",
            ErrorSeverity::Tool,
            serde_json::json!({"path": "/another/missing.txt"}),
            "test-model",
            5,
        )
        .unwrap();

        let records = em.list_records().unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].count, 2);
        assert_eq!(records[0].fingerprint, "file_read:not_found");

        let warnings = em.active_warnings().unwrap();
        assert_eq!(warnings.len(), 1);

        let summary = em.avoidance_summary();
        assert!(summary.contains("not_found"));
        assert!(summary.contains("×2"));

        let _ = std::fs::remove_dir_all(&temp);
    }

    #[test]
    fn test_new_fingerprint_detection() {
        let temp = std::env::temp_dir().join("gc_err_test2");
        let _ = std::fs::remove_dir_all(&temp);
        let em = ErrorMemory::new(&temp);

        let is_new = em
            .record(
                "code_run",
                "syntax error",
                ErrorSeverity::Tool,
                Value::Null,
                "m",
                1,
            )
            .unwrap();
        assert!(is_new);

        let still_new = em
            .record(
                "file_read",
                "not found",
                ErrorSeverity::Tool,
                Value::Null,
                "m",
                2,
            )
            .unwrap();
        assert!(still_new);

        let not_new = em
            .record(
                "code_run",
                "another syntax error",
                ErrorSeverity::Tool,
                Value::Null,
                "m",
                3,
            )
            .unwrap();
        assert!(!not_new);

        let _ = std::fs::remove_dir_all(&temp);
    }
}
