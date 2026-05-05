//! Generic Coder TUI — Terminal-based coding cockpit
//!
//! A Ratatui-powered terminal UI that mirrors the web UI functionality:
//! chat with LLM, manage models, browse workspace, review git, control workflow.

mod app;
mod chat;
mod event;
mod sessions;
mod settings;
mod sidebar;
mod status;
mod ui;
mod workspace;

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use clap::Parser;
use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::prelude::*;
use tokio::sync::RwLock;

use generic_coder::agent::GenericAgent;
use generic_coder::config;
use generic_coder::error_memory::ErrorMemory;
use generic_coder::skills::SkillsManager;

use crate::app::App;

#[derive(Parser)]
#[command(name = "generic-coder-tui", version, about = "Generic Coder Terminal UI")]
struct Cli {
    /// Project directory (auto-detected if omitted)
    #[arg(long)]
    project_dir: Option<String>,

    /// LLM slot number
    #[arg(long, default_value = "0")]
    llm_no: usize,

    /// Quiet mode (less verbose in background)
    #[arg(long, default_value = "false")]
    quiet: bool,
}

fn project_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("GENERIC_CODER_PROJECT_DIR") {
        let path = PathBuf::from(dir);
        if path.is_dir() {
            return path;
        }
    }
    if let Ok(dir) = std::env::var("CARGO_MANIFEST_DIR") {
        return PathBuf::from(dir);
    }
    // Try parent of tui/ — the main project root
    let exe = std::env::current_exe().ok();
    if let Some(ref path) = exe {
        for ancestor in path.ancestors().skip(2) {
            if ancestor.join("Cargo.toml").is_file()
                && !ancestor.ends_with("tui")
            {
                return ancestor.to_path_buf();
            }
        }
    }
    if let Ok(cwd) = std::env::current_dir() {
        // If we're inside tui/, go up one level
        if cwd.join("Cargo.toml").is_file() == false
            && cwd.parent().map(|p| p.join("Cargo.toml").is_file()).unwrap_or(false)
        {
            return cwd.parent().unwrap().to_path_buf();
        }
        return cwd;
    }
    PathBuf::from(".")
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let root = if let Some(ref dir) = cli.project_dir {
        PathBuf::from(dir)
    } else {
        project_dir()
    };

    // ── Initialize agent (same as main binary) ────────────────
    let cfg = config::load_config(&root);
    let skills_mgr = SkillsManager::new(&root);
    let _ = skills_mgr.bootstrap_presets();
    let skills_summary = skills_mgr.active_skills_summary();
    let error_memory = ErrorMemory::new(&root);
    let error_summary = error_memory.avoidance_summary();
    let mut combined = String::new();
    if !skills_summary.is_empty() {
        combined.push_str(&skills_summary);
    }
    if !error_summary.is_empty() {
        combined.push('\n');
        combined.push_str(&error_summary);
    }
    let system_prompt = config::get_system_prompt_with_skills(&root, Some(&combined));
    let tools_schema = config::load_tool_schema(&root, None);

    let mut agent = GenericAgent::new();
    agent.verbose = !cli.quiet;
    let llm_no = cli.llm_no;

    if !cfg.llm_configs.is_empty() {
        let _ = agent.load_llm_sessions(&cfg.llm_configs, &cfg.mixin_configs);
        if llm_no < agent.llm_clients.len() {
            agent.current_llm_no = llm_no;
        } else {
            agent.current_llm_no = 0;
        }
    }

    let agent = Arc::new(RwLock::new(agent));
    let (task_tx, mut task_rx) = tokio::sync::mpsc::channel::<(String, String, tokio::sync::mpsc::Sender<serde_json::Value>)>(256);

    // Background: process task queue
    let bg_agent = agent.clone();
    let bg_sys = system_prompt.clone();
    let bg_tools = tools_schema.clone();
    tokio::spawn(async move {
        while let Some((query, source, reply)) = task_rx.recv().await {
            bg_agent.write().await.run_task(query, source, reply, bg_sys.clone(), bg_tools.clone()).await;
        }
    });

    // ── Setup terminal ───────────────────────────────────────
    enable_raw_mode().context("enable raw mode")?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)
        .context("enter alternate screen")?;

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).context("create terminal")?;

    // ── Create app state and run ────────────────────────────
    let mut app = App::new(agent, task_tx, root, system_prompt, tools_schema, error_memory).await;
    let result = app.run(&mut terminal).await;

    // ── Cleanup terminal ─────────────────────────────────────
    disable_raw_mode().ok();
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )
    .ok();
    terminal.show_cursor().ok();

    result
}
