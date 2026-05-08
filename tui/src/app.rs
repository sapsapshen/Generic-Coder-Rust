//! Application state and main run loop for Generic Coder TUI.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use anyhow::Result;
use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseEventKind};
use ratatui::prelude::*;
use serde_json::Value;
use tokio::sync::{mpsc, RwLock};

use generic_coder::agent::GenericAgent;
use generic_coder::config;
use generic_coder::error_memory::ErrorMemory;
use generic_coder::provider_profiles;
use generic_coder::session_store;
use generic_coder::skills::SkillsManager;
use generic_coder::types::{LlmConfig, ToolSchema};
use generic_coder::workflow::AgentMode;
use generic_coder::{tools, workspace};

use crate::event::{InputMode, Panel};
use crate::ui;

/// Represents a single chat message
#[derive(Clone)]
pub struct ChatMessage {
    pub role: String, // "user" or "assistant"
    pub content: String,
    pub streaming: bool,
    pub acp: Option<AcpState>,
}

/// ACP multi-agent state
#[derive(Clone)]
#[allow(dead_code)]
pub struct AcpState {
    pub plan: Option<Value>,
    pub active_step: i32,
    pub completed_steps: Vec<Value>,
    pub failed_steps: Vec<Value>,
    pub done: bool,
}

/// Modal dialog types
pub enum Dialog {
    None,
    Settings(SettingsTab),
    Sessions,
    Help,
}

/// Settings sub-tabs
#[derive(Clone, Copy, PartialEq)]
#[allow(dead_code)]
pub enum SettingsTab {
    Model,
    Workspace,
    Remote,
    Skills,
    Interface,
}

/// Main application state
#[allow(dead_code)]
pub struct App {
    // ── Agent ─────────────────────────────────
    pub agent: Arc<RwLock<GenericAgent>>,
    pub task_tx: mpsc::Sender<(String, String, mpsc::Sender<Value>)>,
    pub is_running: bool,
    pub pending_task_id: Option<String>,

    // ── Chat ──────────────────────────────────
    pub messages: Vec<ChatMessage>,
    pub input: String,
    pub input_cursor: usize,
    pub scroll_offset: usize,

    // ── Panels & navigation ───────────────────
    pub active_panel: Panel,
    pub input_mode: InputMode,
    pub dialog: Dialog,
    pub status_msg: String,
    pub status_timer: Option<Instant>,

    // ── Sidebar ───────────────────────────────
    pub sidebar_width: u16,
    pub current_mode: AgentMode,
    pub multi_agent_enabled: bool,
    pub one_shot_enabled: bool,

    // ── Model context ─────────────────────────
    pub models: Vec<(String, String)>, // (key, display_name)
    pub current_model_idx: usize,
    pub model_label: String,

    // ── Config forms ──────────────────────────
    pub llm_configs: HashMap<String, LlmConfig>,
    pub llm_form: LlmForm,
    pub workspace_form: WorkspaceForm,
    pub remote_form: RemoteForm,
    pub workspace_path: String,
    pub workspace_name: String,

    // ── Workspace tree ────────────────────────
    pub workspace_tree: Vec<FileEntry>,
    pub workspace_tree_scroll: usize,
    pub workspace_tree_cursor: usize,

    // ── Git changes ───────────────────────────
    pub changes: Vec<ChangeEntry>,
    pub changes_visible: bool,

    // ── Sessions ──────────────────────────────
    pub sessions: Vec<SessionEntry>,
    pub sessions_cursor: usize,
    pub sessions_checkpoint_cursor: usize,

    // ── System ────────────────────────────────
    pub project_dir: PathBuf,
    pub system_prompt: String,
    pub tools_schema: Vec<ToolSchema>,
    pub error_memory: ErrorMemory,
    pub skills_manager: SkillsManager,
    pub running: bool,

    // ── Settings form ─────────────────────────
    pub settings_tab: SettingsTab,
    pub settings_models: Vec<(String, LlmConfig)>,
    pub settings_cursor: usize,
    pub settings_editing_field: Option<SettingsField>,
    pub skills_list: Vec<SkillInfo>,
    pub skills_cursor: usize,

    // ── Theme ──────────────────────────────────
    pub theme_name: String,

    // ── DeepSeek-TUI inspired features ────────────────────────
    pub yolo_enabled: bool,
    pub reasoning_effort: Option<String>, // None / "off" / "high" / "max"
    pub last_usage: Option<(u64, u64, u64)>, // (prompt, completion, cached)
    pub session_usage: session_store::TokenUsage,
    pub auto_model_enabled: bool,
    pub auto_route: Option<generic_coder::agent::AutoRouteDecision>,
    pub active_session_index: Option<usize>,
}

#[derive(Clone)]
#[allow(dead_code)]
pub struct FileEntry {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub depth: usize,
}

#[derive(Clone)]
#[allow(dead_code)]
pub struct ChangeEntry {
    pub path: String,
    pub basename: String,
    pub time: String,
}

#[derive(Clone)]
#[allow(dead_code)]
pub struct SessionEntry {
    pub index: usize,
    pub preview: String,
    pub rounds: usize,
    pub time: String,
    pub checkpoint_count: usize,
}

fn relative_session_time(saved_at: i64) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or_default();
    let delta = (now - saved_at).max(0);
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

#[derive(Clone)]
pub struct SkillInfo {
    pub name: String,
    pub version: String,
    pub description: String,
    pub enabled: bool,
}

#[derive(Clone)]
#[allow(dead_code)]
pub struct LlmForm {
    pub entry_key: String,
    pub session_type: String,
    pub protocol_preset: String,
    pub api_mode: String,
    pub provider: String,
    pub display_name: String,
    pub model: String,
    pub api_base: String,
    pub api_key: String,
}

impl Default for LlmForm {
    fn default() -> Self {
        Self {
            entry_key: "generic_coder_native_oai_config".into(),
            session_type: "native_oai".into(),
            protocol_preset: "custom".into(),
            api_mode: "chat_completions".into(),
            provider: String::new(),
            display_name: String::new(),
            model: String::new(),
            api_base: String::new(),
            api_key: String::new(),
        }
    }
}

#[derive(Clone)]
#[allow(dead_code)]
pub struct WorkspaceForm {
    pub name: String,
    pub path: String,
}

impl Default for WorkspaceForm {
    fn default() -> Self {
        Self {
            name: String::new(),
            path: String::new(),
        }
    }
}

#[derive(Clone)]
#[allow(dead_code)]
pub struct RemoteForm {
    pub enabled: bool,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password: String,
    pub key_path: String,
    pub cwd: String,
}

impl Default for RemoteForm {
    fn default() -> Self {
        Self {
            enabled: false,
            name: String::new(),
            host: String::new(),
            port: 22,
            username: "root".into(),
            password: String::new(),
            key_path: String::new(),
            cwd: String::new(),
        }
    }
}

#[derive(Clone)]
#[allow(dead_code)]
pub enum SettingsField {
    ProtocolPreset,
    SessionType,
    Provider,
    DisplayName,
    ModelName,
    BaseUrl,
    ApiKey,
    WorkspaceName,
    WorkspacePath,
    RemoteName,
    RemoteHost,
    RemotePort,
    RemoteUsername,
    RemotePassword,
    RemoteKeyPath,
    RemoteCwd,
}

impl App {
    pub async fn new(
        agent: Arc<RwLock<GenericAgent>>,
        task_tx: mpsc::Sender<(String, String, mpsc::Sender<Value>)>,
        project_dir: PathBuf,
        system_prompt: String,
        tools_schema: Vec<ToolSchema>,
        error_memory: ErrorMemory,
    ) -> Self {
        let skills_manager = SkillsManager::new(&project_dir);
        let _ = skills_manager.bootstrap_presets();

        let llm_configs = config::load_ui_llm_configs();
        let mut models: Vec<(String, String)> = llm_configs
            .iter()
            .map(|(k, v)| (k.clone(), v.name.clone()))
            .collect();
        if models.is_empty() {
            models.push(("none".into(), "No models configured".into()));
        }

        // Load workspace info
        let ws = workspace::get_active_workspace();
        let ws_path = ws
            .get("path")
            .and_then(|v| v.as_str())
            .map(String::from)
            .unwrap_or_default();
        let ws_name = ws
            .get("name")
            .and_then(|v| v.as_str())
            .map(String::from)
            .unwrap_or_default();

        // Load skills
        let skills_list: Vec<SkillInfo> = skills_manager
            .list_skills()
            .unwrap_or_default()
            .iter()
            .map(|s| SkillInfo {
                name: s.name.clone(),
                version: s.version.clone(),
                description: s.description.clone(),
                enabled: s.enabled,
            })
            .collect();

        // Load theme
        let theme = std::fs::read_to_string(config::config_dir().join("theme.json"))
            .ok()
            .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
            .and_then(|v| v.get("theme").and_then(|t| t.as_str()).map(String::from))
            .unwrap_or_else(|| "solarflare".into());

        let auto_model_enabled = agent
            .try_read()
            .map(|guard| guard.is_auto_model())
            .unwrap_or(false);

        Self {
            agent,
            task_tx,
            is_running: false,
            pending_task_id: None,
            messages: Vec::new(),
            input: String::new(),
            input_cursor: 0,
            scroll_offset: 0,
            active_panel: Panel::Chat,
            input_mode: InputMode::Normal,
            dialog: Dialog::None,
            status_msg: String::from("Ready. Type /help for commands."),
            status_timer: None,
            sidebar_width: 24,
            current_mode: AgentMode::Work,
            multi_agent_enabled: false,
            one_shot_enabled: false,
            models,
            current_model_idx: 0,
            model_label: String::from("Not configured"),
            llm_configs,
            llm_form: LlmForm::default(),
            workspace_form: WorkspaceForm {
                name: ws_name,
                path: ws_path,
            },
            remote_form: RemoteForm::default(),
            workspace_path: String::new(),
            workspace_name: String::new(),
            workspace_tree: Vec::new(),
            workspace_tree_scroll: 0,
            workspace_tree_cursor: 0,
            changes: Vec::new(),
            changes_visible: false,
            sessions: Vec::new(),
            sessions_cursor: 0,
            sessions_checkpoint_cursor: 0,
            project_dir,
            system_prompt,
            tools_schema,
            error_memory,
            skills_manager,
            running: true,
            settings_tab: SettingsTab::Model,
            settings_models: Vec::new(),
            settings_cursor: 0,
            settings_editing_field: None,
            skills_list,
            skills_cursor: 0,
            theme_name: theme,
            yolo_enabled: false,
            reasoning_effort: None,
            last_usage: None,
            session_usage: session_store::TokenUsage::default(),
            auto_model_enabled,
            auto_route: None,
            active_session_index: None,
        }
    }

    /// Main event loop
    pub async fn run<B: Backend>(&mut self, terminal: &mut Terminal<B>) -> Result<()> {
        // Load initial data
        self.refresh_workspace_tree();
        self.refresh_changes();
        self.refresh_sessions();
        self.update_model_label();

        while self.running {
            terminal.draw(|frame| ui::draw(frame, self))?;

            let event = crate::event::read_event()?;
            self.handle_event(event).await;
        }
        Ok(())
    }

    /// Handle a single crossterm event
    async fn handle_event(&mut self, event: Event) {
        // Status message timeout
        if let Some(t) = self.status_timer {
            if t.elapsed().as_secs() > 5 {
                self.status_msg.clear();
                self.status_timer = None;
            }
        }

        match &mut self.dialog {
            Dialog::None => match self.input_mode {
                InputMode::Normal => self.handle_normal_event(event).await,
                InputMode::Insert => self.handle_insert_event(event).await,
                InputMode::Settings => self.handle_settings_event(event),
            },
            Dialog::Settings(_) => self.handle_settings_event(event),
            Dialog::Sessions => self.handle_sessions_event(event),
            Dialog::Help => self.handle_help_event(event),
        }
    }

    /// Handle events in Normal mode
    async fn handle_normal_event(&mut self, event: Event) {
        match event {
            Event::Key(key) if key.kind == KeyEventKind::Press => {
                self.handle_normal_key(key).await;
            }
            Event::Mouse(mouse) => {
                self.handle_mouse(mouse);
            }
            _ => {}
        }
    }

    async fn handle_normal_key(&mut self, key: KeyEvent) {
        // Check Ctrl+char combos first (before general Char match)
        if key.modifiers == KeyModifiers::CONTROL {
            match key.code {
                KeyCode::Char('q') | KeyCode::Char('c') => {
                    self.running = false;
                    return;
                }
                KeyCode::Char('s') => {
                    self.open_settings();
                    return;
                }
                KeyCode::Char('n') => {
                    self.new_session();
                    return;
                }
                KeyCode::Char('r') => {
                    self.dialog = Dialog::Sessions;
                    self.refresh_sessions();
                    return;
                }
                KeyCode::Char('w') => {
                    self.sidebar_width = if self.sidebar_width > 0 { 0 } else { 24 };
                    return;
                }
                _ => {}
            }
        }

        match key.code {
            KeyCode::Enter => {
                self.send_message().await;
            }
            KeyCode::Esc => {
                self.stop_task().await;
            }
            KeyCode::Tab => {
                self.active_panel = match self.active_panel {
                    Panel::Chat => Panel::Sidebar,
                    Panel::Sidebar => Panel::Chat,
                };
            }
            KeyCode::Up => {
                if self.scroll_offset < self.messages.len().saturating_sub(1) {
                    self.scroll_offset += 1;
                }
            }
            KeyCode::Down => {
                self.scroll_offset = self.scroll_offset.saturating_sub(1);
            }
            KeyCode::PageUp => {
                self.scroll_offset =
                    (self.scroll_offset + 10).min(self.messages.len().saturating_sub(1));
            }
            KeyCode::PageDown => {
                self.scroll_offset = self.scroll_offset.saturating_sub(10);
            }
            KeyCode::Home => {
                self.scroll_offset = self.messages.len().saturating_sub(1);
            }
            KeyCode::End => {
                self.scroll_offset = 0;
            }
            KeyCode::F(1) => self.switch_mode(AgentMode::Work),
            KeyCode::F(2) => self.switch_mode(AgentMode::Plan),
            KeyCode::F(3) => self.switch_mode(AgentMode::Review),
            KeyCode::F(4) => self.toggle_multi_agent(),
            KeyCode::F(5) => self.toggle_one_shot(),
            KeyCode::F(6) => self.toggle_yolo(),
            KeyCode::F(7) => self.toggle_auto_model(),
            KeyCode::BackTab => self.cycle_reasoning_effort(),
            KeyCode::F(8) => {
                self.changes_visible = !self.changes_visible;
                if self.changes_visible {
                    self.refresh_changes();
                }
            }
            KeyCode::F(9) => self.refresh_sessions(),
            KeyCode::Char(c) => {
                self.input_mode = InputMode::Insert;
                self.input.push(c);
                self.input_cursor = self.input.len();
            }
            _ => {}
        }
    }

    /// Handle insert mode (typing in chat)
    async fn handle_insert_event(&mut self, event: Event) {
        match event {
            Event::Key(key) if key.kind == KeyEventKind::Press => match key.code {
                KeyCode::Enter => {
                    self.input_mode = InputMode::Normal;
                    self.send_message().await;
                }
                KeyCode::Esc => {
                    self.input_mode = InputMode::Normal;
                }
                KeyCode::Backspace => {
                    if self.input_cursor > 0 {
                        self.input.remove(self.input_cursor - 1);
                        self.input_cursor -= 1;
                    }
                }
                KeyCode::Delete => {
                    if self.input_cursor < self.input.len() {
                        self.input.remove(self.input_cursor);
                    }
                }
                KeyCode::Left => {
                    self.input_cursor = self.input_cursor.saturating_sub(1);
                }
                KeyCode::Right => {
                    self.input_cursor = (self.input_cursor + 1).min(self.input.len());
                }
                KeyCode::Home => {
                    self.input_cursor = 0;
                }
                KeyCode::End => {
                    self.input_cursor = self.input.len();
                }
                KeyCode::Up => {
                    // Simple history: restore last user message
                    if let Some(last) = self.messages.iter().rev().find(|m| m.role == "user") {
                        self.input = last.content.clone();
                        self.input_cursor = self.input.len();
                    }
                }
                KeyCode::Tab => {
                    // Autocomplete file paths
                    self.autocomplete_path();
                }
                KeyCode::Char(c) => {
                    self.input.insert(self.input_cursor, c);
                    self.input_cursor += 1;
                }
                _ => {}
            },
            _ => {}
        }
    }

    fn handle_mouse(&mut self, mouse: crossterm::event::MouseEvent) {
        match mouse.kind {
            MouseEventKind::ScrollDown => {
                self.scroll_offset = self.scroll_offset.saturating_sub(3);
            }
            MouseEventKind::ScrollUp => {
                self.scroll_offset =
                    (self.scroll_offset + 3).min(self.messages.len().saturating_sub(1));
            }
            _ => {}
        }
    }

    // ── Actions ───────────────────────────────────────────────────

    async fn send_message(&mut self) {
        let text = self.input.trim().to_string();
        self.input.clear();
        self.input_cursor = 0;

        if text.is_empty() {
            return;
        }

        // Handle slash commands
        if text.starts_with('/') {
            self.handle_command(&text).await;
            return;
        }

        // Check if Multi-Agent is suitable
        if text.len() < 8 || (!text.contains(' ') && text.len() < 20) {
            if self.multi_agent_enabled {
                self.multi_agent_enabled = false;
                if let Ok(a) = self.agent.try_read() {
                    a.set_multi_agent(false);
                }
            }
            if self.one_shot_enabled {
                self.one_shot_enabled = false;
                if let Ok(a) = self.agent.try_read() {
                    a.set_one_shot(false);
                }
            }
        }

        self.messages.push(ChatMessage {
            role: "user".into(),
            content: text.clone(),
            streaming: false,
            acp: None,
        });
        self.scroll_offset = 0;

        // Add placeholder for streaming
        self.messages.push(ChatMessage {
            role: "assistant".into(),
            content: "Thinking...".into(),
            streaming: true,
            acp: None,
        });
        let placeholder_idx = self.messages.len() - 1;

        self.is_running = true;
        self.set_status("Running...");

        let (display_tx, mut display_rx) = mpsc::channel::<Value>(256);
        let _ = self
            .task_tx
            .send((text.clone(), "tui".into(), display_tx))
            .await;

        // Stream responses
        let mut output = String::new();
        let mut acp_state: Option<AcpState> = None;
        let mut turn_usage: Option<session_store::TokenUsage> = None;
        while let Some(item) = display_rx.recv().await {
            if let Some(route) = item.get("route") {
                self.auto_route = Some(generic_coder::agent::AutoRouteDecision {
                    model_index: self.current_model_idx,
                    model: route
                        .get("model")
                        .and_then(|value| value.as_str())
                        .unwrap_or_default()
                        .to_string(),
                    display_name: route
                        .get("display_name")
                        .and_then(|value| value.as_str())
                        .unwrap_or_default()
                        .to_string(),
                    reasoning_effort: route
                        .get("reasoning_effort")
                        .and_then(|value| value.as_str())
                        .map(|value| value.to_string()),
                    reason: route
                        .get("reason")
                        .and_then(|value| value.as_str())
                        .unwrap_or_default()
                        .to_string(),
                });
            }
            if let Some(next) = item.get("next").and_then(|v| v.as_str()) {
                output.push_str(next);
            }
            if let Some(done) = item.get("done").and_then(|v| v.as_str()) {
                output = done.to_string();
                // Capture token usage if present
                if let Some(usage) = item.get("usage") {
                    let pt = usage
                        .get("prompt_tokens")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0);
                    let ct = usage
                        .get("completion_tokens")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0);
                    let ca = usage
                        .get("prompt_cache_hit_tokens")
                        .and_then(|v| v.as_u64())
                        .or_else(|| usage.get("cached_tokens").and_then(|v| v.as_u64()))
                        .unwrap_or(0);
                    self.last_usage = Some((pt, ct, ca));
                    turn_usage = session_store::usage_from_value(usage);
                }
                break;
            }
            if let Some(acp) = item.get("acp") {
                if !acp.is_null() {
                    acp_state = Self::parse_acp_event(acp);
                }
            }
            self.messages[placeholder_idx] = ChatMessage {
                role: "assistant".into(),
                content: output.clone(),
                streaming: true,
                acp: acp_state.clone(),
            };
        }

        self.messages[placeholder_idx] = ChatMessage {
            role: "assistant".into(),
            content: output,
            streaming: false,
            acp: acp_state,
        };

        self.is_running = false;
        self.set_status("Ready.");
        self.persist_current_session(turn_usage);
        self.refresh_changes();
        self.refresh_workspace_tree();
        self.refresh_sessions();
    }

    fn parse_acp_event(acp: &Value) -> Option<AcpState> {
        let event_type = acp.get("acp_event").and_then(|v| v.as_str())?;
        match event_type {
            "plan" => {
                let plan = acp.get("plan").cloned();
                Some(AcpState {
                    plan,
                    active_step: -1,
                    completed_steps: vec![],
                    failed_steps: vec![],
                    done: false,
                })
            }
            "step_start" => Some(AcpState {
                plan: None,
                active_step: acp.get("step").and_then(|v| v.as_i64()).unwrap_or(-1) as i32,
                completed_steps: vec![],
                failed_steps: vec![],
                done: false,
            }),
            "step_done" => Some(AcpState {
                plan: None,
                active_step: -1,
                completed_steps: vec![acp.clone()],
                failed_steps: vec![],
                done: false,
            }),
            "step_failed" => Some(AcpState {
                plan: None,
                active_step: -1,
                completed_steps: vec![],
                failed_steps: vec![acp.clone()],
                done: false,
            }),
            "done" => Some(AcpState {
                plan: None,
                active_step: -1,
                completed_steps: vec![],
                failed_steps: vec![],
                done: true,
            }),
            _ => None,
        }
    }

    async fn handle_command(&mut self, cmd: &str) {
        match cmd {
            "/new" => self.new_session(),
            "/clear" => {
                self.messages.clear();
                self.set_status("Chat cleared.");
            }
            "/help" => {
                self.dialog = Dialog::Help;
            }
            "/settings" => {
                self.open_settings();
            }
            "/stop" => {
                self.stop_task().await;
            }
            "/refresh" => {
                self.refresh_workspace_tree();
                self.refresh_changes();
                self.set_status("Refreshed.");
            }
            "/sessions" => {
                self.dialog = Dialog::Sessions;
                self.refresh_sessions();
            }
            "/profiles" => {
                let profiles = provider_profiles::built_in_provider_profiles()
                    .into_iter()
                    .map(|profile| {
                        format!(
                            "  {} -> {} ({})",
                            profile.id, profile.model, profile.apibase
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                self.messages.push(ChatMessage {
                    role: "assistant".into(),
                    content: format!(
                        "DeepSeek provider presets:\n{}\n\nUse /preset <id> to apply one.",
                        profiles
                    ),
                    streaming: false,
                    acp: None,
                });
            }
            "/auto" | "/model auto" => {
                self.toggle_auto_model();
            }
            other if other.starts_with("/preset ") => {
                self.apply_provider_profile(other.trim_start_matches("/preset ").trim())
                    .await;
            }
            other if other.starts_with("/continue ") => {
                self.restore_session_target(other.trim_start_matches("/continue ").trim());
            }
            other if other.starts_with("/fork ") => {
                self.fork_session_target(other.trim_start_matches("/fork ").trim());
            }
            other if other.starts_with("/delete ") => {
                self.delete_session_target(other.trim_start_matches("/delete ").trim());
            }
            other => {
                self.messages.push(ChatMessage {
                    role: "assistant".into(),
                    content: format!("Unknown command: {other}\n\nCommands:\n  /new, /clear, /help, /settings, /stop, /refresh, /sessions, /profiles, /preset <id>, /continue <session[@checkpoint]>, /fork <session[@checkpoint]>, /delete <session>, /auto"),
                    streaming: false,
                    acp: None,
                });
            }
        }
    }

    fn new_session(&mut self) {
        self.persist_current_session(None);
        self.messages.clear();
        self.scroll_offset = 0;
        self.active_session_index = None;
        self.auto_route = None;
        self.last_usage = None;
        self.session_usage = session_store::TokenUsage::default();
        self.refresh_sessions();
        self.set_status("New session started.");
    }

    async fn stop_task(&mut self) {
        if let Ok(agent) = self.agent.try_read() {
            agent.abort();
        }
        self.is_running = false;
        self.set_status("Stopped.");
    }

    fn open_settings(&mut self) {
        self.settings_models = self
            .llm_configs
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        self.settings_tab = SettingsTab::Model;
        self.settings_cursor = 0;
        self.dialog = Dialog::Settings(self.settings_tab);
    }

    // ── Helpers ────────────────────────────────────────────────────

    fn set_status(&mut self, msg: &str) {
        self.status_msg = msg.to_string();
        self.status_timer = Some(Instant::now());
    }

    fn update_model_label(&mut self) {
        self.model_label = self
            .models
            .get(self.current_model_idx)
            .map(|(_, name)| name.clone())
            .unwrap_or_else(|| "Not configured".into());
    }

    fn switch_mode(&mut self, mode: AgentMode) {
        self.current_mode = mode;
        if let Ok(agent) = self.agent.try_read() {
            agent.set_mode(mode);
        }
        self.set_status(&format!("Mode: {:?}", mode));
    }

    fn toggle_multi_agent(&mut self) {
        if self.one_shot_enabled {
            self.one_shot_enabled = false;
            if let Ok(a) = self.agent.try_read() {
                a.set_one_shot(false);
            }
        }
        self.multi_agent_enabled = !self.multi_agent_enabled;
        if let Ok(a) = self.agent.try_read() {
            a.set_multi_agent(self.multi_agent_enabled);
        }
        self.set_status(&format!(
            "Multi-Agent: {}",
            if self.multi_agent_enabled {
                "ON"
            } else {
                "OFF"
            }
        ));
    }

    fn toggle_one_shot(&mut self) {
        if self.multi_agent_enabled {
            self.multi_agent_enabled = false;
            if let Ok(a) = self.agent.try_read() {
                a.set_multi_agent(false);
            }
        }
        self.one_shot_enabled = !self.one_shot_enabled;
        if let Ok(a) = self.agent.try_read() {
            a.set_one_shot(self.one_shot_enabled);
        }
        self.set_status(&format!(
            "One Shot: {}",
            if self.one_shot_enabled { "ON" } else { "OFF" }
        ));
    }

    fn toggle_yolo(&mut self) {
        self.yolo_enabled = !self.yolo_enabled;
        if let Ok(a) = self.agent.try_read() {
            a.set_yolo(self.yolo_enabled);
        }
        self.set_status(&format!(
            "YOLO mode: {}{}",
            if self.yolo_enabled { "ON" } else { "OFF" },
            if self.yolo_enabled {
                " ⚡ AI will execute autonomously"
            } else {
                ""
            }
        ));
    }

    fn toggle_auto_model(&mut self) {
        self.auto_model_enabled = !self.auto_model_enabled;
        self.auto_route = None;
        if let Ok(agent) = self.agent.try_read() {
            agent.set_auto_model(self.auto_model_enabled);
        }
        self.set_status(if self.auto_model_enabled {
            "Auto model routing: ON"
        } else {
            "Auto model routing: OFF"
        });
    }

    async fn apply_provider_profile(&mut self, profile_id: &str) {
        let Some(profile) = provider_profiles::get_provider_profile(profile_id) else {
            self.set_status("Unknown provider preset.");
            return;
        };

        let key = format!(
            "generic_coder_{}_{}_config",
            profile.session_type,
            profile.id.replace('-', "_")
        );
        let api_key = self
            .llm_configs
            .get(
                self.models
                    .get(self.current_model_idx)
                    .map(|entry| entry.0.as_str())
                    .unwrap_or(""),
            )
            .map(|cfg| cfg.apikey.clone())
            .unwrap_or_default();
        let mut config = self
            .llm_configs
            .get(
                self.models
                    .get(self.current_model_idx)
                    .map(|entry| entry.0.as_str())
                    .unwrap_or(""),
            )
            .cloned()
            .unwrap_or(LlmConfig {
                name: String::new(),
                apikey: String::new(),
                apibase: String::new(),
                model: String::new(),
                context_win: 0,
                proxy: None,
                verify: false,
                max_retries: 1,
                stream: false,
                timeout: 0,
                read_timeout: 0,
                temperature: 0.0,
                max_tokens: None,
                reasoning_effort: None,
                service_tier: None,
                thinking_type: None,
                thinking_budget_tokens: None,
                api_mode: "chat_completions".to_string(),
                extra_sys_prompt: String::new(),
            });
        config.name = profile.label.to_string();
        config.model = profile.model.to_string();
        config.apibase = profile.apibase.to_string();
        config.apikey = api_key;
        config.api_mode = profile.api_mode.to_string();
        config.reasoning_effort = profile.reasoning_effort.map(|value| value.to_string());

        if let Err(err) = config::save_ui_llm_config_entry(&key, &config) {
            self.set_status(&format!("Failed to save preset: {err:#}"));
            return;
        }

        self.llm_configs = config::load_ui_llm_configs();
        self.models = self
            .llm_configs
            .iter()
            .map(|(entry_key, cfg)| {
                let display = if cfg.name.trim().is_empty() {
                    cfg.model.clone()
                } else {
                    cfg.name.clone()
                };
                (entry_key.clone(), display)
            })
            .collect();
        self.current_model_idx = self
            .models
            .iter()
            .position(|(entry_key, _)| entry_key == &key)
            .unwrap_or(
                self.current_model_idx
                    .min(self.models.len().saturating_sub(1)),
            );
        self.reasoning_effort = config.reasoning_effort.clone();
        self.update_model_label();

        if let Ok(mut agent) = self.agent.try_write() {
            let cfg = config::load_config(&self.project_dir);
            let _ = agent.load_llm_sessions(&cfg.llm_configs, &cfg.mixin_configs);
            let _ = agent.next_llm(self.current_model_idx as isize);
            agent.set_reasoning_effort(self.reasoning_effort.clone());
        }

        self.set_status(&format!("Applied preset: {}", profile.label));
    }

    fn parse_session_target(raw: &str) -> Option<(usize, Option<usize>)> {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return None;
        }
        let (session, checkpoint) = trimmed
            .split_once('@')
            .map(|(left, right)| (left.trim(), Some(right.trim())))
            .unwrap_or((trimmed, None));
        let session_index = session.parse::<usize>().ok()?;
        let checkpoint_index = checkpoint.and_then(|value| value.parse::<usize>().ok());
        Some((session_index, checkpoint_index))
    }

    fn restore_session_target(&mut self, raw: &str) {
        let Some((session_index, checkpoint_index)) = Self::parse_session_target(raw) else {
            self.set_status("Use /continue <session> or /continue <session>@<checkpoint>");
            return;
        };
        let Some(saved) = session_store::get_session(session_index) else {
            self.set_status("Session not found.");
            return;
        };
        let source_messages = if let Some(checkpoint_index) = checkpoint_index {
            match session_store::get_checkpoint(session_index, checkpoint_index) {
                Some(checkpoint) => {
                    self.session_usage = checkpoint.usage_totals.clone();
                    self.last_usage = checkpoint.last_usage.as_ref().map(|usage| {
                        (
                            usage.prompt_tokens,
                            usage.completion_tokens,
                            usage.cached_tokens,
                        )
                    });
                    checkpoint.messages
                }
                None => {
                    self.set_status("Checkpoint not found.");
                    return;
                }
            }
        } else {
            self.session_usage = saved.usage_totals.clone();
            self.last_usage = saved.last_usage.as_ref().map(|usage| {
                (
                    usage.prompt_tokens,
                    usage.completion_tokens,
                    usage.cached_tokens,
                )
            });
            saved.messages.clone()
        };
        self.messages = source_messages
            .into_iter()
            .map(|message| ChatMessage {
                role: message
                    .get("role")
                    .and_then(|value| value.as_str())
                    .unwrap_or("assistant")
                    .to_string(),
                content: message
                    .get("content")
                    .and_then(|value| value.as_str())
                    .unwrap_or_default()
                    .to_string(),
                streaming: false,
                acp: None,
            })
            .collect();
        self.active_session_index = Some(session_index);
        self.scroll_offset = 0;
        self.dialog = Dialog::None;
        self.set_status(&format!(
            "Restored session #{}{}",
            session_index,
            checkpoint_index
                .map(|value| format!(" @ checkpoint {value}"))
                .unwrap_or_default()
        ));
    }

    fn fork_session_target(&mut self, raw: &str) {
        let Some((session_index, checkpoint_index)) = Self::parse_session_target(raw) else {
            self.set_status("Use /fork <session> or /fork <session>@<checkpoint>");
            return;
        };
        match session_store::fork_session(session_index, checkpoint_index) {
            Ok(forked) => {
                self.active_session_index = Some(forked.index);
                self.session_usage = forked.usage_totals.clone();
                self.last_usage = forked.last_usage.as_ref().map(|usage| {
                    (
                        usage.prompt_tokens,
                        usage.completion_tokens,
                        usage.cached_tokens,
                    )
                });
                self.messages = forked
                    .messages
                    .into_iter()
                    .map(|message| ChatMessage {
                        role: message
                            .get("role")
                            .and_then(|value| value.as_str())
                            .unwrap_or("assistant")
                            .to_string(),
                        content: message
                            .get("content")
                            .and_then(|value| value.as_str())
                            .unwrap_or_default()
                            .to_string(),
                        streaming: false,
                        acp: None,
                    })
                    .collect();
                self.scroll_offset = 0;
                self.dialog = Dialog::None;
                self.refresh_sessions();
                self.set_status(&format!(
                    "Forked session #{} into #{}",
                    session_index, forked.index
                ));
            }
            Err(err) => self.set_status(&format!("Fork failed: {err:#}")),
        }
    }

    fn delete_session_target(&mut self, raw: &str) {
        if raw.trim().is_empty() || raw.contains('@') {
            self.set_status("Use /delete <session>");
            return;
        }
        let Ok(session_index) = raw.trim().parse::<usize>() else {
            self.set_status("Use /delete <session>");
            return;
        };

        match session_store::delete_session(session_index) {
            Ok(true) => {
                if self.active_session_index == Some(session_index) {
                    self.messages.clear();
                    self.scroll_offset = 0;
                    self.active_session_index = None;
                    self.auto_route = None;
                    self.last_usage = None;
                    self.session_usage = session_store::TokenUsage::default();
                }
                self.refresh_sessions();
                self.set_status(&format!("Deleted session #{}", session_index));
            }
            Ok(false) => self.set_status("Session not found."),
            Err(err) => self.set_status(&format!("Delete failed: {err:#}")),
        }
    }

    fn cycle_reasoning_effort(&mut self) {
        let efforts: &[Option<&str>] = &[None, Some("off"), Some("high"), Some("max")];
        let current_idx = efforts
            .iter()
            .position(|e| e.map(|s| s.to_string()) == self.reasoning_effort)
            .unwrap_or(0);
        let next = efforts[(current_idx + 1) % efforts.len()];
        self.reasoning_effort = next.map(|s| s.to_string());
        if let Ok(a) = self.agent.try_read() {
            a.set_reasoning_effort(self.reasoning_effort.clone());
        }
        let label = self.reasoning_effort.as_deref().unwrap_or("default");
        self.set_status(&format!("Reasoning effort: {label}"));
    }

    fn autocomplete_path(&mut self) {
        // Simple file path autocomplete from workspace tree
        let prefix_len = self.input_cursor;
        let prefix = self.input[..prefix_len].to_string();
        if let Some(last_word) = prefix.rsplit(|c: char| c.is_whitespace()).next() {
            let lw = last_word.to_string();
            for entry in &self.workspace_tree {
                if entry.name.starts_with(&lw) && !entry.is_dir {
                    let lw_len = lw.len();
                    self.input
                        .replace_range(prefix_len - lw_len..prefix_len, &entry.name);
                    self.input_cursor = prefix_len - lw_len + entry.name.len();
                    return;
                }
            }
        }
    }

    // ── Data refreshers ────────────────────────────────────────────

    pub fn refresh_workspace_tree(&mut self) {
        use generic_coder::workspace;
        let ws = workspace::get_active_workspace();
        if let Some(path) = ws.get("path").and_then(|v| v.as_str()) {
            self.workspace_path = path.to_string();
            if let Some(name) = ws.get("name").and_then(|v| v.as_str()) {
                self.workspace_name = name.to_string();
            }
            // List files
            self.workspace_tree = Self::list_files(Path::new(path), 0, 200);
        }
    }

    fn list_files(dir: &Path, depth: usize, limit: usize) -> Vec<FileEntry> {
        let mut entries = Vec::new();
        if depth > 3 || entries.len() >= limit {
            return entries;
        }
        if let Ok(iter) = std::fs::read_dir(dir) {
            for item in iter.flatten() {
                let path = item.path();
                let name = item.file_name().to_string_lossy().to_string();
                if name.starts_with('.') && name != "." {
                    continue;
                }
                let is_dir = path.is_dir();
                entries.push(FileEntry {
                    name: name.clone(),
                    path: path.to_string_lossy().to_string(),
                    is_dir,
                    depth,
                });
                if is_dir && entries.len() < limit {
                    let mut children = Self::list_files(&path, depth + 1, limit - entries.len());
                    entries.append(&mut children);
                }
                if entries.len() >= limit {
                    break;
                }
            }
        }
        entries
    }

    pub fn refresh_changes(&mut self) {
        // Use git_status from tools.rs
        let status =
            tools::git_status(None).unwrap_or_else(|_| serde_json::json!({"error": "git failed"}));
        self.changes = status
            .get("changes")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|item| {
                        let path = item.get("path").and_then(|v| v.as_str()).unwrap_or("?");
                        Some(ChangeEntry {
                            path: path.to_string(),
                            basename: Path::new(path)
                                .file_name()
                                .map(|n| n.to_string_lossy().to_string())
                                .unwrap_or_else(|| path.to_string()),
                            time: item
                                .get("status")
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                                .to_string(),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
    }

    pub fn refresh_sessions(&mut self) {
        self.sessions = session_store::load_sessions()
            .into_iter()
            .rev()
            .map(|session| SessionEntry {
                index: session.index,
                preview: session.preview,
                rounds: session.rounds,
                time: relative_session_time(session.saved_at),
                checkpoint_count: session.checkpoints.len(),
            })
            .collect();
        self.sessions_cursor = self
            .sessions_cursor
            .min(self.sessions.len().saturating_sub(1));
        let max_checkpoint_cursor = self
            .sessions
            .get(self.sessions_cursor)
            .map(|session| session.checkpoint_count)
            .unwrap_or(0);
        self.sessions_checkpoint_cursor =
            self.sessions_checkpoint_cursor.min(max_checkpoint_cursor);
    }

    fn persist_current_session(&mut self, last_usage: Option<session_store::TokenUsage>) {
        let messages: Vec<Value> = self
            .messages
            .iter()
            .filter(|message| !message.content.trim().is_empty())
            .map(|message| {
                serde_json::json!({
                    "role": message.role,
                    "content": message.content,
                })
            })
            .collect();
        if messages.is_empty() {
            return;
        }

        if let Ok(saved) =
            session_store::upsert_session(self.active_session_index, &messages, last_usage)
        {
            self.active_session_index = Some(saved.index);
            self.session_usage = saved.usage_totals;
            self.last_usage = saved.last_usage.as_ref().map(|usage| {
                (
                    usage.prompt_tokens,
                    usage.completion_tokens,
                    usage.cached_tokens,
                )
            });
        }
    }

    // ── Settings event handler ─────────────────────────────────────

    fn handle_settings_event(&mut self, event: Event) {
        match event {
            Event::Key(key) if key.kind == KeyEventKind::Press => match key.code {
                KeyCode::Esc => {
                    self.dialog = Dialog::None;
                    self.settings_editing_field = None;
                }
                KeyCode::Tab => {
                    self.settings_tab = match self.settings_tab {
                        SettingsTab::Model => SettingsTab::Workspace,
                        SettingsTab::Workspace => SettingsTab::Interface,
                        SettingsTab::Interface => SettingsTab::Skills,
                        SettingsTab::Skills => SettingsTab::Model,
                        _ => SettingsTab::Model,
                    };
                }
                KeyCode::Up => {
                    self.settings_cursor = self.settings_cursor.saturating_sub(1);
                }
                KeyCode::Down => {
                    self.settings_cursor += 1;
                }
                KeyCode::Enter => {
                    if self.settings_tab == SettingsTab::Model && !self.settings_models.is_empty() {
                        // Save model config
                        let (key, llm_cfg) = &self.settings_models
                            [self.settings_cursor.min(self.settings_models.len() - 1)];
                        if let Err(e) = config::save_ui_llm_config_entry(key, llm_cfg) {
                            self.set_status(&format!("Save error: {e}"));
                        } else {
                            self.set_status(&format!("Saved: {key}"));
                        }
                    }
                }
                _ => {}
            },
            Event::Key(key)
                if key.kind == KeyEventKind::Press
                    && key.code == KeyCode::Char('q')
                    && key.modifiers == KeyModifiers::CONTROL =>
            {
                self.dialog = Dialog::None;
                self.settings_editing_field = None;
            }
            _ => {}
        }
    }

    fn handle_sessions_event(&mut self, event: Event) {
        match event {
            Event::Key(key) if key.kind == KeyEventKind::Press => match key.code {
                KeyCode::Esc => {
                    self.dialog = Dialog::None;
                }
                KeyCode::Up => {
                    self.sessions_cursor = self.sessions_cursor.saturating_sub(1);
                    self.sessions_checkpoint_cursor = 0;
                }
                KeyCode::Down => {
                    self.sessions_cursor =
                        (self.sessions_cursor + 1).min(self.sessions.len().saturating_sub(1));
                    self.sessions_checkpoint_cursor = 0;
                }
                KeyCode::Left => {
                    self.sessions_checkpoint_cursor =
                        self.sessions_checkpoint_cursor.saturating_sub(1);
                }
                KeyCode::Right => {
                    let max_checkpoint_cursor = self
                        .sessions
                        .get(self.sessions_cursor)
                        .map(|session| session_store::list_checkpoints(session.index).len())
                        .unwrap_or(0);
                    self.sessions_checkpoint_cursor =
                        (self.sessions_checkpoint_cursor + 1).min(max_checkpoint_cursor);
                }
                KeyCode::Enter => {
                    if let Some((session_index, checkpoint_index)) =
                        self.selected_sessions_dialog_target()
                    {
                        let target = checkpoint_index
                            .map(|checkpoint| format!("{session_index}@{checkpoint}"))
                            .unwrap_or_else(|| session_index.to_string());
                        self.restore_session_target(&target);
                    }
                }
                KeyCode::Char('f') => {
                    if let Some((session_index, checkpoint_index)) =
                        self.selected_sessions_dialog_target()
                    {
                        let target = checkpoint_index
                            .map(|checkpoint| format!("{session_index}@{checkpoint}"))
                            .unwrap_or_else(|| session_index.to_string());
                        self.fork_session_target(&target);
                    }
                }
                KeyCode::Char('d') => {
                    if let Some(session) = self.sessions.get(self.sessions_cursor) {
                        self.delete_session_target(&session.index.to_string());
                    }
                }
                _ => {}
            },
            _ => {}
        }
    }

    fn handle_help_event(&mut self, event: Event) {
        match event {
            Event::Key(_) => {
                self.dialog = Dialog::None;
            }
            _ => {}
        }
    }

    pub fn selected_sessions_dialog_target(&self) -> Option<(usize, Option<usize>)> {
        let session = self.sessions.get(self.sessions_cursor)?;
        if self.sessions_checkpoint_cursor == 0 {
            return Some((session.index, None));
        }
        let checkpoints = session_store::list_checkpoints(session.index);
        let checkpoint = checkpoints
            .into_iter()
            .rev()
            .nth(self.sessions_checkpoint_cursor - 1)?;
        Some((session.index, Some(checkpoint.index)))
    }

    pub fn selected_sessions_dialog_checkpoints(&self) -> Vec<session_store::SessionCheckpoint> {
        self.sessions
            .get(self.sessions_cursor)
            .map(|session| {
                session_store::list_checkpoints(session.index)
                    .into_iter()
                    .rev()
                    .collect()
            })
            .unwrap_or_default()
    }
}
