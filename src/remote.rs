//! SSH remote server manager for Generic Coder.
//! Mirrors remoteserver.py — ssh2 crate primary, OpenSSH CLI fallback.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Arc};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, Context, Result};
use parking_lot::Mutex;
use serde_json::{json, Value};

use crate::types::ServerConfig;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

pub type JsonResult = Result<Value>;

#[derive(Debug, Clone)]
pub enum StreamEvent {
    Line(String),
    Done(Value),
}

#[derive(Debug, Clone)]
struct ConnectionConfig {
    host: String,
    port: u16,
    username: String,
    key_path: String,
    #[allow(dead_code)]
    jump_host: String,
    #[allow(dead_code)]
    jump_port: u16,
    #[allow(dead_code)]
    jump_username: String,
}

impl From<&ServerConfig> for ConnectionConfig {
    fn from(c: &ServerConfig) -> Self {
        Self {
            host: c.host.clone(),
            port: if c.port == 0 { 22 } else { c.port },
            username: if c.username.is_empty() {
                "root".into()
            } else {
                c.username.clone()
            },
            key_path: c.key_path.clone(),
            jump_host: c.jump_host.clone(),
            jump_port: if c.jump_port == 0 { 22 } else { c.jump_port },
            jump_username: c.jump_username.clone(),
        }
    }
}

// ---------------------------------------------------------------------------
// Shell helpers (fallback path)
// ---------------------------------------------------------------------------

fn shell_quote(s: &str) -> String {
    let escaped = s.replace('\'', "'\\''");
    format!("'{}'", escaped)
}

fn run_ssh_command(cfg: &ConnectionConfig, remote_cmd: &str) -> Result<std::process::Output> {
    let mut cmd = std::process::Command::new("ssh");
    cmd.args([
        "-o",
        "StrictHostKeyChecking=accept-new",
        "-o",
        "ConnectTimeout=10",
        "-o",
        "ServerAliveInterval=30",
        "-o",
        "BatchMode=yes",
    ]);
    if cfg.port != 22 {
        cmd.args(["-p", &cfg.port.to_string()]);
    }
    if !cfg.key_path.is_empty() {
        cmd.args(["-i", &cfg.key_path]);
    }
    cmd.arg(format!("{}@{}", cfg.username, cfg.host));
    cmd.arg(remote_cmd);

    cmd.stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .output()
        .with_context(|| format!("Failed to execute ssh for {}", cfg.host))
}

fn run_ssh_command_with_timeout(
    cfg: &ConnectionConfig,
    remote_cmd: &str,
    timeout_secs: u64,
) -> Result<std::process::Output> {
    let mut cmd = std::process::Command::new("ssh");
    cmd.args([
        "-o",
        "StrictHostKeyChecking=accept-new",
        "-o",
        "ConnectTimeout=10",
        "-o",
        "ServerAliveInterval=30",
        "-o",
        "BatchMode=yes",
    ]);
    if cfg.port != 22 {
        cmd.args(["-p", &cfg.port.to_string()]);
    }
    if !cfg.key_path.is_empty() {
        cmd.args(["-i", &cfg.key_path]);
    }
    cmd.arg(format!("{}@{}", cfg.username, cfg.host));
    cmd.arg(remote_cmd);
    cmd.stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    let mut child = cmd
        .spawn()
        .with_context(|| format!("Failed to execute ssh for {}", cfg.host))?;
    let timeout = Duration::from_secs(timeout_secs.max(1));
    let start = std::time::Instant::now();

    loop {
        if child.try_wait()?.is_some() {
            return child
                .wait_with_output()
                .with_context(|| format!("Failed to collect ssh output for {}", cfg.host));
        }
        if start.elapsed() >= timeout {
            let _ = child.kill();
            let output = child.wait_with_output().with_context(|| {
                format!("Failed to collect timed-out ssh output for {}", cfg.host)
            })?;
            return Err(anyhow!(
                "ssh command timed out after {}s: {}",
                timeout_secs.max(1),
                String::from_utf8_lossy(&output.stderr).trim()
            ));
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

fn scp_transfer(cfg: &ConnectionConfig, src: &str, dst: &str) -> Result<()> {
    let mut cmd = std::process::Command::new("scp");
    cmd.args([
        "-o",
        "StrictHostKeyChecking=accept-new",
        "-o",
        "ConnectTimeout=30",
        "-o",
        "BatchMode=yes",
        "-r",
    ]);
    if cfg.port != 22 {
        cmd.args(["-P", &cfg.port.to_string()]);
    }
    if !cfg.key_path.is_empty() {
        cmd.args(["-i", &cfg.key_path]);
    }
    cmd.arg(src);
    cmd.arg(dst);

    let output = cmd
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .output()
        .with_context(|| format!("scp transfer failed for {} -> {}", src, dst))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("scp error: {}", stderr.trim()));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// RemoteServerConnection
// ---------------------------------------------------------------------------

struct InnerState {
    session: Option<ssh2::Session>,
    sftp: Option<ssh2::Sftp>,
    config: Option<ConnectionConfig>,
    connected: bool,
}

// SAFETY: InnerState is only ever accessed through parking_lot::Mutex,
// which provides exclusive access across threads. ssh2::Session and
// ssh2::Sftp are !Send but guarded by the outer lock.
unsafe impl Send for InnerState {}

pub struct RemoteServerConnection {
    state: Mutex<InnerState>,
}

impl RemoteServerConnection {
    pub fn new() -> Self {
        Self {
            state: Mutex::new(InnerState {
                session: None,
                sftp: None,
                config: None,
                connected: false,
            }),
        }
    }

    // -- connect -----------------------------------------------------------

    pub fn connect(
        &self,
        host: &str,
        port: u16,
        username: &str,
        password: &str,
        key_path: &str,
        jump_host: &str,
        jump_port: u16,
        jump_username: &str,
        jump_password: &str,
    ) -> JsonResult {
        let mut s = self.state.lock();
        if s.connected {
            drop(s);
            self.disconnect();
            s = self.state.lock();
        }

        let cfg = ConnectionConfig {
            host: host.to_string(),
            port: if port == 0 { 22 } else { port },
            username: username.to_string(),
            key_path: key_path.to_string(),
            jump_host: jump_host.to_string(),
            jump_port: if jump_port == 0 { 22 } else { jump_port },
            jump_username: jump_username.to_string(),
        };

        // Attempt ssh2 first; fall back to shell ssh on failure.
        match Self::connect_via_ssh2_inner(
            &mut s,
            &cfg,
            password,
            key_path,
            jump_host,
            jump_port,
            jump_username,
            jump_password,
        ) {
            Ok(v) => {
                s.config = Some(cfg);
                s.connected = true;
                return Ok(v);
            }
            Err(_e) => {
                // ssh2 failed — try shell fallback
            }
        }

        // --- shell fallback ---
        if !password.is_empty() && key_path.is_empty() {
            return Ok(json!({
                "status": "error",
                "msg": "Password auth in OpenSSH fallback mode is unsupported. Use key-based auth."
            }));
        }

        let test_output = run_ssh_command(&cfg, "echo OK")?;
        let stdout = String::from_utf8_lossy(&test_output.stdout);
        if test_output.status.success() && stdout.contains("OK") {
            s.config = Some(cfg);
            s.connected = true;
            return Ok(json!({"status": "connected", "host": host, "method": "openssh_cli"}));
        }
        let stderr = String::from_utf8_lossy(&test_output.stderr);
        Ok(json!({"status": "error", "msg": format!("SSH connection failed: {}", stderr.trim())}))
    }

    fn connect_via_ssh2_inner(
        s: &mut parking_lot::MutexGuard<'_, InnerState>,
        cfg: &ConnectionConfig,
        password: &str,
        key_path: &str,
        jump_host: &str,
        jump_port: u16,
        jump_username: &str,
        jump_password: &str,
    ) -> JsonResult {
        let mut sess =
            ssh2::Session::new().map_err(|e| anyhow!("ssh2 session create failed: {e}"))?;

        // TCP connection (direct or via jump host)
        let tcp = if !jump_host.is_empty() {
            let mut jump_sess = ssh2::Session::new()
                .map_err(|e| anyhow!("ssh2 jump session create failed: {e}"))?;
            let jump_addr = format!("{jump_host}:{jump_port}");
            let jump_tcp = std::net::TcpStream::connect(&jump_addr)
                .with_context(|| format!("Failed to connect to jump host {jump_addr}"))?;
            jump_tcp
                .set_read_timeout(Some(Duration::from_secs(30)))
                .ok();
            jump_sess.set_tcp_stream(jump_tcp);
            jump_sess
                .handshake()
                .context("Jump host handshake failed")?;

            if !jump_password.is_empty() {
                jump_sess
                    .userauth_password(jump_username, jump_password)
                    .with_context(|| format!("Jump host auth failed for {jump_username}"))?;
            } else {
                jump_sess
                    .userauth_agent(jump_username)
                    .or_else(|_| {
                        jump_sess.userauth_pubkey_file(
                            jump_username,
                            None,
                            Path::new(key_path),
                            None,
                        )
                    })
                    .with_context(|| {
                        format!(
                            "Jump host auth failed — no password, agent, or key for {jump_username}"
                        )
                    })?;
            }

            let _ch = jump_sess
                .channel_direct_tcpip(&cfg.host, cfg.port, None)
                .with_context(|| {
                    format!("Jump direct-tcpip to {}:{} failed", cfg.host, cfg.port)
                })?;
            // ssh2::Channel does not implement AsRawFd; jump-host tunneling via
            // the native ssh2 backend is therefore unsupported.  Return an error
            // so the caller can fall back to the system `ssh` binary, which has
            // full ProxyJump support.
            #[cfg(unix)]
            {
                return Err(anyhow!(
                    "Jump host tunneling requires the system ssh binary (ssh2 library limitation)"
                ));
            }
            #[cfg(not(unix))]
            {
                return Err(anyhow!("Jump host via ssh2 not supported on this platform"));
            }
        } else {
            let addr = format!("{}:{}", cfg.host, cfg.port);
            std::net::TcpStream::connect(&addr)
                .with_context(|| format!("Failed to connect to {addr}"))?
        };

        tcp.set_read_timeout(Some(Duration::from_secs(30))).ok();
        sess.set_tcp_stream(tcp);
        sess.handshake().context("SSH handshake failed")?;

        // Authenticate
        if !password.is_empty() {
            sess.userauth_password(&cfg.username, password)
                .with_context(|| format!("Password auth failed for {}", cfg.username))?;
        } else if !key_path.is_empty() {
            sess.userauth_pubkey_file(&cfg.username, None, Path::new(key_path), None)
                .with_context(|| format!("Key auth failed for {}", cfg.username))?;
        } else {
            sess.userauth_agent(&cfg.username)
                .or_else(|_| {
                    sess.userauth_pubkey_file(&cfg.username, None, Path::new(key_path), None)
                })
                .with_context(|| {
                    format!(
                        "Auth failed — no password, key, or agent for {}",
                        cfg.username
                    )
                })?;
        }

        let sftp = sess.sftp().context("SFTP session open failed")?;
        s.session = Some(sess);
        s.sftp = Some(sftp);

        Ok(json!({"status": "connected", "host": cfg.host, "method": "ssh2"}))
    }

    // -- disconnect --------------------------------------------------------

    pub fn disconnect(&self) {
        let mut s = self.state.lock();
        s.sftp = None;
        s.session = None;
        s.config = None;
        s.connected = false;
    }

    // -- is_connected ------------------------------------------------------

    pub fn is_connected(&self) -> bool {
        let s = self.state.lock();
        if !s.connected {
            return false;
        }
        if let Some(ref session) = s.session {
            return session.authenticated();
        }
        s.connected
    }

    // -- exec_command (streaming) ------------------------------------------

    /// Spawns a blocking thread and returns a channel receiver.
    /// Each output line is delivered as `StreamEvent::Line`.
    /// The final status dict is delivered as `StreamEvent::Done`.
    pub fn exec_command(
        &self,
        command: &str,
        timeout_secs: u64,
        cwd: &str,
    ) -> mpsc::Receiver<StreamEvent> {
        let (tx, rx) = mpsc::channel();

        if !self.is_connected() {
            let _ = tx.send(StreamEvent::Line(
                "[Error] Not connected to any server\n".into(),
            ));
            let _ = tx.send(StreamEvent::Done(
                json!({"status": "error", "msg": "Not connected"}),
            ));
            return rx;
        }

        let full_cmd = if cwd.is_empty() {
            command.to_string()
        } else {
            format!("cd {} && {}", shell_quote(cwd), command)
        };

        let has_session = { self.state.lock().session.is_some() };
        let cfg = { self.state.lock().config.clone() };
        let use_shell_fallback = !has_session;
        let command_owned = command.to_string();

        std::thread::spawn(move || {
            let _ = tx.send(StreamEvent::Line(format!(
                "[Remote] Executing: {command_owned}\n"
            )));

            if use_shell_fallback {
                if let Some(cfg) = cfg {
                    Self::exec_via_shell(&tx, &cfg, &full_cmd, timeout_secs);
                } else {
                    let _ = tx.send(StreamEvent::Done(
                        json!({"status": "error", "msg": "No connection config"}),
                    ));
                }
                return;
            }

            // ssh2 streaming not available from a detached thread —
            // caller should use exec_command_sync for ssh2 path.
            let _ = tx.send(StreamEvent::Done(
                json!({"status": "error", "msg": "ssh2 streaming: use exec_command_sync"}),
            ));
        });

        rx
    }

    fn exec_via_shell(
        tx: &mpsc::Sender<StreamEvent>,
        cfg: &ConnectionConfig,
        command: &str,
        _timeout_secs: u64,
    ) {
        let mut cmd = std::process::Command::new("ssh");
        cmd.args([
            "-o",
            "StrictHostKeyChecking=accept-new",
            "-o",
            "ConnectTimeout=10",
            "-o",
            "ServerAliveInterval=30",
        ]);
        if cfg.port != 22 {
            cmd.args(["-p", &cfg.port.to_string()]);
        }
        if !cfg.key_path.is_empty() {
            cmd.args(["-i", &cfg.key_path]);
        }
        cmd.arg(format!("{}@{}", cfg.username, cfg.host));
        cmd.arg(command);

        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());

        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => {
                let _ = tx.send(StreamEvent::Done(
                    json!({"status": "error", "msg": format!("spawn failed: {e}")}),
                ));
                return;
            }
        };

        let stdout = child.stdout.take().unwrap();
        let reader = BufReader::new(stdout);
        let mut output_lines = Vec::new();

        for line in reader.lines() {
            match line {
                Ok(l) => {
                    output_lines.push(l.clone());
                    let _ = tx.send(StreamEvent::Line(format!("{l}\n")));
                }
                Err(_) => break,
            }
        }

        match child.wait() {
            Ok(status) => {
                let exit_code = status.code().unwrap_or(-1);
                let _ = tx.send(StreamEvent::Line(format!(
                    "\n[Remote] Exit Code: {exit_code}\n"
                )));
                let _ = tx.send(StreamEvent::Done(json!({
                    "status": if exit_code == 0 { "success" } else { "error" },
                    "stdout": output_lines.join("\n"),
                    "exit_code": exit_code,
                })));
            }
            Err(e) => {
                let _ = tx.send(StreamEvent::Done(
                    json!({"status": "error", "msg": format!("wait failed: {e}")}),
                ));
            }
        }
    }

    // -- exec_command_sync -------------------------------------------------

    pub fn exec_command_sync(&self, command: &str, timeout_secs: u64, cwd: &str) -> JsonResult {
        if !self.is_connected() {
            return Ok(json!({"status": "error", "msg": "Not connected"}));
        }

        let full_cmd = if cwd.is_empty() {
            command.to_string()
        } else {
            format!("cd {} && {}", shell_quote(cwd), command)
        };

        let st = self.state.lock();
        let config = st.config.clone();

        // ssh2 path
        if let Some(ref session) = st.session {
            session.set_timeout((timeout_secs.max(1) * 1000) as u32);
            if let Ok(mut ch) = session.channel_session() {
                if let Err(err) = ch.exec(&full_cmd) {
                    return Ok(json!({"status": "error", "msg": format!("exec failed: {err}")}));
                }

                let mut stdout = String::new();
                if let Err(err) = ch.read_to_string(&mut stdout) {
                    return Ok(
                        json!({"status": "error", "msg": format!("read stdout failed or timed out: {err}")}),
                    );
                }

                let mut stderr_buf = String::new();
                let mut stderr_stream = ch.stderr();
                if let Err(err) = stderr_stream.read_to_string(&mut stderr_buf) {
                    return Ok(
                        json!({"status": "error", "msg": format!("read stderr failed or timed out: {err}")}),
                    );
                }

                let exit_code = ch.exit_status().unwrap_or(-1);

                return Ok(json!({
                    "status": if exit_code == 0 { "success" } else { "error" },
                    "stdout": stdout,
                    "stderr": stderr_buf,
                    "exit_code": exit_code,
                }));
            }
        }
        drop(st);

        // shell fallback path
        if let Some(cfg) = &config {
            match run_ssh_command_with_timeout(cfg, &full_cmd, timeout_secs) {
                Ok(output) => {
                    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                    let exit_code = output.status.code().unwrap_or(-1);
                    return Ok(json!({
                        "status": if exit_code == 0 { "success" } else { "error" },
                        "stdout": stdout,
                        "stderr": stderr,
                        "exit_code": exit_code,
                    }));
                }
                Err(e) => {
                    return Ok(json!({"status": "error", "msg": format!("{e:#}")}));
                }
            }
        }

        Ok(json!({"status": "error", "msg": "No connection"}))
    }

    // -- SFTP helpers ------------------------------------------------------

    fn has_sftp(&self) -> bool {
        self.state.lock().sftp.is_some()
    }

    fn clone_config(&self) -> Option<ConnectionConfig> {
        self.state.lock().config.clone()
    }

    // -- remote_read -------------------------------------------------------

    pub fn remote_read(&self, path: &str) -> JsonResult {
        if !self.is_connected() {
            return Ok(json!({"status": "error", "msg": "Not connected"}));
        }

        let cfg = self.clone_config();

        if self.has_sftp() {
            let s = self.state.lock();
            if let Some(ref sftp) = s.sftp {
                let p = Path::new(path);
                return match sftp.open_mode(p, ssh2::OpenFlags::READ, 0o644, ssh2::OpenType::File) {
                    Ok(mut file) => {
                        let mut buf = Vec::new();
                        file.read_to_end(&mut buf)
                            .with_context(|| format!("reading remote file {path}"))?;
                        match String::from_utf8(buf.clone()) {
                            Ok(text) => Ok(json!({"status": "success", "content": text})),
                            Err(_) => Ok(json!({
                                "status": "success",
                                "binary": true,
                                "size": buf.len(),
                                "preview": format!("{:?}", &buf[..buf.len().min(200)]),
                            })),
                        }
                    }
                    Err(e) => Ok(json!({"status": "error", "msg": format!("{e}")})),
                };
            }
        }

        // shell fallback
        let cfg = cfg.ok_or_else(|| anyhow!("No config"))?;
        let output = run_ssh_command(&cfg, &format!("cat {}", shell_quote(path)))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Ok(json!({"status": "error", "msg": stderr.trim()}));
        }
        let content = String::from_utf8_lossy(&output.stdout).to_string();
        Ok(json!({"status": "success", "content": content}))
    }

    // -- remote_write ------------------------------------------------------

    pub fn remote_write(&self, path: &str, content: &str) -> JsonResult {
        if !self.is_connected() {
            return Ok(json!({"status": "error", "msg": "Not connected"}));
        }

        let cfg = self.clone_config();

        if self.has_sftp() {
            let p = Path::new(path);
            let mut s = self.state.lock();
            if let Some(ref sftp) = s.sftp {
                // Ensure parent directory exists
                if let Some(parent) = p.parent() {
                    if !parent.as_os_str().is_empty() && sftp.stat(parent).is_err() {
                        drop(s);
                        self.exec_command_sync(
                            &format!("mkdir -p {}", shell_quote(&parent.to_string_lossy())),
                            10,
                            "",
                        )?;
                        s = self.state.lock();
                    }
                }

                if let Some(ref sftp) = s.sftp {
                    return match sftp.open_mode(
                        p,
                        ssh2::OpenFlags::WRITE
                            | ssh2::OpenFlags::CREATE
                            | ssh2::OpenFlags::TRUNCATE,
                        0o644,
                        ssh2::OpenType::File,
                    ) {
                        Ok(mut file) => {
                            file.write_all(content.as_bytes())
                                .with_context(|| format!("writing remote file {path}"))?;
                            Ok(json!({"status": "success", "path": path}))
                        }
                        Err(e) => Ok(json!({"status": "error", "msg": format!("{e}")})),
                    };
                }
            }
        }

        // shell fallback
        let cfg = cfg.ok_or_else(|| anyhow!("No config"))?;
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        let local_tmp = std::env::temp_dir().join(format!(
            "generic-coder-remote-write-{}-{stamp}.tmp",
            std::process::id()
        ));
        std::fs::write(&local_tmp, content)
            .with_context(|| format!("writing temporary file for remote path {path}"))?;

        let remote_tmp = format!("/tmp/generic-coder-remote-write-{stamp}.tmp");
        let remote_target = format!("{}@{}:{remote_tmp}", cfg.username, cfg.host);
        let transfer_result = scp_transfer(&cfg, &local_tmp.display().to_string(), &remote_target);
        let _ = std::fs::remove_file(&local_tmp);
        transfer_result?;

        let move_cmd = if let Some(parent) = Path::new(path).parent() {
            if parent.as_os_str().is_empty() {
                format!("mv {} {}", shell_quote(&remote_tmp), shell_quote(path))
            } else {
                format!(
                    "mkdir -p {} && mv {} {}",
                    shell_quote(&parent.to_string_lossy()),
                    shell_quote(&remote_tmp),
                    shell_quote(path)
                )
            }
        } else {
            format!("mv {} {}", shell_quote(&remote_tmp), shell_quote(path))
        };
        let output = run_ssh_command(&cfg, &move_cmd)?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Ok(json!({"status": "error", "msg": stderr.trim()}));
        }
        Ok(json!({"status": "success", "path": path}))
    }

    // -- remote_list_dir ---------------------------------------------------

    pub fn remote_list_dir(&self, path: &str) -> JsonResult {
        if !self.is_connected() {
            return Ok(json!({"status": "error", "msg": "Not connected"}));
        }

        let cfg = self.clone_config();

        if self.has_sftp() {
            let s = self.state.lock();
            if let Some(ref sftp) = s.sftp {
                return match sftp.readdir(Path::new(path)) {
                    Ok(entries) => {
                        let items: Vec<Value> = entries
                            .into_iter()
                            .map(|(name, stat)| {
                                let is_dir = stat.perm.map_or(false, |p| p & 0o40000 != 0);
                                json!({
                                    "name": name.to_string_lossy(),
                                    "type": if is_dir { "directory" } else { "file" },
                                    "size": stat.size.unwrap_or(0),
                                    "modified": stat.mtime.unwrap_or(0),
                                })
                            })
                            .collect();
                        Ok(json!({"status": "success", "items": items, "path": path}))
                    }
                    Err(e) => Ok(json!({"status": "error", "msg": format!("{e}")})),
                };
            }
        }

        // shell fallback
        let cfg = cfg.ok_or_else(|| anyhow!("No config"))?;
        let output = run_ssh_command(
            &cfg,
            &format!("ls -la --time-style=+%s {}", shell_quote(path)),
        )?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut items = Vec::new();
        for line in stdout.lines().skip(1) {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 7 {
                items.push(json!({
                    "name": parts[6..].join(" "),
                    "type": if parts[0].starts_with('d') { "directory" } else { "file" },
                    "size": parts[4].parse::<u64>().unwrap_or(0),
                    "modified": parts[5].parse::<f64>().unwrap_or(0.0),
                }));
            }
        }
        Ok(json!({"status": "success", "items": items, "path": path}))
    }

    // -- remote_delete -----------------------------------------------------

    pub fn remote_delete(&self, path: &str) -> JsonResult {
        if !self.is_connected() {
            return Ok(json!({"status": "error", "msg": "Not connected"}));
        }

        if !self.has_sftp() {
            return self.exec_command_sync(&format!("rm -rf {}", shell_quote(path)), 30, "");
        }

        let s = self.state.lock();
        let sftp = match s.sftp.as_ref() {
            Some(sftp) => sftp,
            None => {
                return Ok(json!({"status": "error", "msg": "SFTP not available"}));
            }
        };
        let p = Path::new(path);

        match sftp.stat(p) {
            Err(_) => Ok(json!({"status": "error", "msg": format!("Path not found: {path}")})),
            Ok(stat) => {
                let is_dir = stat.perm.map_or(false, |perm| perm & 0o40000 != 0);
                if is_dir {
                    if let Err(e) = Self::rmtree_sftp_inner(sftp, p) {
                        return Ok(json!({"status": "error", "msg": format!("{e}")}));
                    }
                } else if let Err(e) = sftp.unlink(p) {
                    return Ok(json!({"status": "error", "msg": format!("{e}")}));
                }
                Ok(json!({"status": "success", "path": path}))
            }
        }
    }

    fn rmtree_sftp_inner(sftp: &ssh2::Sftp, path: &Path) -> Result<()> {
        for (entry_path, stat) in sftp.readdir(path)? {
            let is_dir = stat.perm.map_or(false, |p| p & 0o40000 != 0);
            if is_dir {
                Self::rmtree_sftp_inner(sftp, &entry_path)?;
            } else {
                sftp.unlink(&entry_path)?;
            }
        }
        sftp.rmdir(path)?;
        Ok(())
    }

    // -- remote_stat -------------------------------------------------------

    pub fn remote_stat(&self, path: &str) -> JsonResult {
        if !self.is_connected() {
            return Ok(json!({"status": "error", "msg": "Not connected"}));
        }

        let cfg = self.clone_config();

        if self.has_sftp() {
            let s = self.state.lock();
            if let Some(ref sftp) = s.sftp {
                return match sftp.stat(Path::new(path)) {
                    Ok(stat) => {
                        let is_dir = stat.perm.map_or(false, |p| p & 0o40000 != 0);
                        let perm_str = stat
                            .perm
                            .map(|p| format!("{:03o}", p & 0o777))
                            .unwrap_or_default();
                        Ok(json!({
                            "status": "success",
                            "type": if is_dir { "directory" } else { "file" },
                            "size": stat.size.unwrap_or(0),
                            "modified": stat.mtime.unwrap_or(0),
                            "permissions": perm_str,
                        }))
                    }
                    Err(e) => Ok(json!({"status": "error", "msg": format!("{e}")})),
                };
            }
        }

        // shell fallback
        let cfg = cfg.ok_or_else(|| anyhow!("No config"))?;
        let output = run_ssh_command(
            &cfg,
            &format!("stat --format=%F|%s|%Y|%a {}", shell_quote(path)),
        )?;
        if !output.status.success() {
            return Ok(json!({"status": "error", "msg": "File not found"}));
        }
        let stdout = String::from_utf8_lossy(&output.stdout);
        let parts: Vec<&str> = stdout.trim().split('|').collect();
        Ok(json!({
            "status": "success",
            "type": if parts.first().map_or(false, |p| p.contains("directory")) {
                "directory"
            } else {
                "file"
            },
            "size": parts.get(1).and_then(|s| s.parse::<u64>().ok()).unwrap_or(0),
            "modified": parts.get(2).and_then(|s| s.parse::<f64>().ok()).unwrap_or(0.0),
            "permissions": parts.get(3).unwrap_or(&""),
        }))
    }

    // -- upload_file -------------------------------------------------------

    pub fn upload_file(&self, local_path: &str, remote_path: &str) -> JsonResult {
        if !self.is_connected() {
            return Ok(json!({"status": "error", "msg": "Not connected"}));
        }

        let local = Path::new(local_path);
        if !local.exists() {
            return Ok(json!({
                "status": "error",
                "msg": format!("Local file not found: {local_path}")
            }));
        }

        let cfg = self.clone_config();

        if self.has_sftp() {
            let rp = Path::new(remote_path);
            let s = self.state.lock();
            if let Some(ref sftp) = s.sftp {
                if let Some(parent) = rp.parent() {
                    if !parent.as_os_str().is_empty() && sftp.stat(parent).is_err() {
                        drop(s);
                        self.exec_command_sync(
                            &format!("mkdir -p {}", shell_quote(&parent.to_string_lossy())),
                            10,
                            "",
                        )?;
                        return self.upload_file(local_path, remote_path);
                    }
                }

                let mut local_file = std::fs::File::open(local)
                    .with_context(|| format!("opening local file {local_path}"))?;
                let mut remote_file = sftp
                    .open_mode(
                        rp,
                        ssh2::OpenFlags::WRITE
                            | ssh2::OpenFlags::CREATE
                            | ssh2::OpenFlags::TRUNCATE,
                        0o644,
                        ssh2::OpenType::File,
                    )
                    .with_context(|| format!("opening remote file {remote_path}"))?;
                let written = std::io::copy(&mut local_file, &mut remote_file)
                    .with_context(|| format!("uploading {local_path} -> {remote_path}"))?;
                return Ok(json!({
                    "status": "success",
                    "local": local_path,
                    "remote": remote_path,
                    "size": written,
                }));
            }
        }

        // shell fallback
        let cfg = cfg.ok_or_else(|| anyhow!("No config"))?;
        scp_transfer(
            &cfg,
            local_path,
            &format!("{}@{}:{remote_path}", cfg.username, cfg.host),
        )?;
        Ok(json!({"status": "success", "local": local_path, "remote": remote_path}))
    }

    // -- download_file -----------------------------------------------------

    pub fn download_file(&self, remote_path: &str, local_path: &str) -> JsonResult {
        if !self.is_connected() {
            return Ok(json!({"status": "error", "msg": "Not connected"}));
        }

        if let Some(parent) = Path::new(local_path).parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("creating local dir {}", parent.display()))?;
        }

        let cfg = self.clone_config();

        if self.has_sftp() {
            let s = self.state.lock();
            if let Some(ref sftp) = s.sftp {
                let mut remote_file = sftp
                    .open_mode(
                        Path::new(remote_path),
                        ssh2::OpenFlags::READ,
                        0o644,
                        ssh2::OpenType::File,
                    )
                    .with_context(|| format!("opening remote file {remote_path}"))?;
                let mut local_file = std::fs::File::create(local_path)
                    .with_context(|| format!("creating local file {local_path}"))?;
                std::io::copy(&mut remote_file, &mut local_file)
                    .with_context(|| format!("downloading {remote_path} -> {local_path}"))?;
                return Ok(json!({
                    "status": "success",
                    "remote": remote_path,
                    "local": local_path,
                }));
            }
        }

        // shell fallback
        let cfg = cfg.ok_or_else(|| anyhow!("No config"))?;
        scp_transfer(
            &cfg,
            &format!("{}@{}:{remote_path}", cfg.username, cfg.host),
            local_path,
        )?;
        Ok(json!({"status": "success", "remote": remote_path, "local": local_path}))
    }
}

impl Default for RemoteServerConnection {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// RemoteServerManager
// ---------------------------------------------------------------------------

pub struct RemoteServerManager {
    connections: Mutex<HashMap<String, Arc<RemoteServerConnection>>>,
}

impl RemoteServerManager {
    pub fn new() -> Self {
        Self {
            connections: Mutex::new(HashMap::new()),
        }
    }

    pub fn list_configs(&self) -> Vec<Value> {
        load_remote_config()
            .iter()
            .map(|c| {
                json!({
                    "name": c.name,
                    "host": c.host,
                    "port": c.port,
                    "username": c.username,
                })
            })
            .collect()
    }

    pub fn connect_to(&self, name: &str, password: &str) -> JsonResult {
        let configs = load_remote_config();
        let config = configs.iter().find(|c| c.name == name).cloned();

        let config = match config {
            Some(c) => c,
            None => {
                return Ok(json!({
                    "status": "error",
                    "msg": format!("Server config \"{name}\" not found")
                }));
            }
        };

        let conn = Arc::new(RemoteServerConnection::new());
        let result = conn.connect(
            &config.host,
            config.port,
            &config.username,
            password,
            &config.key_path,
            &config.jump_host,
            config.jump_port,
            &config.jump_username,
            "",
        )?;

        if result.get("status").and_then(|s| s.as_str()) == Some("connected") {
            self.connections
                .lock()
                .insert(name.to_string(), Arc::clone(&conn));
        }

        Ok(result)
    }

    pub fn disconnect_from(&self, name: &str) {
        if let Some(conn) = self.connections.lock().remove(name) {
            conn.disconnect();
        }
    }

    pub fn get_connection(&self, name: &str) -> Option<Arc<RemoteServerConnection>> {
        self.connections.lock().get(name).cloned()
    }

    /// Execute an operation on a named connection.
    pub fn with_connection<F, R>(&self, name: &str, f: F) -> Option<R>
    where
        F: FnOnce(&RemoteServerConnection) -> R,
    {
        self.connections.lock().get(name).map(|c| f(c))
    }

    pub fn is_connected_to(&self, name: &str) -> bool {
        self.connections
            .lock()
            .get(name)
            .map_or(false, |c| c.is_connected())
    }

    pub fn list_active_connections(&self) -> Vec<String> {
        self.connections
            .lock()
            .iter()
            .filter(|(_, c)| c.is_connected())
            .map(|(n, _)| n.clone())
            .collect()
    }

    pub fn disconnect_all(&self) {
        let names: Vec<String> = self.connections.lock().keys().cloned().collect();
        for name in names {
            self.disconnect_from(&name);
        }
    }
}

impl Default for RemoteServerManager {
    fn default() -> Self {
        Self::new()
    }
}

lazy_static::lazy_static! {
    static ref GLOBAL_REMOTE_MANAGER: Mutex<RemoteServerManager> = Mutex::new(RemoteServerManager::new());
}

pub fn list_configs() -> Vec<Value> {
    GLOBAL_REMOTE_MANAGER.lock().list_configs()
}

pub fn list_active_connections() -> Vec<String> {
    GLOBAL_REMOTE_MANAGER.lock().list_active_connections()
}

pub fn connect_global(
    name: &str,
    host: &str,
    port: u16,
    username: &str,
    password: &str,
    key_path: &str,
    jump_host: &str,
    jump_port: u16,
    jump_username: &str,
) -> JsonResult {
    add_server_config(
        name,
        host,
        port,
        username,
        password,
        key_path,
        jump_host,
        jump_port,
        jump_username,
    );
    GLOBAL_REMOTE_MANAGER.lock().connect_to(name, password)
}

pub fn disconnect_global(name: &str) {
    GLOBAL_REMOTE_MANAGER.lock().disconnect_from(name);
}

pub fn is_connected(name: &str) -> bool {
    GLOBAL_REMOTE_MANAGER.lock().is_connected_to(name)
}

pub fn exec_global(name: &str, command: &str, timeout_secs: u64, cwd: &str) -> JsonResult {
    GLOBAL_REMOTE_MANAGER
        .lock()
        .with_connection(name, |conn| {
            conn.exec_command_sync(command, timeout_secs, cwd)
        })
        .unwrap_or_else(|| {
            Ok(json!({"status": "error", "msg": format!("Remote connection \"{name}\" not found")}))
        })
}

pub fn read_global(name: &str, path: &str) -> JsonResult {
    GLOBAL_REMOTE_MANAGER
        .lock()
        .with_connection(name, |conn| conn.remote_read(path))
        .unwrap_or_else(|| {
            Ok(json!({"status": "error", "msg": format!("Remote connection \"{name}\" not found")}))
        })
}

pub fn write_global(name: &str, path: &str, content: &str) -> JsonResult {
    GLOBAL_REMOTE_MANAGER
        .lock()
        .with_connection(name, |conn| conn.remote_write(path, content))
        .unwrap_or_else(|| {
            Ok(json!({"status": "error", "msg": format!("Remote connection \"{name}\" not found")}))
        })
}

pub fn list_dir_global(name: &str, path: &str) -> JsonResult {
    GLOBAL_REMOTE_MANAGER
        .lock()
        .with_connection(name, |conn| conn.remote_list_dir(path))
        .unwrap_or_else(|| {
            Ok(json!({"status": "error", "msg": format!("Remote connection \"{name}\" not found")}))
        })
}

// ---------------------------------------------------------------------------
// Config persistence (free functions)
// ---------------------------------------------------------------------------

fn config_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".genericagent")
}

fn config_path() -> PathBuf {
    config_dir().join("remote_config.json")
}

pub fn load_remote_config() -> Vec<ServerConfig> {
    let path = config_path();
    if !path.exists() {
        return Vec::new();
    }
    match std::fs::read_to_string(&path) {
        Ok(data) => serde_json::from_str(&data).unwrap_or_default(),
        Err(_) => Vec::new(),
    }
}

pub fn save_remote_config(configs: &[ServerConfig]) -> Result<()> {
    let dir = config_dir();
    std::fs::create_dir_all(&dir)
        .with_context(|| format!("creating config dir {}", dir.display()))?;
    let json = serde_json::to_string_pretty(configs)?;
    std::fs::write(config_path(), json)?;
    Ok(())
}

pub fn add_server_config(
    name: &str,
    host: &str,
    port: u16,
    username: &str,
    _password: &str,
    key_path: &str,
    jump_host: &str,
    jump_port: u16,
    jump_username: &str,
) -> bool {
    let mut configs = load_remote_config();

    if let Some(existing) = configs.iter_mut().find(|c| c.name == name) {
        existing.host = host.to_string();
        existing.port = port;
        existing.username = username.to_string();
        existing.key_path = key_path.to_string();
        existing.jump_host = jump_host.to_string();
        existing.jump_port = jump_port;
        existing.jump_username = jump_username.to_string();
        let _ = save_remote_config(&configs);
        return true;
    }

    configs.push(ServerConfig {
        name: name.to_string(),
        host: host.to_string(),
        port,
        username: username.to_string(),
        key_path: key_path.to_string(),
        jump_host: jump_host.to_string(),
        jump_port,
        jump_username: jump_username.to_string(),
    });
    let _ = save_remote_config(&configs);
    true
}

pub fn remove_server_config(name: &str) -> bool {
    let configs = load_remote_config();
    let len_before = configs.len();
    let filtered: Vec<ServerConfig> = configs.into_iter().filter(|c| c.name != name).collect();
    if filtered.len() != len_before {
        let _ = save_remote_config(&filtered);
        return true;
    }
    false
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shell_quote_simple() {
        assert_eq!(shell_quote("hello"), "'hello'");
    }

    #[test]
    fn test_shell_quote_with_single_quote() {
        assert_eq!(shell_quote("it's"), "'it'\\''s'");
    }

    #[test]
    fn test_config_roundtrip() {
        let cfg = ServerConfig {
            name: "test".into(),
            host: "1.2.3.4".into(),
            port: 2222,
            username: "deploy".into(),
            key_path: "/tmp/key".into(),
            jump_host: "bastion".into(),
            jump_port: 22,
            jump_username: "ops".into(),
        };
        let configs = vec![cfg];
        let json = serde_json::to_string(&configs).unwrap();
        let back: Vec<ServerConfig> = serde_json::from_str(&json).unwrap();
        assert_eq!(back.len(), 1);
        assert_eq!(back[0].name, "test");
    }

    #[test]
    fn test_connection_defaults() {
        let conn = RemoteServerConnection::new();
        assert!(!conn.is_connected());
    }

    #[test]
    fn test_manager_empty() {
        let mgr = RemoteServerManager::new();
        assert!(mgr.list_active_connections().is_empty());
    }
}
