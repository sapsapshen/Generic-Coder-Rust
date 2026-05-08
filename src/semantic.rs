use anyhow::{anyhow, Context, Result};
use regex::Regex;
use serde::Serialize;
use serde_json::{json, Value};
use std::cmp::Reverse;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

const MAX_FILE_BYTES: u64 = 512 * 1024;
const MAX_SCAN_FILES: usize = 2_000;
const MAX_INDEX_BYTES: u64 = 64 * 1024 * 1024;
const DEFAULT_RESULTS: usize = 25;

lazy_static::lazy_static! {
    static ref RUST_FN_RE: Regex = Regex::new(
        r"^(?P<indent>\s*)(?:pub(?:\([^)]*\))?\s+)?(?:async\s+)?fn\s+(?P<name>[A-Za-z_][A-Za-z0-9_]*)"
    ).unwrap();
    static ref RUST_TYPE_RE: Regex = Regex::new(
        r"^(?P<indent>\s*)(?:pub(?:\([^)]*\))?\s+)?(?P<kind>struct|enum|trait|type|const|static|mod)\s+(?P<name>[A-Za-z_][A-Za-z0-9_]*)"
    ).unwrap();
    static ref JS_FN_RE: Regex = Regex::new(
        r"^(?P<indent>\s*)(?:export\s+)?(?:async\s+)?function\s+(?P<name>[A-Za-z_$][A-Za-z0-9_$]*)"
    ).unwrap();
    static ref JS_DECL_RE: Regex = Regex::new(
        r"^(?P<indent>\s*)(?:export\s+)?(?:const|let|var|class|interface|type)\s+(?P<name>[A-Za-z_$][A-Za-z0-9_$]*)"
    ).unwrap();
    static ref PY_DEF_RE: Regex = Regex::new(
        r"^(?P<indent>\s*)(?:async\s+)?def\s+(?P<name>[A-Za-z_][A-Za-z0-9_]*)"
    ).unwrap();
    static ref PY_CLASS_RE: Regex = Regex::new(
        r"^(?P<indent>\s*)class\s+(?P<name>[A-Za-z_][A-Za-z0-9_]*)"
    ).unwrap();
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SymbolKind {
    Function,
    Method,
    Struct,
    Enum,
    Trait,
    TypeAlias,
    Const,
    Static,
    Module,
    Class,
    Variable,
    Unknown,
}

impl SymbolKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Function => "function",
            Self::Method => "method",
            Self::Struct => "struct",
            Self::Enum => "enum",
            Self::Trait => "trait",
            Self::TypeAlias => "type_alias",
            Self::Const => "const",
            Self::Static => "static",
            Self::Module => "module",
            Self::Class => "class",
            Self::Variable => "variable",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
struct SymbolRecord {
    name: String,
    kind: SymbolKind,
    file: String,
    relative_path: String,
    line: usize,
    signature: String,
}

#[derive(Debug, Clone, Serialize)]
struct ReferenceRecord {
    file: String,
    relative_path: String,
    line: usize,
    content: String,
    is_definition: bool,
}

fn workspace_root() -> Result<PathBuf> {
    crate::workspace::effective_root()
        .or_else(|| std::env::current_dir().ok())
        .map(|path| fs::canonicalize(&path).unwrap_or(path))
        .ok_or_else(|| anyhow!("Cannot resolve workspace root"))
}

fn resolve_scope(path: Option<&str>) -> Result<(PathBuf, PathBuf)> {
    let root = workspace_root()?;
    let scope = match path {
        Some(raw) if !raw.trim().is_empty() => {
            let requested = PathBuf::from(raw);
            let resolved = if requested.is_absolute() {
                requested
            } else {
                root.join(requested)
            };
            fs::canonicalize(&resolved)
                .with_context(|| format!("Cannot resolve search path: {}", resolved.display()))?
        }
        _ => root.clone(),
    };

    if !scope.starts_with(&root) {
        return Err(anyhow!(
            "Search path is outside the active workspace: {}",
            scope.display()
        ));
    }

    Ok((root, scope))
}

fn should_skip_dir(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
        return false;
    };
    matches!(
        name,
        ".git"
            | "node_modules"
            | "target"
            | "dist"
            | "build"
            | ".next"
            | ".turbo"
            | ".idea"
            | ".vscode"
            | "__pycache__"
    )
}

fn is_supported_file(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|value| value.to_str())
            .unwrap_or_default(),
        "rs" | "py"
            | "js"
            | "jsx"
            | "ts"
            | "tsx"
            | "go"
            | "java"
            | "kt"
            | "kts"
            | "c"
            | "cc"
            | "cpp"
            | "h"
            | "hpp"
            | "cs"
    )
}

fn collect_files(scope: &Path, results: &mut Vec<PathBuf>) -> Result<()> {
    if results.len() >= MAX_SCAN_FILES {
        return Ok(());
    }

    if scope.is_file() {
        if is_supported_file(scope) {
            results.push(scope.to_path_buf());
        }
        return Ok(());
    }

    let entries = fs::read_dir(scope)
        .with_context(|| format!("Cannot read directory {}", scope.display()))?;
    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            if should_skip_dir(&path) {
                continue;
            }
            collect_files(&path, results)?;
        } else if is_supported_file(&path) {
            let metadata = entry.metadata()?;
            if metadata.len() <= MAX_FILE_BYTES {
                results.push(path);
            }
        }

        if results.len() >= MAX_SCAN_FILES {
            break;
        }
    }

    Ok(())
}

fn relative_path(root: &Path, file: &Path) -> String {
    file.strip_prefix(root)
        .unwrap_or(file)
        .display()
        .to_string()
}

fn classify_rust_type(kind: &str) -> SymbolKind {
    match kind {
        "struct" => SymbolKind::Struct,
        "enum" => SymbolKind::Enum,
        "trait" => SymbolKind::Trait,
        "type" => SymbolKind::TypeAlias,
        "const" => SymbolKind::Const,
        "static" => SymbolKind::Static,
        "mod" => SymbolKind::Module,
        _ => SymbolKind::Unknown,
    }
}

fn symbol_from_line(file: &Path, relative: &str, index: usize, line: &str) -> Option<SymbolRecord> {
    let trimmed = line.trim();
    if trimmed.is_empty()
        || trimmed.starts_with("//")
        || trimmed.starts_with('#')
        || trimmed.starts_with('*')
    {
        return None;
    }

    let ext = file
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    let (name, kind) = match ext {
        "rs" => {
            if let Some(caps) = RUST_FN_RE.captures(line) {
                let indent = caps
                    .name("indent")
                    .map(|value| value.as_str())
                    .unwrap_or("");
                let name = caps.name("name")?.as_str().to_string();
                let kind = if indent.len() >= 4 {
                    SymbolKind::Method
                } else {
                    SymbolKind::Function
                };
                (name, kind)
            } else if let Some(caps) = RUST_TYPE_RE.captures(line) {
                let kind = classify_rust_type(caps.name("kind")?.as_str());
                (caps.name("name")?.as_str().to_string(), kind)
            } else {
                return None;
            }
        }
        "py" => {
            if let Some(caps) = PY_DEF_RE.captures(line) {
                let indent = caps
                    .name("indent")
                    .map(|value| value.as_str())
                    .unwrap_or("");
                let kind = if indent.len() >= 4 {
                    SymbolKind::Method
                } else {
                    SymbolKind::Function
                };
                (caps.name("name")?.as_str().to_string(), kind)
            } else if let Some(caps) = PY_CLASS_RE.captures(line) {
                (caps.name("name")?.as_str().to_string(), SymbolKind::Class)
            } else {
                return None;
            }
        }
        "js" | "jsx" | "ts" | "tsx" => {
            if let Some(caps) = JS_FN_RE.captures(line) {
                (
                    caps.name("name")?.as_str().to_string(),
                    SymbolKind::Function,
                )
            } else if let Some(caps) = JS_DECL_RE.captures(line) {
                let kind = if trimmed.contains("class ") {
                    SymbolKind::Class
                } else if trimmed.contains("interface ") || trimmed.contains("type ") {
                    SymbolKind::TypeAlias
                } else {
                    SymbolKind::Variable
                };
                (caps.name("name")?.as_str().to_string(), kind)
            } else {
                return None;
            }
        }
        "go" | "java" | "kt" | "kts" | "c" | "cc" | "cpp" | "h" | "hpp" | "cs" => {
            if let Some(caps) = JS_FN_RE.captures(line) {
                (
                    caps.name("name")?.as_str().to_string(),
                    SymbolKind::Function,
                )
            } else if let Some(caps) = JS_DECL_RE.captures(line) {
                (caps.name("name")?.as_str().to_string(), SymbolKind::Class)
            } else {
                return None;
            }
        }
        _ => return None,
    };

    Some(SymbolRecord {
        name,
        kind,
        file: file.display().to_string(),
        relative_path: relative.to_string(),
        line: index + 1,
        signature: trimmed.chars().take(200).collect(),
    })
}

fn build_symbol_index(path: Option<&str>) -> Result<(PathBuf, Vec<PathBuf>, Vec<SymbolRecord>)> {
    let (root, scope) = resolve_scope(path)?;
    let mut files = Vec::new();
    collect_files(&scope, &mut files)?;
    files.sort();

    let mut symbols = Vec::new();
    let mut indexed_files = Vec::new();
    let mut total_bytes = 0u64;
    for file in &files {
        let metadata = match fs::metadata(file) {
            Ok(metadata) => metadata,
            Err(_) => continue,
        };
        total_bytes = total_bytes.saturating_add(metadata.len());
        if total_bytes > MAX_INDEX_BYTES {
            break;
        }
        let relative = relative_path(&root, file);
        let Ok(content) = fs::read_to_string(file) else {
            continue;
        };
        indexed_files.push(file.clone());
        for (index, line) in content.lines().enumerate() {
            if let Some(symbol) = symbol_from_line(file, &relative, index, line) {
                symbols.push(symbol);
            }
        }
    }

    Ok((root, indexed_files, symbols))
}

fn query_tokens(query: &str) -> Vec<String> {
    query
        .split(|ch: char| !ch.is_alphanumeric() && ch != '_' && ch != '-')
        .filter(|token| !token.is_empty())
        .map(|token| token.to_ascii_lowercase())
        .collect()
}

fn score_symbol(query: &str, tokens: &[String], symbol: &SymbolRecord) -> i64 {
    let name = symbol.name.to_ascii_lowercase();
    let path = symbol.relative_path.to_ascii_lowercase();
    let signature = symbol.signature.to_ascii_lowercase();
    let query = query.to_ascii_lowercase();

    let mut score = 0;
    if name == query {
        score += 120;
    } else if name.starts_with(&query) {
        score += 95;
    } else if name.contains(&query) {
        score += 80;
    }

    if path.ends_with(&query) || path.contains(&query) {
        score += 50;
    }

    if signature.contains(&query) {
        score += 20;
    }

    for token in tokens {
        if name.contains(token) {
            score += 20;
        }
        if path.contains(token) {
            score += 10;
        }
        if signature.contains(token) {
            score += 5;
        }
    }

    score
}

pub fn semantic_search(
    query: &str,
    path: Option<&str>,
    max_results: Option<usize>,
) -> Result<Value> {
    let cleaned = query.trim();
    if cleaned.is_empty() {
        return Err(anyhow!("semantic_search requires a query"));
    }

    let (_root, files, symbols) = build_symbol_index(path)?;
    let tokens = query_tokens(cleaned);
    let limit = max_results.unwrap_or(DEFAULT_RESULTS).clamp(1, 200);

    let mut ranked: Vec<(i64, &SymbolRecord)> = symbols
        .iter()
        .map(|symbol| (score_symbol(cleaned, &tokens, symbol), symbol))
        .filter(|(score, _)| *score > 0)
        .collect();
    ranked.sort_by_key(|(score, symbol)| (Reverse(*score), &symbol.relative_path, symbol.line));
    ranked.truncate(limit);

    let results = ranked
        .into_iter()
        .map(|(score, symbol)| {
            json!({
                "name": symbol.name,
                "kind": symbol.kind.as_str(),
                "file": symbol.file,
                "relative_path": symbol.relative_path,
                "line": symbol.line,
                "signature": symbol.signature,
                "score": score,
            })
        })
        .collect::<Vec<_>>();

    Ok(json!({
        "status": "ok",
        "query": cleaned,
        "indexed_files": files.len(),
        "indexed_symbols": symbols.len(),
        "count": results.len(),
        "results": results,
    }))
}

pub fn find_definition(
    symbol: &str,
    path: Option<&str>,
    max_results: Option<usize>,
) -> Result<Value> {
    let cleaned = symbol.trim();
    if cleaned.is_empty() {
        return Err(anyhow!("lsp_find_definition requires a symbol"));
    }

    let (_root, files, symbols) = build_symbol_index(path)?;
    let cleaned_lower = cleaned.to_ascii_lowercase();
    let limit = max_results.unwrap_or(DEFAULT_RESULTS).clamp(1, 100);

    let mut matches = symbols
        .into_iter()
        .filter(|record| {
            let name = record.name.to_ascii_lowercase();
            name == cleaned_lower
                || name.starts_with(&cleaned_lower)
                || name.contains(&cleaned_lower)
        })
        .collect::<Vec<_>>();
    matches.sort_by(|a, b| {
        a.relative_path
            .cmp(&b.relative_path)
            .then_with(|| a.line.cmp(&b.line))
    });
    matches.truncate(limit);

    Ok(json!({
        "status": "ok",
        "symbol": cleaned,
        "indexed_files": files.len(),
        "count": matches.len(),
        "results": matches.into_iter().map(|record| json!({
            "name": record.name,
            "kind": record.kind.as_str(),
            "file": record.file,
            "relative_path": record.relative_path,
            "line": record.line,
            "signature": record.signature,
        })).collect::<Vec<_>>(),
    }))
}

pub fn find_references(
    symbol: &str,
    path: Option<&str>,
    max_results: Option<usize>,
) -> Result<Value> {
    let cleaned = symbol.trim();
    if cleaned.is_empty() {
        return Err(anyhow!("lsp_find_references requires a symbol"));
    }

    let (root, files, symbols) = build_symbol_index(path)?;
    let limit = max_results.unwrap_or(50).clamp(1, 500);
    let escaped = regex::escape(cleaned);
    let pattern = Regex::new(&format!(r"\b{}\b", escaped))
        .with_context(|| format!("Invalid symbol pattern: {cleaned}"))?;

    let definitions = symbols
        .iter()
        .filter(|record| record.name == cleaned)
        .map(|record| format!("{}:{}", record.relative_path, record.line))
        .collect::<HashSet<_>>();

    let mut matches = Vec::new();
    for file in files {
        let relative = relative_path(&root, &file);
        let Ok(content) = fs::read_to_string(&file) else {
            continue;
        };
        for (index, line) in content.lines().enumerate() {
            if pattern.is_match(line) {
                let key = format!("{}:{}", relative, index + 1);
                matches.push(ReferenceRecord {
                    file: file.display().to_string(),
                    relative_path: relative.clone(),
                    line: index + 1,
                    content: line.trim().chars().take(220).collect(),
                    is_definition: definitions.contains(&key),
                });
                if matches.len() >= limit {
                    break;
                }
            }
        }
        if matches.len() >= limit {
            break;
        }
    }

    Ok(json!({
        "status": "ok",
        "symbol": cleaned,
        "count": matches.len(),
        "results": matches,
    }))
}

pub fn rename_preview(
    symbol: &str,
    new_name: &str,
    path: Option<&str>,
    max_results: Option<usize>,
) -> Result<Value> {
    let symbol = symbol.trim();
    let new_name = new_name.trim();
    if symbol.is_empty() || new_name.is_empty() {
        return Err(anyhow!(
            "lsp_rename_preview requires both symbol and new_name"
        ));
    }

    let definitions = find_definition(symbol, path, max_results)?;
    let references = find_references(symbol, path, max_results)?;

    let definition_count = definitions
        .get("count")
        .and_then(|value| value.as_u64())
        .unwrap_or(0);
    let reference_count = references
        .get("count")
        .and_then(|value| value.as_u64())
        .unwrap_or(0);

    let preview = references
        .get("results")
        .and_then(|value| value.as_array())
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .take(max_results.unwrap_or(DEFAULT_RESULTS).clamp(1, 100))
        .map(|item| {
            let original = item
                .get("content")
                .and_then(|value| value.as_str())
                .unwrap_or_default();
            json!({
                "file": item.get("file").cloned().unwrap_or(Value::Null),
                "relative_path": item.get("relative_path").cloned().unwrap_or(Value::Null),
                "line": item.get("line").cloned().unwrap_or(Value::Null),
                "before": original,
                "after": original.replace(symbol, new_name),
                "is_definition": item.get("is_definition").cloned().unwrap_or(Value::Bool(false)),
            })
        })
        .collect::<Vec<_>>();

    Ok(json!({
        "status": "ok",
        "symbol": symbol,
        "new_name": new_name,
        "definition_count": definition_count,
        "reference_count": reference_count,
        "preview": preview,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_root(label: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("gc-semantic-{label}-{unique}"));
        fs::create_dir_all(&root).unwrap();
        root
    }

    #[test]
    fn semantic_search_finds_rust_symbols() {
        let root = temp_root("search");
        fs::write(
            root.join("lib.rs"),
            "pub struct UserService;\nimpl UserService {\n    pub fn authenticate_user(&self) {}\n}\n",
        )
        .unwrap();
        crate::workspace::open_folder(&root.display().to_string(), "semantic-test");

        let result =
            semantic_search("authenticate", Some(&root.display().to_string()), Some(10)).unwrap();
        assert_eq!(result["status"], "ok");
        assert!(result["count"].as_u64().unwrap() >= 1);
    }

    #[test]
    fn rename_preview_reports_reference_changes() {
        let root = temp_root("rename");
        fs::write(
            root.join("main.rs"),
            "fn old_name() {}\nfn run() { old_name(); }\n",
        )
        .unwrap();
        crate::workspace::open_folder(&root.display().to_string(), "semantic-test");

        let result = rename_preview(
            "old_name",
            "new_name",
            Some(&root.display().to_string()),
            Some(10),
        )
        .unwrap();
        assert_eq!(result["status"], "ok");
        assert_eq!(result["new_name"], "new_name");
        assert!(result["reference_count"].as_u64().unwrap() >= 2);
    }
}
