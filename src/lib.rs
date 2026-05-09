//! Generic Coder — Autonomous coding agent cockpit (Rust rewrite)
//!
//! Core modules:
//! - `types` — Shared data structures
//! - `config` — Configuration loading
//! - `llm` — LLM backends (Claude / OpenAI-compatible)
//! - `agent` — ReAct agent loop + tool dispatch
//! - `tools` — Tool implementations
//! - `workspace` — Workspace manager
//! - `remote` — SSH remote manager
//! - `media` — Media file handler
//! - `web` — Frontend/backend server (Axum)

pub mod acp;
pub mod agent;
pub mod computer_use;
pub mod config;
pub mod dream;
pub mod error_memory;
pub mod llm;
pub mod mcp;
pub mod media;
pub mod oneshot;
pub mod provider_profiles;
pub mod remote;
pub mod semantic;
pub mod session_store;
pub mod skills;
pub mod tools;
pub mod types;
pub mod web;
pub mod workflow;
pub mod workspace;
