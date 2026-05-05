use std::collections::HashMap;
use std::path::{Path as StdPath, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::Html;
use axum::routing::{get, post};
use axum::{Json, Router};
use base64::Engine;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::sync::{mpsc, RwLock as TokioRwLock};
use tower_http::services::ServeDir;

use crate::agent::GenericAgent;
use crate::config;
use crate::remote;
use crate::error_memory::ErrorMemory;
use crate::skills::SkillsManager;
use crate::tools;
use crate::types::{FileEntry, LlmConfig};
use crate::workflow::{AgentMode, Workflow, WorkflowNode, WorkflowNodePayload};
use crate::workspace;

const APP_NAME: &str = "Generic Coder";
const APP_SUBTITLE: &str = "Autonomous development cockpit";
const MAX_CHAT_PROMPT_LEN: usize = 64_000;
const MAX_PENDING_TASKS: usize = 32;
const MAX_WORKSPACE_FILE_LIMIT: usize = 200;
const MAX_SAVED_SESSIONS: usize = 100;
const MAX_UPLOAD_BYTES: usize = 10 * 1024 * 1024;
const MAX_UPLOAD_BASE64_LEN: usize = ((MAX_UPLOAD_BYTES + 2) / 3) * 4;
const MAX_WORKSPACE_TEXT_PREVIEW_BYTES: usize = 2 * 1024 * 1024;
const MAX_WORKSPACE_IMAGE_PREVIEW_BYTES: usize = 5 * 1024 * 1024;

#[derive(Clone)]
pub struct ServeConfig {
    pub host: String,
    pub port: u16,
    pub project_dir: PathBuf,
    pub agent: Arc<TokioRwLock<GenericAgent>>,
    pub task_tx: mpsc::Sender<(String, String, mpsc::Sender<Value>)>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PendingTask {
    pub preview: String,
    pub final_text: String,
    pub done: bool,
    #[serde(default)]
    pub acp_events: Vec<serde_json::Value>,
}

#[derive(Debug, Clone)]
struct StoredSession {
    index: usize,
    preview: String,
    rounds: usize,
    saved_at: i64,
    messages: Vec<Value>,
}

pub struct AppState {
    project_dir: PathBuf,
    agent: Arc<TokioRwLock<GenericAgent>>,
    task_tx: mpsc::Sender<(String, String, mpsc::Sender<Value>)>,
    messages: RwLock<Vec<Value>>,
    pending: RwLock<HashMap<String, PendingTask>>,
    theme: RwLock<String>,
    sessions: RwLock<Vec<StoredSession>>,
    remote_form: RwLock<Value>,
    uploads_dir: PathBuf,
    local_only_ui: bool,
    workspace_picker_token: Option<String>,
    skills_manager: SkillsManager,
    error_memory: ErrorMemory,
    workflow: RwLock<Workflow>,
    current_mode: RwLock<AgentMode>,
    multi_agent_enabled: RwLock<bool>,
    one_shot_enabled: RwLock<bool>,
    computer_use_enabled: RwLock<bool>,
}

#[derive(Deserialize)]
struct FilesQuery {
    q: Option<String>,
    limit: Option<usize>,
}

#[derive(Deserialize)]
struct WorkspacePreviewQuery {
    path: String,
}

#[derive(Deserialize)]
struct IndexPayload {
    index: usize,
}

#[derive(Deserialize)]
struct WorkspacePayload {
    path: String,
    name: Option<String>,
}

#[derive(Deserialize)]
struct RemotePayload {
    enabled: bool,
    server_name: Option<String>,
    name: Option<String>,
    host: Option<String>,
    port: Option<u16>,
    username: Option<String>,
    password: Option<String>,
    key_path: Option<String>,
    cwd: Option<String>,
}

#[derive(Deserialize)]
struct ChatPayload {
    prompt: String,
}

#[derive(Deserialize)]
struct PathPayload {
    path: String,
}

#[derive(Deserialize)]
struct UploadPayload {
    data: String,
}

#[derive(Deserialize)]
struct LlmConfigPayload {
    entry_key: Option<String>,
    session_type: String,
    api_mode: Option<String>,
    name: Option<String>,
    model: String,
    apibase: String,
    apikey: String,
}

fn current_timestamp() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or_default()
}

fn canonicalize_for_access(path: &StdPath, allow_missing: bool) -> Option<PathBuf> {
    if !allow_missing || path.exists() {
        return std::fs::canonicalize(path).ok();
    }

    let parent = path.parent()?;
    let canonical_parent = std::fs::canonicalize(parent).ok()?;
    let file_name = path.file_name()?;
    Some(canonical_parent.join(file_name))
}

fn path_within_root(root: &StdPath, path: &str, allow_missing: bool) -> bool {
    let requested = if StdPath::new(path).is_absolute() {
        PathBuf::from(path)
    } else {
        root.join(path)
    };
    let Some(canonical_root) = std::fs::canonicalize(root).ok() else {
        return false;
    };
    let Some(canonical_path) = canonicalize_for_access(&requested, allow_missing) else {
        return false;
    };
    canonical_path.starts_with(canonical_root)
}

fn ui_path_allowed(state: &AppState, path: &str, allow_missing: bool) -> bool {
    let active = workspace::get_active_workspace();
    if active.get("status").and_then(|value| value.as_str()) == Some("success") {
        workspace::is_within_workspace(path)
    } else {
        path_within_root(&state.project_dir, path, allow_missing)
    }
}

fn preview_kind_and_mime(path: &StdPath) -> (&'static str, &'static str) {
    let ext = path
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_lowercase())
        .unwrap_or_default();

    match ext.as_str() {
        "png" => ("image", "image/png"),
        "jpg" | "jpeg" => ("image", "image/jpeg"),
        "gif" => ("image", "image/gif"),
        "webp" => ("image", "image/webp"),
        "svg" => ("image", "image/svg+xml"),
        "bmp" => ("image", "image/bmp"),
        "ico" => ("image", "image/x-icon"),
        _ => ("text", "text/plain; charset=utf-8"),
    }
}

fn summarize_messages(messages: &[Value]) -> String {
    messages
        .iter()
        .rev()
        .find_map(|msg| msg.get("content").and_then(|v| v.as_str()))
        .unwrap_or("No preview available")
        .chars()
        .take(120)
        .collect()
}

fn relative_time_label(saved_at: i64) -> String {
    let delta = (current_timestamp() - saved_at).max(0);
    if delta < 60 {
        "just now".into()
    } else if delta < 3600 {
        format!("{}m ago", delta / 60)
    } else if delta < 86_400 {
        format!("{}h ago", delta / 3600)
    } else {
        format!("{}d ago", delta / 86_400)
    }
}

fn normalize_session_name(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
        } else if (ch == '-' || ch == '_') && !out.ends_with('_') {
            out.push('_');
        }
    }
    out.trim_matches('_').to_string()
}

fn build_llm_key(payload: &LlmConfigPayload) -> String {
    let base = payload
        .name
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(&payload.model);
    let slug = normalize_session_name(base);
    if slug.is_empty() {
        format!("generic_coder_{}_config", payload.session_type)
    } else {
        format!("generic_coder_{}_{}_config", payload.session_type, slug)
    }
}

fn build_llm_config(payload: &LlmConfigPayload) -> LlmConfig {
    LlmConfig {
        name: payload
            .name
            .clone()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| payload.model.clone()),
        apikey: payload.apikey.clone(),
        apibase: payload.apibase.clone(),
        model: payload.model.clone(),
        context_win: 128_000,
        proxy: None,
        verify: true,
        max_retries: 1,
        stream: true,
        timeout: 300,
        read_timeout: 300,
        temperature: 1.0,
        max_tokens: Some(8192),
        reasoning_effort: None,
        service_tier: None,
        thinking_type: None,
        thinking_budget_tokens: None,
        api_mode: payload
            .api_mode
            .clone()
            .unwrap_or_else(|| "chat_completions".into()),
        extra_sys_prompt: String::new(),
    }
}

fn existing_llm_config(project_dir: &StdPath, key: &str) -> Option<LlmConfig> {
    current_llm_entries(project_dir)
        .into_iter()
        .find(|(entry_key, _)| entry_key == key)
        .map(|(_, cfg)| cfg)
}

fn current_llm_entries(project_dir: &StdPath) -> Vec<(String, LlmConfig)> {
    let cfg = config::load_config(project_dir);
    let mut entries: Vec<(String, LlmConfig)> = cfg.llm_configs.into_iter().collect();
    entries.sort_by(|(left, _), (right, _)| left.cmp(right));
    entries
}

async fn current_llm_index(state: &AppState) -> usize {
    state.agent.read().await.current_llm_no
}

async fn models_payload(state: &AppState) -> Value {
    let entries = current_llm_entries(&state.project_dir);
    let current_index = current_llm_index(state).await;
    let models: Vec<Value> = entries
        .iter()
        .enumerate()
        .map(|(index, (_key, cfg))| {
            let label = if cfg.name.trim().is_empty()
                || cfg.name.trim().eq_ignore_ascii_case(cfg.model.trim())
            {
                cfg.model.clone()
            } else {
                format!("{} · {}", cfg.name, cfg.model)
            };
            json!({
                "index": index,
                "label": label,
                "name": cfg.name,
                "model": cfg.model,
            })
        })
        .collect();
    json!({
        "models": models,
        "current_index": if models.is_empty() { 0 } else { current_index.min(models.len().saturating_sub(1)) },
    })
}

fn infer_provider_from_config(cfg: &LlmConfig) -> &'static str {
    let base = cfg.apibase.trim().to_ascii_lowercase();
    let model = cfg.model.trim().to_ascii_lowercase();
    if base.contains("deepseek.com") || model.starts_with("deepseek") {
        "DeepSeek"
    } else if base.contains("dashscope.aliyuncs.com")
        || model.starts_with("qwen")
        || model.starts_with("qwq")
    {
        "Qwen"
    } else if base.contains("moonshot.ai")
        || base.contains("moonshot.cn")
        || model.starts_with("kimi-")
    {
        "Kimi"
    } else if base.contains("minimaxi.com") || model.starts_with("minimax-") {
        "MiniMax"
    } else if base.contains("volces.com") || model.starts_with("doubao") {
        "Doubao"
    } else if base.contains("hunyuan.cloud.tencent.com") || model.starts_with("hunyuan") {
        "Hunyuan"
    } else if base.contains("qianfan.baidubce.com") || model.starts_with("ernie") {
        "ERNIE"
    } else if base.contains("bigmodel.cn") || model.starts_with("glm") {
        "Zhipu"
    } else if model.starts_with("mimo") {
        "Xiaomi"
    } else if base.contains("openrouter.ai") {
        "OpenRouter"
    } else if base.contains("openai.com") {
        "OpenAI"
    } else if base.contains("anthropic.com") || model.starts_with("claude") {
        "Anthropic"
    } else {
        ""
    }
}

async fn current_llm_form(state: &AppState) -> Value {
    let entries = current_llm_entries(&state.project_dir);
    let current_index = current_llm_index(state).await;
    let Some((key, cfg)) = entries.get(current_index) else {
        return json!({});
    };

    json!({
        "entry_key": key,
        "session_type": config::infer_session_type(key),
        "protocol_preset": "custom",
        "api_mode": cfg.api_mode,
        "provider": infer_provider_from_config(cfg),
        "name": cfg.name,
        "apikey": "",
        "has_apikey": state.local_only_ui && !cfg.apikey.trim().is_empty(),
        "apibase": cfg.apibase,
        "model": cfg.model,
    })
}

fn sanitized_remote_form(form: &Value) -> Value {
    let mut sanitized = form.clone();
    if let Some(obj) = sanitized.as_object_mut() {
        obj.insert("password".into(), Value::String(String::new()));
    }
    sanitized
}

fn workspace_payload() -> Value {
    let active = workspace::get_active_workspace();
    let active_payload = if active.get("status").and_then(|v| v.as_str()) == Some("success") {
        Some(json!({
            "name": active.get("name").and_then(|v| v.as_str()).unwrap_or(""),
            "path": active.get("path").and_then(|v| v.as_str()).unwrap_or(""),
        }))
    } else {
        None
    };

    let recent_folders = workspace::list_workspaces()
        .iter()
        .filter_map(|item| {
            item.get("path")
                .and_then(|v| v.as_str())
                .map(str::to_string)
        })
        .collect::<Vec<_>>();

    json!({
        "active": active_payload,
        "workspaces": workspace::list_workspaces(),
        "recent_folders": recent_folders,
    })
}

fn remote_payload(state: &AppState) -> Value {
    let form = sanitized_remote_form(&state.remote_form.read().clone());
    let active_connections = remote::list_active_connections();
    json!({
        "form": form,
        "configs": remote::list_configs(),
        "active_connections": active_connections,
        "connected": !active_connections.is_empty(),
    })
}

async fn reload_agent_from_disk(
    state: &AppState,
    preferred_key: Option<&str>,
) -> Result<usize, String> {
    let cfg = config::load_config(&state.project_dir);
    let mut entries: Vec<(String, LlmConfig)> = cfg
        .llm_configs
        .iter()
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect();
    entries.sort_by(|(left, _), (right, _)| left.cmp(right));

    let preferred_index = preferred_key
        .and_then(|key| entries.iter().position(|(entry_key, _)| entry_key == key))
        .unwrap_or(0);

    let mut agent = state.agent.write().await;
    agent
        .load_llm_sessions(&cfg.llm_configs, &cfg.mixin_configs)
        .map_err(|err| format!("{err:#}"))?;
    if !agent.llm_clients.is_empty() {
        let safe_index = preferred_index.min(agent.llm_clients.len().saturating_sub(1));
        agent
            .next_llm(safe_index as isize)
            .map_err(|err| format!("{err:#}"))?;
        Ok(safe_index)
    } else {
        agent.current_llm_no = 0;
        Ok(0)
    }
}

fn flatten_tree(entry: &FileEntry, depth: usize, rows: &mut Vec<Value>) {
    rows.push(json!({
        "name": entry.name,
        "path": entry.path,
        "depth": depth,
        "type": if entry.entry_type == "directory" { "dir" } else { "file" },
    }));
    for child in &entry.children {
        flatten_tree(child, depth + 1, rows);
    }
}

fn latest_backups(project_dir: &StdPath) -> HashMap<String, (String, PathBuf)> {
    let backup_dir = project_dir.join("temp").join("backups");
    let mut latest = HashMap::new();
    let Ok(entries) = std::fs::read_dir(backup_dir) else {
        return latest;
    };

    for entry in entries.flatten() {
        let filename = entry.file_name().to_string_lossy().to_string();
        let Some(marker) = filename.rfind("_global_") else {
            continue;
        };
        let encoded = &filename[..marker];
        let backup_time = filename[marker + "_global_".len()..].to_string();
        let decoded = encoded
            .replace("_COLON_", ":")
            .replace("_FS_", &std::path::MAIN_SEPARATOR.to_string());

        let replace = latest
            .get(&decoded)
            .map(|(existing, _)| backup_time > *existing)
            .unwrap_or(true);
        if replace {
            latest.insert(decoded, (backup_time, entry.path()));
        }
    }

    latest
}

fn json_error(status: StatusCode, message: impl Into<String>) -> (StatusCode, Json<Value>) {
    (status, Json(json!({ "error": message.into() })))
}

fn archive_current_session(state: &AppState) {
    let messages = state.messages.read().clone();
    if messages.is_empty() {
        return;
    }

    let mut sessions = state.sessions.write();
    let next_index = sessions
        .last()
        .map(|session| session.index + 1)
        .unwrap_or(1);
    let rounds = messages
        .iter()
        .filter(|message| message.get("role").and_then(|v| v.as_str()) == Some("assistant"))
        .count();
    sessions.push(StoredSession {
        index: next_index,
        preview: summarize_messages(&messages),
        rounds,
        saved_at: current_timestamp(),
        messages,
    });
    if sessions.len() > MAX_SAVED_SESSIONS {
        let overflow = sessions.len() - MAX_SAVED_SESSIONS;
        sessions.drain(0..overflow);
    }
}

async fn bootstrap(State(state): State<Arc<AppState>>) -> Json<Value> {
    let messages = state.messages.read().clone();
    let theme = state.theme.read().clone();
    let models = models_payload(&state).await;
    let current_form = current_llm_form(&state).await;
    let model_label = models
        .get("models")
        .and_then(|value| value.as_array())
        .and_then(|items| {
            items.get(
                models
                    .get("current_index")
                    .and_then(|value| value.as_u64())
                    .unwrap_or(0) as usize,
            )
        })
        .and_then(|item| item.get("model"))
        .and_then(|value| value.as_str())
        .unwrap_or("")
        .to_string();

    let agent_mode = { *state.current_mode.read() };
    let mode_str = match agent_mode {
        AgentMode::Work => "work",
        AgentMode::Plan => "plan",
        AgentMode::Review => "review",
    };
    let workflow = { state.workflow.read().clone() };

    Json(json!({
        "app_name": APP_NAME,
        "subtitle": APP_SUBTITLE,
        "theme": theme,
        "messages": messages,
        "is_running": state.agent.read().await.is_busy(),
        "model": model_label,
        "model_index": models.get("current_index").and_then(|value| value.as_u64()).unwrap_or(0),
        "workspace": workspace_payload(),
        "remote": remote_payload(&state),
        "llm_form": current_form,
        "models": models,
        "last_reply_time": current_timestamp(),
        "mode": mode_str,
        "workflow": serde_json::to_value(&workflow).unwrap_or_default(),
        "picker_token": state.workspace_picker_token.as_deref().unwrap_or(""),
        "multi_agent_enabled": *state.multi_agent_enabled.read(),
        "one_shot_enabled": *state.one_shot_enabled.read(),
        "computer_use_enabled": *state.computer_use_enabled.read(),
        "computer_use_available": cfg!(target_os = "macos") || cfg!(target_os = "linux"),
    }))
}

async fn health() -> Json<Value> {
    Json(json!({"status": "ok"}))
}

async fn set_theme(State(state): State<Arc<AppState>>, Json(payload): Json<Value>) -> Json<Value> {
    if let Some(theme) = payload.get("theme").and_then(|value| value.as_str()) {
        *state.theme.write() = theme.to_string();
    }
    Json(json!({"theme": state.theme.read().clone()}))
}

async fn models(State(state): State<Arc<AppState>>) -> Json<Value> {
    Json(models_payload(&state).await)
}

async fn settings(State(state): State<Arc<AppState>>) -> Json<Value> {
    Json(json!({
        "llm_form": current_llm_form(&state).await,
        "workspace": workspace_payload(),
        "remote": remote_payload(&state),
        "models": models_payload(&state).await,
    }))
}

async fn set_model(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<IndexPayload>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let mut agent = state.agent.write().await;
    if agent.llm_clients.is_empty() {
        return Err(json_error(StatusCode::BAD_REQUEST, "No model configured"));
    }
    let index = payload.index.min(agent.llm_clients.len().saturating_sub(1));
    agent
        .next_llm(index as isize)
        .map_err(|err| json_error(StatusCode::BAD_REQUEST, format!("{err:#}")))?;
    let model = agent.get_llm_name(true);
    Ok(Json(json!({"current_index": index, "model": model})))
}

async fn llm_config(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<LlmConfigPayload>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let generated_key = build_llm_key(&payload);
    let key = payload
        .entry_key
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(&generated_key)
        .to_string();
    let mut cfg = build_llm_config(&payload);
    if cfg.apikey.trim().is_empty() {
        let lookup_key = payload.entry_key.as_deref().unwrap_or(&key);
        if let Some(existing) = existing_llm_config(&state.project_dir, lookup_key) {
            cfg.apikey = existing.apikey;
        }
    }
    config::save_ui_llm_config_entry(&key, &cfg)
        .map_err(|err| json_error(StatusCode::INTERNAL_SERVER_ERROR, format!("{err:#}")))?;

    let index = reload_agent_from_disk(&state, Some(&key))
        .await
        .map_err(|err| json_error(StatusCode::INTERNAL_SERVER_ERROR, err))?;
    let model = state.agent.read().await.get_llm_name(true);

    Ok(Json(json!({
        "saved": true,
        "current_index": index,
        "model": model,
    })))
}

async fn set_workspace(
    State(_state): State<Arc<AppState>>,
    Json(payload): Json<WorkspacePayload>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let result = workspace::open_folder(&payload.path, payload.name.as_deref().unwrap_or(""));
    if result.get("status").and_then(|value| value.as_str()) != Some("success") {
        return Err(json_error(
            StatusCode::BAD_REQUEST,
            result
                .get("msg")
                .and_then(|value| value.as_str())
                .unwrap_or("Failed to open workspace"),
        ));
    }

    Ok(Json(json!({
        "active": {
            "name": result.get("name").and_then(|value| value.as_str()).unwrap_or(""),
            "path": result.get("path").and_then(|value| value.as_str()).unwrap_or(""),
        },
        "workspaces": workspace::list_workspaces(),
        "recent_folders": workspace::list_workspaces()
            .iter()
            .filter_map(|item| item.get("path").and_then(|value| value.as_str()).map(str::to_string))
            .collect::<Vec<_>>(),
    })))
}

/// Expose the workspace picker token to the frontend.
/// Only callable on loopback to prevent exposing the token externally.
async fn picker_token(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    if !state.local_only_ui {
        return Err(json_error(
            StatusCode::FORBIDDEN,
            "Picker token is only available on loopback",
        ));
    }
    Ok(Json(json!({
        "token": state.workspace_picker_token.as_deref().unwrap_or(""),
    })))
}

async fn pick_workspace_folder(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    if !state.local_only_ui {
        return Err(json_error(
            StatusCode::FORBIDDEN,
            "Folder picker is only available when Generic Coder is bound to loopback",
        ));
    }

    let is_ui_request = headers
        .get("x-generic-coder-ui")
        .and_then(|value| value.to_str().ok())
        == Some("1");
    if !is_ui_request {
        return Err(json_error(
            StatusCode::FORBIDDEN,
            "Folder picker requests must originate from the Generic Coder UI",
        ));
    }
    let Some(expected_token) = state.workspace_picker_token.as_deref() else {
        return Err(json_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "Folder picker is unavailable in this launch mode",
        ));
    };
    let provided_token = headers
        .get("x-generic-coder-picker-token")
        .and_then(|value| value.to_str().ok());
    if provided_token != Some(expected_token) {
        return Err(json_error(
            StatusCode::FORBIDDEN,
            "Folder picker token is missing or invalid",
        ));
    }

    let picked = tokio::task::spawn_blocking(move || -> Option<std::path::PathBuf> {
        // Strategy 1: rfd (native Rust file dialog) — skip on macOS terminal
        // because rfd panics+process-aborts when NSApplication isn't running.
        #[cfg(not(target_os = "macos"))]
        {
            let rfd_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                rfd::FileDialog::new()
                    .set_title("Select workspace folder")
                    .pick_folder()
            }));
            if let Ok(Some(path)) = rfd_result {
                return Some(path);
            }
        }

        // Strategy 2: osascript on macOS (works from any terminal process)
        #[cfg(target_os = "macos")]
        {
            let output = std::process::Command::new("osascript")
                .arg("-e")
                .arg(r#"tell application "Finder" to set f to choose folder with prompt "Select workspace folder""#)
                .arg("-e")
                .arg("POSIX path of f")
                .output()
                .ok();
            if let Some(out) = output {
                if out.status.success() {
                    let path_str = String::from_utf8_lossy(&out.stdout).trim().to_string();
                    if !path_str.is_empty() {
                        return Some(std::path::PathBuf::from(path_str));
                    }
                }
                // User cancelled (non-zero exit) → return None
                return None;
            }
        }

        // Strategy 3: zenity on Linux
        #[cfg(not(target_os = "macos"))]
        {
            let output = std::process::Command::new("zenity")
                .args(["--file-selection", "--directory", "--title=Select workspace folder"])
                .output()
                .ok();
            if let Some(out) = output {
                if out.status.success() {
                    let path_str = String::from_utf8_lossy(&out.stdout).trim().to_string();
                    if !path_str.is_empty() {
                        return Some(std::path::PathBuf::from(path_str));
                    }
                }
                return None;
            }
        }

        None
    })
    .await
    .map_err(|err| {
        json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Folder picker failed: {err}"),
        )
    })?;

    let Some(path) = picked else {
        return Ok(Json(json!({"path": null, "cancelled": true})));
    };

    Ok(Json(json!({
        "path": path.display().to_string(),
        "cancelled": false,
    })))
}

async fn connect_remote(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<RemotePayload>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let server_name = payload
        .server_name
        .clone()
        .or(payload.name.clone())
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "default".into());
    let host = payload.host.clone().unwrap_or_default();
    let port = payload.port.unwrap_or(22);
    let username = payload.username.clone().unwrap_or_else(|| "root".into());
    let password = payload.password.clone().unwrap_or_default();
    let key_path = payload.key_path.clone().unwrap_or_default();
    let cwd = payload.cwd.clone().unwrap_or_default();

    if payload.enabled && host.trim().is_empty() {
        return Err(json_error(StatusCode::BAD_REQUEST, "Remote host is required"));
    }

    let result = if payload.enabled {
        let value = remote::connect_global(
            &server_name,
            &host,
            port,
            &username,
            &password,
            &key_path,
            "",
            22,
            "",
        )
        .map_err(|err| json_error(StatusCode::BAD_REQUEST, format!("{err:#}")))?;
        if value.get("status").and_then(|item| item.as_str()) != Some("connected") {
            return Err(json_error(
                StatusCode::BAD_REQUEST,
                value
                    .get("msg")
                    .and_then(|item| item.as_str())
                    .unwrap_or("Remote connection failed"),
            ));
        }
        value
    } else {
        remote::disconnect_global(&server_name);
        json!({"status": "disconnected"})
    };

    *state.remote_form.write() = json!({
        "enabled": payload.enabled,
        "server_name": server_name,
        "name": server_name,
        "host": host,
        "port": port,
        "username": username,
        "password": "",
        "key_path": key_path,
        "cwd": cwd,
    });

    Ok(Json(json!({
        "message": result.get("msg").and_then(|value| value.as_str()).unwrap_or(if payload.enabled { "Remote environment connected" } else { "Remote environment disconnected" }),
        "form": sanitized_remote_form(&state.remote_form.read().clone()),
        "configs": remote::list_configs(),
        "active_connections": remote::list_active_connections(),
        "connected": !remote::list_active_connections().is_empty(),
    })))
}

async fn chat(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<ChatPayload>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let prompt = payload.prompt.trim().to_string();
    if prompt.is_empty() {
        return Err(json_error(StatusCode::BAD_REQUEST, "Prompt is required"));
    }
    if prompt.len() > MAX_CHAT_PROMPT_LEN {
        return Err(json_error(
            StatusCode::PAYLOAD_TOO_LARGE,
            format!("Prompt exceeds {} characters", MAX_CHAT_PROMPT_LEN),
        ));
    }
    if state.pending.read().len() >= MAX_PENDING_TASKS {
        return Err(json_error(
            StatusCode::TOO_MANY_REQUESTS,
            "Too many pending tasks; wait for current work to finish",
        ));
    }

    if prompt == "/new" {
        archive_current_session(&state);
        state.messages.write().clear();
        return Ok(Json(
            json!({"handled": true, "messages": Vec::<Value>::new()}),
        ));
    }

    if let Some(index_str) = prompt.strip_prefix("/continue ") {
        let Ok(target) = index_str.trim().parse::<usize>() else {
            return Err(json_error(StatusCode::BAD_REQUEST, "Invalid session index"));
        };
        let sessions = state.sessions.read();
        let Some(session) = sessions.iter().find(|item| item.index == target) else {
            return Err(json_error(StatusCode::NOT_FOUND, "Session not found"));
        };
        state.messages.write().clone_from(&session.messages);
        return Ok(Json(json!({
            "handled": true,
            "messages": session.messages.clone(),
            "notice": format!("Restored session #{target}"),
        })));
    }

    let task_id = uuid::Uuid::new_v4().to_string();
    state.pending.write().insert(
        task_id.clone(),
        PendingTask {
            preview: "Starting task...".into(),
            final_text: String::new(),
            done: false,
            acp_events: Vec::new(),
        },
    );
    state
        .messages
        .write()
        .push(json!({"role": "user", "content": prompt.clone()}));

    let (display_tx, mut display_rx) = mpsc::channel::<Value>(256);
    state
        .task_tx
        .send((prompt, "web".into(), display_tx))
        .await
        .map_err(|_| json_error(StatusCode::INTERNAL_SERVER_ERROR, "Agent task queue closed"))?;

    let task_id_for_spawn = task_id.clone();
    let state_for_spawn = state.clone();
    tokio::spawn(async move {
        let mut preview = String::new();
        let mut acp_events: Vec<serde_json::Value> = Vec::new();
        while let Some(item) = display_rx.recv().await {
            if let Some(next) = item.get("next").and_then(|value| value.as_str()) {
                preview.push_str(next);
                if let Some(entry) = state_for_spawn.pending.write().get_mut(&task_id_for_spawn) {
                    entry.preview = preview.clone();
                }
            }
            if let Some(acp) = item.get("acp") {
                if !acp.is_null() {
                    acp_events.push(acp.clone());
                    if let Some(entry) = state_for_spawn.pending.write().get_mut(&task_id_for_spawn) {
                        entry.acp_events = acp_events.clone();
                    }
                }
            }
            if let Some(done) = item.get("done").and_then(|value| value.as_str()) {
                let final_text = done.to_string();
                if let Some(entry) = state_for_spawn.pending.write().get_mut(&task_id_for_spawn) {
                    entry.preview = preview.clone();
                    entry.final_text = final_text.clone();
                    entry.done = true;
                }
                state_for_spawn
                    .messages
                    .write()
                    .push(json!({"role": "assistant", "content": final_text}));
                return;
            }
        }

        if let Some(entry) = state_for_spawn.pending.write().get_mut(&task_id_for_spawn) {
            entry.done = true;
            entry.final_text = preview.clone();
        }
        if !preview.is_empty() {
            state_for_spawn
                .messages
                .write()
                .push(json!({"role": "assistant", "content": preview}));
        }
    });

    Ok(Json(json!({"task_id": task_id})))
}

async fn task(State(state): State<Arc<AppState>>, Path(task_id): Path<String>) -> Json<Value> {
    let payload = {
        let mut pending = state.pending.write();
        let Some(existing) = pending.get(&task_id).cloned() else {
            return Json(json!({
                "done": true,
                "preview": "",
                "final": "",
            }));
        };
        if existing.done {
            pending.remove(&task_id);
        }
        existing
    };
    Json(json!({
        "done": payload.done,
        "preview": payload.preview,
        "final": payload.final_text,
        "acp_events": payload.acp_events,
    }))
}

async fn sessions(State(state): State<Arc<AppState>>) -> Json<Value> {
    let sessions = state.sessions.read();
    let payload: Vec<Value> = sessions
        .iter()
        .rev()
        .map(|session| {
            json!({
                "index": session.index,
                "rounds": session.rounds,
                "relative_time": relative_time_label(session.saved_at),
                "preview": session.preview,
            })
        })
        .collect();
    Json(json!({"sessions": payload}))
}

async fn stop_agent(State(state): State<Arc<AppState>>) -> Json<Value> {
    state.agent.read().await.abort();
    Json(json!({"ok": true}))
}

async fn list_changes(State(state): State<Arc<AppState>>) -> Json<Value> {
    let backups = latest_backups(&state.project_dir);
    let changes: Vec<Value> = backups
        .into_iter()
        .filter(|(path, _)| ui_path_allowed(&state, path, true))
        .map(|(path, (backup_time, _backup_path))| {
            json!({
                "path": path,
                "basename": StdPath::new(&path).file_name().and_then(|value| value.to_str()).unwrap_or(&path),
                "backup_time": backup_time,
            })
        })
        .collect();
    Json(json!({"changes": changes}))
}

async fn show_diff(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<PathPayload>,
) -> Json<Value> {
    if !ui_path_allowed(&state, &payload.path, false) {
        return Json(json!({"error": "Path is outside the active workspace"}));
    }
    let backups = latest_backups(&state.project_dir);
    let Some((_backup_time, backup_path)) = backups.get(&payload.path) else {
        return Json(json!({"error": "No backup found for this file"}));
    };

    let output = std::process::Command::new("git")
        .args([
            "--no-pager",
            "diff",
            "--no-index",
            "--",
            &backup_path.display().to_string(),
            &payload.path,
        ])
        .output();

    match output {
        Ok(result) => {
            let diff = String::from_utf8_lossy(&result.stdout).to_string();
            Json(json!({
                "has_changes": !diff.trim().is_empty(),
                "diff": diff,
            }))
        }
        Err(err) => Json(json!({"error": format!("{err}")})),
    }
}

async fn revert_file(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<PathPayload>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    if !ui_path_allowed(&state, &payload.path, true) {
        return Err(json_error(
            StatusCode::BAD_REQUEST,
            "Path is outside the active workspace",
        ));
    }
    let result = tools::file_revert(&payload.path, None)
        .map_err(|err| json_error(StatusCode::INTERNAL_SERVER_ERROR, format!("{err:#}")))?;
    if result.get("status").and_then(|value| value.as_str()) == Some("ok") {
        Ok(Json(result))
    } else {
        Err(json_error(
            StatusCode::BAD_REQUEST,
            result
                .get("message")
                .and_then(|value| value.as_str())
                .unwrap_or("Revert failed"),
        ))
    }
}

async fn workspace_tree() -> Json<Value> {
    let tree_result = workspace::get_tree("", 4);
    if tree_result.get("status").and_then(|value| value.as_str()) != Some("success") {
        return Json(
            json!({"error": tree_result.get("msg").and_then(|value| value.as_str()).unwrap_or("No workspace open")}),
        );
    }

    let Some(tree_value) = tree_result.get("tree") else {
        return Json(json!({"tree": Vec::<Value>::new()}));
    };
    let Ok(tree) = serde_json::from_value::<FileEntry>(tree_value.clone()) else {
        return Json(json!({"error": "Failed to decode workspace tree"}));
    };

    let mut rows = Vec::new();
    for child in &tree.children {
        flatten_tree(child, 0, &mut rows);
    }
    Json(json!({"tree": rows}))
}

async fn workspace_files(Query(query): Query<FilesQuery>) -> Json<Value> {
    let limit = query
        .limit
        .unwrap_or(10)
        .clamp(1, MAX_WORKSPACE_FILE_LIMIT);
    let results = workspace::search_files(query.q.as_deref().unwrap_or(""), "", limit);
    if results.get("status").and_then(|value| value.as_str()) != Some("success") {
        return Json(json!({"files": Vec::<Value>::new()}));
    }

    let files = results
        .get("results")
        .and_then(|value| value.as_array())
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(|item| {
            json!({
                "name": item.get("name").and_then(|value| value.as_str()).unwrap_or(""),
                "path": item.get("path").and_then(|value| value.as_str()).unwrap_or(""),
                "rel": item.get("relative").and_then(|value| value.as_str()).unwrap_or(""),
            })
        })
        .collect::<Vec<_>>();

    Json(json!({"files": files}))
}

async fn workspace_preview(
    State(state): State<Arc<AppState>>,
    Query(query): Query<WorkspacePreviewQuery>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let requested_path = query.path.trim();
    if requested_path.is_empty() {
        return Err(json_error(StatusCode::BAD_REQUEST, "File path is required"));
    }

    if !ui_path_allowed(&state, requested_path, false) {
        return Err(json_error(
            StatusCode::FORBIDDEN,
            "Path is outside the active workspace",
        ));
    }

    let path = std::fs::canonicalize(requested_path)
        .map_err(|err| json_error(StatusCode::NOT_FOUND, format!("{err}")))?;
    let metadata = std::fs::metadata(&path)
        .map_err(|err| json_error(StatusCode::NOT_FOUND, format!("{err}")))?;
    if metadata.is_dir() {
        return Err(json_error(StatusCode::BAD_REQUEST, "Preview supports files only"));
    }

    let (kind, mime) = preview_kind_and_mime(&path);
    let path_string = path.display().to_string();
    let relative = workspace::get_relative_path(&path_string);
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_string();

    let raw = std::fs::read(&path)
        .map_err(|err| json_error(StatusCode::INTERNAL_SERVER_ERROR, format!("{err}")))?;

    if kind == "image" {
        if raw.len() > MAX_WORKSPACE_IMAGE_PREVIEW_BYTES {
            return Err(json_error(
                StatusCode::PAYLOAD_TOO_LARGE,
                format!(
                    "Image preview exceeds {} bytes",
                    MAX_WORKSPACE_IMAGE_PREVIEW_BYTES
                ),
            ));
        }

        return Ok(Json(json!({
            "name": file_name,
            "path": path_string,
            "rel": relative,
            "kind": "image",
            "mime": mime,
            "size": raw.len(),
            "data_url": format!(
                "data:{};base64,{}",
                mime,
                base64::engine::general_purpose::STANDARD.encode(&raw)
            ),
        })));
    }

    let truncated = raw.len() > MAX_WORKSPACE_TEXT_PREVIEW_BYTES;
    let visible = if truncated {
        &raw[..MAX_WORKSPACE_TEXT_PREVIEW_BYTES]
    } else {
        &raw[..]
    };

    if visible.contains(&0) {
        return Ok(Json(json!({
            "name": file_name,
            "path": path_string,
            "rel": relative,
            "kind": "binary",
            "mime": "application/octet-stream",
            "size": raw.len(),
            "message": "Binary file preview is not supported.",
        })));
    }

    let content = String::from_utf8_lossy(visible).to_string();
    Ok(Json(json!({
        "name": file_name,
        "path": path_string,
        "rel": relative,
        "kind": "text",
        "mime": mime,
        "size": raw.len(),
        "truncated": truncated,
        "content": content,
    })))
}

async fn plan_status() -> Json<Value> {
    Json(json!({
        "in_plan": false,
        "plan_path": "",
        "remaining": -1,
    }))
}

async fn upload_image(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<UploadPayload>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let Some((header, encoded)) = payload.data.split_once(",") else {
        return Err(json_error(StatusCode::BAD_REQUEST, "Invalid data URL"));
    };
    let extension = if header.contains("image/png") {
        "png"
    } else if header.contains("image/jpeg") {
        "jpg"
    } else if header.contains("image/webp") {
        "webp"
    } else {
        return Err(json_error(
            StatusCode::BAD_REQUEST,
            "Only PNG, JPEG, and WebP uploads are supported",
        ));
    };
    if encoded.len() > MAX_UPLOAD_BASE64_LEN {
        return Err(json_error(
            StatusCode::PAYLOAD_TOO_LARGE,
            "Upload exceeds 10 MB limit",
        ));
    }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .map_err(|err| {
            json_error(
                StatusCode::BAD_REQUEST,
                format!("Invalid base64 payload: {err}"),
            )
        })?;
    if bytes.len() > MAX_UPLOAD_BYTES {
        return Err(json_error(
            StatusCode::PAYLOAD_TOO_LARGE,
            "Upload exceeds 10 MB limit",
        ));
    }

    std::fs::create_dir_all(&state.uploads_dir)
        .map_err(|err| json_error(StatusCode::INTERNAL_SERVER_ERROR, format!("{err}")))?;
    let filename = format!("upload_{}.{}", uuid::Uuid::new_v4(), extension);
    let file_path = state.uploads_dir.join(filename);
    std::fs::write(&file_path, bytes)
        .map_err(|err| json_error(StatusCode::INTERNAL_SERVER_ERROR, format!("{err}")))?;

    let relative_path = file_path
        .strip_prefix(&state.project_dir)
        .map(|path| path.display().to_string())
        .unwrap_or_else(|_| file_path.display().to_string());

    Ok(Json(json!({"path": relative_path})))
}

// ── Skills API handlers ──────────────────────────────────

#[derive(Deserialize)]
struct InstallSkillPayload {
    url: String,
}

async fn skills_list(State(state): State<Arc<AppState>>) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let skills = state
        .skills_manager
        .list_skills()
        .map_err(|e| json_error(StatusCode::INTERNAL_SERVER_ERROR, e))?;
    Ok(Json(serde_json::to_value(skills).unwrap_or_default()))
}

async fn skills_install(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<InstallSkillPayload>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let url = payload.url.trim().to_string();
    if url.is_empty() {
        return Err(json_error(StatusCode::BAD_REQUEST, "URL is required"));
    }
    let entry = tokio::task::spawn_blocking(move || state.skills_manager.install_skill(&url))
        .await
        .map_err(|e| json_error(StatusCode::INTERNAL_SERVER_ERROR, format!("{e}")))?
        .map_err(|e| json_error(StatusCode::BAD_REQUEST, e))?;
    Ok(Json(serde_json::to_value(entry).unwrap_or_default()))
}

async fn skills_toggle(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let entry = state
        .skills_manager
        .toggle_skill(&name)
        .map_err(|e| json_error(StatusCode::NOT_FOUND, e))?;
    Ok(Json(serde_json::to_value(entry).unwrap_or_default()))
}

async fn skills_delete(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    state
        .skills_manager
        .delete_skill(&name)
        .map_err(|e| json_error(StatusCode::NOT_FOUND, e))?;
    Ok(Json(json!({"deleted": name})))
}

async fn skills_upgrade(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let entry = tokio::task::spawn_blocking(move || state.skills_manager.upgrade_skill(&name))
        .await
        .map_err(|e| json_error(StatusCode::INTERNAL_SERVER_ERROR, format!("{e}")))?
        .map_err(|e| json_error(StatusCode::NOT_FOUND, e))?;
    Ok(Json(serde_json::to_value(entry).unwrap_or_default()))
}

async fn skills_preview(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let preview = state
        .skills_manager
        .preview_skill(&name)
        .map_err(|e| json_error(StatusCode::NOT_FOUND, e))?;
    Ok(Json(preview))
}

// ── Error Log API handlers ──────────────────────────────

async fn errors_list(State(state): State<Arc<AppState>>) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let records = state
        .error_memory
        .list_records()
        .map_err(|e| json_error(StatusCode::INTERNAL_SERVER_ERROR, e))?;
    let total = state
        .error_memory
        .total_errors()
        .unwrap_or(0);
    let warnings = state
        .error_memory
        .active_warnings()
        .map_err(|e| json_error(StatusCode::INTERNAL_SERVER_ERROR, e))?;
    Ok(Json(json!({
        "records": records,
        "total": total,
        "active_warnings": warnings.len(),
    })))
}

async fn errors_clear(State(state): State<Arc<AppState>>) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    state
        .error_memory
        .clear()
        .map_err(|e| json_error(StatusCode::INTERNAL_SERVER_ERROR, e))?;
    Ok(Json(json!({"cleared": true})))
}

async fn errors_forget(
    State(state): State<Arc<AppState>>,
    Path(fingerprint): Path<String>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    state
        .error_memory
        .forget(&fingerprint)
        .map_err(|e| json_error(StatusCode::NOT_FOUND, e))?;
    Ok(Json(json!({"forgotten": fingerprint})))
}

// ── Mode API handlers ────────────────────────────────────

#[derive(Deserialize)]
struct ModePayload {
    mode: String,
}

async fn get_mode(State(state): State<Arc<AppState>>) -> Json<Value> {
    let mode = *state.current_mode.read();
    let mode_str = match mode {
        AgentMode::Work => "work",
        AgentMode::Plan => "plan",
        AgentMode::Review => "review",
    };
    Json(json!({
        "mode": mode_str,
        "workflow_active": state.workflow.read().active,
    }))
}

async fn set_mode(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<ModePayload>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let mode = AgentMode::from_str(&payload.mode)
        .ok_or_else(|| json_error(StatusCode::BAD_REQUEST, "Invalid mode. Use: work, plan, review"))?;
    *state.current_mode.write() = mode;
    state.agent.read().await.set_mode(mode);
    let mode_str = match mode {
        AgentMode::Work => "work",
        AgentMode::Plan => "plan",
        AgentMode::Review => "review",
    };
    Ok(Json(json!({"mode": mode_str})))
}

// ── Workflow API handlers ────────────────────────────────

#[derive(Deserialize)]
struct WorkflowSetPayload {
    nodes: Vec<WorkflowNodePayload>,
}

async fn get_workflow(State(state): State<Arc<AppState>>) -> Json<Value> {
    let wf = state.workflow.read();
    Json(serde_json::to_value(&*wf).unwrap_or_default())
}

async fn set_workflow(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<WorkflowSetPayload>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let nodes: Vec<WorkflowNode> = payload
        .nodes
        .into_iter()
        .map(|n| {
            let mode = AgentMode::from_str(&n.mode).unwrap_or(AgentMode::Work);
            WorkflowNode {
                mode,
                label: n.label,
                completed: false,
            }
        })
        .collect();

    let validation = Workflow::validate(&nodes);
    if !validation.valid {
        return Err(json_error(
            StatusCode::BAD_REQUEST,
            validation.reason.unwrap_or_else(|| "Invalid workflow".into()),
        ));
    }

    let wf_snapshot = {
        let mut guard = state.workflow.write();
        guard.set_active(nodes);
        guard.clone()
    };
    state.agent.read().await.set_workflow(wf_snapshot.clone());
    Ok(Json(serde_json::to_value(&wf_snapshot).unwrap_or_default()))
}

async fn reset_workflow(State(state): State<Arc<AppState>>) -> Json<Value> {
    let wf_snapshot = {
        let mut guard = state.workflow.write();
        guard.reset();
        guard.clone()
    };
    state.agent.read().await.set_workflow(wf_snapshot);
    Json(json!({"reset": true}))
}

async fn get_multi_agent(State(state): State<Arc<AppState>>) -> Json<Value> {
    let enabled = *state.multi_agent_enabled.read();
    Json(json!({"enabled": enabled}))
}

async fn set_multi_agent(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<Value>,
) -> Json<Value> {
    let enabled = payload.get("enabled").and_then(|v| v.as_bool()).unwrap_or(false);
    if enabled && *state.one_shot_enabled.read() {
        // If one_shot is also being disabled, allow it
        let also_disable_one_shot = payload.get("disable_one_shot").and_then(|v| v.as_bool()).unwrap_or(false);
        if !also_disable_one_shot {
            return Json(json!({"error": "不能同时启用 Multi-Agent 和 One Shot"}));
        }
    }
    *state.multi_agent_enabled.write() = enabled;
    state.agent.read().await.set_multi_agent(enabled);
    Json(json!({"enabled": enabled}))
}

/// Heuristic: determine whether a prompt is suitable for multi-agent decomposition.
/// Suitable prompts involve multiple steps, file exploration, implementation + review,
/// or complex reasoning. Unsuitable prompts are trivial queries, single calculations,
/// or very short commands.
async fn check_multi_agent_suitable(
    Json(payload): Json<Value>,
) -> Json<Value> {
    let prompt = payload
        .get("prompt")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();

    if prompt.is_empty() {
        return Json(json!({"suitable": false, "reason": "no prompt provided"}));
    }

    let lower = prompt.to_lowercase();

    // — unsuitability heuristics —
    // Very short prompts (single trivial query)
    let char_count = prompt.chars().count();
    if char_count < 8 {
        return Json(json!({
            "suitable": false,
            "reason": "prompt too short for multi-agent decomposition"
        }));
    }

    // Pure calculation / single-answer queries
    let trivial_patterns = [
        "几点了", "现在几点", "今天星期", "今天日期",
        "what time", "what day", "today's date",
        "你好", "hello", "hi", "hey",
        "谢谢", "thanks", "thank you",
        "退出", "exit", "quit",
        "帮助", "help",
        "clear", "清除", "重置", "reset",
    ];
    for pat in &trivial_patterns {
        if lower.contains(pat) {
            return Json(json!({
                "suitable": false,
                "reason": "trivial query, no multi-agent needed"
            }));
        }
    }

    // Pure arithmetic without broader context
    let arithmetic_only = lower
        .chars()
        .all(|c| c.is_ascii_digit() || "+-*/=×÷加减乘除等于".contains(c) || c.is_whitespace());
    if arithmetic_only && !lower.contains("文件") && !lower.contains("代码") && !lower.contains("实现") {
        return Json(json!({
            "suitable": false,
            "reason": "pure arithmetic doesn't benefit from multi-agent decomposition"
        }));
    }

    // Multi-step structure patterns (user explicitly describes sequential steps)
    let structural_patterns = [
        "一个", "另一个", "第一步", "第二步", "首先", "然后", "接着", "最后",
        "first", "then", "next", "finally",
        "step 1", "step 2", "step1", "step2",
        "之后", "再", "还要",
    ];
    for pat in &structural_patterns {
        if lower.contains(pat) && char_count >= 15 {
            return Json(json!({"suitable": true}));
        }
    }

    // — suitability heuristics —
    let suitable_keywords = [
        "实现", "implement", "开发", "develop",
        "重构", "refactor", "优化", "optimize",
        "分析", "analyze", "审查", "review", "audit",
        "搜索", "search", "查找", "find",
        "修复", "fix", "调试", "debug",
        "设计", "design", "架构", "architecture",
        "测试", "test", "部署", "deploy",
        "文档", "document", "迁移", "migrate",
        "添加功能", "add feature", "添加特性",
        "修改", "modify", "change", "更新", "update",
        "创建", "create", "新建", "build",
        "并", "and then", "然后", "之后",
        "多个", "multiple", "全部", "all",
    ];
    let mut suitable = false;
    for kw in &suitable_keywords {
        if lower.contains(kw) {
            suitable = true;
            break;
        }
    }

    // Complex enough (longer prompts are more likely multi-step)
    if !suitable && char_count >= 60 {
        suitable = true;
    }

    if suitable {
        Json(json!({"suitable": true}))
    } else {
        Json(json!({
            "suitable": false,
            "reason": "prompt is too simple to benefit from multi-agent decomposition; try a task involving multiple steps or file/code operations"
        }))
    }
}

// ── One Shot API ──────────────────────────────────────────────────────────────

async fn get_one_shot(State(state): State<Arc<AppState>>) -> Json<Value> {
    let enabled = *state.one_shot_enabled.read();
    Json(json!({"enabled": enabled}))
}

async fn set_one_shot(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<Value>,
) -> Json<Value> {
    let enabled = payload.get("enabled").and_then(|v| v.as_bool()).unwrap_or(false);
    if enabled && *state.multi_agent_enabled.read() {
        // If multi_agent is also being disabled, allow it
        let also_disable_ma = payload.get("disable_multi_agent").and_then(|v| v.as_bool()).unwrap_or(false);
        if !also_disable_ma {
            return Json(json!({"error": "不能同时启用 One Shot 和 Multi-Agent"}));
        }
    }
    *state.one_shot_enabled.write() = enabled;
    state.agent.read().await.set_one_shot(enabled);
    Json(json!({"enabled": enabled}))
}

async fn get_computer_use(State(state): State<Arc<AppState>>) -> Json<Value> {
    let enabled = *state.computer_use_enabled.read();
    Json(json!({"enabled": enabled, "available": cfg!(target_os = "macos") || cfg!(target_os = "linux")}))
}

async fn set_computer_use(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<Value>,
) -> Json<Value> {
    let enabled = payload.get("enabled").and_then(|v| v.as_bool()).unwrap_or(false);
    *state.computer_use_enabled.write() = enabled;
    Json(json!({"enabled": enabled}))
}

pub fn create_app(state: Arc<AppState>) -> Router {
    let assets_dir = state.project_dir.join("assets").join("generic_coder");
    Router::new()
        .route(
            "/",
            get({
                let state = state.clone();
                move || {
                    let state = state.clone();
                    async move {
                        let html = std::fs::read_to_string(
                            state
                                .project_dir
                                .join("assets")
                                .join("generic_coder")
                                .join("index.html"),
                        )
                        .unwrap_or_default();
                        Html(html)
                    }
                }
            }),
        )
        .nest_service("/static", ServeDir::new(assets_dir))
        .route("/health", get(health))
        .route("/api/bootstrap", get(bootstrap))
        .route("/api/theme", post(set_theme))
        .route("/api/models", get(models))
        .route("/api/settings", get(settings))
        .route("/api/model", post(set_model))
        .route("/api/llm-config", post(llm_config))
        .route("/api/workspace", post(set_workspace))
        .route("/api/workspace/pick", post(pick_workspace_folder))
        .route("/api/workspace/picker-token", get(picker_token))
        .route("/api/remote/connect", post(connect_remote))
        .route("/api/chat", post(chat))
        .route("/api/tasks/{task_id}", get(task))
        .route("/api/sessions", get(sessions))
        .route("/api/stop", post(stop_agent))
        .route("/api/changes", get(list_changes))
        .route("/api/diff", post(show_diff))
        .route("/api/revert", post(revert_file))
        .route("/api/workspace/tree", get(workspace_tree))
        .route("/api/workspace/files", get(workspace_files))
        .route("/api/workspace/preview", get(workspace_preview))
        .route("/api/plan/status", get(plan_status))
        .route("/api/upload", post(upload_image))
        .route("/api/skills", get(skills_list))
        .route("/api/skills/install", post(skills_install))
        .route("/api/skills/{name}/toggle", post(skills_toggle))
        .route("/api/skills/{name}/delete", post(skills_delete))
        .route("/api/skills/{name}/upgrade", post(skills_upgrade))
        .route("/api/skills/{name}/preview", get(skills_preview))
        .route("/api/errors", get(errors_list))
        .route("/api/errors", post(errors_clear))
        .route("/api/errors/{fingerprint}", post(errors_forget))
        .route("/api/mode", get(get_mode))
        .route("/api/mode", post(set_mode))
        .route("/api/workflow", get(get_workflow))
        .route("/api/workflow", post(set_workflow))
        .route("/api/workflow/reset", post(reset_workflow))
        .route("/api/multi-agent", get(get_multi_agent))
        .route("/api/multi-agent", post(set_multi_agent))
        .route("/api/multi-agent/suitable", post(check_multi_agent_suitable))
        .route("/api/one-shot", get(get_one_shot))
        .route("/api/one-shot", post(set_one_shot))
        .route("/api/computer-use", get(get_computer_use))
        .route("/api/computer-use", post(set_computer_use))
        .with_state(state)
}

pub async fn serve(config: ServeConfig) -> anyhow::Result<()> {
    let initial_remote_form = json!({
        "enabled": false,
        "server_name": "",
        "name": "",
        "host": "",
        "port": 22,
        "username": "root",
        "password": "",
        "key_path": "",
        "cwd": "",
    });

    let state = Arc::new(AppState {
        uploads_dir: config.project_dir.join("temp").join("uploads"),
        project_dir: config.project_dir.clone(),
        agent: config.agent,
        task_tx: config.task_tx,
        messages: RwLock::new(Vec::new()),
        pending: RwLock::new(HashMap::new()),
        theme: RwLock::new("solarflare".into()),
        sessions: RwLock::new(Vec::new()),
        remote_form: RwLock::new(initial_remote_form),
        local_only_ui: matches!(config.host.as_str(), "127.0.0.1" | "::1" | "localhost"),
        workspace_picker_token: {
            let env_token = std::env::var("GENERIC_CODER_PICKER_TOKEN")
                .ok()
                .filter(|v| !v.trim().is_empty());
            // When running on loopback without an explicit env token, auto-generate one
            // so the native folder picker works out of the box on local dev.
            env_token.or_else(|| {
                if matches!(config.host.as_str(), "127.0.0.1" | "::1" | "localhost") {
                    let token = uuid::Uuid::new_v4().to_string().replace('-', "");
                    log::info!("Auto-generated workspace picker token: {token}");
                    Some(token)
                } else {
                    None
                }
            })
        },
        skills_manager: SkillsManager::new(&config.project_dir),
        error_memory: ErrorMemory::new(&config.project_dir),
        workflow: RwLock::new(Workflow::default()),
        current_mode: RwLock::new(AgentMode::Work),
        multi_agent_enabled: RwLock::new(false),
        one_shot_enabled: RwLock::new(false),
        computer_use_enabled: RwLock::new(true), // enabled by default on supported platforms
    });

    // Bootstrap preset skills (auto-register any new skill dirs in skills/)
    if let Err(e) = state.skills_manager.bootstrap_presets() {
        log::warn!("Failed to bootstrap preset skills: {e}");
    }

    let addr = format!("{}:{}", config.host, config.port);
    log::info!("{APP_NAME} web UI starting at http://{addr}");
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, create_app(state)).await?;
    Ok(())
}
