use std::cell::RefCell;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use glob::Pattern;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::types::FileEntry;

// ── Constants ──────────────────────────────────────────────────────

pub static DEFAULT_EXCLUDE_PATTERNS: &[&str] = &[
    ".git",
    "__pycache__",
    "*.pyc",
    ".DS_Store",
    "node_modules",
    ".venv",
    "venv",
    ".env",
    "*.egg-info",
    ".idea",
    ".vscode",
    "*.o",
    "*.so",
    "*.dylib",
    "*.dll",
    "*.exe",
    ".pytest_cache",
    ".mypy_cache",
    ".ruff_cache",
    "dist",
    "build",
    ".tox",
];

// ── Type alias ─────────────────────────────────────────────────────

pub type JsonResult = Value;

// ── Config persistence ─────────────────────────────────────────────

fn config_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".genericagent")
}

fn config_path() -> PathBuf {
    config_dir().join("workspace_config.json")
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct WorkspaceConfig {
    #[serde(default)]
    workspaces: Vec<WorkspaceEntry>,
    #[serde(default)]
    recent_folders: Vec<String>,
    #[serde(default)]
    active_workspace: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct WorkspaceEntry {
    name: String,
    path: String,
    #[serde(default)]
    exclude: Vec<String>,
}

#[derive(Debug, Clone)]
struct WorkspaceInfo {
    path: String,
    exclude: Vec<String>,
}

fn _matches_glob(pattern: &str, name: &str) -> bool {
    let pat = if pattern.ends_with('/') {
        &pattern[..pattern.len() - 1]
    } else if pattern.starts_with('!') {
        return false; // negation patterns not supported in simple matching
    } else {
        pattern
    };
    Pattern::new(pat).map_or(false, |p| p.matches(name))
}

// ── FileTreeBuilder ────────────────────────────────────────────────

pub struct FileTreeBuilder {
    exclude_patterns: Vec<String>,
    gitignore_patterns: RefCell<HashMap<String, Vec<String>>>,
}

impl Default for FileTreeBuilder {
    fn default() -> Self {
        Self::new(None)
    }
}

impl FileTreeBuilder {
    pub fn new(exclude_patterns: Option<Vec<String>>) -> Self {
        Self {
            exclude_patterns: exclude_patterns.unwrap_or_else(|| {
                DEFAULT_EXCLUDE_PATTERNS
                    .iter()
                    .map(|s| s.to_string())
                    .collect()
            }),
            gitignore_patterns: RefCell::new(HashMap::new()),
        }
    }

    fn _should_skip_dot_item(name: &str) -> bool {
        if !name.starts_with('.') {
            return false;
        }
        if name == ".gitignore" || name == ".env.example" || name == ".editorconfig" {
            return false;
        }
        true
    }

    fn _load_gitignore(&self, folder_path: &str) -> Vec<String> {
        {
            let cache = self.gitignore_patterns.borrow();
            if let Some(patterns) = cache.get(folder_path) {
                return patterns.clone();
            }
        }
        let mut patterns = Vec::new();
        let gitignore_path = Path::new(folder_path).join(".gitignore");
        if gitignore_path.is_file() {
            if let Ok(content) = fs::read_to_string(&gitignore_path) {
                for line in content.lines() {
                    let line = line.trim();
                    if line.is_empty() || line.starts_with('#') {
                        continue;
                    }
                    patterns.push(line.to_string());
                }
            }
        }
        self.gitignore_patterns
            .borrow_mut()
            .insert(folder_path.to_string(), patterns.clone());
        patterns
    }

    fn _is_excluded(&self, name: &str, parent: &str) -> bool {
        if self.exclude_patterns.iter().any(|p| _matches_glob(p, name)) {
            return true;
        }
        let gitignore = self._load_gitignore(parent);
        gitignore.iter().any(|p| _matches_glob(p, name))
    }

    pub fn build_tree(
        &self,
        root_path: &str,
        max_depth: usize,
        current_depth: usize,
        max_items: usize,
    ) -> FileEntry {
        let root_path = std::path::absolute(Path::new(root_path))
            .map(|p| p.display().to_string())
            .unwrap_or_else(|_| root_path.to_string());

        let path = Path::new(&root_path);
        if !path.is_dir() {
            return FileEntry {
                name: path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| root_path.clone()),
                path: root_path,
                entry_type: "file".to_string(),
                size: 0,
                children: Vec::new(),
                truncated: false,
                error: None,
            };
        }

        let mut entry = FileEntry {
            name: path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| root_path.clone()),
            path: root_path.clone(),
            entry_type: "directory".to_string(),
            size: 0,
            children: Vec::new(),
            truncated: false,
            error: None,
        };

        if current_depth >= max_depth {
            entry.truncated = true;
            return entry;
        }

        let mut items: Vec<fs::DirEntry> = match fs::read_dir(&root_path) {
            Ok(entries) => entries.filter_map(|e| e.ok()).collect(),
            Err(e) => {
                entry.error = Some(format!("{}", e));
                return entry;
            }
        };

        items.sort_by(|a, b| {
            let a_is_dir = a.file_type().map(|t| t.is_dir()).unwrap_or(false);
            let b_is_dir = b.file_type().map(|t| t.is_dir()).unwrap_or(false);
            if a_is_dir == b_is_dir {
                a.file_name()
                    .to_string_lossy()
                    .to_lowercase()
                    .cmp(&b.file_name().to_string_lossy().to_lowercase())
            } else if a_is_dir {
                std::cmp::Ordering::Less
            } else {
                std::cmp::Ordering::Greater
            }
        });

        let mut count: usize = 0;
        for item in &items {
            if count >= max_items {
                entry.truncated = true;
                break;
            }

            let item_name = item.file_name().to_string_lossy().to_string();
            if Self::_should_skip_dot_item(&item_name) {
                continue;
            }
            if self._is_excluded(&item_name, &root_path) {
                continue;
            }

            let item_path = item.path();
            let is_dir = item.file_type().map(|t| t.is_dir()).unwrap_or(false);

            if is_dir {
                let child_max = std::cmp::max(50, max_items / 4);
                let child = self.build_tree(
                    &item_path.display().to_string(),
                    max_depth,
                    current_depth + 1,
                    child_max,
                );
                if !child.children.is_empty() || child.error.is_none() {
                    entry.children.push(child);
                    count += 1;
                }
            } else {
                let size = item.metadata().map(|m| m.len()).unwrap_or(0);
                entry.children.push(FileEntry {
                    name: item_name,
                    path: item_path.display().to_string(),
                    entry_type: "file".to_string(),
                    size,
                    children: Vec::new(),
                    truncated: false,
                    error: None,
                });
                count += 1;
            }
        }

        entry
    }

    pub fn build_flat_list(&self, root_path: &str, pattern: &str, max_items: usize) -> Vec<Value> {
        let root_path = std::path::absolute(Path::new(root_path))
            .map(|p| p.display().to_string())
            .unwrap_or_else(|_| root_path.to_string());

        let check_pattern = pattern != "*";
        let mut results: Vec<Value> = Vec::new();

        let entries = match fs::read_dir(&root_path) {
            Ok(e) => e,
            Err(_) => return results,
        };

        for entry in entries.flatten() {
            if results.len() >= max_items {
                break;
            }

            let name = entry.file_name().to_string_lossy().to_string();
            if self._is_excluded(&name, &root_path) {
                continue;
            }
            if check_pattern && !_matches_glob(pattern, &name) {
                continue;
            }

            let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
            let size = if is_dir {
                0
            } else {
                entry.metadata().map(|m| m.len()).unwrap_or(0)
            };

            results.push(serde_json::json!({
                "name": name,
                "path": entry.path().display().to_string(),
                "type": if is_dir { "directory" } else { "file" },
                "size": size,
            }));
        }

        results.sort_by(|a, b| {
            let a_is_dir = a["type"].as_str().unwrap_or("") == "directory";
            let b_is_dir = b["type"].as_str().unwrap_or("") == "directory";
            if a_is_dir == b_is_dir {
                a["name"]
                    .as_str()
                    .unwrap_or("")
                    .to_lowercase()
                    .cmp(&b["name"].as_str().unwrap_or("").to_lowercase())
            } else if a_is_dir {
                std::cmp::Ordering::Less
            } else {
                std::cmp::Ordering::Greater
            }
        });

        results
    }
}

// ── WorkspaceManager ───────────────────────────────────────────────

pub struct WorkspaceManager {
    active_workspace: String,
    workspaces: HashMap<String, WorkspaceInfo>,
    tree_builder: FileTreeBuilder,
}

impl WorkspaceManager {
    pub fn new() -> Self {
        let mut mgr = Self {
            active_workspace: String::new(),
            workspaces: HashMap::new(),
            tree_builder: FileTreeBuilder::default(),
        };
        mgr._load_state();
        mgr
    }

    // ── persistence ────────────────────────────────────────────

    fn _load_state(&mut self) {
        let path = config_path();
        if !path.is_file() {
            return;
        }
        let data = match fs::read_to_string(&path) {
            Ok(d) => d,
            Err(_) => return,
        };
        let config: WorkspaceConfig = match serde_json::from_str(&data) {
            Ok(c) => c,
            Err(_) => return,
        };
        for ws in &config.workspaces {
            if !ws.name.is_empty() && !ws.path.is_empty() && Path::new(&ws.path).is_dir() {
                self.workspaces.insert(
                    ws.name.clone(),
                    WorkspaceInfo {
                        path: ws.path.clone(),
                        exclude: ws.exclude.clone(),
                    },
                );
            }
        }
        if !config.active_workspace.is_empty()
            && self.workspaces.contains_key(&config.active_workspace)
        {
            self.active_workspace = config.active_workspace;
        } else if !self.workspaces.is_empty() {
            // fallback: activate the first workspace
            self.active_workspace = self.workspaces.keys().next().cloned().unwrap_or_default();
        }
    }

    fn _save_state(&self) {
        let workspaces: Vec<WorkspaceEntry> = self
            .workspaces
            .iter()
            .map(|(name, info)| WorkspaceEntry {
                name: name.clone(),
                path: info.path.clone(),
                exclude: info.exclude.clone(),
            })
            .collect();

        let mut recent_folders: Vec<String> =
            self.workspaces.values().map(|w| w.path.clone()).collect();
        recent_folders.reverse();
        recent_folders.truncate(20);

        let config = WorkspaceConfig {
            workspaces,
            recent_folders,
            active_workspace: self.active_workspace.clone(),
        };

        if let Some(parent) = config_path().parent() {
            let _ = fs::create_dir_all(parent);
        }
        if let Ok(json) = serde_json::to_string_pretty(&config) {
            let _ = fs::write(config_path(), json);
        }
    }

    // ── workspace lifecycle ────────────────────────────────────

    pub fn open_folder(&mut self, path: &str, name: &str) -> JsonResult {
        let abs = std::path::absolute(Path::new(path))
            .map(|p| p.display().to_string())
            .unwrap_or_else(|_| path.to_string());

        if !Path::new(&abs).is_dir() {
            return serde_json::json!({"status": "error", "msg": format!("Folder not found: {}", abs)});
        }

        let name = if name.is_empty() {
            Path::new(&abs)
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| abs.clone())
        } else {
            name.to_string()
        };

        self.workspaces.insert(
            name.clone(),
            WorkspaceInfo {
                path: abs.clone(),
                exclude: Vec::new(),
            },
        );
        self.active_workspace = name.clone();
        self._save_state();

        let tree = self.tree_builder.build_tree(&abs, 3, 0, 500);
        serde_json::json!({"status": "success", "name": name, "path": abs, "tree": tree})
    }

    pub fn close_workspace(&mut self, name: &str) -> JsonResult {
        let target = if name.is_empty() {
            self.active_workspace.clone()
        } else {
            name.to_string()
        };

        if !self.workspaces.contains_key(&target) {
            return serde_json::json!({"status": "error", "msg": format!("Workspace \"{}\" not open", target)});
        }

        self.workspaces.remove(&target);

        if self.active_workspace == target {
            self.active_workspace = self.workspaces.keys().next().cloned().unwrap_or_default();
        }
        self._save_state();

        serde_json::json!({"status": "success", "name": target})
    }

    pub fn switch_workspace(&mut self, name: &str) -> JsonResult {
        if !self.workspaces.contains_key(name) {
            return serde_json::json!({"status": "error", "msg": format!("Workspace \"{}\" not found", name)});
        }

        self.active_workspace = name.to_string();
        self._save_state();

        let ws = &self.workspaces[name];
        serde_json::json!({"status": "success", "name": name, "path": ws.path})
    }

    pub fn get_active_workspace(&self) -> JsonResult {
        if self.active_workspace.is_empty() {
            return serde_json::json!({"status": "error", "msg": "No active workspace"});
        }
        let ws = match self.workspaces.get(&self.active_workspace) {
            Some(w) => w,
            None => {
                return serde_json::json!({"status": "error", "msg": "Active workspace missing from registry"});
            }
        };
        serde_json::json!({"status": "success", "name": self.active_workspace, "path": ws.path})
    }

    pub fn list_workspaces(&self) -> Vec<Value> {
        self.workspaces
            .iter()
            .map(|(name, ws)| {
                serde_json::json!({
                    "name": name,
                    "path": ws.path,
                    "active": *name == self.active_workspace,
                })
            })
            .collect()
    }

    // ── file tree ──────────────────────────────────────────────

    pub fn get_tree(&self, path: &str, max_depth: usize) -> JsonResult {
        let abs = if path.is_empty() {
            let active = self.get_active_workspace();
            match active["status"].as_str() {
                Some("success") => active["path"].as_str().unwrap_or("").to_string(),
                _ => return active,
            }
        } else {
            std::path::absolute(Path::new(path))
                .map(|p| p.display().to_string())
                .unwrap_or_else(|_| path.to_string())
        };

        if !Path::new(&abs).is_dir() {
            return serde_json::json!({"status": "error", "msg": format!("Not a directory: {}", abs)});
        }

        let tree = self.tree_builder.build_tree(&abs, max_depth, 0, 500);
        serde_json::json!({"status": "success", "tree": tree})
    }

    pub fn list_files(&self, path: &str, pattern: &str) -> JsonResult {
        let abs = if path.is_empty() {
            let active = self.get_active_workspace();
            match active["status"].as_str() {
                Some("success") => active["path"].as_str().unwrap_or("").to_string(),
                _ => return active,
            }
        } else {
            std::path::absolute(Path::new(path))
                .map(|p| p.display().to_string())
                .unwrap_or_else(|_| path.to_string())
        };

        if !Path::new(&abs).is_dir() {
            return serde_json::json!({"status": "error", "msg": format!("Not a directory: {}", abs)});
        }

        let files = self.tree_builder.build_flat_list(&abs, pattern, 500);
        serde_json::json!({"status": "success", "files": files, "path": abs})
    }

    pub fn search_files(&self, query: &str, path: &str, max_results: usize) -> JsonResult {
        let abs = if path.is_empty() {
            let active = self.get_active_workspace();
            match active["status"].as_str() {
                Some("success") => active["path"].as_str().unwrap_or("").to_string(),
                _ => return active,
            }
        } else {
            std::path::absolute(Path::new(path))
                .map(|p| p.display().to_string())
                .unwrap_or_else(|_| path.to_string())
        };

        let mut results: Vec<Value> = Vec::new();
        let query_lower = query.to_lowercase();
        self._walk_for_search(&abs, &query_lower, max_results, &mut results);

        serde_json::json!({"status": "success", "results": results, "path": abs})
    }

    fn _walk_for_search(
        &self,
        dir: &str,
        query_lower: &str,
        max_results: usize,
        results: &mut Vec<Value>,
    ) {
        if results.len() >= max_results {
            return;
        }

        let entries = match fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => return,
        };

        let mut dirs: Vec<PathBuf> = Vec::new();

        for entry in entries.flatten() {
            if results.len() >= max_results {
                return;
            }

            let name = entry.file_name().to_string_lossy().to_string();
            let entry_path = entry.path();
            let path_str = entry_path.display().to_string();

            if self.tree_builder._is_excluded(&name, dir) {
                continue;
            }

            let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);

            if is_dir {
                dirs.push(entry_path);
            } else if query_lower.is_empty() || name.to_lowercase().contains(query_lower) {
                let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
                results.push(serde_json::json!({
                    "name": name,
                    "path": path_str,
                    "size": size,
                    "relative": self.get_relative_path(&path_str),
                }));
            }
        }

        for d in dirs {
            self._walk_for_search(&d.display().to_string(), query_lower, max_results, results);
        }
    }

    // ── path helpers ───────────────────────────────────────────

    pub fn get_relative_path(&self, abs_path: &str) -> String {
        let active = self.get_active_workspace();
        let ws_path = match active["status"].as_str() {
            Some("success") => active["path"].as_str().unwrap_or(""),
            _ => return abs_path.to_string(),
        };

        let abs = std::path::absolute(PathBuf::from(abs_path))
            .unwrap_or_else(|_| PathBuf::from(abs_path));

        abs.strip_prefix(ws_path)
            .map(|p| p.display().to_string())
            .unwrap_or_else(|_| abs_path.to_string())
    }

    fn canonical_workspace_root(&self) -> Option<PathBuf> {
        let active = self.get_active_workspace();
        let ws_path = active.get("path").and_then(|value| value.as_str())?;
        fs::canonicalize(ws_path).ok()
    }

    fn canonicalize_for_access(&self, path: &Path, allow_missing: bool) -> Option<PathBuf> {
        if !allow_missing || path.exists() {
            return fs::canonicalize(path).ok();
        }

        let parent = path.parent()?;
        let canonical_parent = fs::canonicalize(parent).ok()?;
        let file_name = path.file_name()?;
        Some(canonical_parent.join(file_name))
    }

    pub fn is_within_workspace(&self, path: &str) -> bool {
        let Some(workspace_root) = self.canonical_workspace_root() else {
            return false;
        };
        let path = std::path::absolute(Path::new(path)).unwrap_or_else(|_| PathBuf::from(path));
        let Some(canonical_path) = self.canonicalize_for_access(&path, true) else {
            return false;
        };
        canonical_path.starts_with(&workspace_root)
    }

    // ── file operations ────────────────────────────────────────

    fn _resolve_workspace_path(
        &self,
        path: &str,
        label: &str,
        allow_missing: bool,
    ) -> Result<String, JsonResult> {
        let abs = std::path::absolute(Path::new(path))
            .map(|p| p.display().to_string())
            .unwrap_or_else(|_| path.to_string());

        let Some(workspace_root) = self.canonical_workspace_root() else {
            return Err(serde_json::json!({"status": "error", "msg": "No active workspace"}));
        };
        let Some(canonical_path) = self.canonicalize_for_access(Path::new(&abs), allow_missing)
        else {
            return Err(serde_json::json!({
                "status": "error",
                "msg": format!("Unable to resolve {}: {}", label, abs),
            }));
        };

        if !canonical_path.starts_with(&workspace_root) {
            return Err(serde_json::json!({
                "status": "error",
                "msg": format!("{} is outside the active workspace: {}", label, abs),
            }));
        }

        if !allow_missing && !Path::new(&abs).exists() {
            return Err(serde_json::json!({
                "status": "error",
                "msg": format!("{} not found: {}", label, abs),
            }));
        }

        Ok(canonical_path.display().to_string())
    }

    pub fn create_folder(&self, path: &str) -> JsonResult {
        let resolved = match self._resolve_workspace_path(path, "Path", true) {
            Ok(p) => p,
            Err(e) => return e,
        };
        if let Err(e) = fs::create_dir_all(&resolved) {
            return serde_json::json!({"status": "error", "msg": format!("{}", e)});
        }
        serde_json::json!({"status": "success", "path": resolved})
    }

    pub fn delete_item(&self, path: &str) -> JsonResult {
        let resolved = match self._resolve_workspace_path(path, "Path", false) {
            Ok(p) => p,
            Err(e) => return e,
        };

        let p = Path::new(&resolved);
        let result = if p.is_dir() {
            fs::remove_dir_all(p)
        } else {
            fs::remove_file(p)
        };

        match result {
            Ok(_) => serde_json::json!({"status": "success", "path": resolved}),
            Err(e) => serde_json::json!({"status": "error", "msg": format!("{}", e)}),
        }
    }

    pub fn move_item(&self, src: &str, dst: &str) -> JsonResult {
        let src_path = match self._resolve_workspace_path(src, "Source", false) {
            Ok(p) => p,
            Err(e) => return e,
        };
        let dst_path = match self._resolve_workspace_path(dst, "Destination", true) {
            Ok(p) => p,
            Err(e) => return e,
        };

        if let Some(parent) = Path::new(&dst_path).parent() {
            let _ = fs::create_dir_all(parent);
        }

        match fs::rename(&src_path, &dst_path) {
            Ok(_) => {
                serde_json::json!({"status": "success", "source": src_path, "destination": dst_path})
            }
            Err(e) => serde_json::json!({"status": "error", "msg": format!("{}", e)}),
        }
    }
}

// ── Global singleton ───────────────────────────────────────────────

lazy_static::lazy_static! {
    static ref WM: Mutex<WorkspaceManager> = Mutex::new(WorkspaceManager::new());
}

// ── Public API free functions ──────────────────────────────────────

pub fn open_folder(path: &str, name: &str) -> JsonResult {
    WM.lock().open_folder(path, name)
}

pub fn close_workspace(name: &str) -> JsonResult {
    WM.lock().close_workspace(name)
}

pub fn switch_workspace(name: &str) -> JsonResult {
    WM.lock().switch_workspace(name)
}

pub fn get_active_workspace() -> JsonResult {
    WM.lock().get_active_workspace()
}

pub fn list_workspaces() -> Vec<Value> {
    WM.lock().list_workspaces()
}

pub fn get_tree(path: &str, max_depth: usize) -> JsonResult {
    WM.lock().get_tree(path, max_depth)
}

pub fn list_files(path: &str, pattern: &str) -> JsonResult {
    WM.lock().list_files(path, pattern)
}

pub fn get_relative_path(abs_path: &str) -> String {
    WM.lock().get_relative_path(abs_path)
}

pub fn is_within_workspace(path: &str) -> bool {
    WM.lock().is_within_workspace(path)
}

pub fn search_files(query: &str, path: &str, max_results: usize) -> JsonResult {
    WM.lock().search_files(query, path, max_results)
}

pub fn create_folder(path: &str) -> JsonResult {
    WM.lock().create_folder(path)
}

pub fn delete_item(path: &str) -> JsonResult {
    WM.lock().delete_item(path)
}

pub fn move_item(src: &str, dst: &str) -> JsonResult {
    WM.lock().move_item(src, dst)
}
