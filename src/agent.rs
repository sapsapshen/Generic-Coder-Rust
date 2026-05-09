use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, RwLock,
};
use std::time::Duration;

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use regex::Regex;
use serde_json::{json, Value};
use tokio::sync::{mpsc, Mutex, RwLock as TokioRwLock};
use tokio::task::JoinHandle;

use crate::config;
use crate::error_memory::{ErrorMemory, ErrorSeverity};
use crate::llm;
use crate::types::{
    LlmConfig, LlmResponse, Message, MessageContent, StepOutcome, ToolCall, ToolSchema,
};
use crate::workflow::{AgentMode, Workflow};

fn default_agent_cwd() -> Result<PathBuf> {
    let root = std::env::var("GENERIC_CODER_PROJECT_DIR")
        .ok()
        .map(PathBuf::from)
        .filter(|path| path.is_dir())
        .or_else(crate::workspace::effective_root)
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_else(|| PathBuf::from("."));

    let temp_dir = if root.ends_with("temp") {
        root
    } else {
        root.join("temp")
    };

    fs::create_dir_all(&temp_dir)?;
    Ok(fs::canonicalize(&temp_dir).unwrap_or(temp_dir))
}

fn extract_tagged_block(response_content: &str, tag_name: &str) -> Option<String> {
    let open_tag = format!("<{tag_name}>");
    let close_tag = format!("</{tag_name}>");
    let start = response_content.find(&open_tag)? + open_tag.len();
    let end = response_content[start..].find(&close_tag)? + start;
    Some(response_content[start..end].to_string())
}

fn extract_first_fenced_code_block(response_content: &str) -> Option<String> {
    let start = response_content.find("```")?;
    let after_open = &response_content[start + 3..];
    let content_start = after_open.find('\n')? + start + 4;
    let content_end = response_content[content_start..].find("```")? + content_start;
    Some(response_content[content_start..content_end].to_string())
}

fn file_write_content_from_response(response_content: &str) -> Option<String> {
    extract_tagged_block(response_content, "file_content")
        .or_else(|| extract_first_fenced_code_block(response_content))
}

fn file_write_content_from_tool_block(response_content: &str, path: &str) -> Option<String> {
    let tool_re =
        Regex::new(r"(?s)<(?:tool_use|tool_call)>([\s\S]{15,}?)</(?:tool_use|tool_call)>").ok()?;
    for caps in tool_re.captures_iter(response_content) {
        let raw = caps.get(1)?.as_str().trim();
        let parsed = crate::llm::tryparse_json(raw).ok()?;
        if parsed.get("name").and_then(|value| value.as_str()) != Some("file_write") {
            continue;
        }
        let arguments = parsed
            .get("arguments")
            .or_else(|| parsed.get("args"))
            .or_else(|| parsed.get("input"))?;
        let candidate_path = arguments
            .get("path")
            .and_then(|value| value.as_str())
            .unwrap_or("");
        if !path.is_empty() && !candidate_path.is_empty() && candidate_path != path {
            continue;
        }
        if let Some(content) = arguments
            .get("content")
            .or_else(|| arguments.get("text"))
            .and_then(|value| value.as_str())
        {
            return Some(content.to_string());
        }
    }
    None
}

fn code_run_request_from_args(args: &Value) -> (String, String) {
    let command = args
        .get("command")
        .or_else(|| args.get("cmd"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let code = args
        .get("code")
        .or_else(|| args.get("script"))
        .and_then(|v| v.as_str())
        .unwrap_or(command)
        .to_string();
    let code_type = args
        .get("type")
        .or_else(|| args.get("language"))
        .and_then(|v| v.as_str())
        .unwrap_or_else(|| {
            if !command.is_empty() {
                "bash"
            } else {
                "python"
            }
        })
        .to_string();
    (code, code_type)
}

fn canonicalize_tool_invocation(tool_name: &str, args: &Value) -> (String, Value) {
    match tool_name {
        "bash" | "sh" | "execute_command" => {
            let mut normalized = args.clone();
            if let Some(object) = normalized.as_object_mut() {
                if object.get("type").is_none() && object.get("language").is_none() {
                    object.insert("type".into(), Value::String("bash".into()));
                }
            }
            ("code_run".into(), normalized)
        }
        "file_list" => ("workspace_list".into(), args.clone()),
        "git_show" => {
            let hash = args
                .get("hash")
                .and_then(|value| value.as_str())
                .unwrap_or("");
            if hash.is_empty() {
                return (tool_name.into(), args.clone());
            }
            let path_repo = args
                .get("path_repo")
                .or_else(|| args.get("cwd"))
                .and_then(|value| value.as_str())
                .unwrap_or(".");
            let max_lines = args
                .get("max_lines")
                .or_else(|| args.get("count"))
                .and_then(|value| value.as_u64())
                .unwrap_or(200);
            let command = format!(
                "cd {} && git --no-pager show {} | head -n {}",
                shell_quote(path_repo),
                shell_quote(hash),
                max_lines
            );
            (
                "code_run".into(),
                json!({
                    "command": command,
                    "type": "bash",
                    "cwd": path_repo,
                }),
            )
        }
        _ => (tool_name.into(), args.clone()),
    }
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

// ── 1. LlmClient trait ───────────────────────────────────────────────────────

#[async_trait]
pub trait LlmClient: Send + Sync {
    fn name(&self) -> &str;
    fn model(&self) -> &str;
    fn clear_tools_cache(&mut self);
    fn set_tools(&mut self, tools: Vec<ToolSchema>);
    fn set_system(&mut self, system: &str);
    fn set_reasoning_effort(&mut self, effort: Option<String>);
    async fn chat(
        &mut self,
        messages: Vec<Message>,
        tools: Vec<ToolSchema>,
    ) -> Result<(mpsc::Receiver<String>, JoinHandle<Result<LlmResponse>>)>;
}

struct ToolClientSession {
    config: LlmConfig,
    client: llm::ToolClient,
}

#[async_trait]
impl LlmClient for ToolClientSession {
    fn name(&self) -> &str {
        &self.config.name
    }

    fn model(&self) -> &str {
        &self.config.model
    }

    fn clear_tools_cache(&mut self) {
        self.client.last_tools.clear();
    }

    fn set_tools(&mut self, _tools: Vec<ToolSchema>) {}

    fn set_system(&mut self, _system: &str) {}
    fn set_reasoning_effort(&mut self, effort: Option<String>) {
        self.config.reasoning_effort = effort;
    }

    async fn chat(
        &mut self,
        messages: Vec<Message>,
        tools: Vec<ToolSchema>,
    ) -> Result<(mpsc::Receiver<String>, JoinHandle<Result<LlmResponse>>)> {
        let json_messages = messages_to_values(messages)?;
        let (stream, _result, _) = self.client.chat(json_messages, &tools).await;
        let (tx, rx) = mpsc::channel(256);

        let handle = tokio::spawn(async move {
            let mut raw = String::new();
            let mut stream = stream;
            while let Some(item) = futures_util::StreamExt::next(&mut stream).await {
                match item {
                    Ok(chunk) => {
                        raw.push_str(&chunk);
                        if tx.send(chunk).await.is_err() {
                            break;
                        }
                    }
                    Err(err) => {
                        let msg = format!("!!!Error: {err}");
                        raw.push_str(&msg);
                        if tx.send(msg).await.is_err() {
                            break;
                        }
                    }
                }
            }
            Ok(parse_text_response(&raw))
        });

        Ok((rx, handle))
    }
}

struct NativeClaudeClientSession {
    config: LlmConfig,
    client: llm::NativeToolClient,
}

fn mode_emoji(mode: AgentMode) -> &'static str {
    match mode {
        AgentMode::Work => "⚡",
        AgentMode::Plan => "📋",
        AgentMode::Review => "🔍",
    }
}

fn mode_str(mode: AgentMode) -> &'static str {
    match mode {
        AgentMode::Work => "WORK",
        AgentMode::Plan => "PLAN",
        AgentMode::Review => "REVIEW",
    }
}

#[derive(Debug, Clone)]
pub struct AutoRouteDecision {
    pub model_index: usize,
    pub model: String,
    pub display_name: String,
    pub reasoning_effort: Option<String>,
    pub reason: String,
}

#[async_trait]
impl LlmClient for NativeClaudeClientSession {
    fn name(&self) -> &str {
        &self.config.name
    }

    fn model(&self) -> &str {
        &self.config.model
    }

    fn clear_tools_cache(&mut self) {}

    fn set_tools(&mut self, _tools: Vec<ToolSchema>) {}

    fn set_system(&mut self, _system: &str) {}
    fn set_reasoning_effort(&mut self, effort: Option<String>) {
        self.config.reasoning_effort = effort;
    }

    async fn chat(
        &mut self,
        messages: Vec<Message>,
        tools: Vec<ToolSchema>,
    ) -> Result<(mpsc::Receiver<String>, JoinHandle<Result<LlmResponse>>)> {
        let json_messages = messages_to_values(messages)?;
        let (stream, result) = self.client.chat(json_messages, &tools).await;
        let (tx, rx) = mpsc::channel(256);

        let handle = tokio::spawn(async move {
            let mut stream = stream;
            while let Some(item) = futures_util::StreamExt::next(&mut stream).await {
                match item {
                    Ok(chunk) => {
                        if tx.send(chunk).await.is_err() {
                            break;
                        }
                    }
                    Err(err) => {
                        if tx.send(format!("!!!Error: {err}")).await.is_err() {
                            break;
                        }
                    }
                }
            }

            let blocks = wait_for_blocks(result).await;
            Ok(llm::blocks_to_response(&blocks))
        });

        Ok((rx, handle))
    }
}

fn messages_to_values(messages: Vec<Message>) -> Result<Vec<Value>> {
    messages
        .into_iter()
        .map(|message| serde_json::to_value(message).map_err(Into::into))
        .collect()
}

fn parse_text_response(raw_text: &str) -> LlmResponse {
    let mut thinking = String::new();
    let mut remaining = raw_text.to_string();

    if let Some(start) = remaining.find("<thinking>") {
        if let Some(end_rel) = remaining[start + "<thinking>".len()..].find("</thinking>") {
            let content_start = start + "<thinking>".len();
            let content_end = content_start + end_rel;
            thinking = remaining[content_start..content_end].to_string();
            let close_end = content_end + "</thinking>".len();
            remaining.replace_range(start..close_end, "");
        }
    }

    let (tool_calls, content) = llm::parse_text_tool_calls(&remaining);
    let stop_reason = if tool_calls.is_empty() {
        "end_turn".into()
    } else {
        "tool_use".into()
    };
    LlmResponse {
        thinking,
        content,
        tool_calls,
        raw: raw_text.to_string(),
        stop_reason,
        usage: None,
    }
}

async fn wait_for_blocks(result: Arc<Mutex<Option<Vec<Value>>>>) -> Vec<Value> {
    for _ in 0..200 {
        if let Some(blocks) = result.lock().await.take() {
            return blocks;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    Vec::new()
}

// ── 2. GenericAgent ──────────────────────────────────────────────────────────

pub struct GenericAgent {
    pub llm_clients: Vec<Arc<TokioRwLock<dyn LlmClient>>>,
    pub current_llm_no: usize,
    pub handler: Option<Arc<RwLock<AgentHandler>>>,
    pub history: Vec<String>,
    pub stop_sig: Arc<AtomicBool>,
    pub is_running: Arc<AtomicBool>,
    pub verbose: bool,
    pub agent_mode: RwLock<AgentMode>,
    pub agent_workflow: RwLock<Workflow>,
    pub multi_agent_enabled: RwLock<bool>,
    pub one_shot_enabled: RwLock<bool>,
    pub yolo_enabled: RwLock<bool>,
    pub reasoning_effort: RwLock<Option<String>>,
    pub auto_model_enabled: RwLock<bool>,
    pub last_auto_route: RwLock<Option<AutoRouteDecision>>,
}

impl GenericAgent {
    pub fn new() -> Self {
        Self {
            llm_clients: Vec::new(),
            current_llm_no: 0,
            handler: None,
            history: Vec::new(),
            stop_sig: Arc::new(AtomicBool::new(false)),
            is_running: Arc::new(AtomicBool::new(false)),
            verbose: true,
            agent_mode: RwLock::new(AgentMode::Work),
            agent_workflow: RwLock::new(Workflow::default()),
            multi_agent_enabled: RwLock::new(false),
            one_shot_enabled: RwLock::new(false),
            yolo_enabled: RwLock::new(false),
            reasoning_effort: RwLock::new(None),
            auto_model_enabled: RwLock::new(false),
            last_auto_route: RwLock::new(None),
        }
    }

    /// Check if multi-agent collaboration is enabled
    pub fn is_multi_agent(&self) -> bool {
        *self.multi_agent_enabled.read().unwrap()
    }

    /// Enable multi-agent mode
    pub fn enable_multi_agent(&self) {
        *self.multi_agent_enabled.write().unwrap() = true;
    }

    /// Disable multi-agent mode (back to single-agent)
    pub fn disable_multi_agent(&self) {
        *self.multi_agent_enabled.write().unwrap() = false;
    }

    /// Set multi-agent enabled/disabled
    pub fn set_multi_agent(&self, enabled: bool) {
        *self.multi_agent_enabled.write().unwrap() = enabled;
    }

    /// Check if One Shot autonomous mode is enabled
    pub fn is_one_shot(&self) -> bool {
        *self.one_shot_enabled.read().unwrap()
    }

    /// Set One Shot enabled/disabled
    pub fn set_one_shot(&self, enabled: bool) {
        *self.one_shot_enabled.write().unwrap() = enabled;
    }

    /// Check if YOLO (auto-approve) mode is enabled
    pub fn is_yolo(&self) -> bool {
        *self.yolo_enabled.read().unwrap()
    }

    /// Set YOLO (auto-approve) mode
    pub fn set_yolo(&self, enabled: bool) {
        *self.yolo_enabled.write().unwrap() = enabled;
    }

    pub fn is_auto_model(&self) -> bool {
        *self.auto_model_enabled.read().unwrap()
    }

    pub fn set_auto_model(&self, enabled: bool) {
        *self.auto_model_enabled.write().unwrap() = enabled;
        if !enabled {
            *self.last_auto_route.write().unwrap() = None;
        }
    }

    pub fn get_last_auto_route(&self) -> Option<AutoRouteDecision> {
        self.last_auto_route.read().unwrap().clone()
    }

    /// Get current reasoning effort override
    pub fn get_reasoning_effort(&self) -> Option<String> {
        self.reasoning_effort.read().unwrap().clone()
    }

    /// Set reasoning effort override (None / Some("off") / Some("high") / Some("max"))
    pub fn set_reasoning_effort(&self, effort: Option<String>) {
        *self.reasoning_effort.write().unwrap() = effort;
    }

    fn client(&self) -> Option<Arc<TokioRwLock<dyn LlmClient>>> {
        self.llm_clients.get(self.current_llm_no).cloned()
    }

    fn pick_auto_route(&self, query: &str) -> Option<AutoRouteDecision> {
        if self.llm_clients.is_empty() {
            return None;
        }

        let inventory: Vec<(usize, String, String)> = self
            .llm_clients
            .iter()
            .enumerate()
            .filter_map(|(index, client)| {
                client
                    .try_read()
                    .ok()
                    .map(|guard| (index, guard.model().to_string(), guard.name().to_string()))
            })
            .collect();
        if inventory.is_empty() {
            return None;
        }

        let current = inventory
            .iter()
            .find(|(index, _, _)| *index == self.current_llm_no)
            .cloned()
            .unwrap_or_else(|| inventory[0].clone());

        let flash = inventory
            .iter()
            .find(|(_, model, _)| {
                let lower = model.to_ascii_lowercase();
                lower.contains("deepseek-v4-flash") || lower.contains("deepseek-chat")
            })
            .cloned();
        let pro = inventory
            .iter()
            .find(|(_, model, _)| {
                let lower = model.to_ascii_lowercase();
                lower.contains("deepseek-v4-pro") || lower.contains("deepseek-reasoner")
            })
            .cloned();

        let lower = query.to_ascii_lowercase();
        let lines = query.lines().count();
        let chars = query.chars().count();
        let mut score = 0;

        if chars > 240 {
            score += 1;
        }
        if chars > 900 {
            score += 2;
        }
        if lines > 6 {
            score += 1;
        }

        let strong_keywords = [
            "debug",
            "fix",
            "refactor",
            "architecture",
            "design",
            "security",
            "migrate",
            "release",
            "incident",
            "compare",
            "audit",
            "carefully",
            "complex",
            "仔细",
            "修复",
            "重构",
            "架构",
            "设计",
            "安全",
            "迁移",
            "发布",
            "审查",
            "分析",
        ];
        let medium_keywords = [
            "test",
            "plan",
            "review",
            "performance",
            "optimize",
            "workflow",
            "session",
            "implement",
            "explain",
            "实现",
            "测试",
            "规划",
            "评审",
            "性能",
            "优化",
            "工作流",
            "会话",
            "说明",
        ];

        score += strong_keywords
            .iter()
            .filter(|keyword| lower.contains(**keyword))
            .count() as i32
            * 2;
        score += medium_keywords
            .iter()
            .filter(|keyword| lower.contains(**keyword))
            .count() as i32;

        let mode = *self.agent_mode.read().unwrap();
        if !matches!(mode, AgentMode::Work) {
            score += 2;
        }
        if self.is_multi_agent() || self.is_one_shot() || self.is_yolo() {
            score += 1;
        }

        let routed = if score >= 4 {
            pro.clone().or(flash.clone()).unwrap_or(current.clone())
        } else {
            flash.clone().or(pro.clone()).unwrap_or(current.clone())
        };
        let reasoning_effort = if score >= 6 {
            Some("max".to_string())
        } else if score >= 2 {
            Some("high".to_string())
        } else {
            Some("low".to_string())
        };

        let reason = if score >= 6 {
            "High-complexity task: elevated to stronger model and maximum reasoning".to_string()
        } else if score >= 2 {
            "Coding/debugging style task: balanced toward stronger reasoning".to_string()
        } else {
            "Short/simple turn: stay on faster route".to_string()
        };

        Some(AutoRouteDecision {
            model_index: routed.0,
            model: routed.1,
            display_name: routed.2,
            reasoning_effort,
            reason,
        })
    }

    fn select_client_for_query(
        &self,
        query: &str,
    ) -> Option<(Arc<TokioRwLock<dyn LlmClient>>, Option<AutoRouteDecision>)> {
        if self.is_auto_model() {
            let route = self.pick_auto_route(query)?;
            let client = self.llm_clients.get(route.model_index).cloned()?;
            *self.last_auto_route.write().unwrap() = Some(route.clone());
            Some((client, Some(route)))
        } else {
            *self.last_auto_route.write().unwrap() = None;
            self.client().map(|client| (client, None))
        }
    }

    pub fn load_llm_sessions(
        &mut self,
        configs: &HashMap<String, LlmConfig>,
        _mixins: &[config::MixinConfig],
    ) -> Result<()> {
        let mut clients: Vec<Arc<TokioRwLock<dyn LlmClient>>> = Vec::new();

        let mut entries: Vec<(&String, &LlmConfig)> = configs.iter().collect();
        entries.sort_by(|(left, _), (right, _)| left.cmp(right));

        for (key, cfg) in entries {
            let mut cfg = cfg.clone();
            if cfg.name.is_empty() {
                cfg.name = key.clone();
            }

            let session_type = config::infer_session_type(key);
            let client: Arc<TokioRwLock<dyn LlmClient>> = match session_type {
                "native_claude" => {
                    let backend = Arc::new(llm::NativeClaudeSession::new(cfg.clone()));
                    Arc::new(TokioRwLock::new(NativeClaudeClientSession {
                        config: cfg,
                        client: llm::NativeToolClient::new(backend),
                    }))
                }
                "claude" => {
                    let backend: Arc<dyn llm::BaseSession> =
                        Arc::new(llm::ClaudeSession::new(cfg.clone()));
                    Arc::new(TokioRwLock::new(ToolClientSession {
                        config: cfg,
                        client: llm::ToolClient::new(backend, true),
                    }))
                }
                "native_oai" => {
                    let backend: Arc<dyn llm::BaseSession> =
                        Arc::new(llm::NativeOaiSession::new(cfg.clone()));
                    Arc::new(TokioRwLock::new(ToolClientSession {
                        config: cfg,
                        client: llm::ToolClient::new(backend, true),
                    }))
                }
                _ => {
                    let backend: Arc<dyn llm::BaseSession> =
                        Arc::new(llm::OaiSession::new(cfg.clone()));
                    Arc::new(TokioRwLock::new(ToolClientSession {
                        config: cfg,
                        client: llm::ToolClient::new(backend, true),
                    }))
                }
            };
            clients.push(client);
        }

        self.llm_clients = clients;
        Ok(())
    }

    pub fn next_llm(&mut self, n: isize) -> Result<()> {
        if self.llm_clients.is_empty() {
            return Err(anyhow!("No valid LLM session configured."));
        }
        let len = self.llm_clients.len();
        if n < 0 {
            self.current_llm_no = (self.current_llm_no + 1) % len;
        } else {
            self.current_llm_no = (n as usize) % len;
        }
        if let Some(client) = self.client() {
            client.try_write().map(|mut g| g.clear_tools_cache()).ok();
        }
        Ok(())
    }

    pub fn list_llms(&self) -> Vec<(usize, String, bool)> {
        self.llm_clients
            .iter()
            .enumerate()
            .map(|(i, c)| {
                let name = c
                    .try_read()
                    .map(|g| g.name().to_string())
                    .unwrap_or_default();
                (i, name, i == self.current_llm_no)
            })
            .collect()
    }

    pub fn get_llm_name(&self, model: bool) -> String {
        match self.client() {
            None => {
                if model {
                    String::new()
                } else {
                    "NO_LLM_CONFIGURED".into()
                }
            }
            Some(c) => match c.try_read() {
                Ok(guard) => {
                    if model {
                        guard.model().to_string()
                    } else {
                        guard.name().to_string()
                    }
                }
                Err(_) => "LOCKED".into(),
            },
        }
    }

    pub fn abort(&self) {
        self.stop_sig.store(true, Ordering::SeqCst);
    }

    pub fn is_busy(&self) -> bool {
        self.is_running.load(Ordering::SeqCst)
    }

    /// Set the current agent mode (work/plan/review)
    pub fn set_mode(&self, mode: AgentMode) {
        *self.agent_mode.write().unwrap() = mode;
        // Also update current handler if exists
        if let Some(h) = &self.handler {
            *h.write().unwrap().mode.write().unwrap() = mode;
        }
    }

    /// Set the workflow on the agent
    pub fn set_workflow(&self, workflow: Workflow) {
        *self.agent_workflow.write().unwrap() = workflow;
        // Also update current handler if exists
        if let Some(h) = &self.handler {
            *h.write().unwrap().workflow.write().unwrap() =
                self.agent_workflow.read().unwrap().clone();
        }
    }

    /// Get current agent mode
    pub fn get_mode(&self) -> AgentMode {
        *self.agent_mode.read().unwrap()
    }

    /// Get current workflow
    pub fn get_workflow(&self) -> Workflow {
        self.agent_workflow.read().unwrap().clone()
    }

    pub fn put_task(&self, _query: String, _source: String) -> mpsc::Receiver<Value> {
        let (tx, rx) = mpsc::channel(256);
        let _ = tx.try_send(Value::String(
            "put_task stub — use run_task for execution".into(),
        ));
        rx
    }

    pub async fn run_task_loop(
        &self,
        task_rx: &mut mpsc::Receiver<(String, String, mpsc::Sender<Value>)>,
        sys_prompt: String,
        tools: Vec<ToolSchema>,
    ) {
        while let Some((query, _source, reply_tx)) = task_rx.recv().await {
            if self.stop_sig.load(Ordering::SeqCst) {
                break;
            }

            self.is_running.store(true, Ordering::SeqCst);

            let (client, route) = match self.select_client_for_query(&query) {
                Some(c) => c,
                None => {
                    let _ = reply_tx
                        .send(Value::String("[ERROR] No LLM configured".into()))
                        .await;
                    self.is_running.store(false, Ordering::SeqCst);
                    continue;
                }
            };

            {
                let effort = route
                    .as_ref()
                    .and_then(|decision| decision.reasoning_effort.clone())
                    .or_else(|| self.reasoning_effort.read().unwrap().clone());
                if let Ok(mut guard) = client.try_write() {
                    guard.set_reasoning_effort(effort);
                }
            }

            let handler_cwd = match default_agent_cwd() {
                Ok(path) => path,
                Err(err) => {
                    let _ = reply_tx
                        .send(Value::String(format!(
                            "[ERROR] Failed to initialize agent workspace: {err:#}"
                        )))
                        .await;
                    self.is_running.store(false, Ordering::SeqCst);
                    continue;
                }
            };
            let handler = Arc::new(RwLock::new(AgentHandler::new(handler_cwd)));
            handler.write().unwrap().code_stop_signal = self.stop_sig.clone();
            let (output_tx, mut output_rx) = mpsc::channel::<String>(256);

            let exit_reason = agent_runner_loop(
                client,
                sys_prompt.clone(),
                query,
                handler,
                tools.clone(),
                70,
                true,
                output_tx,
                None,
            )
            .await;
            let final_output = exit_reason
                .ok()
                .and_then(|payload| {
                    payload
                        .get("final_output")
                        .and_then(|value| value.as_str())
                        .map(str::to_string)
                })
                .filter(|value| !value.trim().is_empty());

            let mut streamed_any = false;
            while let Some(chunk) = output_rx.recv().await {
                if !chunk.trim().is_empty() {
                    streamed_any = true;
                }
                let _ = reply_tx.send(Value::String(chunk)).await;
            }

            if !streamed_any {
                if let Some(text) = final_output {
                    let _ = reply_tx.send(Value::String(text)).await;
                }
            }

            let _ = reply_tx.send(Value::String("[DONE]".into())).await;

            self.is_running.store(false, Ordering::SeqCst);
        }
    }

    pub async fn run_task(
        &mut self,
        query: String,
        source: String,
        display_tx: mpsc::Sender<Value>,
        sys_prompt: String,
        tools_schema: Vec<ToolSchema>,
    ) {
        self.is_running.store(true, Ordering::SeqCst);
        self.stop_sig.store(false, Ordering::SeqCst);

        let rquery = query.replace('\n', " ");
        let trunc = if rquery.len() > 200 {
            format!("{}...", &rquery[..200])
        } else {
            rquery
        };
        self.history.push(format!("[USER]: {}", trunc));

        let (client, route) = match self.select_client_for_query(&query) {
            Some(c) => c,
            None => {
                let _ = display_tx
                    .send(serde_json::json!({
                        "done": "[ERROR] No valid LLM session configured.",
                        "source": source
                    }))
                    .await;
                self.is_running.store(false, Ordering::SeqCst);
                return;
            }
        };

        let handler_cwd = match default_agent_cwd() {
            Ok(path) => path,
            Err(err) => {
                let _ = display_tx
                    .send(serde_json::json!({
                        "done": format!("[ERROR] Failed to initialize agent workspace: {err:#}"),
                        "source": source
                    }))
                    .await;
                self.is_running.store(false, Ordering::SeqCst);
                return;
            }
        };
        let handler = Arc::new(RwLock::new(AgentHandler::new(handler_cwd)));
        handler.write().unwrap().code_stop_signal = self.stop_sig.clone();
        // Set model name for error attribution
        let model_name = client
            .try_read()
            .map(|g| g.model().to_string())
            .unwrap_or_default();
        handler.write().unwrap().model_name = RwLock::new(model_name);

        // Inherit mode and workflow from the frontend-selected agent state.
        *handler.write().unwrap().mode.write().unwrap() = *self.agent_mode.read().unwrap();
        *handler.write().unwrap().workflow.write().unwrap() =
            self.agent_workflow.read().unwrap().clone();
        self.handler = Some(handler.clone());

        // Apply runtime reasoning effort override to the current LLM client
        {
            let effort = route
                .as_ref()
                .and_then(|decision| decision.reasoning_effort.clone())
                .or_else(|| self.reasoning_effort.read().unwrap().clone());
            if let Ok(mut guard) = client.try_write() {
                guard.set_reasoning_effort(effort);
            }
        }

        if let Some(route) = &route {
            let _ = display_tx
                .send(serde_json::json!({
                    "route": {
                        "model": route.model,
                        "display_name": route.display_name,
                        "reasoning_effort": route.reasoning_effort,
                        "reason": route.reason,
                    },
                    "source": source,
                }))
                .await;
        }

        // YOLO mode: inject auto-approve note into system prompt
        let sys_prompt = if *self.yolo_enabled.read().unwrap() {
            format!("{}\n\n[YOLO MODE ACTIVE] Execute all tool calls immediately without asking for confirmation. Be autonomous and decisive.", sys_prompt)
        } else {
            sys_prompt
        };

        // ── Multi-agent ACP execution ────────────────────────────────
        if self.is_multi_agent() {
            crate::acp::run_acp_task(
                client,
                query,
                handler.clone(),
                tools_schema,
                display_tx.clone(),
                self.verbose,
            )
            .await;
            self.is_running.store(false, Ordering::SeqCst);
            return;
        }

        // ── One Shot autonomous execution ────────────────────────────
        if self.is_one_shot() {
            crate::oneshot::run_one_shot_task(
                client,
                query,
                handler.clone(),
                tools_schema,
                display_tx.clone(),
                sys_prompt,
                self.verbose,
            )
            .await;
            self.is_running.store(false, Ordering::SeqCst);
            return;
        }

        // ── Workflow execution ──────────────────────────────────────
        let workflow_active = { handler.read().unwrap().workflow.read().unwrap().active };
        if workflow_active {
            let nodes = {
                handler
                    .read()
                    .unwrap()
                    .workflow
                    .read()
                    .unwrap()
                    .nodes
                    .clone()
            };
            let current_node_idx = {
                handler
                    .read()
                    .unwrap()
                    .workflow
                    .read()
                    .unwrap()
                    .current_node
            };
            let mut all_responses = String::new();

            for _node_index in current_node_idx..nodes.len() {
                let (mode, max_turns) = {
                    let handler_guard = handler.read().unwrap();
                    let wf = handler_guard.workflow.read().unwrap();
                    if let Some(mode) = wf.current_mode() {
                        (mode, mode.max_turns())
                    } else {
                        break;
                    }
                };

                // Set mode on handler
                *handler.write().unwrap().mode.write().unwrap() = mode;

                // Build mode-prefixed system prompt
                let mode_prompt = format!("{}{}", mode.system_prompt_prefix(), sys_prompt);

                let _ = display_tx
                    .send(serde_json::json!({
                        "next": format!("\n## {} Mode: {}\n", mode_emoji(mode), mode_str(mode)),
                        "source": source,
                    }))
                    .await;

                let (output_tx, mut output_rx) = mpsc::channel::<String>(256);
                let verbose = self.verbose;
                let source_for_stream = source.clone();
                let display_tx_for_stream = display_tx.clone();

                let stream_task = tokio::spawn(async move {
                    let mut full_resp = String::new();
                    let mut last_pos: usize = 0;
                    while let Some(chunk) = output_rx.recv().await {
                        full_resp.push_str(&chunk);
                        if full_resp.len() - last_pos > 50 || chunk.contains("LLM Running") {
                            let _ = display_tx_for_stream
                                .send(serde_json::json!({
                                    "next": &full_resp[last_pos..],
                                    "source": source_for_stream
                                }))
                                .await;
                            last_pos = full_resp.len();
                        }
                    }
                    if last_pos < full_resp.len() {
                        let _ = display_tx_for_stream
                            .send(serde_json::json!({
                                "next": &full_resp[last_pos..],
                                "source": source_for_stream
                            }))
                            .await;
                    }
                    full_resp
                });

                let exit_reason = agent_runner_loop(
                    client.clone(),
                    mode_prompt,
                    query.clone(),
                    handler.clone(),
                    tools_schema.clone(),
                    max_turns,
                    verbose,
                    output_tx,
                    Some(self.stop_sig.clone()),
                )
                .await;

                if let Err(ref e) = exit_reason {
                    handler.read().unwrap().record_error(
                        "agent_runner_loop",
                        &format!("{:#}", e),
                        ErrorSeverity::Critical,
                        serde_json::json!({"query": &query[..query.len().min(200)], "mode": mode_str(mode)}),
                    );
                }

                let full_resp = stream_task.await.unwrap_or_default();
                all_responses.push_str(&format!(
                    "\n## {} Mode Output\n\n{}\n",
                    mode_emoji(mode),
                    full_resp
                ));

                // Advance workflow — signal transition between modes
                let has_next = handler.write().unwrap().workflow.write().unwrap().advance();
                if has_next {
                    let _ = display_tx
                        .send(serde_json::json!({
                            "next": "\n\n---\n\n",
                            "source": source,
                        }))
                        .await;
                }

                if self.stop_sig.load(Ordering::SeqCst) {
                    break;
                }
            }

            let _ = display_tx
                .send(serde_json::json!({
                    "done": all_responses,
                    "source": source,
                }))
                .await;
            self.is_running.store(false, Ordering::SeqCst);
            return;
        }

        // ── Single mode execution (no workflow) ────────────────────
        let mode = { *handler.read().unwrap().mode.read().unwrap() };
        let max_turns = mode.max_turns();
        let mode_prompt = format!("{}{}", mode.system_prompt_prefix(), sys_prompt);

        let (output_tx, mut output_rx) = mpsc::channel::<String>(256);

        let verbose = self.verbose;
        let source_for_stream = source.clone();
        let display_tx_for_stream = display_tx.clone();

        let stream_task = tokio::spawn(async move {
            let mut full_resp = String::new();
            let mut last_pos: usize = 0;

            while let Some(chunk) = output_rx.recv().await {
                full_resp.push_str(&chunk);
                if full_resp.len() - last_pos > 50 || chunk.contains("LLM Running") {
                    let _ = display_tx_for_stream
                        .send(serde_json::json!({
                            "next": &full_resp[last_pos..],
                            "source": source_for_stream
                        }))
                        .await;
                    last_pos = full_resp.len();
                }
            }

            if last_pos < full_resp.len() {
                let _ = display_tx_for_stream
                    .send(serde_json::json!({
                        "next": &full_resp[last_pos..],
                        "source": source_for_stream
                    }))
                    .await;
            }

            full_resp
        });

        let query_preview: String = query.chars().take(200).collect();
        let exit_reason = agent_runner_loop(
            client,
            mode_prompt,
            query,
            handler.clone(),
            tools_schema,
            max_turns,
            verbose,
            output_tx,
            Some(self.stop_sig.clone()),
        )
        .await;

        // Record any agent_runner_loop error in persistent memory
        if let Err(ref e) = exit_reason {
            handler.read().unwrap().record_error(
                "agent_runner_loop",
                &format!("{:#}", e),
                ErrorSeverity::Critical,
                serde_json::json!({"query": query_preview}),
            );
        }

        let full_resp = stream_task.await.unwrap_or_default();
        let final_resp = exit_reason
            .as_ref()
            .ok()
            .and_then(|payload| {
                payload
                    .get("final_output")
                    .and_then(|value| value.as_str())
                    .filter(|value| !value.trim().is_empty())
                    .map(str::to_string)
            })
            .unwrap_or_else(|| full_resp.clone());

        let _ = display_tx
            .send(serde_json::json!({
                "done": final_resp,
                "source": source
            }))
            .await;

        let _ = exit_reason;

        self.is_running.store(false, Ordering::SeqCst);
    }
}

// ── 3. AgentHandler ──────────────────────────────────────────────────────────

pub struct AgentHandler {
    pub working: RwLock<HashMap<String, String>>,
    pub current_turn: RwLock<usize>,
    pub history_info: RwLock<Vec<String>>,
    pub max_turns: RwLock<usize>,
    pub code_stop_signal: Arc<AtomicBool>,
    pub _done_hooks: Vec<String>,
    pub cwd: PathBuf,
    /// Error memory — persists across sessions
    pub error_memory: ErrorMemory,
    /// Current model name for error attribution
    pub model_name: RwLock<String>,
    /// Current agent mode
    pub mode: RwLock<AgentMode>,
    /// Workflow state
    pub workflow: RwLock<Workflow>,
}

impl AgentHandler {
    pub fn new(cwd: PathBuf) -> Self {
        // Derive project_dir from cwd: cwd is usually ./temp, so go up one level
        let project_dir = if cwd.ends_with("temp") {
            cwd.parent().map(|p| p.to_path_buf()).unwrap_or(cwd.clone())
        } else {
            cwd.clone()
        };
        Self {
            working: RwLock::new(HashMap::new()),
            current_turn: RwLock::new(0),
            history_info: RwLock::new(Vec::new()),
            max_turns: RwLock::new(70),
            code_stop_signal: Arc::new(AtomicBool::new(false)),
            _done_hooks: Vec::new(),
            cwd,
            error_memory: ErrorMemory::new(&project_dir),
            model_name: RwLock::new(String::new()),
            mode: RwLock::new(AgentMode::Work),
            workflow: RwLock::new(Workflow::default()),
        }
    }

    /// Record an error to persistent memory. Called from every dispatch error path.
    pub fn record_error(&self, tool: &str, message: &str, severity: ErrorSeverity, context: Value) {
        let turn = *self.current_turn.read().unwrap();
        let model = self.model_name.read().unwrap().clone();
        if let Err(e) = self
            .error_memory
            .record(tool, message, severity, context, &model, turn)
        {
            log::warn!("Failed to record error to memory: {e}");
        }
    }

    // ── dispatch ─────────────────────────────────────────────────────────

    pub fn dispatch(&self, tool_name: &str, args: Value, response_content: &str) -> StepOutcome {
        let (tool_name, args) = canonicalize_tool_invocation(tool_name, &args);
        match tool_name.as_str() {
            "code_run" => self.do_code_run(&args),
            "run_tests" => self.do_run_tests(&args),
            "file_read" => self.do_file_read(&args),
            "file_patch" => self.do_file_patch(&args),
            "file_write" => self.do_file_write(&args, response_content),
            "file_revert" => self.do_file_revert(&args),
            "web_scan" => self.do_web_scan(&args),
            "web_execute_js" => self.do_web_execute_js(&args),
            "web_search" => self.do_web_search(&args),
            "web_fetch" => self.do_web_fetch(&args),
            "ask_user" => self.do_ask_user(&args),
            "update_working_checkpoint" => self.do_update_working_checkpoint(&args),
            "no_tool" => self.do_no_tool(response_content),
            "start_long_term_update" => self.do_start_long_term_update(),
            "workspace_open" => self.do_workspace_open(&args),
            "workspace_list" => self.do_workspace_list(&args),
            "workspace_search" => self.do_workspace_search(&args),
            "file_search" => self.do_file_search(&args),
            "content_search" => self.do_content_search(&args),
            "semantic_search" => self.do_semantic_search(&args),
            "lsp_find_definition" => self.do_lsp_find_definition(&args),
            "lsp_find_references" => self.do_lsp_find_references(&args),
            "lsp_get_diagnostics" => self.do_lsp_get_diagnostics(&args),
            "lsp_rename_preview" => self.do_lsp_rename_preview(&args),
            "mcp_list_servers" => self.do_mcp_list_servers(),
            "mcp_list_tools" => self.do_mcp_list_tools(&args),
            "mcp_call_tool" => self.do_mcp_call_tool(&args),
            "git_status" => self.do_git_status(&args),
            "git_diff" => self.do_git_diff(&args),
            "git_log" => self.do_git_log(&args),
            "remote_connect" => self.do_remote_connect(&args),
            "remote_exec" => self.do_remote_exec(&args),
            "remote_file_read" => self.do_remote_file_read(&args),
            "remote_file_write" => self.do_remote_file_write(&args),
            "remote_list_dir" => self.do_remote_list_dir(&args),
            "media_info" => self.do_media_info(&args),
            "media_extract" => self.do_media_extract(&args),
            "computer_screenshot" => self.do_computer_screenshot(&args),
            "computer_zoom" => self.do_computer_zoom(&args),
            "computer_left_click" => self.do_computer_left_click(&args),
            "computer_right_click" => self.do_computer_right_click(&args),
            "computer_middle_click" => self.do_computer_middle_click(&args),
            "computer_double_click" => self.do_computer_double_click(&args),
            "computer_triple_click" => self.do_computer_triple_click(&args),
            "computer_left_click_drag" => self.do_computer_left_click_drag(&args),
            "computer_mouse_move" => self.do_computer_mouse_move(&args),
            "computer_left_mouse_down" => self.do_computer_left_mouse_down(&args),
            "computer_left_mouse_up" => self.do_computer_left_mouse_up(&args),
            "computer_cursor_position" => self.do_computer_cursor_position(),
            "computer_scroll" => self.do_computer_scroll(&args),
            "computer_type" => self.do_computer_type(&args),
            "computer_key" => self.do_computer_key(&args),
            "computer_hold_key" => self.do_computer_hold_key(&args),
            "computer_open_application" => self.do_computer_open_application(&args),
            "computer_switch_display" => self.do_computer_switch_display(),
            "computer_request_access" => self.do_computer_request_access(&args),
            "computer_list_granted_applications" => self.do_computer_list_granted_applications(),
            "computer_read_clipboard" => self.do_computer_read_clipboard(),
            "computer_write_clipboard" => self.do_computer_write_clipboard(&args),
            "computer_wait" => self.do_computer_wait(&args),
            "computer_batch" => self.do_computer_batch(&args),
            "computer_open" => self.do_computer_open(&args),
            "computer_action" => self.do_computer_action(&args),
            _ => {
                self.record_error(
                    &tool_name,
                    "Unknown tool invoked",
                    ErrorSeverity::Validation,
                    serde_json::json!({"args": crate::tools::smart_format(&args, Some(200), None)}),
                );
                StepOutcome {
                    data: Value::Null,
                    next_prompt: Some(format!("unknown tool: {}", tool_name)),
                    should_exit: false,
                }
            }
        }
    }

    // ── do_* methods ─────────────────────────────────────────────────────

    fn cwd_str(&self) -> String {
        self.cwd.to_string_lossy().to_string()
    }

    fn do_code_run(&self, args: &Value) -> StepOutcome {
        let (code, code_type) = code_run_request_from_args(args);
        let timeout = args.get("timeout").and_then(|v| v.as_u64());
        let code_cwd = args.get("cwd").and_then(|v| v.as_str());
        let cwd = self.cwd_str();

        match crate::tools::code_run(
            &code,
            &code_type,
            timeout,
            Some(&cwd),
            code_cwd,
            Some(self.code_stop_signal.clone()),
        ) {
            Ok(result) => StepOutcome {
                data: result,
                next_prompt: Some(String::new()),
                should_exit: false,
            },
            Err(e) => {
                let msg = format!("{:#}", e);
                self.record_error(
                    "code_run",
                    &msg,
                    ErrorSeverity::Tool,
                    serde_json::json!({"code_type": code_type, "timeout": timeout}),
                );
                StepOutcome {
                    data: serde_json::json!({"error": msg}),
                    next_prompt: Some(String::new()),
                    should_exit: false,
                }
            }
        }
    }

    fn do_run_tests(&self, args: &Value) -> StepOutcome {
        let command = args.get("command").and_then(|v| v.as_str());
        let path = args
            .get("path")
            .or_else(|| args.get("cwd"))
            .and_then(|v| v.as_str());
        let max_output_chars = args
            .get("max_output_chars")
            .or_else(|| args.get("max_chars"))
            .and_then(|v| v.as_u64())
            .map(|value| value as usize);

        match crate::tools::run_tests(command, path, max_output_chars) {
            Ok(result) => StepOutcome {
                data: result,
                next_prompt: Some(String::new()),
                should_exit: false,
            },
            Err(e) => {
                let msg = format!("{:#}", e);
                self.record_error(
                    "run_tests",
                    &msg,
                    ErrorSeverity::Tool,
                    serde_json::json!({"command": command, "path": path}),
                );
                StepOutcome {
                    data: serde_json::json!({"error": msg}),
                    next_prompt: Some(String::new()),
                    should_exit: false,
                }
            }
        }
    }

    fn do_file_read(&self, args: &Value) -> StepOutcome {
        let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
        let start = args
            .get("start")
            .and_then(|v| v.as_u64())
            .map(|n| n as usize);
        let keyword = args.get("keyword").and_then(|v| v.as_str());
        let count = args
            .get("count")
            .and_then(|v| v.as_u64())
            .map(|n| n as usize);
        let show_linenos = args.get("show_linenos").and_then(|v| v.as_bool());

        match crate::tools::file_read(path, start, keyword, count, show_linenos) {
            Ok(text) => StepOutcome {
                data: serde_json::json!({"content": text}),
                next_prompt: Some(String::new()),
                should_exit: false,
            },
            Err(e) => {
                let msg = format!("{:#}", e);
                self.record_error(
                    "file_read",
                    &msg,
                    ErrorSeverity::Tool,
                    serde_json::json!({"path": path}),
                );
                StepOutcome {
                    data: serde_json::json!({"error": msg}),
                    next_prompt: Some(String::new()),
                    should_exit: false,
                }
            }
        }
    }

    fn do_file_patch(&self, args: &Value) -> StepOutcome {
        let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
        let old_content = args
            .get("old_content")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let new_content = args
            .get("new_content")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        match crate::tools::file_patch(path, old_content, new_content) {
            Ok(result) => StepOutcome {
                data: result,
                next_prompt: Some(String::new()),
                should_exit: false,
            },
            Err(e) => {
                let msg = format!("{:#}", e);
                self.record_error(
                    "file_patch",
                    &msg,
                    ErrorSeverity::Tool,
                    serde_json::json!({"path": path}),
                );
                StepOutcome {
                    data: serde_json::json!({"error": msg}),
                    next_prompt: Some(String::new()),
                    should_exit: false,
                }
            }
        }
    }

    fn do_file_write(&self, args: &Value, response_content: &str) -> StepOutcome {
        let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
        let content_from_tool_block = file_write_content_from_tool_block(response_content, path);
        let content_from_response = file_write_content_from_response(response_content);
        let content = args
            .get("content")
            .or_else(|| args.get("text"))
            .and_then(|v| v.as_str())
            .map(str::to_string)
            .or_else(|| content_from_tool_block.clone())
            .or_else(|| content_from_response.clone())
            .unwrap_or_default();
        let mode = args
            .get("mode")
            .and_then(|v| v.as_str())
            .unwrap_or("overwrite");

        if content.is_empty()
            && args.get("content").is_none()
            && args.get("text").is_none()
            && content_from_tool_block.is_none()
            && content_from_response.is_none()
        {
            let msg = "file_write requires content in args.content/text or in a <file_content>...</file_content> block";
            self.record_error(
                "file_write",
                msg,
                ErrorSeverity::Validation,
                serde_json::json!({"path": path, "mode": mode}),
            );
            return StepOutcome {
                data: serde_json::json!({"error": msg}),
                next_prompt: Some(String::new()),
                should_exit: false,
            };
        }

        match crate::tools::file_write(path, &content, Some(mode)) {
            Ok(result) => StepOutcome {
                data: result,
                next_prompt: Some(String::new()),
                should_exit: false,
            },
            Err(e) => {
                let msg = format!("{:#}", e);
                self.record_error(
                    "file_write",
                    &msg,
                    ErrorSeverity::Tool,
                    serde_json::json!({"path": path, "mode": mode}),
                );
                StepOutcome {
                    data: serde_json::json!({"error": msg}),
                    next_prompt: Some(String::new()),
                    should_exit: false,
                }
            }
        }
    }

    fn do_file_revert(&self, args: &Value) -> StepOutcome {
        let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
        let task_id = args.get("task_id").and_then(|v| v.as_str());

        match crate::tools::file_revert(path, task_id) {
            Ok(result) => StepOutcome {
                data: result,
                next_prompt: Some(String::new()),
                should_exit: false,
            },
            Err(e) => {
                let msg = format!("{:#}", e);
                self.record_error(
                    "file_revert",
                    &msg,
                    ErrorSeverity::Tool,
                    serde_json::json!({"path": path}),
                );
                StepOutcome {
                    data: serde_json::json!({"error": msg}),
                    next_prompt: Some(String::new()),
                    should_exit: false,
                }
            }
        }
    }

    fn do_web_scan(&self, args: &Value) -> StepOutcome {
        match crate::tools::web_scan(
            args.get("tabs_only").and_then(|v| v.as_bool()),
            args.get("switch_tab_id").and_then(|v| v.as_str()),
            args.get("text_only").and_then(|v| v.as_bool()),
        ) {
            Ok(data) => StepOutcome {
                data,
                next_prompt: Some(String::new()),
                should_exit: false,
            },
            Err(e) => {
                let msg = format!("{:#}", e);
                self.record_error("web_scan", &msg, ErrorSeverity::Tool, Value::Null);
                StepOutcome {
                    data: serde_json::json!({"error": msg}),
                    next_prompt: Some(String::new()),
                    should_exit: false,
                }
            }
        }
    }

    fn do_web_execute_js(&self, args: &Value) -> StepOutcome {
        match crate::tools::web_execute_js(
            args.get("script").and_then(|v| v.as_str()).unwrap_or(""),
            args.get("switch_tab_id").and_then(|v| v.as_str()),
            args.get("no_monitor").and_then(|v| v.as_bool()),
        ) {
            Ok(data) => StepOutcome {
                data,
                next_prompt: Some(String::new()),
                should_exit: false,
            },
            Err(e) => {
                let msg = format!("{:#}", e);
                self.record_error("web_execute_js", &msg, ErrorSeverity::Tool, Value::Null);
                StepOutcome {
                    data: serde_json::json!({"error": msg}),
                    next_prompt: Some(String::new()),
                    should_exit: false,
                }
            }
        }
    }

    fn do_ask_user(&self, args: &Value) -> StepOutcome {
        let question = args
            .get("question")
            .and_then(|v| v.as_str())
            .unwrap_or("Please provide input:");
        let candidates: Vec<String> = args
            .get("candidates")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();

        let data = crate::tools::ask_user(
            question,
            if candidates.is_empty() {
                None
            } else {
                Some(candidates.as_slice())
            },
        );

        StepOutcome {
            data,
            next_prompt: Some(String::new()),
            should_exit: true,
        }
    }

    fn do_update_working_checkpoint(&self, args: &Value) -> StepOutcome {
        let mut working = self.working.write().unwrap();
        if let Some(ki) = args.get("key_info").and_then(|v| v.as_str()) {
            working.insert("key_info".into(), ki.to_string());
        }
        if let Some(rs) = args.get("related_sop").and_then(|v| v.as_str()) {
            working.insert("related_sop".into(), rs.to_string());
        }
        working.insert("passed_sessions".into(), "0".to_string());

        StepOutcome {
            data: serde_json::json!({"result": "working key_info updated"}),
            next_prompt: Some(String::new()),
            should_exit: false,
        }
    }

    fn do_no_tool(&self, response_content: &str) -> StepOutcome {
        if response_content.trim().is_empty() {
            return StepOutcome {
                data: Value::Null,
                next_prompt: Some("[System] Blank response, regenerate and tooluse".into()),
                should_exit: false,
            };
        }
        StepOutcome {
            data: serde_json::json!({"content": response_content}),
            next_prompt: None,
            should_exit: false,
        }
    }

    fn do_start_long_term_update(&self) -> StepOutcome {
        StepOutcome {
            data: Value::Null,
            next_prompt: Some(
                "[System] Please extract long-term validated info from recent tasks and update memory."
                    .into(),
            ),
            should_exit: false,
        }
    }

    fn do_workspace_open(&self, args: &Value) -> StepOutcome {
        let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
        let name = args.get("name").and_then(|v| v.as_str()).unwrap_or("");
        StepOutcome {
            data: crate::workspace::open_folder(path, name),
            next_prompt: Some(String::new()),
            should_exit: false,
        }
    }

    fn do_workspace_list(&self, args: &Value) -> StepOutcome {
        let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
        let pattern = args.get("pattern").and_then(|v| v.as_str()).unwrap_or("*");
        StepOutcome {
            data: crate::workspace::list_files(path, pattern),
            next_prompt: Some(String::new()),
            should_exit: false,
        }
    }

    fn do_workspace_search(&self, args: &Value) -> StepOutcome {
        let query = args.get("query").and_then(|v| v.as_str()).unwrap_or("");
        let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
        let max_results = args
            .get("max_results")
            .and_then(|v| v.as_u64())
            .map(|value| value as usize)
            .unwrap_or(50);
        StepOutcome {
            data: crate::workspace::search_files(query, path, max_results),
            next_prompt: Some(String::new()),
            should_exit: false,
        }
    }

    fn do_file_search(&self, args: &Value) -> StepOutcome {
        let query = args
            .get("query")
            .or_else(|| args.get("pattern"))
            .or_else(|| args.get("name"))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
        let max_results = args
            .get("max_results")
            .or_else(|| args.get("count"))
            .and_then(|v| v.as_u64())
            .map(|value| value as usize)
            .unwrap_or(50);

        StepOutcome {
            data: crate::workspace::search_files(query, path, max_results),
            next_prompt: Some(String::new()),
            should_exit: false,
        }
    }

    fn do_content_search(&self, args: &Value) -> StepOutcome {
        let pattern = args.get("pattern").and_then(|v| v.as_str()).unwrap_or("");
        let path = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");
        let glob_pat = args.get("glob").and_then(|v| v.as_str());
        let context_lines = args
            .get("context_lines")
            .and_then(|v| v.as_u64())
            .map(|n| n as usize);
        let max_results = args
            .get("max_results")
            .and_then(|v| v.as_u64())
            .map(|n| n as usize);
        let case_sensitive = args.get("case_sensitive").and_then(|v| v.as_bool());

        match crate::tools::content_search(
            pattern,
            path,
            glob_pat,
            context_lines,
            max_results,
            case_sensitive,
        ) {
            Ok(result) => StepOutcome {
                data: result,
                next_prompt: Some(String::new()),
                should_exit: false,
            },
            Err(e) => {
                let msg = format!("{:#}", e);
                self.record_error(
                    "content_search",
                    &msg,
                    ErrorSeverity::Tool,
                    serde_json::json!({"pattern": pattern, "path": path}),
                );
                StepOutcome {
                    data: serde_json::json!({"error": msg}),
                    next_prompt: Some(String::new()),
                    should_exit: false,
                }
            }
        }
    }

    fn do_semantic_search(&self, args: &Value) -> StepOutcome {
        let query = args.get("query").and_then(|v| v.as_str()).unwrap_or("");
        let path = args.get("path").and_then(|v| v.as_str());
        let max_results = args
            .get("max_results")
            .and_then(|v| v.as_u64())
            .map(|value| value as usize);

        match crate::tools::semantic_search(query, path, max_results) {
            Ok(result) => StepOutcome {
                data: result,
                next_prompt: Some(String::new()),
                should_exit: false,
            },
            Err(e) => {
                let msg = format!("{:#}", e);
                self.record_error(
                    "semantic_search",
                    &msg,
                    ErrorSeverity::Tool,
                    serde_json::json!({"query": query, "path": path}),
                );
                StepOutcome {
                    data: serde_json::json!({"error": msg}),
                    next_prompt: Some(String::new()),
                    should_exit: false,
                }
            }
        }
    }

    fn do_lsp_find_definition(&self, args: &Value) -> StepOutcome {
        let symbol = args
            .get("symbol")
            .or_else(|| args.get("query"))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let path = args.get("path").and_then(|v| v.as_str());
        let max_results = args
            .get("max_results")
            .and_then(|v| v.as_u64())
            .map(|value| value as usize);

        match crate::tools::lsp_find_definition(symbol, path, max_results) {
            Ok(result) => StepOutcome {
                data: result,
                next_prompt: Some(String::new()),
                should_exit: false,
            },
            Err(e) => {
                let msg = format!("{:#}", e);
                self.record_error(
                    "lsp_find_definition",
                    &msg,
                    ErrorSeverity::Tool,
                    serde_json::json!({"symbol": symbol, "path": path}),
                );
                StepOutcome {
                    data: serde_json::json!({"error": msg}),
                    next_prompt: Some(String::new()),
                    should_exit: false,
                }
            }
        }
    }

    fn do_lsp_find_references(&self, args: &Value) -> StepOutcome {
        let symbol = args
            .get("symbol")
            .or_else(|| args.get("query"))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let path = args.get("path").and_then(|v| v.as_str());
        let max_results = args
            .get("max_results")
            .and_then(|v| v.as_u64())
            .map(|value| value as usize);

        match crate::tools::lsp_find_references(symbol, path, max_results) {
            Ok(result) => StepOutcome {
                data: result,
                next_prompt: Some(String::new()),
                should_exit: false,
            },
            Err(e) => {
                let msg = format!("{:#}", e);
                self.record_error(
                    "lsp_find_references",
                    &msg,
                    ErrorSeverity::Tool,
                    serde_json::json!({"symbol": symbol, "path": path}),
                );
                StepOutcome {
                    data: serde_json::json!({"error": msg}),
                    next_prompt: Some(String::new()),
                    should_exit: false,
                }
            }
        }
    }

    fn do_lsp_get_diagnostics(&self, args: &Value) -> StepOutcome {
        let path = args.get("path").and_then(|v| v.as_str());
        let max_results = args
            .get("max_results")
            .and_then(|v| v.as_u64())
            .map(|value| value as usize);

        match crate::tools::lsp_get_diagnostics(path, max_results) {
            Ok(result) => StepOutcome {
                data: result,
                next_prompt: Some(String::new()),
                should_exit: false,
            },
            Err(e) => {
                let msg = format!("{:#}", e);
                self.record_error(
                    "lsp_get_diagnostics",
                    &msg,
                    ErrorSeverity::Tool,
                    serde_json::json!({"path": path}),
                );
                StepOutcome {
                    data: serde_json::json!({"error": msg}),
                    next_prompt: Some(String::new()),
                    should_exit: false,
                }
            }
        }
    }

    fn do_lsp_rename_preview(&self, args: &Value) -> StepOutcome {
        let symbol = args.get("symbol").and_then(|v| v.as_str()).unwrap_or("");
        let new_name = args.get("new_name").and_then(|v| v.as_str()).unwrap_or("");
        let path = args.get("path").and_then(|v| v.as_str());
        let max_results = args
            .get("max_results")
            .and_then(|v| v.as_u64())
            .map(|value| value as usize);

        match crate::tools::lsp_rename_preview(symbol, new_name, path, max_results) {
            Ok(result) => StepOutcome {
                data: result,
                next_prompt: Some(String::new()),
                should_exit: false,
            },
            Err(e) => {
                let msg = format!("{:#}", e);
                self.record_error(
                    "lsp_rename_preview",
                    &msg,
                    ErrorSeverity::Tool,
                    serde_json::json!({"symbol": symbol, "new_name": new_name, "path": path}),
                );
                StepOutcome {
                    data: serde_json::json!({"error": msg}),
                    next_prompt: Some(String::new()),
                    should_exit: false,
                }
            }
        }
    }

    fn do_mcp_list_servers(&self) -> StepOutcome {
        match crate::tools::mcp_list_servers() {
            Ok(result) => StepOutcome {
                data: result,
                next_prompt: Some(String::new()),
                should_exit: false,
            },
            Err(e) => {
                let msg = format!("{:#}", e);
                self.record_error(
                    "mcp_list_servers",
                    &msg,
                    ErrorSeverity::Tool,
                    serde_json::json!({}),
                );
                StepOutcome {
                    data: serde_json::json!({"error": msg}),
                    next_prompt: Some(String::new()),
                    should_exit: false,
                }
            }
        }
    }

    fn do_mcp_list_tools(&self, args: &Value) -> StepOutcome {
        let server = args.get("server").and_then(|v| v.as_str());
        match crate::tools::mcp_list_tools(server) {
            Ok(result) => StepOutcome {
                data: result,
                next_prompt: Some(String::new()),
                should_exit: false,
            },
            Err(e) => {
                let msg = format!("{:#}", e);
                self.record_error(
                    "mcp_list_tools",
                    &msg,
                    ErrorSeverity::Tool,
                    serde_json::json!({"server": server}),
                );
                StepOutcome {
                    data: serde_json::json!({"error": msg}),
                    next_prompt: Some(String::new()),
                    should_exit: false,
                }
            }
        }
    }

    fn do_mcp_call_tool(&self, args: &Value) -> StepOutcome {
        let server = args.get("server").and_then(|v| v.as_str()).unwrap_or("");
        let tool = args.get("tool").and_then(|v| v.as_str()).unwrap_or("");
        let arguments = args
            .get("arguments")
            .cloned()
            .unwrap_or_else(|| Value::Object(serde_json::Map::new()));

        match crate::tools::mcp_call_tool(server, tool, arguments) {
            Ok(result) => StepOutcome {
                data: result,
                next_prompt: Some(String::new()),
                should_exit: false,
            },
            Err(e) => {
                let msg = format!("{:#}", e);
                self.record_error(
                    "mcp_call_tool",
                    &msg,
                    ErrorSeverity::Tool,
                    serde_json::json!({"server": server, "tool": tool}),
                );
                StepOutcome {
                    data: serde_json::json!({"error": msg}),
                    next_prompt: Some(String::new()),
                    should_exit: false,
                }
            }
        }
    }

    fn do_git_status(&self, args: &Value) -> StepOutcome {
        let path = args.get("path").and_then(|v| v.as_str());
        match crate::tools::git_status(path) {
            Ok(result) => StepOutcome {
                data: result,
                next_prompt: Some(String::new()),
                should_exit: false,
            },
            Err(e) => {
                let msg = format!("{:#}", e);
                self.record_error(
                    "git_status",
                    &msg,
                    ErrorSeverity::Tool,
                    serde_json::json!({"path": path}),
                );
                StepOutcome {
                    data: serde_json::json!({"error": msg}),
                    next_prompt: Some(String::new()),
                    should_exit: false,
                }
            }
        }
    }

    fn do_git_diff(&self, args: &Value) -> StepOutcome {
        let staged = args.get("staged").and_then(|v| v.as_bool());
        let path = args.get("path").and_then(|v| v.as_str());
        let path_repo = args.get("path_repo").and_then(|v| v.as_str());

        match crate::tools::git_diff(staged, path, path_repo) {
            Ok(result) => StepOutcome {
                data: result,
                next_prompt: Some(String::new()),
                should_exit: false,
            },
            Err(e) => {
                let msg = format!("{:#}", e);
                self.record_error(
                    "git_diff",
                    &msg,
                    ErrorSeverity::Tool,
                    serde_json::json!({"path": path, "path_repo": path_repo}),
                );
                StepOutcome {
                    data: serde_json::json!({"error": msg}),
                    next_prompt: Some(String::new()),
                    should_exit: false,
                }
            }
        }
    }

    fn do_git_log(&self, args: &Value) -> StepOutcome {
        let count = args
            .get("count")
            .and_then(|v| v.as_u64())
            .map(|n| n as usize);
        let path_repo = args.get("path_repo").and_then(|v| v.as_str());

        match crate::tools::git_log(count, path_repo) {
            Ok(result) => StepOutcome {
                data: result,
                next_prompt: Some(String::new()),
                should_exit: false,
            },
            Err(e) => {
                let msg = format!("{:#}", e);
                self.record_error(
                    "git_log",
                    &msg,
                    ErrorSeverity::Tool,
                    serde_json::json!({"path_repo": path_repo}),
                );
                StepOutcome {
                    data: serde_json::json!({"error": msg}),
                    next_prompt: Some(String::new()),
                    should_exit: false,
                }
            }
        }
    }

    fn do_remote_connect(&self, args: &Value) -> StepOutcome {
        let name = args
            .get("name")
            .and_then(|v| v.as_str())
            .filter(|value| !value.is_empty())
            .unwrap_or("default");
        let host = args.get("host").and_then(|v| v.as_str()).unwrap_or("");
        let port = args.get("port").and_then(|v| v.as_u64()).unwrap_or(22) as u16;
        let username = args
            .get("username")
            .and_then(|v| v.as_str())
            .unwrap_or("root");
        let password = args.get("password").and_then(|v| v.as_str()).unwrap_or("");
        let key_path = args.get("key_path").and_then(|v| v.as_str()).unwrap_or("");

        let data = match crate::remote::connect_global(
            name, host, port, username, password, key_path, "", 22, "",
        ) {
            Ok(value) => value,
            Err(err) => {
                let msg = format!("{:#}", err);
                self.record_error(
                    "remote_connect",
                    &msg,
                    ErrorSeverity::System,
                    serde_json::json!({"host": host, "port": port}),
                );
                serde_json::json!({"status": "error", "msg": msg})
            }
        };

        StepOutcome {
            data,
            next_prompt: Some(String::new()),
            should_exit: false,
        }
    }

    fn do_remote_exec(&self, args: &Value) -> StepOutcome {
        let server = args
            .get("server")
            .and_then(|v| v.as_str())
            .unwrap_or("default");
        let command = args.get("command").and_then(|v| v.as_str()).unwrap_or("");
        let timeout = args.get("timeout").and_then(|v| v.as_u64()).unwrap_or(60);
        let cwd = args.get("cwd").and_then(|v| v.as_str()).unwrap_or("");

        let data = match crate::remote::exec_global(server, command, timeout, cwd) {
            Ok(value) => value,
            Err(err) => {
                let msg = format!("{:#}", err);
                self.record_error(
                    "remote_exec",
                    &msg,
                    ErrorSeverity::Tool,
                    serde_json::json!({"server": server, "command": command.chars().take(200).collect::<String>()}),
                );
                serde_json::json!({"status": "error", "msg": msg})
            }
        };

        StepOutcome {
            data,
            next_prompt: Some(String::new()),
            should_exit: false,
        }
    }

    fn do_remote_file_read(&self, args: &Value) -> StepOutcome {
        let server = args
            .get("server")
            .and_then(|v| v.as_str())
            .unwrap_or("default");
        let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
        let data = match crate::remote::read_global(server, path) {
            Ok(value) => value,
            Err(err) => {
                let msg = format!("{:#}", err);
                self.record_error(
                    "remote_file_read",
                    &msg,
                    ErrorSeverity::Tool,
                    serde_json::json!({"server": server, "path": path}),
                );
                serde_json::json!({"status": "error", "msg": msg})
            }
        };
        StepOutcome {
            data,
            next_prompt: Some(String::new()),
            should_exit: false,
        }
    }

    fn do_remote_file_write(&self, args: &Value) -> StepOutcome {
        let server = args
            .get("server")
            .and_then(|v| v.as_str())
            .unwrap_or("default");
        let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
        let content = args.get("content").and_then(|v| v.as_str()).unwrap_or("");
        let data = match crate::remote::write_global(server, path, content) {
            Ok(value) => value,
            Err(err) => {
                let msg = format!("{:#}", err);
                self.record_error(
                    "remote_file_write",
                    &msg,
                    ErrorSeverity::Tool,
                    serde_json::json!({"server": server, "path": path}),
                );
                serde_json::json!({"status": "error", "msg": msg})
            }
        };
        StepOutcome {
            data,
            next_prompt: Some(String::new()),
            should_exit: false,
        }
    }

    fn do_remote_list_dir(&self, args: &Value) -> StepOutcome {
        let server = args
            .get("server")
            .and_then(|v| v.as_str())
            .unwrap_or("default");
        let path = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");
        let data = match crate::remote::list_dir_global(server, path) {
            Ok(value) => value,
            Err(err) => {
                let msg = format!("{:#}", err);
                self.record_error(
                    "remote_list_dir",
                    &msg,
                    ErrorSeverity::Tool,
                    serde_json::json!({"server": server, "path": path}),
                );
                serde_json::json!({"status": "error", "msg": msg})
            }
        };
        StepOutcome {
            data,
            next_prompt: Some(String::new()),
            should_exit: false,
        }
    }

    fn do_media_info(&self, args: &Value) -> StepOutcome {
        let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
        StepOutcome {
            data: crate::media::get_file_info(path),
            next_prompt: Some(String::new()),
            should_exit: false,
        }
    }

    fn do_media_extract(&self, args: &Value) -> StepOutcome {
        let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
        let extract_type = args.get("type").and_then(|v| v.as_str()).unwrap_or("text");
        let data = match extract_type {
            "info" => crate::media::get_file_info(path),
            _ => crate::media::extract_text(path),
        };
        StepOutcome {
            data,
            next_prompt: Some(String::new()),
            should_exit: false,
        }
    }

    fn do_web_search(&self, args: &Value) -> StepOutcome {
        let query = args.get("query").and_then(|v| v.as_str()).unwrap_or("");
        let max_results = args
            .get("max_results")
            .or_else(|| args.get("count"))
            .and_then(|v| v.as_u64())
            .map(|value| value as usize);

        match crate::tools::web_search(query, max_results) {
            Ok(result) => StepOutcome {
                data: result,
                next_prompt: Some(String::new()),
                should_exit: false,
            },
            Err(e) => {
                let msg = format!("{:#}", e);
                self.record_error(
                    "web_search",
                    &msg,
                    ErrorSeverity::Tool,
                    json!({"query": query, "max_results": max_results}),
                );
                StepOutcome {
                    data: json!({"status": "error", "error": msg}),
                    next_prompt: Some(String::new()),
                    should_exit: false,
                }
            }
        }
    }

    fn do_web_fetch(&self, args: &Value) -> StepOutcome {
        let url = args.get("url").and_then(|v| v.as_str()).unwrap_or("");
        let max_chars = args
            .get("max_chars")
            .and_then(|v| v.as_u64())
            .map(|value| value as usize);

        match crate::tools::web_fetch(url, max_chars) {
            Ok(result) => StepOutcome {
                data: result,
                next_prompt: Some(String::new()),
                should_exit: false,
            },
            Err(e) => {
                let msg = format!("{:#}", e);
                self.record_error(
                    "web_fetch",
                    &msg,
                    ErrorSeverity::Tool,
                    json!({"url": url, "max_chars": max_chars}),
                );
                StepOutcome {
                    data: json!({"status": "error", "error": msg}),
                    next_prompt: Some(String::new()),
                    should_exit: false,
                }
            }
        }
    }

    // ── Computer Use ─────────────────────────────────────────────────────

    fn do_computer_screenshot(&self, args: &Value) -> StepOutcome {
        let region = args
            .get("region")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(|v| v.as_u64()).collect::<Vec<_>>());
        let display = args.get("display").and_then(|v| v.as_u64());

        match crate::computer_use::screenshot(region.as_deref(), display) {
            Ok(result) => StepOutcome {
                data: result,
                next_prompt: Some(String::new()),
                should_exit: false,
            },
            Err(e) => {
                let msg = format!("{:#}", e);
                self.record_error("computer_screenshot", &msg, ErrorSeverity::Tool, json!({}));
                StepOutcome {
                    data: json!({"status": "error", "error": msg}),
                    next_prompt: Some(String::new()),
                    should_exit: false,
                }
            }
        }
    }

    fn do_computer_open(&self, args: &Value) -> StepOutcome {
        let application = args.get("application").and_then(|v| v.as_str());
        let target = args.get("target").and_then(|v| v.as_str());
        let wait_timeout_ms = args.get("wait_timeout_ms").and_then(|v| v.as_u64());

        match crate::computer_use::computer_open(application, target, wait_timeout_ms) {
            Ok(result) => StepOutcome {
                data: result,
                next_prompt: Some(String::new()),
                should_exit: false,
            },
            Err(e) => {
                let msg = format!("{:#}", e);
                self.record_error(
                    "computer_open",
                    &msg,
                    ErrorSeverity::Tool,
                    json!({"application": application, "target": target}),
                );
                StepOutcome {
                    data: json!({"status": "error", "error": msg}),
                    next_prompt: Some(String::new()),
                    should_exit: false,
                }
            }
        }
    }

    fn do_computer_action(&self, args: &Value) -> StepOutcome {
        let action = args.get("action").and_then(|v| v.as_str()).unwrap_or("");
        let x = args.get("x").and_then(|v| v.as_u64());
        let y = args.get("y").and_then(|v| v.as_u64());
        let text = args.get("text").and_then(|v| v.as_str());
        let direction = args.get("direction").and_then(|v| v.as_str());
        let amount = args.get("amount").and_then(|v| v.as_u64());
        let duration = args.get("duration").and_then(|v| v.as_f64());

        match crate::computer_use::computer_action(action, x, y, text, direction, amount, duration) {
            Ok(result) => StepOutcome {
                data: result,
                next_prompt: Some(String::new()),
                should_exit: false,
            },
            Err(e) => {
                let msg = format!("{:#}", e);
                self.record_error(
                    "computer_action",
                    &msg,
                    ErrorSeverity::Tool,
                    json!({"action": action}),
                );
                StepOutcome {
                    data: json!({"status": "error", "error": msg}),
                    next_prompt: Some(String::new()),
                    should_exit: false,
                }
            }
        }
    }

    fn do_computer_zoom(&self, args: &Value) -> StepOutcome {
        let x0 = args.get("x0").and_then(|v| v.as_u64()).unwrap_or(0);
        let y0 = args.get("y0").and_then(|v| v.as_u64()).unwrap_or(0);
        let x1 = args.get("x1").and_then(|v| v.as_u64()).unwrap_or(0);
        let y1 = args.get("y1").and_then(|v| v.as_u64()).unwrap_or(0);
        match crate::computer_use::zoom(x0, y0, x1, y1) {
            Ok(result) => StepOutcome { data: result, next_prompt: Some(String::new()), should_exit: false },
            Err(e) => { let msg = format!("{:#}", e); self.record_error("computer_zoom", &msg, ErrorSeverity::Tool, json!({})); StepOutcome { data: json!({"status":"error","error":msg}), next_prompt: Some(String::new()), should_exit: false } }
        }
    }

    fn do_computer_left_click(&self, args: &Value) -> StepOutcome {
        let x = args.get("x").and_then(|v| v.as_u64()).unwrap_or(0);
        let y = args.get("y").and_then(|v| v.as_u64()).unwrap_or(0);
        match crate::computer_use::left_click(x, y) {
            Ok(result) => StepOutcome { data: result, next_prompt: Some(String::new()), should_exit: false },
            Err(e) => { let msg = format!("{:#}", e); self.record_error("computer_left_click", &msg, ErrorSeverity::Tool, json!({"x":x,"y":y})); StepOutcome { data: json!({"status":"error","error":msg}), next_prompt: Some(String::new()), should_exit: false } }
        }
    }

    fn do_computer_right_click(&self, args: &Value) -> StepOutcome {
        let x = args.get("x").and_then(|v| v.as_u64()).unwrap_or(0);
        let y = args.get("y").and_then(|v| v.as_u64()).unwrap_or(0);
        match crate::computer_use::right_click(x, y) {
            Ok(result) => StepOutcome { data: result, next_prompt: Some(String::new()), should_exit: false },
            Err(e) => { let msg = format!("{:#}", e); self.record_error("computer_right_click", &msg, ErrorSeverity::Tool, json!({"x":x,"y":y})); StepOutcome { data: json!({"status":"error","error":msg}), next_prompt: Some(String::new()), should_exit: false } }
        }
    }

    fn do_computer_middle_click(&self, args: &Value) -> StepOutcome {
        let x = args.get("x").and_then(|v| v.as_u64()).unwrap_or(0);
        let y = args.get("y").and_then(|v| v.as_u64()).unwrap_or(0);
        match crate::computer_use::middle_click(x, y) {
            Ok(result) => StepOutcome { data: result, next_prompt: Some(String::new()), should_exit: false },
            Err(e) => { let msg = format!("{:#}", e); self.record_error("computer_middle_click", &msg, ErrorSeverity::Tool, json!({"x":x,"y":y})); StepOutcome { data: json!({"status":"error","error":msg}), next_prompt: Some(String::new()), should_exit: false } }
        }
    }

    fn do_computer_double_click(&self, args: &Value) -> StepOutcome {
        let x = args.get("x").and_then(|v| v.as_u64()).unwrap_or(0);
        let y = args.get("y").and_then(|v| v.as_u64()).unwrap_or(0);
        match crate::computer_use::double_click(x, y) {
            Ok(result) => StepOutcome { data: result, next_prompt: Some(String::new()), should_exit: false },
            Err(e) => { let msg = format!("{:#}", e); self.record_error("computer_double_click", &msg, ErrorSeverity::Tool, json!({"x":x,"y":y})); StepOutcome { data: json!({"status":"error","error":msg}), next_prompt: Some(String::new()), should_exit: false } }
        }
    }

    fn do_computer_triple_click(&self, args: &Value) -> StepOutcome {
        let x = args.get("x").and_then(|v| v.as_u64()).unwrap_or(0);
        let y = args.get("y").and_then(|v| v.as_u64()).unwrap_or(0);
        match crate::computer_use::triple_click(x, y) {
            Ok(result) => StepOutcome { data: result, next_prompt: Some(String::new()), should_exit: false },
            Err(e) => { let msg = format!("{:#}", e); self.record_error("computer_triple_click", &msg, ErrorSeverity::Tool, json!({"x":x,"y":y})); StepOutcome { data: json!({"status":"error","error":msg}), next_prompt: Some(String::new()), should_exit: false } }
        }
    }

    fn do_computer_left_click_drag(&self, args: &Value) -> StepOutcome {
        let start_x = args.get("start_x").and_then(|v| v.as_u64()).unwrap_or(0);
        let start_y = args.get("start_y").and_then(|v| v.as_u64()).unwrap_or(0);
        let x = args.get("x").and_then(|v| v.as_u64()).unwrap_or(0);
        let y = args.get("y").and_then(|v| v.as_u64()).unwrap_or(0);
        match crate::computer_use::left_click_drag(start_x, start_y, x, y) {
            Ok(result) => StepOutcome { data: result, next_prompt: Some(String::new()), should_exit: false },
            Err(e) => { let msg = format!("{:#}", e); self.record_error("computer_left_click_drag", &msg, ErrorSeverity::Tool, json!({})); StepOutcome { data: json!({"status":"error","error":msg}), next_prompt: Some(String::new()), should_exit: false } }
        }
    }

    fn do_computer_mouse_move(&self, args: &Value) -> StepOutcome {
        let x = args.get("x").and_then(|v| v.as_u64()).unwrap_or(0);
        let y = args.get("y").and_then(|v| v.as_u64()).unwrap_or(0);
        match crate::computer_use::mouse_move(x, y) {
            Ok(result) => StepOutcome { data: result, next_prompt: Some(String::new()), should_exit: false },
            Err(e) => { let msg = format!("{:#}", e); self.record_error("computer_mouse_move", &msg, ErrorSeverity::Tool, json!({"x":x,"y":y})); StepOutcome { data: json!({"status":"error","error":msg}), next_prompt: Some(String::new()), should_exit: false } }
        }
    }

    fn do_computer_left_mouse_down(&self, args: &Value) -> StepOutcome {
        let x = args.get("x").and_then(|v| v.as_u64()).unwrap_or(0);
        let y = args.get("y").and_then(|v| v.as_u64()).unwrap_or(0);
        match crate::computer_use::left_mouse_down(x, y) {
            Ok(result) => StepOutcome { data: result, next_prompt: Some(String::new()), should_exit: false },
            Err(e) => { let msg = format!("{:#}", e); self.record_error("computer_left_mouse_down", &msg, ErrorSeverity::Tool, json!({"x":x,"y":y})); StepOutcome { data: json!({"status":"error","error":msg}), next_prompt: Some(String::new()), should_exit: false } }
        }
    }

    fn do_computer_left_mouse_up(&self, args: &Value) -> StepOutcome {
        let x = args.get("x").and_then(|v| v.as_u64()).unwrap_or(0);
        let y = args.get("y").and_then(|v| v.as_u64()).unwrap_or(0);
        match crate::computer_use::left_mouse_up(x, y) {
            Ok(result) => StepOutcome { data: result, next_prompt: Some(String::new()), should_exit: false },
            Err(e) => { let msg = format!("{:#}", e); self.record_error("computer_left_mouse_up", &msg, ErrorSeverity::Tool, json!({"x":x,"y":y})); StepOutcome { data: json!({"status":"error","error":msg}), next_prompt: Some(String::new()), should_exit: false } }
        }
    }

    fn do_computer_cursor_position(&self) -> StepOutcome {
        match crate::computer_use::cursor_position() {
            Ok(result) => StepOutcome { data: result, next_prompt: Some(String::new()), should_exit: false },
            Err(e) => { let msg = format!("{:#}", e); self.record_error("computer_cursor_position", &msg, ErrorSeverity::Tool, json!({})); StepOutcome { data: json!({"status":"error","error":msg}), next_prompt: Some(String::new()), should_exit: false } }
        }
    }

    fn do_computer_scroll(&self, args: &Value) -> StepOutcome {
        let x = args.get("x").and_then(|v| v.as_u64()).unwrap_or(0);
        let y = args.get("y").and_then(|v| v.as_u64()).unwrap_or(0);
        let direction = args.get("direction").and_then(|v| v.as_str()).unwrap_or("down");
        let amount = args.get("amount").and_then(|v| v.as_u64()).unwrap_or(3);
        match crate::computer_use::scroll(x, y, direction, amount) {
            Ok(result) => StepOutcome { data: result, next_prompt: Some(String::new()), should_exit: false },
            Err(e) => { let msg = format!("{:#}", e); self.record_error("computer_scroll", &msg, ErrorSeverity::Tool, json!({"x":x,"y":y,"direction":direction})); StepOutcome { data: json!({"status":"error","error":msg}), next_prompt: Some(String::new()), should_exit: false } }
        }
    }

    fn do_computer_type(&self, args: &Value) -> StepOutcome {
        let text = args.get("text").and_then(|v| v.as_str()).unwrap_or("");
        match crate::computer_use::type_text(text) {
            Ok(result) => StepOutcome { data: result, next_prompt: Some(String::new()), should_exit: false },
            Err(e) => { let msg = format!("{:#}", e); self.record_error("computer_type", &msg, ErrorSeverity::Tool, json!({"text_len": text.len()})); StepOutcome { data: json!({"status":"error","error":msg}), next_prompt: Some(String::new()), should_exit: false } }
        }
    }

    fn do_computer_key(&self, args: &Value) -> StepOutcome {
        let text = args.get("text").and_then(|v| v.as_str()).unwrap_or("");
        match crate::computer_use::key(text) {
            Ok(result) => StepOutcome { data: result, next_prompt: Some(String::new()), should_exit: false },
            Err(e) => { let msg = format!("{:#}", e); self.record_error("computer_key", &msg, ErrorSeverity::Tool, json!({"key": text})); StepOutcome { data: json!({"status":"error","error":msg}), next_prompt: Some(String::new()), should_exit: false } }
        }
    }

    fn do_computer_hold_key(&self, args: &Value) -> StepOutcome {
        let text = args.get("text").and_then(|v| v.as_str()).unwrap_or("");
        let duration = args.get("duration").and_then(|v| v.as_f64()).unwrap_or(1.0);
        match crate::computer_use::hold_key(text, duration) {
            Ok(result) => StepOutcome { data: result, next_prompt: Some(String::new()), should_exit: false },
            Err(e) => { let msg = format!("{:#}", e); self.record_error("computer_hold_key", &msg, ErrorSeverity::Tool, json!({"key": text})); StepOutcome { data: json!({"status":"error","error":msg}), next_prompt: Some(String::new()), should_exit: false } }
        }
    }

    fn do_computer_open_application(&self, args: &Value) -> StepOutcome {
        let application = args.get("application").and_then(|v| v.as_str()).unwrap_or("");
        let target = args.get("target").and_then(|v| v.as_str());
        match crate::computer_use::open_application(application, target) {
            Ok(result) => StepOutcome { data: result, next_prompt: Some(String::new()), should_exit: false },
            Err(e) => { let msg = format!("{:#}", e); self.record_error("computer_open_application", &msg, ErrorSeverity::Tool, json!({"application":application})); StepOutcome { data: json!({"status":"error","error":msg}), next_prompt: Some(String::new()), should_exit: false } }
        }
    }

    fn do_computer_switch_display(&self) -> StepOutcome {
        match crate::computer_use::switch_display() {
            Ok(result) => StepOutcome { data: result, next_prompt: Some(String::new()), should_exit: false },
            Err(e) => { let msg = format!("{:#}", e); self.record_error("computer_switch_display", &msg, ErrorSeverity::Tool, json!({})); StepOutcome { data: json!({"status":"error","error":msg}), next_prompt: Some(String::new()), should_exit: false } }
        }
    }

    fn do_computer_request_access(&self, args: &Value) -> StepOutcome {
        let applications: Vec<String> = args.get("applications")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
            .unwrap_or_default();
        match crate::computer_use::request_access(&applications) {
            Ok(result) => StepOutcome { data: result, next_prompt: Some(String::new()), should_exit: false },
            Err(e) => { let msg = format!("{:#}", e); self.record_error("computer_request_access", &msg, ErrorSeverity::Tool, json!({"applications":applications})); StepOutcome { data: json!({"status":"error","error":msg}), next_prompt: Some(String::new()), should_exit: false } }
        }
    }

    fn do_computer_list_granted_applications(&self) -> StepOutcome {
        match crate::computer_use::list_granted_applications() {
            Ok(result) => StepOutcome { data: result, next_prompt: Some(String::new()), should_exit: false },
            Err(e) => { let msg = format!("{:#}", e); self.record_error("computer_list_granted_applications", &msg, ErrorSeverity::Tool, json!({})); StepOutcome { data: json!({"status":"error","error":msg}), next_prompt: Some(String::new()), should_exit: false } }
        }
    }

    fn do_computer_read_clipboard(&self) -> StepOutcome {
        match crate::computer_use::read_clipboard() {
            Ok(result) => StepOutcome { data: result, next_prompt: Some(String::new()), should_exit: false },
            Err(e) => { let msg = format!("{:#}", e); self.record_error("computer_read_clipboard", &msg, ErrorSeverity::Tool, json!({})); StepOutcome { data: json!({"status":"error","error":msg}), next_prompt: Some(String::new()), should_exit: false } }
        }
    }

    fn do_computer_write_clipboard(&self, args: &Value) -> StepOutcome {
        let text = args.get("text").and_then(|v| v.as_str()).unwrap_or("");
        match crate::computer_use::write_clipboard(text) {
            Ok(result) => StepOutcome { data: result, next_prompt: Some(String::new()), should_exit: false },
            Err(e) => { let msg = format!("{:#}", e); self.record_error("computer_write_clipboard", &msg, ErrorSeverity::Tool, json!({"text_len":text.len()})); StepOutcome { data: json!({"status":"error","error":msg}), next_prompt: Some(String::new()), should_exit: false } }
        }
    }

    fn do_computer_wait(&self, args: &Value) -> StepOutcome {
        let duration = args.get("duration").and_then(|v| v.as_f64()).unwrap_or(1.0);
        match crate::computer_use::wait(duration) {
            Ok(result) => StepOutcome { data: result, next_prompt: Some(String::new()), should_exit: false },
            Err(e) => { let msg = format!("{:#}", e); self.record_error("computer_wait", &msg, ErrorSeverity::Tool, json!({})); StepOutcome { data: json!({"status":"error","error":msg}), next_prompt: Some(String::new()), should_exit: false } }
        }
    }

    fn do_computer_batch(&self, args: &Value) -> StepOutcome {
        match crate::computer_use::computer_batch(args) {
            Ok(result) => StepOutcome { data: result, next_prompt: Some(String::new()), should_exit: false },
            Err(e) => { let msg = format!("{:#}", e); self.record_error("computer_batch", &msg, ErrorSeverity::Tool, json!({})); StepOutcome { data: json!({"status":"error","error":msg}), next_prompt: Some(String::new()), should_exit: false } }
        }
    }

    // ── Plan mode helpers ────────────────────────────────────────────────

    pub fn enter_plan_mode(&mut self, plan_path: String) {
        self.working
            .write()
            .unwrap()
            .insert("in_plan_mode".into(), plan_path);
        *self.max_turns.write().unwrap() = 100;
    }

    pub fn _exit_plan_mode(&mut self) {
        self.working.write().unwrap().remove("in_plan_mode");
    }

    pub fn _in_plan_mode(&self) -> bool {
        self.working.read().unwrap().contains_key("in_plan_mode")
    }

    pub fn _get_anchor_prompt(&self, skip: bool, _history: &[String]) -> String {
        if skip {
            return "\n".to_string();
        }
        let hist = self.history_info.read().unwrap();
        let hist_start = if hist.len() > 40 { hist.len() - 40 } else { 0 };
        let h_str = hist[hist_start..].join("\n");
        let turn = *self.current_turn.read().unwrap();
        let mut prompt = format!(
            "\n### [WORKING MEMORY]\n<history>\n{}\n</history>\nCurrent turn: {}\n",
            h_str, turn
        );
        let w = self.working.read().unwrap();
        if let Some(ki) = w.get("key_info") {
            if !ki.is_empty() {
                prompt.push_str(&format!("\n<key_info>{}</key_info>", ki));
            }
        }
        if let Some(rs) = w.get("related_sop") {
            if !rs.is_empty() {
                prompt.push_str(&format!("\n有不清晰的地方请再次读取{}", rs));
            }
        }
        prompt
    }

    pub fn turn_end_callback(
        &self,
        _response: &LlmResponse,
        tool_calls: &[ToolCall],
        tool_results: &[Value],
        _turn: usize,
        next_prompt: &str,
    ) -> String {
        let summary = if let Some(tc) = tool_calls.first() {
            if tc.name == "no_tool" {
                "直接回答了用户问题".to_string()
            } else {
                format!(
                    "调用工具: {} args: {}",
                    tc.name,
                    crate::tools::smart_format(&tc.arguments, Some(100), None)
                )
            }
        } else if tool_calls.is_empty() {
            "direct response".to_string()
        } else {
            String::new()
        };
        self.history_info
            .write()
            .unwrap()
            .push(format!("[Agent] {}", summary));

        if let Some(run_tests_result) =
            tool_calls
                .iter()
                .zip(tool_results.iter())
                .find_map(|(call, result)| {
                    if call.name == "run_tests" {
                        Some(result)
                    } else {
                        None
                    }
                })
        {
            let status = run_tests_result
                .get("status")
                .and_then(|value| value.as_str())
                .unwrap_or("");
            if matches!(status, "failed" | "timeout") {
                let feedback = run_tests_result
                    .get("feedback")
                    .cloned()
                    .unwrap_or(Value::Null);
                let command = run_tests_result
                    .get("command")
                    .and_then(|value| value.as_str())
                    .unwrap_or("tests");
                return format!(
                    "{}\n[System] The latest `{}` run failed. Use the structured feedback below to fix the issue before running tests again.\n{}",
                    next_prompt,
                    command,
                    crate::tools::smart_format(&feedback, Some(4000), None)
                );
            }
        }

        next_prompt.to_string()
    }
}

// ── 4. agent_runner_loop ─────────────────────────────────────────────────────

pub async fn agent_runner_loop(
    client: Arc<TokioRwLock<dyn LlmClient>>,
    system_prompt: String,
    user_input: String,
    handler: Arc<RwLock<AgentHandler>>,
    tools_schema: Vec<ToolSchema>,
    max_turns: usize,
    verbose: bool,
    output_tx: mpsc::Sender<String>,
    stop_signal: Option<Arc<AtomicBool>>,
) -> Result<HashMap<String, Value>> {
    use crate::types::ContentBlock;

    // Build initial messages
    let mut messages: Vec<Message> = Vec::new();
    messages.push(Message {
        role: "system".to_string(),
        content: MessageContent::Text(system_prompt),
        tool_results: None,
    });
    messages.push(Message {
        role: "user".to_string(),
        content: MessageContent::Text(user_input.clone()),
        tool_results: None,
    });

    // Accumulate tool events for dream consolidation at session end.
    // Each entry: (tool_name, args, result)
    let mut all_tool_events: Vec<(String, Value, Value)> = Vec::new();

    for turn in 0..max_turns {
        // Check for stop signal before each turn
        if stop_signal
            .as_ref()
            .map_or(false, |s| s.load(Ordering::SeqCst))
        {
            let _ = output_tx
                .send("\n\n[ABORTED] Stopped by user.\n".to_string())
                .await;
            let mut exit = HashMap::new();
            exit.insert("reason".into(), Value::String("aborted".into()));
            exit.insert("message".into(), Value::String("Stopped by user".into()));
            exit.insert("final_output".into(), Value::String("[ABORTED]".into()));
            return Ok(exit);
        }
        *handler.write().unwrap().current_turn.write().unwrap() = turn;

        if verbose {
            let _ = output_tx
                .send(format!("\nLLM Running (Turn {}) ...\n", turn + 1))
                .await;
        }

        // Call the LLM — use tokio::sync::RwLock so guard is Send across .await
        let (mut stream_rx, response_handle) = match {
            let mut guard = client.write().await;
            guard.chat(messages.clone(), tools_schema.clone()).await
        } {
            Ok(v) => v,
            Err(e) => {
                let msg = format!("{:#}", e);
                handler.read().unwrap().record_error(
                    "llm_chat",
                    &msg,
                    ErrorSeverity::Critical,
                    serde_json::json!({"turn": turn, "model": handler.read().unwrap().model_name.read().unwrap().as_str()}),
                );
                return Err(e);
            }
        };

        // Stream text chunks to output — check stop signal continuously
        // so the user can abort mid-stream instead of waiting for the turn to finish.
        let stop_sig_for_stream = stop_signal.clone();
        let output_tx2 = output_tx.clone();
        tokio::pin!(response_handle);
        let mut aborted = false;
        loop {
            tokio::select! {
                biased;
                _ = tokio::time::sleep(Duration::from_millis(50)), if stop_sig_for_stream.as_ref().map_or(false, |s| s.load(Ordering::SeqCst)) => {
                    // User hit stop — abort the response handle and break out
                    response_handle.abort();
                    aborted = true;
                    break;
                }
                chunk = stream_rx.recv() => {
                    match chunk {
                        Some(chunk) => {
                            if stop_sig_for_stream.as_ref().map_or(false, |s| s.load(Ordering::SeqCst)) {
                                response_handle.abort();
                                aborted = true;
                                break;
                            }
                            let _ = output_tx2.send(chunk).await;
                        }
                        None => break, // stream exhausted — response_handle will produce the result
                    }
                }
            }
        }

        if aborted {
            let _ = output_tx
                .send("\n\n[ABORTED] Stopped by user.\n".to_string())
                .await;
            let mut exit = HashMap::new();
            exit.insert("reason".into(), Value::String("aborted".into()));
            exit.insert(
                "message".into(),
                Value::String("Stopped by user mid-stream".into()),
            );
            exit.insert("final_output".into(), Value::String("[ABORTED]".into()));
            return Ok(exit);
        }

        let response = match response_handle.await {
            Ok(Ok(v)) => v,
            Ok(Err(e)) => {
                let msg = format!("{:#}", e);
                handler.read().unwrap().record_error(
                    "llm_response",
                    &msg,
                    ErrorSeverity::Critical,
                    serde_json::json!({"turn": turn}),
                );
                return Err(e);
            }
            Err(join_err) => {
                // Distinguish aborted tasks from panics
                if join_err.is_cancelled() {
                    let mut exit = HashMap::new();
                    exit.insert("reason".into(), Value::String("aborted".into()));
                    exit.insert(
                        "message".into(),
                        Value::String("Stream cancelled by user".into()),
                    );
                    exit.insert("final_output".into(), Value::String("[ABORTED]".into()));
                    return Ok(exit);
                }
                let msg = format!("LLM response task panic: {join_err}");
                handler.read().unwrap().record_error(
                    "llm_task",
                    &msg,
                    ErrorSeverity::Critical,
                    serde_json::json!({"turn": turn}),
                );
                return Err(anyhow!("{msg}"));
            }
        };

        // Parse tool calls from the response
        let tool_calls = response.tool_calls.clone();
        let mut tool_results: Vec<Value> = Vec::new();
        let mut should_exit = false;

        if tool_calls.is_empty() {
            let outcome =
                handler
                    .read()
                    .unwrap()
                    .dispatch("no_tool", Value::Null, &response.content);
            tool_results.push(outcome.data);
            if outcome.should_exit || outcome.next_prompt.is_none() {
                should_exit = true;
            }
        } else {
            for tc in &tool_calls {
                let outcome = handler.read().unwrap().dispatch(
                    &tc.name,
                    tc.arguments.clone(),
                    &response.content,
                );
                all_tool_events.push((tc.name.clone(), tc.arguments.clone(), outcome.data.clone()));
                tool_results.push(outcome.data);
                if outcome.should_exit {
                    should_exit = true;
                    break;
                }
            }
        }

        // Turn-end callback
        let next_prompt = handler.read().unwrap().turn_end_callback(
            &response,
            &tool_calls,
            &tool_results,
            turn,
            "",
        );

        if should_exit {
            let interrupt_payload = tool_results.iter().find_map(|result| {
                if result.get("status").and_then(|value| value.as_str()) == Some("INTERRUPT") {
                    Some(result.clone())
                } else {
                    None
                }
            });
            let final_output = if let Some(interrupt) = interrupt_payload.as_ref() {
                interrupt
                    .get("message")
                    .and_then(|value| value.as_str())
                    .map(str::to_string)
                    .unwrap_or_else(|| {
                        if response.content.trim().is_empty() {
                            response.raw.clone()
                        } else {
                            response.content.clone()
                        }
                    })
            } else if response.content.trim().is_empty() {
                response.raw.clone()
            } else {
                response.content.clone()
            };

            let mut exit = HashMap::new();
            exit.insert("reason".into(), Value::String("should_exit".into()));
            exit.insert(
                "message".into(),
                Value::String(format!("exited at turn {}", turn)),
            );
            exit.insert("final_output".into(), Value::String(final_output.clone()));
            if let Some(interrupt) = interrupt_payload {
                exit.insert("interrupt".into(), interrupt);
            }

            // Fire-and-forget dream consolidation
            {
                let project_dir = {
                    let cwd = handler.read().unwrap().cwd.clone();
                    if cwd.ends_with("temp") {
                        cwd.parent().map(|p| p.to_path_buf()).unwrap_or(cwd)
                    } else {
                        cwd
                    }
                };
                let intent = user_input.chars().take(200).collect::<String>();
                let outcome = final_output.chars().take(300).collect::<String>();
                let events = all_tool_events.clone();
                let completed_turns = turn + 1;
                tokio::spawn(async move {
                    crate::dream::consolidate(&project_dir, &intent, &events, completed_turns, &outcome);
                });
            }

            return Ok(exit);
        }

        // Build assistant message
        let assistant_blocks: Vec<ContentBlock> = if !response.thinking.is_empty() {
            vec![
                ContentBlock::Thinking {
                    thinking: response.thinking.clone(),
                    signature: String::new(),
                },
                ContentBlock::Text {
                    text: response.content.clone(),
                },
            ]
        } else {
            vec![ContentBlock::Text {
                text: response.content.clone(),
            }]
        };

        messages.push(Message {
            role: "assistant".to_string(),
            content: MessageContent::Blocks(assistant_blocks),
            tool_results: None,
        });

        // Build user message with tool results
        let mut result_blocks: Vec<ContentBlock> = Vec::new();
        for (i, tc) in tool_calls.iter().enumerate() {
            let result_str = tool_results
                .get(i)
                .map(|v| crate::tools::smart_format(v, Some(8000), None))
                .unwrap_or_default();
            result_blocks.push(ContentBlock::ToolResult {
                tool_use_id: tc.id.clone(),
                content: result_str,
            });
        }
        result_blocks.push(ContentBlock::Text { text: next_prompt });

        messages.push(Message {
            role: "user".to_string(),
            content: MessageContent::Blocks(result_blocks),
            tool_results: None,
        });
    }

    let mut exit = HashMap::new();
    exit.insert("reason".into(), Value::String("max_turns_reached".into()));
    exit.insert(
        "message".into(),
        Value::String(format!("reached max turns ({})", max_turns)),
    );

    // Fire-and-forget dream consolidation (max turns path)
    {
        let project_dir = {
            let cwd = handler.read().unwrap().cwd.clone();
            if cwd.ends_with("temp") {
                cwd.parent().map(|p| p.to_path_buf()).unwrap_or(cwd)
            } else {
                cwd
            }
        };
        let intent = user_input.chars().take(200).collect::<String>();
        let events = all_tool_events;
        let completed_turns = max_turns;
        tokio::spawn(async move {
            crate::dream::consolidate(&project_dir, &intent, &events, completed_turns, "max_turns_reached");
        });
    }

    Ok(exit)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};
    use tokio::sync::mpsc;

    struct MockLlmClient {
        calls: Arc<AtomicUsize>,
        response: LlmResponse,
        chunk: String,
    }

    #[async_trait]
    impl LlmClient for MockLlmClient {
        fn name(&self) -> &str {
            "mock"
        }

        fn model(&self) -> &str {
            "mock-model"
        }

        fn clear_tools_cache(&mut self) {}

        fn set_tools(&mut self, _tools: Vec<ToolSchema>) {}

        fn set_system(&mut self, _system: &str) {}
        fn set_reasoning_effort(&mut self, _effort: Option<String>) {}

        async fn chat(
            &mut self,
            _messages: Vec<Message>,
            _tools: Vec<ToolSchema>,
        ) -> Result<(mpsc::Receiver<String>, JoinHandle<Result<LlmResponse>>)> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            let (tx, rx) = mpsc::channel(8);
            let chunk = self.chunk.clone();
            let response = self.response.clone();
            let handle = tokio::spawn(async move {
                let _ = tx.send(chunk).await;
                Ok(response)
            });
            Ok((rx, handle))
        }
    }

    #[tokio::test]
    async fn direct_response_exits_after_first_turn() {
        let calls = Arc::new(AtomicUsize::new(0));
        let client: Arc<TokioRwLock<dyn LlmClient>> = Arc::new(TokioRwLock::new(MockLlmClient {
            calls: calls.clone(),
            response: LlmResponse {
                thinking: String::new(),
                content: "Hello!".to_string(),
                tool_calls: Vec::new(),
                raw: "Hello!".to_string(),
                stop_reason: "end_turn".to_string(),
                usage: None,
            },
            chunk: "Hello!".to_string(),
        }));
        let handler = Arc::new(RwLock::new(AgentHandler::new(PathBuf::from("."))));
        let (output_tx, mut output_rx) = mpsc::channel(8);

        let exit = agent_runner_loop(
            client.clone(),
            String::new(),
            "hello".to_string(),
            handler,
            Vec::new(),
            5,
            false,
            output_tx,
            None,
        )
        .await
        .unwrap();

        let mut streamed = String::new();
        while let Some(chunk) = output_rx.recv().await {
            streamed.push_str(&chunk);
        }

        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert_eq!(streamed, "Hello!");
        assert_eq!(
            exit.get("final_output").and_then(|value| value.as_str()),
            Some("Hello!")
        );
    }

    #[tokio::test]
    async fn ask_user_interrupt_is_exposed_in_exit_payload() {
        let calls = Arc::new(AtomicUsize::new(0));
        let client: Arc<TokioRwLock<dyn LlmClient>> = Arc::new(TokioRwLock::new(MockLlmClient {
            calls: calls.clone(),
            response: LlmResponse {
                thinking: String::new(),
                content: String::new(),
                tool_calls: vec![ToolCall {
                    id: "tool-1".to_string(),
                    name: "ask_user".to_string(),
                    arguments: json!({
                        "question": "Pick a mode",
                        "candidates": ["Fast", "Safe"]
                    }),
                }],
                raw: String::new(),
                stop_reason: "tool_use".to_string(),
                usage: None,
            },
            chunk: String::new(),
        }));
        let handler = Arc::new(RwLock::new(AgentHandler::new(PathBuf::from("."))));
        let (output_tx, mut output_rx) = mpsc::channel(8);

        let exit = agent_runner_loop(
            client.clone(),
            String::new(),
            "need guidance".to_string(),
            handler,
            Vec::new(),
            5,
            false,
            output_tx,
            None,
        )
        .await
        .unwrap();

        while output_rx.recv().await.is_some() {}

        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert_eq!(
            exit.get("final_output").and_then(|value| value.as_str()),
            Some("ask_user requires user interaction")
        );
        let interrupt = exit.get("interrupt").cloned().unwrap_or(Value::Null);
        assert_eq!(
            interrupt.get("status").and_then(|value| value.as_str()),
            Some("INTERRUPT")
        );
        assert_eq!(
            interrupt.get("message").and_then(|value| value.as_str()),
            Some("ask_user requires user interaction")
        );
    }

    #[test]
    fn file_write_content_from_response_prefers_file_content_tag() {
        let response =
            "Before\n<file_content>hello\nworld\n</file_content>\nAfter\n```txt\nignored\n```";
        assert_eq!(
            file_write_content_from_response(response).as_deref(),
            Some("hello\nworld\n")
        );
    }

    #[test]
    fn file_write_content_from_response_falls_back_to_fenced_block() {
        let response = "Some text\n```markdown\n# Title\nbody\n```\nMore text";
        assert_eq!(
            file_write_content_from_response(response).as_deref(),
            Some("# Title\nbody\n")
        );
    }

    #[test]
    fn file_write_content_from_tool_block_reads_content_from_raw_tool_json() {
        let response = r##"<tool_use>{"name":"file_write","arguments":{"path":"README.md","content":"# Title\nbody\n"}}</tool_use>"##;
        assert_eq!(
            file_write_content_from_tool_block(response, "README.md").as_deref(),
            Some("# Title\nbody\n")
        );
    }

    #[test]
    fn file_write_content_from_tool_block_matches_requested_path() {
        let response = r##"<tool_use>{"name":"file_write","arguments":{"path":"OTHER.md","content":"ignored"}}</tool_use>
<tool_use>{"name":"file_write","arguments":{"path":"README.md","content":"kept"}}</tool_use>"##;
        assert_eq!(
            file_write_content_from_tool_block(response, "README.md").as_deref(),
            Some("kept")
        );
    }

    #[test]
    fn code_run_request_from_args_supports_command_alias_and_bash_default() {
        let (code, code_type) = code_run_request_from_args(&json!({
            "command": "echo hello",
            "timeout": 5
        }));
        assert_eq!(code, "echo hello");
        assert_eq!(code_type, "bash");
    }

    #[test]
    fn code_run_request_from_args_preserves_explicit_type() {
        let (code, code_type) = code_run_request_from_args(&json!({
            "command": "print('hello')",
            "type": "python"
        }));
        assert_eq!(code, "print('hello')");
        assert_eq!(code_type, "python");
    }

    #[test]
    fn canonicalize_tool_invocation_maps_file_list_to_workspace_list() {
        let (tool_name, args) = canonicalize_tool_invocation(
            "file_list",
            &json!({"path": "/tmp", "pattern": "**/*.png"}),
        );
        assert_eq!(tool_name, "workspace_list");
        assert_eq!(
            args.get("path").and_then(|value| value.as_str()),
            Some("/tmp")
        );
    }

    #[test]
    fn canonicalize_tool_invocation_maps_bash_to_code_run() {
        let (tool_name, args) = canonicalize_tool_invocation("bash", &json!({"command": "pwd"}));
        assert_eq!(tool_name, "code_run");
        assert_eq!(
            args.get("type").and_then(|value| value.as_str()),
            Some("bash")
        );
    }

    #[test]
    fn canonicalize_tool_invocation_maps_git_show_to_code_run() {
        let (tool_name, args) = canonicalize_tool_invocation(
            "git_show",
            &json!({"hash": "abc123", "path_repo": "/repo", "max_lines": 50}),
        );
        assert_eq!(tool_name, "code_run");
        assert_eq!(
            args.get("type").and_then(|value| value.as_str()),
            Some("bash")
        );
        let command = args
            .get("command")
            .and_then(|value| value.as_str())
            .unwrap_or("");
        assert!(command.contains("cd '/repo'"));
        assert!(command.contains("git --no-pager show 'abc123'"));
        assert!(command.contains("head -n 50"));
    }

    #[test]
    fn do_file_write_rejects_missing_content() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let test_root = std::env::temp_dir().join(format!("generic-coder-agent-test-{unique}"));
        fs::create_dir_all(&test_root).unwrap();

        let handler = AgentHandler::new(test_root.clone());
        let outcome =
            handler.do_file_write(&json!({ "path": "ANALYSIS.md" }), "No tagged body here");

        assert_eq!(
            outcome.data.get("error").and_then(|value| value.as_str()),
            Some("file_write requires content in args.content/text or in a <file_content>...</file_content> block")
        );
        assert!(!test_root.join("ANALYSIS.md").exists());

        let _ = fs::remove_dir_all(&test_root);
    }

    #[test]
    fn turn_end_callback_includes_test_feedback_for_failed_run_tests() {
        let handler = AgentHandler::new(PathBuf::from("."));
        let prompt = handler.turn_end_callback(
            &LlmResponse {
                thinking: String::new(),
                content: String::new(),
                tool_calls: Vec::new(),
                raw: String::new(),
                stop_reason: String::new(),
                usage: None,
            },
            &[ToolCall {
                id: "tool-1".to_string(),
                name: "run_tests".to_string(),
                arguments: json!({"command": "cargo test --quiet"}),
            }],
            &[json!({
                "status": "failed",
                "command": "cargo test --quiet",
                "feedback": {
                    "summary": "1 test failed",
                    "failed_tests": ["auth::tests::fails"]
                }
            })],
            0,
            "",
        );

        assert!(prompt.contains("cargo test --quiet"));
        assert!(prompt.contains("auth::tests::fails"));
        assert!(prompt.contains("failed"));
    }
}
