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
//! - `web` — Web UI server (Axum)

pub mod acp;
pub mod agent;
pub mod oneshot;
pub mod config;
pub mod error_memory;
pub mod llm;
pub mod media;
pub mod remote;
pub mod skills;
pub mod tools;
pub mod types;
pub mod web;
pub mod workflow;
pub mod workspace;
