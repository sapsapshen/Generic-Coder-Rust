use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, RwLock,
};
use std::time::Duration;

use anyhow::{anyhow, Result};
use async_trait::async_trait;
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

// ── 1. LlmClient trait ───────────────────────────────────────────────────────

#[async_trait]
pub trait LlmClient: Send + Sync {
    fn name(&self) -> &str;
    fn model(&self) -> &str;
    fn clear_tools_cache(&mut self);
    fn set_tools(&mut self, tools: Vec<ToolSchema>);
    fn set_system(&mut self, system: &str);
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

    fn client(&self) -> Option<Arc<TokioRwLock<dyn LlmClient>>> {
        self.llm_clients.get(self.current_llm_no).cloned()
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
            *h.write().unwrap().workflow.write().unwrap() = self.agent_workflow.read().unwrap().clone();
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

            let client = match self.client() {
                Some(c) => c,
                None => {
                    let _ = reply_tx
                        .send(Value::String("[ERROR] No LLM configured".into()))
                        .await;
                    self.is_running.store(false, Ordering::SeqCst);
                    continue;
                }
            };

            let handler = Arc::new(RwLock::new(AgentHandler::new(PathBuf::from("temp"))));
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

        let client = match self.client() {
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

        let handler = Arc::new(RwLock::new(AgentHandler::new(PathBuf::from("temp"))));
        // Set model name for error attribution
        let model_name = client
            .try_read()
            .map(|g| g.model().to_string())
            .unwrap_or_default();
        handler.write().unwrap().model_name = RwLock::new(model_name);

        // Inherit mode and workflow from agent (set by web UI)
        *handler.write().unwrap().mode.write().unwrap() = *self.agent_mode.read().unwrap();
        *handler.write().unwrap().workflow.write().unwrap() = self.agent_workflow.read().unwrap().clone();
        self.handler = Some(handler.clone());

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
            let nodes = { handler.read().unwrap().workflow.read().unwrap().nodes.clone() };
            let current_node_idx = { handler.read().unwrap().workflow.read().unwrap().current_node };
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
                all_responses.push_str(&format!("\n## {} Mode Output\n\n{}\n", mode_emoji(mode), full_resp));

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
        let cwd = if cwd.is_absolute() {
            cwd
        } else {
            std::env::current_dir()
                .map(|base| base.join(&cwd))
                .unwrap_or(cwd)
        };
        let cwd = cwd.canonicalize().unwrap_or(cwd);
        // Derive project_dir from cwd: cwd is usually the temp task directory.
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
        if let Err(e) = self.error_memory.record(tool, message, severity, context, &model, turn) {
            log::warn!("Failed to record error to memory: {e}");
        }
    }

    // ── dispatch ─────────────────────────────────────────────────────────

    pub fn dispatch(&self, tool_name: &str, args: Value, response_content: &str) -> StepOutcome {
        match tool_name {
            "code_run" => self.do_code_run(&args),
            "file_read" => self.do_file_read(&args),
            "file_patch" => self.do_file_patch(&args),
            "file_write" => self.do_file_write(&args),
            "file_revert" => self.do_file_revert(&args),
            "web_scan" => self.do_web_scan(&args),
            "web_execute_js" => self.do_web_execute_js(&args),
            "ask_user" => self.do_ask_user(&args),
            "update_working_checkpoint" => self.do_update_working_checkpoint(&args),
            "no_tool" => self.do_no_tool(response_content),
            "start_long_term_update" => self.do_start_long_term_update(),
            "workspace_open" => self.do_workspace_open(&args),
            "workspace_list" => self.do_workspace_list(&args),
            "workspace_search" => self.do_workspace_search(&args),
            "content_search" => self.do_content_search(&args),
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
            "computer_open" => self.do_computer_open(&args),
            "computer_action" => self.do_computer_action(&args),
            _ => {
                self.record_error(
                    tool_name,
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
        let code = args
            .get("code")
            .or_else(|| args.get("script"))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let code_type = args
            .get("type")
            .and_then(|v| v.as_str())
            .unwrap_or("python");
        let timeout = args.get("timeout").and_then(|v| v.as_u64());
        let code_cwd = args.get("cwd").and_then(|v| v.as_str());
        let cwd = self.cwd_str();

        match crate::tools::code_run(
            code,
            code_type,
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
            },
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

    fn do_file_write(&self, args: &Value) -> StepOutcome {
        let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
        let content = args
            .get("content")
            .or_else(|| args.get("text"))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let mode = args
            .get("mode")
            .and_then(|v| v.as_str())
            .unwrap_or("overwrite");

        match crate::tools::file_write(path, content, Some(mode)) {
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

    // ── Computer Use ─────────────────────────────────────────────────────

    fn do_computer_screenshot(&self, args: &Value) -> StepOutcome {
        let region = args.get("region").and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(|v| v.as_u64()).collect::<Vec<_>>());
        let display = args.get("display").and_then(|v| v.as_u64());

        match crate::tools::computer_screenshot(
            region.as_deref(),
            display,
        ) {
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

    fn do_computer_action(&self, args: &Value) -> StepOutcome {
        let action = args.get("action").and_then(|v| v.as_str()).unwrap_or("");
        let x = args.get("x").and_then(|v| v.as_u64());
        let y = args.get("y").and_then(|v| v.as_u64());
        let text = args.get("text").and_then(|v| v.as_str());
        let direction = args.get("direction").and_then(|v| v.as_str());
        let amount = args.get("amount").and_then(|v| v.as_u64());
        let duration = args.get("duration").and_then(|v| v.as_f64());

        match crate::tools::computer_action(action, x, y, text, direction, amount, duration) {
            Ok(result) => StepOutcome {
                data: result,
                next_prompt: Some(String::new()),
                should_exit: false,
            },
            Err(e) => {
                let msg = format!("{:#}", e);
                self.record_error("computer_action", &msg, ErrorSeverity::Tool, json!({"action": action}));
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

        match crate::tools::computer_open(application, target, wait_timeout_ms) {
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
        _tool_results: &[Value],
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

    let aborted_exit = || {
        let mut exit = HashMap::new();
        exit.insert("reason".into(), Value::String("aborted".into()));
        exit.insert("message".into(), Value::String("Stopped by user".into()));
        exit.insert("final_output".into(), Value::String("[ABORTED]".into()));
        exit
    };

    // Build initial messages
    let mut messages: Vec<Message> = Vec::new();
    messages.push(Message {
        role: "system".to_string(),
        content: MessageContent::Text(system_prompt),
        tool_results: None,
    });
    messages.push(Message {
        role: "user".to_string(),
        content: MessageContent::Text(user_input),
        tool_results: None,
    });

    for turn in 0..max_turns {
        // Check for stop signal before each turn
        if stop_signal.as_ref().map_or(false, |s| s.load(Ordering::SeqCst)) {
            let _ = output_tx.send("\n\n[ABORTED] Stopped by user.\n".to_string()).await;
            return Ok(aborted_exit());
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

        // Stream text chunks to output
        loop {
            tokio::select! {
                maybe_chunk = stream_rx.recv() => {
                    match maybe_chunk {
                        Some(chunk) => {
                            if stop_signal.as_ref().map_or(false, |s| s.load(Ordering::SeqCst)) {
                                response_handle.abort();
                                let _ = output_tx.send("\n\n[ABORTED] Stopped by user.\n".to_string()).await;
                                return Ok(aborted_exit());
                            }
                            let _ = output_tx.send(chunk).await;
                        }
                        None => break,
                    }
                }
                _ = tokio::time::sleep(Duration::from_millis(50)), if stop_signal.is_some() => {
                    if stop_signal.as_ref().map_or(false, |s| s.load(Ordering::SeqCst)) {
                        response_handle.abort();
                        let _ = output_tx.send("\n\n[ABORTED] Stopped by user.\n".to_string()).await;
                        return Ok(aborted_exit());
                    }
                }
            }
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
            let mut exit = HashMap::new();
            exit.insert("reason".into(), Value::String("should_exit".into()));
            exit.insert(
                "message".into(),
                Value::String(format!("exited at turn {}", turn)),
            );
            exit.insert(
                "final_output".into(),
                Value::String(if response.content.trim().is_empty() {
                    response.raw.clone()
                } else {
                    response.content.clone()
                }),
            );
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
    Ok(exit)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio::sync::mpsc;

    struct MockLlmClient {
        calls: Arc<AtomicUsize>,
        response: LlmResponse,
        chunk: String,
    }

    struct SlowMockLlmClient {
        calls: Arc<AtomicUsize>,
        response: LlmResponse,
        chunks: Vec<(u64, String)>,
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

    #[async_trait]
    impl LlmClient for SlowMockLlmClient {
        fn name(&self) -> &str {
            "mock"
        }

        fn model(&self) -> &str {
            "mock-model"
        }

        fn clear_tools_cache(&mut self) {}

        fn set_tools(&mut self, _tools: Vec<ToolSchema>) {}

        fn set_system(&mut self, _system: &str) {}

        async fn chat(
            &mut self,
            _messages: Vec<Message>,
            _tools: Vec<ToolSchema>,
        ) -> Result<(mpsc::Receiver<String>, JoinHandle<Result<LlmResponse>>)> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            let (tx, rx) = mpsc::channel(8);
            let chunks = self.chunks.clone();
            let response = self.response.clone();
            let handle = tokio::spawn(async move {
                for (delay_ms, chunk) in chunks {
                    if delay_ms > 0 {
                        tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                    }
                    let _ = tx.send(chunk).await;
                }
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
    async fn stop_signal_interrupts_streaming_immediately() {
        let calls = Arc::new(AtomicUsize::new(0));
        let client: Arc<TokioRwLock<dyn LlmClient>> =
            Arc::new(TokioRwLock::new(SlowMockLlmClient {
                calls: calls.clone(),
                response: LlmResponse {
                    thinking: String::new(),
                    content: "Hello world".to_string(),
                    tool_calls: Vec::new(),
                    raw: "Hello world".to_string(),
                    stop_reason: "end_turn".to_string(),
                },
                chunks: vec![(0, "Hello".to_string()), (300, " world".to_string())],
            }));
        let handler = Arc::new(RwLock::new(AgentHandler::new(PathBuf::from("."))));
        let (output_tx, mut output_rx) = mpsc::channel(8);
        let stop_signal = Arc::new(AtomicBool::new(false));
        let stop_setter = stop_signal.clone();

        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(50)).await;
            stop_setter.store(true, Ordering::SeqCst);
        });

        let exit = agent_runner_loop(
            client,
            String::new(),
            "hello".to_string(),
            handler,
            Vec::new(),
            5,
            false,
            output_tx,
            Some(stop_signal),
        )
        .await
        .unwrap();

        let mut streamed = String::new();
        while let Some(chunk) = output_rx.recv().await {
            streamed.push_str(&chunk);
        }

        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert!(streamed.contains("Hello"));
        assert!(!streamed.contains(" world"));
        assert_eq!(exit.get("reason").and_then(|value| value.as_str()), Some("aborted"));
    }
}
