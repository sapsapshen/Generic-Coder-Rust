use std::fs;
use std::path::PathBuf;

use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::config;

const MAX_PERSISTED_SESSIONS: usize = 100;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TokenUsage {
    #[serde(default)]
    pub prompt_tokens: u64,
    #[serde(default)]
    pub completion_tokens: u64,
    #[serde(default)]
    pub cached_tokens: u64,
    #[serde(default)]
    pub turns: u64,
}

impl TokenUsage {
    pub fn add_turn(&mut self, usage: &TokenUsage) {
        self.prompt_tokens += usage.prompt_tokens;
        self.completion_tokens += usage.completion_tokens;
        self.cached_tokens += usage.cached_tokens;
        self.turns += 1;
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SessionCheckpoint {
    pub index: usize,
    pub preview: String,
    pub rounds: usize,
    pub saved_at: i64,
    #[serde(default)]
    pub messages: Vec<Value>,
    #[serde(default)]
    pub usage_totals: TokenUsage,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_usage: Option<TokenUsage>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PersistedSession {
    pub index: usize,
    pub preview: String,
    pub rounds: usize,
    pub saved_at: i64,
    #[serde(default)]
    pub messages: Vec<Value>,
    #[serde(default)]
    pub checkpoints: Vec<SessionCheckpoint>,
    #[serde(default)]
    pub usage_totals: TokenUsage,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_usage: Option<TokenUsage>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin_session_index: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin_checkpoint_index: Option<usize>,
}

fn sessions_file_path() -> PathBuf {
    config::config_dir().join("sessions.json")
}

fn current_timestamp() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or_default()
}

fn summarize_messages(messages: &[Value]) -> String {
    messages
        .iter()
        .rev()
        .find_map(|msg| msg.get("content").and_then(|v| v.as_str()))
        .unwrap_or("No preview available")
        .chars()
        .take(120)
        .collect()
}

fn count_rounds(messages: &[Value]) -> usize {
    messages
        .iter()
        .filter(|message| message.get("role").and_then(|v| v.as_str()) == Some("assistant"))
        .count()
}

pub fn load_sessions() -> Vec<PersistedSession> {
    let path = sessions_file_path();
    if !path.exists() {
        return Vec::new();
    }

    let Ok(raw) = fs::read_to_string(path) else {
        return Vec::new();
    };
    let Ok(mut sessions) = serde_json::from_str::<Vec<PersistedSession>>(&raw) else {
        return Vec::new();
    };
    sessions.sort_by_key(|session| session.index);
    sessions
}

pub fn get_session(index: usize) -> Option<PersistedSession> {
    load_sessions()
        .into_iter()
        .find(|session| session.index == index)
}

pub fn get_checkpoint(session_index: usize, checkpoint_index: usize) -> Option<SessionCheckpoint> {
    get_session(session_index)?
        .checkpoints
        .into_iter()
        .find(|checkpoint| checkpoint.index == checkpoint_index)
}

pub fn list_checkpoints(session_index: usize) -> Vec<SessionCheckpoint> {
    get_session(session_index)
        .map(|session| session.checkpoints)
        .unwrap_or_default()
}

pub fn delete_session(index: usize) -> Result<bool> {
    let mut sessions = load_sessions();
    let original_len = sessions.len();
    sessions.retain(|session| session.index != index);
    if sessions.len() == original_len {
        return Ok(false);
    }

    fs::create_dir_all(config::config_dir())?;
    fs::write(
        sessions_file_path(),
        serde_json::to_string_pretty(&sessions)?,
    )?;
    Ok(true)
}

pub fn usage_from_value(value: &Value) -> Option<TokenUsage> {
    let prompt_tokens = value
        .get("prompt_tokens")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let completion_tokens = value
        .get("completion_tokens")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let cached_tokens = value
        .get("prompt_cache_hit_tokens")
        .and_then(|v| v.as_u64())
        .or_else(|| value.get("cached_tokens").and_then(|v| v.as_u64()))
        .unwrap_or(0);
    if prompt_tokens == 0 && completion_tokens == 0 && cached_tokens == 0 {
        None
    } else {
        Some(TokenUsage {
            prompt_tokens,
            completion_tokens,
            cached_tokens,
            turns: 0,
        })
    }
}

pub fn upsert_session(
    index: Option<usize>,
    messages: &[Value],
    last_usage: Option<TokenUsage>,
) -> Result<PersistedSession> {
    fs::create_dir_all(config::config_dir())?;

    let mut sessions = load_sessions();
    let session_index = index.unwrap_or_else(|| {
        sessions
            .iter()
            .map(|session| session.index)
            .max()
            .unwrap_or(0)
            + 1
    });

    let timestamp = current_timestamp();
    let preview = summarize_messages(messages);
    let rounds = count_rounds(messages);

    let session = if let Some(existing) = sessions
        .iter_mut()
        .find(|existing| existing.index == session_index)
    {
        let mut usage_totals = existing.usage_totals.clone();
        if let Some(ref usage) = last_usage {
            usage_totals.add_turn(usage);
        }

        let mut checkpoints = existing.checkpoints.clone();
        let append_checkpoint = checkpoints
            .last()
            .map(|checkpoint| checkpoint.messages != messages)
            .unwrap_or(true);
        if append_checkpoint {
            let checkpoint_index = checkpoints
                .last()
                .map(|checkpoint| checkpoint.index + 1)
                .unwrap_or(1);
            checkpoints.push(SessionCheckpoint {
                index: checkpoint_index,
                preview: preview.clone(),
                rounds,
                saved_at: timestamp,
                messages: messages.to_vec(),
                usage_totals: usage_totals.clone(),
                last_usage: last_usage.clone(),
            });
        } else if let Some(checkpoint) = checkpoints.last_mut() {
            checkpoint.saved_at = timestamp;
            checkpoint.preview = preview.clone();
            checkpoint.rounds = rounds;
            checkpoint.messages = messages.to_vec();
            checkpoint.usage_totals = usage_totals.clone();
            if last_usage.is_some() {
                checkpoint.last_usage = last_usage.clone();
            }
        }

        let updated = PersistedSession {
            index: session_index,
            preview,
            rounds,
            saved_at: timestamp,
            messages: messages.to_vec(),
            checkpoints,
            usage_totals,
            last_usage: last_usage.clone().or_else(|| existing.last_usage.clone()),
            origin_session_index: existing.origin_session_index,
            origin_checkpoint_index: existing.origin_checkpoint_index,
        };
        *existing = updated.clone();
        updated
    } else {
        let mut usage_totals = TokenUsage::default();
        if let Some(ref usage) = last_usage {
            usage_totals.add_turn(usage);
        }
        PersistedSession {
            index: session_index,
            preview: preview.clone(),
            rounds,
            saved_at: timestamp,
            messages: messages.to_vec(),
            checkpoints: vec![SessionCheckpoint {
                index: 1,
                preview,
                rounds,
                saved_at: timestamp,
                messages: messages.to_vec(),
                usage_totals: usage_totals.clone(),
                last_usage: last_usage.clone(),
            }],
            usage_totals,
            last_usage: last_usage.clone(),
            origin_session_index: None,
            origin_checkpoint_index: None,
        }
    };

    if !sessions
        .iter()
        .any(|existing| existing.index == session_index)
    {
        sessions.push(session.clone());
    }

    sessions.sort_by_key(|existing| existing.saved_at);
    if sessions.len() > MAX_PERSISTED_SESSIONS {
        let overflow = sessions.len() - MAX_PERSISTED_SESSIONS;
        sessions.drain(0..overflow);
    }
    sessions.sort_by_key(|existing| existing.index);

    fs::write(
        sessions_file_path(),
        serde_json::to_string_pretty(&sessions)?,
    )?;
    Ok(session)
}

pub fn fork_session(index: usize, checkpoint_index: Option<usize>) -> Result<PersistedSession> {
    fs::create_dir_all(config::config_dir())?;
    let mut sessions = load_sessions();
    let Some(source) = sessions
        .iter()
        .find(|session| session.index == index)
        .cloned()
    else {
        anyhow::bail!("Session {index} not found");
    };

    let snapshot = checkpoint_index
        .and_then(|checkpoint| {
            source
                .checkpoints
                .iter()
                .find(|entry| entry.index == checkpoint)
                .cloned()
        })
        .unwrap_or_else(|| SessionCheckpoint {
            index: source
                .checkpoints
                .last()
                .map(|entry| entry.index)
                .unwrap_or(1),
            preview: source.preview.clone(),
            rounds: source.rounds,
            saved_at: source.saved_at,
            messages: source.messages.clone(),
            usage_totals: source.usage_totals.clone(),
            last_usage: source.last_usage.clone(),
        });

    let next_index = sessions
        .iter()
        .map(|session| session.index)
        .max()
        .unwrap_or(0)
        + 1;
    let timestamp = current_timestamp();
    let forked = PersistedSession {
        index: next_index,
        preview: snapshot.preview.clone(),
        rounds: snapshot.rounds,
        saved_at: timestamp,
        messages: snapshot.messages.clone(),
        checkpoints: vec![SessionCheckpoint {
            index: 1,
            preview: snapshot.preview,
            rounds: snapshot.rounds,
            saved_at: timestamp,
            messages: snapshot.messages,
            usage_totals: snapshot.usage_totals.clone(),
            last_usage: snapshot.last_usage.clone(),
        }],
        usage_totals: snapshot.usage_totals,
        last_usage: snapshot.last_usage,
        origin_session_index: Some(source.index),
        origin_checkpoint_index: checkpoint_index,
    };

    sessions.push(forked.clone());
    sessions.sort_by_key(|existing| existing.saved_at);
    if sessions.len() > MAX_PERSISTED_SESSIONS {
        let overflow = sessions.len() - MAX_PERSISTED_SESSIONS;
        sessions.drain(0..overflow);
    }
    sessions.sort_by_key(|existing| existing.index);
    fs::write(
        sessions_file_path(),
        serde_json::to_string_pretty(&sessions)?,
    )?;
    Ok(forked)
}
