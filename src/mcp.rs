use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

const MCP_PROTOCOL_VERSION: &str = "2024-11-05";
const MAX_SKIPPED_RESPONSES: usize = 64;

lazy_static::lazy_static! {
    static ref MCP_SESSIONS: Mutex<HashMap<String, McpSession>> = Mutex::new(HashMap::new());
    static ref NEXT_REQUEST_ID: AtomicU64 = AtomicU64::new(1);
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct McpServerConfig {
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: HashMap<String, String>,
    #[serde(default)]
    pub cwd: Option<String>,
    #[serde(default)]
    pub disabled: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct McpConfigFile {
    #[serde(default)]
    pub servers: HashMap<String, McpServerConfig>,
}

struct McpSession {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    initialized: bool,
    config: McpServerConfig,
}

impl Drop for McpSession {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn workspace_root() -> Result<PathBuf> {
    crate::workspace::effective_root()
        .or_else(|| std::env::current_dir().ok())
        .map(|path| fs::canonicalize(&path).unwrap_or(path))
        .ok_or_else(|| anyhow!("Cannot resolve workspace root"))
}

fn config_candidates() -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Ok(root) = workspace_root() {
        candidates.push(root.join("mcp_servers.json"));
        candidates.push(root.join("assets").join("mcp_servers.json"));
    }
    if let Some(config_dir) = dirs::config_dir() {
        candidates.push(config_dir.join("generic-coder").join("mcp_servers.json"));
    }
    candidates
}

fn load_config_file(path: &Path) -> Result<McpConfigFile> {
    let raw = fs::read_to_string(path)
        .with_context(|| format!("Failed to read MCP config: {}", path.display()))?;
    serde_json::from_str(&raw)
        .with_context(|| format!("Failed to parse MCP config JSON: {}", path.display()))
}

pub fn load_server_configs() -> Result<HashMap<String, McpServerConfig>> {
    let mut merged = HashMap::new();
    let mut seen = false;
    for candidate in config_candidates() {
        if !candidate.exists() {
            continue;
        }
        seen = true;
        let config = load_config_file(&candidate)?;
        for (name, server) in config.servers {
            merged.insert(name, server);
        }
    }
    if !seen {
        return Ok(HashMap::new());
    }
    Ok(merged)
}

fn next_request_id() -> u64 {
    NEXT_REQUEST_ID.fetch_add(1, Ordering::SeqCst)
}

fn spawn_session(config: &McpServerConfig) -> Result<McpSession> {
    if config.command.trim().is_empty() {
        return Err(anyhow!("MCP server command cannot be empty"));
    }

    let mut command = Command::new(&config.command);
    command.args(&config.args);
    if let Some(cwd) = &config.cwd {
        command.current_dir(cwd);
    }
    for (key, value) in &config.env {
        command.env(key, value);
    }
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());

    let mut child = command
        .spawn()
        .with_context(|| format!("Failed to spawn MCP server: {}", config.command))?;
    let stdin = child
        .stdin
        .take()
        .ok_or_else(|| anyhow!("MCP server stdin unavailable"))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| anyhow!("MCP server stdout unavailable"))?;

    Ok(McpSession {
        child,
        stdin,
        stdout: BufReader::new(stdout),
        initialized: false,
        config: config.clone(),
    })
}

fn write_message(stdin: &mut ChildStdin, payload: &Value) -> Result<()> {
    let body = serde_json::to_vec(payload)?;
    let header = format!("Content-Length: {}\r\n\r\n", body.len());
    stdin.write_all(header.as_bytes())?;
    stdin.write_all(&body)?;
    stdin.flush()?;
    Ok(())
}

fn read_message(stdout: &mut BufReader<ChildStdout>) -> Result<Value> {
    let mut content_length = None;
    loop {
        let mut header = String::new();
        let read = stdout.read_line(&mut header)?;
        if read == 0 {
            return Err(anyhow!("MCP server closed the connection"));
        }
        if header == "\r\n" {
            break;
        }
        if let Some(value) = header.strip_prefix("Content-Length:") {
            content_length = value.trim().parse::<usize>().ok();
        }
    }

    let content_length =
        content_length.ok_or_else(|| anyhow!("MCP server response missing Content-Length"))?;
    let mut body = vec![0u8; content_length];
    stdout.read_exact(&mut body)?;
    serde_json::from_slice(&body).context("Failed to decode MCP JSON response")
}

fn request(session: &mut McpSession, method: &str, params: Value) -> Result<Value> {
    if session.child.try_wait()?.is_some() {
        return Err(anyhow!(
            "MCP server process exited: {}",
            session.config.command
        ));
    }

    let id = next_request_id();
    let payload = json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": method,
        "params": params,
    });
    write_message(&mut session.stdin, &payload)?;

    let mut skipped = 0usize;
    loop {
        let response = read_message(&mut session.stdout)?;
        if response.get("id").and_then(|value| value.as_u64()) != Some(id) {
            skipped += 1;
            if skipped >= MAX_SKIPPED_RESPONSES {
                return Err(anyhow!(
                    "MCP {} failed: exceeded {} unrelated responses while waiting for request {}",
                    method,
                    MAX_SKIPPED_RESPONSES,
                    id
                ));
            }
            continue;
        }
        if let Some(error) = response.get("error") {
            return Err(anyhow!("MCP {} failed: {}", method, error));
        }
        return Ok(response.get("result").cloned().unwrap_or(Value::Null));
    }
}

fn ensure_initialized(session: &mut McpSession) -> Result<()> {
    if session.initialized {
        return Ok(());
    }
    request(
        session,
        "initialize",
        json!({
            "protocolVersion": MCP_PROTOCOL_VERSION,
            "capabilities": {},
            "clientInfo": {
                "name": "generic-coder-rust",
                "version": env!("CARGO_PKG_VERSION"),
            }
        }),
    )?;
    let notification = json!({
        "jsonrpc": "2.0",
        "method": "notifications/initialized",
        "params": {}
    });
    write_message(&mut session.stdin, &notification)?;
    session.initialized = true;
    Ok(())
}

fn with_session<T, F>(server: &str, operation: F) -> Result<T>
where
    F: FnOnce(&mut McpSession) -> Result<T>,
{
    let configs = load_server_configs()?;
    let config = configs
        .get(server)
        .cloned()
        .ok_or_else(|| anyhow!("Unknown MCP server: {server}"))?;
    if config.disabled {
        return Err(anyhow!("MCP server is disabled: {server}"));
    }

    let mut sessions = MCP_SESSIONS.lock().unwrap();
    if !sessions.contains_key(server) {
        let session = spawn_session(&config)?;
        sessions.insert(server.to_string(), session);
    }
    let session = sessions
        .get_mut(server)
        .ok_or_else(|| anyhow!("Failed to open MCP session: {server}"))?;
    ensure_initialized(session)?;
    operation(session)
}

pub fn list_servers() -> Result<Value> {
    let servers = load_server_configs()?;
    let entries = servers
        .into_iter()
        .map(|(name, config)| {
            json!({
                "name": name,
                "command": config.command,
                "args": config.args,
                "cwd": config.cwd,
                "disabled": config.disabled,
            })
        })
        .collect::<Vec<_>>();

    Ok(json!({
        "status": "ok",
        "count": entries.len(),
        "servers": entries,
    }))
}

pub fn list_tools(server: Option<&str>) -> Result<Value> {
    let configs = load_server_configs()?;
    let servers = if let Some(name) = server {
        vec![name.to_string()]
    } else {
        configs
            .iter()
            .filter(|(_, config)| !config.disabled)
            .map(|(name, _)| name.clone())
            .collect::<Vec<_>>()
    };

    let mut results = Vec::new();
    for name in servers {
        let result = with_session(&name, |session| request(session, "tools/list", json!({})))?;
        results.push(json!({
            "server": name,
            "tools": result.get("tools").cloned().unwrap_or(Value::Array(Vec::new())),
        }));
    }

    Ok(json!({
        "status": "ok",
        "count": results.len(),
        "results": results,
    }))
}

pub fn call_tool(server: &str, tool: &str, arguments: Value) -> Result<Value> {
    if tool.trim().is_empty() {
        return Err(anyhow!("mcp_call_tool requires a tool name"));
    }

    let result = with_session(server, |session| {
        request(
            session,
            "tools/call",
            json!({
                "name": tool,
                "arguments": arguments,
            }),
        )
    })?;

    Ok(json!({
        "status": "ok",
        "server": server,
        "tool": tool,
        "result": result,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_config_is_supported() {
        let parsed: McpConfigFile = serde_json::from_str("{}").unwrap();
        assert!(parsed.servers.is_empty());
    }

    #[test]
    fn parses_server_configs() {
        let parsed: McpConfigFile = serde_json::from_str(
            r#"{
                "servers": {
                    "docs": {
                        "command": "node",
                        "args": ["server.js"],
                        "env": {"TOKEN": "demo"}
                    }
                }
            }"#,
        )
        .unwrap();

        let server = parsed.servers.get("docs").unwrap();
        assert_eq!(server.command, "node");
        assert_eq!(server.args, vec!["server.js"]);
        assert_eq!(server.env.get("TOKEN").map(String::as_str), Some("demo"));
    }
}
