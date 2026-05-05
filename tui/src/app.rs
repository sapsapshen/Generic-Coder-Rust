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
use generic_coder::skills::SkillsManager;
use generic_coder::types::{LlmConfig, ToolSchema};
use generic_coder::workflow::AgentMode;
use generic_coder::{tools, workspace};

use crate::event::{InputMode, Panel};
use crate::ui;

/// Represents a single chat message
#[derive(Clone)]
pub struct ChatMessage {
    pub role: String,      // "user" or "assistant"
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
            workspace_form: WorkspaceForm { name: ws_name, path: ws_path },
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
                self.scroll_offset = (self.scroll_offset + 10).min(self.messages.len().saturating_sub(1));
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
                    if let Some(last) = self
                        .messages
                        .iter()
                        .rev()
                        .find(|m| m.role == "user")
                    {
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
                self.scroll_offset = (self.scroll_offset + 3)
                    .min(self.messages.len().saturating_sub(1));
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
        while let Some(item) = display_rx.recv().await {
            if let Some(next) = item.get("next").and_then(|v| v.as_str()) {
                output.push_str(next);
            }
            if let Some(done) = item.get("done").and_then(|v| v.as_str()) {
                output = done.to_string();
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
        self.refresh_changes();
        self.refresh_workspace_tree();
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
            other => {
                self.messages.push(ChatMessage {
                    role: "assistant".into(),
                    content: format!("Unknown command: {other}\n\nCommands:\n  /new, /clear, /help, /settings, /stop, /refresh, /sessions"),
                    streaming: false,
                    acp: None,
                });
            }
        }
    }

    fn new_session(&mut self) {
        self.messages.clear();
        self.scroll_offset = 0;
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
            if self.multi_agent_enabled { "ON" } else { "OFF" }
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
        let status = tools::git_status(None).unwrap_or_else(|_| serde_json::json!({"error": "git failed"}));
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
                            time: item.get("status").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
    }

    pub fn refresh_sessions(&mut self) {
        self.sessions = Vec::new(); // sessions are in-memory for TUI
        self.set_status("No sessions to restore (TUI sessions are ephemeral)");
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
                        let (key, llm_cfg) = &self.settings_models[self.settings_cursor.min(self.settings_models.len() - 1)];
                        if let Err(e) = config::save_ui_llm_config_entry(key, llm_cfg) {
                            self.set_status(&format!("Save error: {e}"));
                        } else {
                            self.set_status(&format!("Saved: {key}"));
                        }
                    }
                }
                _ => {}
            },
            Event::Key(key) if key.kind == KeyEventKind::Press
                && key.code == KeyCode::Char('q') && key.modifiers == KeyModifiers::CONTROL => {
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
                }
                KeyCode::Down => {
                    self.sessions_cursor = (self.sessions_cursor + 1).min(self.sessions.len().saturating_sub(1));
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
}
