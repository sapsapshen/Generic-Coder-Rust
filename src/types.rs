use serde::{Deserialize, Serialize};

/// Outcome of a single agent step
#[derive(Debug, Clone)]
pub struct StepOutcome {
    pub data: serde_json::Value,
    pub next_prompt: Option<String>,
    pub should_exit: bool,
}

/// LLM response after a turn
#[derive(Debug, Clone)]
pub struct LlmResponse {
    pub thinking: String,
    pub content: String,
    pub tool_calls: Vec<ToolCall>,
    pub raw: String,
    pub stop_reason: String,
}

#[derive(Debug, Clone)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
}

/// Content block in Claude/OAI format
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ContentBlock {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "thinking")]
    Thinking { thinking: String, signature: String },
    #[serde(rename = "tool_use")]
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    #[serde(rename = "tool_result")]
    ToolResult {
        tool_use_id: String,
        content: String,
    },
    #[serde(rename = "image_url")]
    ImageUrl { image_url: ImageUrlDetail },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageUrlDetail {
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: String,
    #[serde(default)]
    pub content: MessageContent,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_results: Option<Vec<ToolResultMsg>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MessageContent {
    Text(String),
    Blocks(Vec<ContentBlock>),
}

impl Default for MessageContent {
    fn default() -> Self {
        MessageContent::Text(String::new())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResultMsg {
    pub tool_use_id: String,
    pub content: String,
}

/// LLM configuration from mykey.py/json
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmConfig {
    pub name: String,
    pub apikey: String,
    pub apibase: String,
    pub model: String,
    #[serde(default)]
    pub context_win: usize,
    #[serde(default)]
    pub proxy: Option<String>,
    #[serde(default)]
    pub verify: bool,
    #[serde(default = "default_max_retries")]
    pub max_retries: usize,
    #[serde(default)]
    pub stream: bool,
    #[serde(default)]
    pub timeout: u64,
    #[serde(default)]
    pub read_timeout: u64,
    #[serde(default)]
    pub temperature: f64,
    #[serde(default)]
    pub max_tokens: Option<usize>,
    #[serde(default)]
    pub reasoning_effort: Option<String>,
    #[serde(default)]
    pub service_tier: Option<String>,
    #[serde(default)]
    pub thinking_type: Option<String>,
    #[serde(default)]
    pub thinking_budget_tokens: Option<usize>,
    #[serde(default = "default_api_mode")]
    pub api_mode: String,
    #[serde(default)]
    pub extra_sys_prompt: String,
}

fn default_max_retries() -> usize {
    1
}
fn default_api_mode() -> String {
    "chat_completions".into()
}

/// Tool schema definition (OpenAI function-calling format)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSchema {
    #[serde(rename = "type")]
    pub tool_type: String,
    pub function: FunctionDef,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionDef {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

/// Workspace entry in file tree
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEntry {
    pub name: String,
    pub path: String,
    #[serde(rename = "type")]
    pub entry_type: String,
    #[serde(default)]
    pub size: u64,
    #[serde(default)]
    pub children: Vec<FileEntry>,
    #[serde(default)]
    pub truncated: bool,
    #[serde(default)]
    pub error: Option<String>,
}

/// Server config
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    pub name: String,
    pub host: String,
    #[serde(default)]
    pub port: u16,
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub key_path: String,
    #[serde(default)]
    pub jump_host: String,
    #[serde(default = "default_ssh_port")]
    pub jump_port: u16,
    #[serde(default)]
    pub jump_username: String,
}

fn default_ssh_port() -> u16 {
    22
}

/// Web UI frontend state
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FrontendState {
    pub workspace: WorkspaceState,
    pub remote: RemoteState,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WorkspaceState {
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteState {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub server_name: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub host: String,
    #[serde(default = "default_ssh_port")]
    pub port: u16,
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub password: String,
    #[serde(default)]
    pub key_path: String,
    #[serde(default)]
    pub cwd: String,
}

impl Default for RemoteState {
    fn default() -> Self {
        Self {
            enabled: false,
            server_name: String::new(),
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

/// Scheduler configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduleConfig {
    pub interval: u64,
    pub script_path: String,
}

impl LlmConfig {
    pub fn is_claude(&self) -> bool {
        self.name.to_lowercase().contains("claude")
    }
    pub fn is_native(&self) -> bool {
        self.name.to_lowercase().contains("native")
    }
    pub fn is_oai(&self) -> bool {
        self.name.to_lowercase().contains("oai")
    }

    pub fn session_type(&self) -> &str {
        if self.is_native() && self.is_claude() {
            "native_claude"
        } else if self.is_native() && self.is_oai() {
            "native_oai"
        } else if self.is_claude() {
            "claude"
        } else {
            "oai"
        }
    }
}

impl ContentBlock {
    pub fn text(text: impl Into<String>) -> Self {
        ContentBlock::Text { text: text.into() }
    }

    pub fn is_text(&self) -> bool {
        matches!(self, ContentBlock::Text { .. })
    }

    pub fn as_text(&self) -> Option<&str> {
        match self {
            ContentBlock::Text { text } => Some(text),
            _ => None,
        }
    }
}
