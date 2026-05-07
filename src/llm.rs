//! Multi-backend LLM client module for Generic Coder.
//!
//! Provides session types for Claude (Anthropic Messages API), OpenAI-compatible
//! (chat/completions), native tool-use variants, and a mixin fallback session.
//! All sessions stream SSE responses as `Stream<Item = Result<String, String>>`.
//!
//! ## Architecture
//! ```text
//! ToolClient ──wraps──> BaseSession (ClaudeSession / OaiSession)
//!   XML `<tool_use>` protocol prompt
//!
//! NativeToolClient ──wraps──> NativeClaudeSession / NativeOaiSession
//!   passes tool schemas directly to API
//!
//! MixinSession ──wraps──> [session, session, ...]
//!   fallback with spring-back to primary
//! ```

use std::collections::{HashMap, HashSet};
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Duration;

use async_trait::async_trait;

use futures::stream::Stream;
use log::{debug, warn};
use regex::Regex;
use reqwest::Client;
use serde_json::{json, Value};
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::types::{LlmConfig, LlmResponse, ToolCall, ToolSchema};

// ── Constants ────────────────────────────────────────────────────────────

const ANTHROPIC_VERSION: &str = "2023-06-01";
const CLAUDE_CODE_BETA: &str = "claude-code-20250219";
const INTERTHINK_BETA: &str = "interleaved-thinking-2025-05-14";
const REDACT_BETA: &str = "redact-thinking-2026-02-12";
const CACHE_SCOPE_BETA: &str = "prompt-caching-scope-2026-01-05";
const CONTEXT_1M_BETA: &str = "context-1m-2025-08-07";
const PROMPT_CACHE_BETA: &str = "prompt-caching-2024-07-31";

const RETRYABLE_STATUSES: &[u16] = &[408, 409, 425, 429, 500, 502, 503, 504, 529];
const SMART_HISTORY_KEEP_RECENT_TURNS: usize = 4;
const SMART_HISTORY_KEEP_RELEVANT_OLDER_TURNS: usize = 2;
const SMART_HISTORY_TRIGGER_MSGS: usize = 18;
const SMART_HISTORY_TRIGGER_TURNS: usize = 8;
const SMART_HISTORY_MIN_TOTAL_CHARS: usize = 12_000;
const SMART_HISTORY_KEYWORD_LIMIT: usize = 24;
const SMART_HISTORY_MIN_KEYWORD_LEN: usize = 3;
const SMART_HISTORY_STOPWORDS: &[&str] = &[
    "the", "and", "for", "with", "that", "this", "from", "into", "your", "about", "have", "need",
    "want", "make", "made", "does", "dont", "cant", "should", "would", "could", "please", "help",
    "work", "works", "working", "current", "latest", "issue", "using", "used", "there", "their",
    "them", "then", "when", "what", "where", "which", "while", "been", "were", "will", "just",
    "more", "less", "very", "much", "some", "than", "over", "also", "only", "still",
];

lazy_static::lazy_static! {
    static ref THINK_TAG_RE: Regex = Regex::new(r"<think(?:ing)?>(.*?)</think(?:ing)?>").unwrap();
    static ref SUMMARY_TAG_RE: Regex = Regex::new(r"(?s)<summary>[\s\S]*?(?:</summary>|$)").unwrap();
    static ref TOOL_PROTOCOL_TAG_RE: Regex = Regex::new(r"(?s)<(?:tool_use|tool_call)>[\s\S]*?(?:</_?(?:tool_use|tool_call)>|$)").unwrap();
    static ref RESP_CACHE_KEY: String = Uuid::new_v4().to_string();
}

// ── Chunk Stream (bridging mpsc → Stream) ────────────────────────────────

type ChunkResult = Result<String, String>;

pub struct ChunkStream {
    rx: tokio::sync::mpsc::Receiver<ChunkResult>,
}

impl Stream for ChunkStream {
    type Item = ChunkResult;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.rx.poll_recv(cx)
    }
}

// ── Utility Functions ─────────────────────────────────────────────────────

pub fn auto_make_url(base: &str, path: &str) -> String {
    let b = base.trim_end_matches('/');
    let p = path.trim_matches('/');
    if b.ends_with('$') {
        return b[..b.len() - 1].trim_end_matches('/').to_string();
    }
    let has_version = Regex::new(r"/v\d+(/|$)").unwrap().is_match(b);
    if has_version {
        format!("{}/{}", b, p)
    } else {
        format!("{}/v1/{}", b, p)
    }
}

pub fn tryparse_json(raw: &str) -> Result<Value, serde_json::Error> {
    if let Ok(v) = serde_json::from_str(raw) {
        return Ok(v);
    }
    let cleaned = raw.trim().trim_start_matches('`').trim_end_matches('`');
    let cleaned = cleaned.strip_prefix("json\n").unwrap_or(cleaned);
    if let Ok(v) = serde_json::from_str(cleaned) {
        return Ok(v);
    }
    if cleaned.len() > 1 {
        if let Ok(v) = serde_json::from_str(&cleaned[..cleaned.len() - 1]) {
            return Ok(v);
        }
    }
    if let Some(pos) = cleaned.rfind('}') {
        let truncated = &cleaned[..pos + 1];
        return serde_json::from_str(truncated);
    }
    serde_json::from_str(raw)
}

fn sanitize_protocol_text(text: &str) -> String {
    let mut cleaned = TOOL_PROTOCOL_TAG_RE.replace_all(text, "").to_string();
    cleaned = SUMMARY_TAG_RE.replace_all(&cleaned, "").to_string();
    cleaned = strip_legacy_protocol_tags(&cleaned);
    cleaned = strip_dsml_protocol_tags(&cleaned);
    for marker in [
        "<summary>",
        "</summary>",
        "<tool_use>",
        "</tool_use>",
        "</_tool_use>",
        "<tool_call>",
        "</tool_call>",
        "</_tool_call>",
    ] {
        cleaned = cleaned.replace(marker, "");
    }
    cleaned
        .lines()
        .map(str::trim_end)
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

fn is_legacy_tool_tag(tag: &str) -> bool {
    !matches!(
        tag,
        "summary"
            | "thinking"
            | "think"
            | "tool_use"
            | "tool_call"
            | "tool_result"
            | "file_content"
            | "history"
            | "key_info"
    ) && tag.contains('_')
}

fn strip_legacy_protocol_tags(text: &str) -> String {
    let legacy_tag_re = Regex::new(
        r"(?s)<([a-z][a-z0-9_]*_[a-z0-9_]*)>([\s\S]*?)</([a-z][a-z0-9_]*_[a-z0-9_]*)>",
    )
    .unwrap();

    legacy_tag_re
        .replace_all(text, |caps: &regex::Captures| {
            let open = caps.get(1).map(|m| m.as_str()).unwrap_or("");
            let close = caps.get(3).map(|m| m.as_str()).unwrap_or("");
            if open == close && (is_legacy_tool_tag(open) || open == "file_content") {
                String::new()
            } else {
                caps.get(0)
                    .map(|m| m.as_str())
                    .unwrap_or_default()
                    .to_string()
            }
        })
        .to_string()
}

fn strip_dsml_protocol_tags(text: &str) -> String {
    let dsml_re = Regex::new(r"(?s)<｜｜DSML｜｜tool_calls>[\s\S]*?</｜｜DSML｜｜tool_calls>").unwrap();
    dsml_re.replace_all(text, "").to_string()
}

fn parse_dsml_tool_calls(content: &str) -> Vec<ToolCall> {
    let invoke_re = Regex::new(
        r#"(?s)<｜｜DSML｜｜invoke\s+name="([^"]+)"[^>]*>([\s\S]*?)</｜｜DSML｜｜invoke>"#,
    )
    .unwrap();
    let param_re = Regex::new(
        r#"(?s)<｜｜DSML｜｜parameter\s+name="([^"]+)"(?:\s+string="([^"]+)")?[^>]*>([\s\S]*?)</｜｜DSML｜｜parameter>"#,
    )
    .unwrap();

    invoke_re
        .captures_iter(content)
        .filter_map(|invoke_caps| {
            let name = invoke_caps.get(1)?.as_str().trim();
            if name.is_empty() {
                return None;
            }
            let body = invoke_caps.get(2)?.as_str();
            let mut args = serde_json::Map::new();
            for param_caps in param_re.captures_iter(body) {
                let Some(key) = param_caps.get(1).map(|m| m.as_str().trim()) else {
                    continue;
                };
                if key.is_empty() {
                    continue;
                }
                let string_hint = param_caps.get(2).map(|m| m.as_str()).unwrap_or("false");
                let raw_value = param_caps
                    .get(3)
                    .map(|m| m.as_str().trim())
                    .unwrap_or_default();
                let value = if string_hint.eq_ignore_ascii_case("true") {
                    Value::String(raw_value.to_string())
                } else {
                    serde_json::from_str(raw_value)
                        .unwrap_or_else(|_| Value::String(raw_value.to_string()))
                };
                args.insert(key.to_string(), value);
            }
            Some(ToolCall {
                id: String::new(),
                name: name.to_string(),
                arguments: Value::Object(args),
            })
        })
        .collect()
}

fn normalize_chat_reasoning_effort(cfg: &LlmConfig) -> Option<String> {
    let effort = cfg.reasoning_effort.as_ref()?.trim().to_ascii_lowercase();
    let base = cfg.apibase.trim().to_ascii_lowercase();
    let model = cfg.model.trim().to_ascii_lowercase();
    let is_deepseek = base.contains("deepseek") || model.contains("deepseek");

    if is_deepseek {
        return Some(match effort.as_str() {
            "off" => "low".to_string(),
            other => other.to_string(),
        });
    }

    Some(effort)
}

fn try_parse_tool_args(raw: &str) -> Vec<Value> {
    if raw.is_empty() {
        return vec![serde_json::Value::Object(Default::default())];
    }
    if let Ok(v) = serde_json::from_str(raw) {
        return vec![v];
    }
    let parts: Vec<&str> = Regex::new(r"(?<=\})(?=\{)").unwrap().split(raw).collect();
    if parts.len() > 1 {
        let mut parsed = Vec::new();
        for p in parts {
            if let Ok(v) = serde_json::from_str(p) {
                parsed.push(v);
            } else {
                let mut m = serde_json::Map::new();
                m.insert("_raw".into(), Value::String(raw.to_string()));
                return vec![Value::Object(m)];
            }
        }
        return parsed;
    }
    let mut m = serde_json::Map::new();
    m.insert("_raw".into(), Value::String(raw.to_string()));
    vec![Value::Object(m)]
}

fn extract_responses_message_text(item: &Value) -> String {
    let Some(parts) = item.get("content").and_then(|v| v.as_array()) else {
        return item
            .get("text")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
    };

    parts
        .iter()
        .filter_map(|part| {
            let part_type = part.get("type").and_then(|v| v.as_str()).unwrap_or("");
            match part_type {
                "output_text" | "input_text" | "text" => part.get("text").and_then(|v| {
                    v.as_str().map(String::from).or_else(|| {
                        v.get("value")
                            .and_then(|inner| inner.as_str())
                            .map(String::from)
                    })
                }),
                _ => part.get("text").and_then(|v| v.as_str()).map(String::from),
            }
        })
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>()
        .join("")
}

fn drain_complete_sse_lines(buffer: &mut String) -> Vec<String> {
    let mut lines = Vec::new();
    while let Some(idx) = buffer.find('\n') {
        let line = buffer[..idx].trim_end_matches('\r').to_string();
        buffer.drain(..idx + 1);
        lines.push(line);
    }
    lines
}

pub fn openai_tools_to_claude(tools: &[ToolSchema]) -> Vec<Value> {
    tools
        .iter()
        .map(|t| {
            if t.tool_type == "input_schema" {
                serde_json::to_value(t).unwrap_or_default()
            } else {
                json!({
                    "name": t.function.name,
                    "description": t.function.description,
                    "input_schema": if t.function.parameters.is_object() {
                        t.function.parameters.clone()
                    } else {
                        json!({"type": "object", "properties": {}})
                    }
                })
            }
        })
        .collect()
}

fn _stamp_oai_cache_markers(messages: &mut [Value], _model: &str) {
    let ml = _model.to_lowercase();
    if !ml.contains("claude") && !ml.contains("anthropic") {
        return;
    }
    let user_idxs: Vec<usize> = messages
        .iter()
        .enumerate()
        .filter(|(_, m)| m.get("role").and_then(|r| r.as_str()) == Some("user"))
        .map(|(i, _)| i)
        .collect();
    for &idx in user_idxs.iter().rev().take(2).rev() {
        let msg = &mut messages[idx];
        let content = msg
            .get("content")
            .cloned()
            .unwrap_or(Value::String(String::new()));
        if content.is_string() {
            let text = content.as_str().unwrap_or("");
            msg["content"] = json!([{
                "type": "text",
                "text": text,
                "cache_control": {"type": "ephemeral"}
            }]);
        } else if content.is_array() {
            let mut arr = content.as_array().cloned().unwrap_or_default();
            if let Some(last) = arr.last_mut() {
                if let Some(obj) = last.as_object_mut() {
                    obj.insert("cache_control".into(), json!({"type": "ephemeral"}));
                }
            }
            msg["content"] = Value::Array(arr);
        }
    }
}

fn _stamp_claude_cache(messages: &mut [Value]) {
    let user_idxs: Vec<usize> = messages
        .iter()
        .enumerate()
        .filter(|(_, m)| m.get("role").and_then(|r| r.as_str()) == Some("user"))
        .map(|(i, _)| i)
        .collect();
    for &idx in user_idxs.iter().rev().take(2).rev() {
        let msg = &mut messages[idx];
        if let Some(arr) = msg["content"].as_array_mut() {
            if let Some(last) = arr.last_mut() {
                if let Some(obj) = last.as_object_mut() {
                    obj.insert("cache_control".into(), json!({"type": "ephemeral"}));
                }
            }
        }
    }
}

fn _sanitize_leading_user_msg(msg: &Value) -> Value {
    let content = msg.get("content");
    if content.map_or(true, |c| !c.is_array()) {
        return msg.clone();
    }
    let blocks = content.unwrap().as_array().unwrap();
    let mut texts = Vec::new();
    for block in blocks {
        if !block.is_object() {
            continue;
        }
        let t = block.get("type").and_then(|v| v.as_str()).unwrap_or("");
        match t {
            "tool_result" => {
                let c = block.get("content");
                if let Some(arr) = c.and_then(|v| v.as_array()) {
                    for b in arr {
                        if b.get("type").and_then(|v| v.as_str()) == Some("text") {
                            if let Some(txt) = b.get("text").and_then(|v| v.as_str()) {
                                texts.push(txt.to_string());
                            }
                        }
                    }
                } else if let Some(s) = c.and_then(|v| v.as_str()) {
                    texts.push(s.to_string());
                }
            }
            "text" => {
                if let Some(txt) = block.get("text").and_then(|v| v.as_str()) {
                    texts.push(txt.to_string());
                }
            }
            _ => {}
        }
    }
    let mut m = msg.clone();
    let joined = texts
        .into_iter()
        .filter(|t| !t.is_empty())
        .collect::<Vec<_>>()
        .join("\n");
    m["content"] = json!([{"type": "text", "text": joined}]);
    m
}

fn compress_history_tags(messages: &mut [Value], keep_recent: usize, max_len: usize, force: bool) {
    // Compression state – only trigger every N calls unless forced
    static COMPRESS_CD: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let cd = if force {
        COMPRESS_CD.store(0, std::sync::atomic::Ordering::Relaxed);
        0
    } else {
        COMPRESS_CD.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1
    };
    if cd % 5 != 0 {
        return;
    }

    let tags: &[&str] = &["thinking", "think", "tool_use", "tool_result"];
    let tag_patterns: Vec<(Regex, Regex, &str)> = tags
        .iter()
        .map(|tag| {
            (
                Regex::new(&format!(r"<{}(?:\s[^>]*)?>", tag)).unwrap(),
                Regex::new(&format!(r"</{}>", tag)).unwrap(),
                *tag,
            )
        })
        .collect();
    let history_pat = Regex::new(r"<history>[\s\S]*?</history>").unwrap();
    let key_info_pat = Regex::new(r"<key_info>[\s\S]*?</key_info>").unwrap();

    let trunc_str = |s: &str| -> String {
        if s.len() <= max_len {
            s.to_string()
        } else {
            format!(
                "{}\n...[Truncated]...\n{}",
                &s[..max_len / 2],
                &s[s.len() - max_len / 2..]
            )
        }
    };

    let _before: usize = messages
        .iter()
        .map(|m| serde_json::to_string(m).unwrap_or_default().len())
        .sum();

    let msg_count = messages.len(); // pre-compute before mutable borrow
    for (i, msg) in messages.iter_mut().enumerate() {
        if i >= msg_count.saturating_sub(keep_recent) {
            break;
        }
        let content = msg.get("content").cloned();
        match content {
            Some(Value::String(ref s)) => {
                let mut text = history_pat
                    .replace_all(s, "<history>[...]</history>")
                    .to_string();
                text = key_info_pat
                    .replace_all(&text, "<key_info>[...]</key_info>")
                    .to_string();
                for (open_re, close_re, tag) in &tag_patterns {
                    let normalized_open = format!("<{tag}>");
                    let normalized_close = format!("</{tag}>");
                    text = open_re.replace_all(&text, normalized_open.as_str()).to_string();
                    text = close_re
                        .replace_all(&text, normalized_close.as_str())
                        .to_string();
                }
                let truncated = trunc_str(&text);
                msg["content"] = Value::String(truncated);
            }
            Some(Value::Array(ref blocks)) => {
                let mut new_blocks = Vec::new();
                for block in blocks {
                    if !block.is_object() {
                        new_blocks.push(block.clone());
                        continue;
                    }
                    let t = block.get("type").and_then(|v| v.as_str()).unwrap_or("");
                    match t {
                        "text" => {
                            let mut b = block.clone();
                            if let Some(txt) = block.get("text").and_then(|v| v.as_str()) {
                                b["text"] = Value::String(trunc_str(txt));
                            }
                            new_blocks.push(b);
                        }
                        "tool_result" => {
                            let mut b = block.clone();
                            let tc = block.get("content");
                            if let Some(s) = tc.and_then(|v| v.as_str()) {
                                b["content"] = Value::String(trunc_str(s));
                            }
                            new_blocks.push(b);
                        }
                        "tool_use" => {
                            let mut b = block.clone();
                            if let Some(input) = block.get("input").and_then(|v| v.as_object()) {
                                let mut new_input = serde_json::Map::new();
                                for (k, v) in input {
                                    let vs =
                                        v.as_str().map(trunc_str).unwrap_or_else(|| v.to_string());
                                    new_input.insert(k.clone(), Value::String(vs));
                                }
                                b["input"] = Value::Object(new_input);
                            }
                            new_blocks.push(b);
                        }
                        _ => new_blocks.push(block.clone()),
                    }
                }
                msg["content"] = Value::Array(new_blocks);
            }
            _ => {}
        }
    }
}

fn trim_messages_history(history: &mut Vec<Value>, context_win: usize) {
    smart_prune_irrelevant_history(history, context_win);
    compress_history_tags(history, 10, 800, false);
    let mut cost: usize = history
        .iter()
        .map(|m| serde_json::to_string(m).unwrap_or_default().len())
        .sum();
    debug!("Current context: {cost} chars, {} messages.", history.len());

    if cost > context_win * 3 {
        compress_history_tags(history, 4, 800, true);
        let target = ((context_win * 3) as f64 * 0.6) as usize;
        while history.len() > 5 && cost > target {
            history.remove(0);
            while !history.is_empty()
                && history[0].get("role").and_then(|r| r.as_str()) != Some("user")
            {
                history.remove(0);
            }
            if let Some(first) = history.first_mut() {
                if first.get("role").and_then(|r| r.as_str()) == Some("user") {
                    *first = _sanitize_leading_user_msg(first);
                }
            }
            cost = history
                .iter()
                .map(|m| serde_json::to_string(m).unwrap_or_default().len())
                .sum();
        }
        debug!(
            "Trimmed context, current: {cost} chars, {} messages.",
            history.len()
        );
    }
}

fn smart_prune_irrelevant_history(history: &mut Vec<Value>, context_win: usize) {
    let user_turns = history
        .iter()
        .filter(|msg| msg.get("role").and_then(|v| v.as_str()) == Some("user"))
        .count();
    let total_cost: usize = history
        .iter()
        .map(|m| serde_json::to_string(m).unwrap_or_default().len())
        .sum();

    if history.len() < SMART_HISTORY_TRIGGER_MSGS
        && user_turns <= SMART_HISTORY_TRIGGER_TURNS
        && total_cost < SMART_HISTORY_MIN_TOTAL_CHARS.min(context_win)
    {
        return;
    }

    let turns = history_turn_ranges(history);
    if turns.len() <= SMART_HISTORY_KEEP_RECENT_TURNS {
        return;
    }

    let focus_terms = recent_focus_terms(history);
    let recent_turn_start = turns.len().saturating_sub(SMART_HISTORY_KEEP_RECENT_TURNS);
    let mut keep_flags = vec![false; turns.len()];
    for flag in keep_flags.iter_mut().skip(recent_turn_start) {
        *flag = true;
    }

    let mut relevant_older_turns = Vec::new();
    for (turn_idx, (start, end)) in turns.iter().copied().enumerate().take(recent_turn_start) {
        let chunk_text = history[start..end]
            .iter()
            .map(extract_message_text)
            .filter(|text| !text.is_empty())
            .collect::<Vec<_>>()
            .join("\n");
        if chunk_text.is_empty() {
            continue;
        }
        if !focus_terms.is_empty() && has_focus_overlap(&chunk_text, &focus_terms) {
            relevant_older_turns.push(turn_idx);
        }
    }

    if relevant_older_turns.is_empty() && recent_turn_start > 0 {
        relevant_older_turns.push(recent_turn_start - 1);
    }

    for turn_idx in relevant_older_turns
        .into_iter()
        .rev()
        .take(SMART_HISTORY_KEEP_RELEVANT_OLDER_TURNS)
    {
        keep_flags[turn_idx] = true;
    }

    if keep_flags.iter().all(|keep| *keep) {
        return;
    }

    let mut next_history = Vec::new();
    for (turn_idx, (start, end)) in turns.iter().copied().enumerate() {
        if keep_flags[turn_idx] {
            next_history.extend(history[start..end].iter().cloned());
        }
    }

    if next_history.is_empty() || next_history.len() >= history.len() {
        return;
    }

    if let Some(first) = next_history.first_mut() {
        if first.get("role").and_then(|r| r.as_str()) == Some("user") {
            *first = _sanitize_leading_user_msg(first);
        }
    }

    debug!(
        "Smart-pruned stale context: {} -> {} messages (focus terms: {:?})",
        history.len(),
        next_history.len(),
        focus_terms
    );
    *history = next_history;
}

fn history_turn_ranges(history: &[Value]) -> Vec<(usize, usize)> {
    let user_indices: Vec<usize> = history
        .iter()
        .enumerate()
        .filter_map(|(idx, msg)| {
            (msg.get("role").and_then(|v| v.as_str()) == Some("user")).then_some(idx)
        })
        .collect();

    if user_indices.is_empty() {
        if history.is_empty() {
            return Vec::new();
        }
        return vec![(0, history.len())];
    }

    let mut turns = Vec::with_capacity(user_indices.len());
    for (idx, start) in user_indices.iter().copied().enumerate() {
        let end = user_indices.get(idx + 1).copied().unwrap_or(history.len());
        turns.push((start, end));
    }
    turns
}

fn extract_message_text(msg: &Value) -> String {
    let mut out = Vec::new();
    if let Some(content) = msg.get("content") {
        append_text_fragments(content, &mut out);
    }
    out.join("\n")
}

fn append_text_fragments(value: &Value, out: &mut Vec<String>) {
    match value {
        Value::String(text) => {
            if !text.trim().is_empty() {
                out.push(text.trim().to_string());
            }
        }
        Value::Array(items) => {
            for item in items {
                append_text_fragments(item, out);
            }
        }
        Value::Object(map) => {
            if let Some(text) = map.get("text") {
                append_text_fragments(text, out);
            }
            if let Some(content) = map.get("content") {
                append_text_fragments(content, out);
            }
            if let Some(input) = map.get("input") {
                append_text_fragments(input, out);
            }
        }
        _ => {}
    }
}

fn recent_focus_terms(history: &[Value]) -> HashSet<String> {
    let mut focus_texts = Vec::new();
    for msg in history.iter().rev() {
        if msg.get("role").and_then(|v| v.as_str()) == Some("user") {
            let text = extract_message_text(msg);
            if !text.is_empty() {
                focus_texts.push(text);
            }
            if focus_texts.len() >= 2 {
                break;
            }
        }
    }
    tokenize_terms(&focus_texts.join("\n"), SMART_HISTORY_KEYWORD_LIMIT)
        .into_iter()
        .collect()
}

fn has_focus_overlap(text: &str, focus_terms: &HashSet<String>) -> bool {
    if focus_terms.is_empty() {
        return false;
    }
    tokenize_terms(text, SMART_HISTORY_KEYWORD_LIMIT * 2)
        .into_iter()
        .any(|term| focus_terms.contains(&term))
}

fn tokenize_terms(text: &str, limit: usize) -> Vec<String> {
    let mut terms = Vec::new();
    let mut seen = HashSet::new();
    let mut current = String::new();

    let push_current = |buf: &mut String, terms: &mut Vec<String>, seen: &mut HashSet<String>| {
        if buf.is_empty() {
            return;
        }
        let token = std::mem::take(buf);
        if token.len() < SMART_HISTORY_MIN_KEYWORD_LEN
            || SMART_HISTORY_STOPWORDS.contains(&token.as_str())
        {
            return;
        }
        if seen.insert(token.clone()) {
            terms.push(token);
        }
    };

    for ch in text.chars() {
        if ch.is_alphanumeric() || matches!(ch, '_' | '-' | '.' | '/' | '\\') {
            for lower in ch.to_lowercase() {
                current.push(lower);
            }
        } else {
            push_current(&mut current, &mut terms, &mut seen);
            if terms.len() >= limit {
                return terms;
            }
        }
    }
    push_current(&mut current, &mut terms, &mut seen);
    terms.truncate(limit);
    terms
}

fn _fix_messages(messages: &[Value]) -> Vec<Value> {
    if messages.is_empty() {
        return vec![];
    }
    let wrap = |c: &Value| -> Vec<Value> {
        if c.is_array() {
            c.as_array().cloned().unwrap_or_default()
        } else {
            vec![json!({"type": "text", "text": c.as_str().unwrap_or(&c.to_string())})]
        }
    };

    let mut fixed: Vec<Value> = Vec::new();
    for m in messages {
        let role = m
            .get("role")
            .and_then(|r| r.as_str())
            .unwrap_or("user")
            .to_string();
        if let Some(last) = fixed.last() {
            let last_role = last.get("role").and_then(|r| r.as_str()).unwrap_or("");
            if last_role == role {
                let last_content = wrap(last.get("content").unwrap_or(&Value::Null));
                let new_content = wrap(m.get("content").unwrap_or(&Value::Null));
                let mut merged = last.clone();
                let mut combined = last_content;
                combined.push(json!({"type": "text", "text": "\n"}));
                combined.extend(new_content);
                merged["content"] = Value::Array(combined);
                fixed.pop();
                fixed.push(merged);
                continue;
            }
        }
        // Ensure tool_use IDs match with tool_result blocks for Claude
        if let Some(last) = fixed.last_mut() {
            let last_role = last.get("role").and_then(|r| r.as_str()).unwrap_or("");
            if last_role == "assistant" && role == "user" {
                let uses: Vec<String> = wrap(last.get("content").unwrap_or(&Value::Null))
                    .iter()
                    .filter_map(|b| {
                        if b.get("type").and_then(|v| v.as_str()) == Some("tool_use") {
                            b.get("id").and_then(|v| v.as_str()).map(String::from)
                        } else {
                            None
                        }
                    })
                    .collect();
                let has: std::collections::HashSet<String> =
                    wrap(m.get("content").unwrap_or(&Value::Null))
                        .iter()
                        .filter_map(|b| {
                            if b.get("type").and_then(|v| v.as_str()) == Some("tool_result") {
                                b.get("tool_use_id")
                                    .and_then(|v| v.as_str())
                                    .map(String::from)
                            } else {
                                None
                            }
                        })
                        .collect();
                let miss: Vec<&String> = uses.iter().filter(|uid| !has.contains(*uid)).collect();
                if !miss.is_empty() {
                    let mut new_m = m.clone();
                    let mut content = wrap(new_m.get("content").unwrap_or(&Value::Null));
                    for uid in miss {
                        content.insert(
                            0,
                            json!({"type": "tool_result", "tool_use_id": uid, "content": "(error)"}),
                        );
                    }
                    new_m["content"] = Value::Array(content);
                    fixed.push(new_m);
                    continue;
                }
            }
        }
        fixed.push(m.clone());
    }
    while !fixed.is_empty() && fixed[0].get("role").and_then(|r| r.as_str()) != Some("user") {
        fixed.remove(0);
    }
    fixed
}

fn _msgs_claude2oai(messages: &[Value]) -> Vec<Value> {
    let mut result = Vec::new();
    for msg in messages {
        let role = msg
            .get("role")
            .and_then(|r| r.as_str())
            .unwrap_or("user")
            .to_string();
        let content = msg
            .get("content")
            .cloned()
            .unwrap_or(Value::String(String::new()));
        let blocks_owned: Vec<Value> = content.as_array().cloned().unwrap_or_else(|| {
            vec![json!({"type": "text", "text": content.as_str().unwrap_or(&content.to_string())})]
        });

        if role == "assistant" {
            let mut text_parts: Vec<Value> = Vec::new();
            let mut tool_calls: Vec<Value> = Vec::new();
            let mut reasoning = String::new();

            for b in &blocks_owned {
                if !b.is_object() {
                    continue;
                }
                let t = b.get("type").and_then(|v| v.as_str()).unwrap_or("");
                match t {
                    "thinking" => {
                        if let Some(th) = b.get("thinking").and_then(|v| v.as_str()) {
                            reasoning = th.to_string();
                        }
                    }
                    "text" => {
                        if let Some(txt) = b.get("text").and_then(|v| v.as_str()) {
                            if !txt.is_empty() {
                                text_parts.push(json!({"type": "text", "text": txt}));
                            }
                        }
                    }
                    "tool_use" => {
                        let args = b
                            .get("input")
                            .map(|v| serde_json::to_string(v).unwrap_or_default())
                            .unwrap_or_default();
                        tool_calls.push(json!({
                            "id": b.get("id").and_then(|v| v.as_str()).unwrap_or(""),
                            "type": "function",
                            "function": {
                                "name": b.get("name").and_then(|v| v.as_str()).unwrap_or(""),
                                "arguments": args
                            }
                        }));
                    }
                    _ => {}
                }
            }

            let mut m = json!({"role": "assistant"});
            if !reasoning.is_empty() {
                m["reasoning_content"] = Value::String(reasoning);
            }
            if !text_parts.is_empty() {
                m["content"] = Value::Array(text_parts);
            } else {
                m["content"] = Value::String(String::new());
            }
            if !tool_calls.is_empty() {
                m["tool_calls"] = Value::Array(tool_calls);
            }
            result.push(m);
        } else if role == "user" {
            let mut text_parts: Vec<Value> = Vec::new();
            for b in &blocks_owned {
                if !b.is_object() {
                    continue;
                }
                let t = b.get("type").and_then(|v| v.as_str()).unwrap_or("");
                match t {
                    "tool_result" => {
                        if !text_parts.is_empty() {
                            result.push(json!({"role": "user", "content": text_parts.clone()}));
                            text_parts.clear();
                        }
                        let tr_content = b.get("content");
                        let tr_str = if let Some(arr) = tr_content.and_then(|v| v.as_array()) {
                            arr.iter()
                                .filter(|x| x.get("type").and_then(|v| v.as_str()) == Some("text"))
                                .filter_map(|x| x.get("text").and_then(|v| v.as_str()))
                                .collect::<Vec<_>>()
                                .join("\n")
                        } else {
                            tr_content
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                                .to_string()
                        };
                        result.push(json!({
                            "role": "tool",
                            "tool_call_id": b.get("tool_use_id").and_then(|v| v.as_str()).unwrap_or(""),
                            "content": tr_str
                        }));
                    }
                    "image" => {
                        let src = b.get("source");
                        if let Some(source) = src {
                            if source.get("type").and_then(|v| v.as_str()) == Some("base64") {
                                let media_type = source
                                    .get("media_type")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("image/png");
                                let data =
                                    source.get("data").and_then(|v| v.as_str()).unwrap_or("");
                                text_parts.push(json!({
                                    "type": "image_url",
                                    "image_url": {
                                        "url": format!("data:{};base64,{}", media_type, data)
                                    }
                                }));
                            }
                        }
                    }
                    "image_url" => text_parts.push(b.clone()),
                    "text" => {
                        if let Some(txt) = b.get("text").and_then(|v| v.as_str()) {
                            if !txt.is_empty() {
                                text_parts.push(json!({"type": "text", "text": txt}));
                            }
                        }
                    }
                    _ => {}
                }
            }
            if !text_parts.is_empty() {
                result.push(json!({"role": "user", "content": text_parts}));
            }
        } else {
            result.push(msg.clone());
        }
    }
    result
}

fn _ensure_thinking_blocks(messages: &[Value], model: &str) -> Vec<Value> {
    if !model.to_lowercase().contains("deepseek") {
        return messages.to_vec();
    }
    messages
        .iter()
        .map(|m| {
            if m.get("role").and_then(|r| r.as_str()) != Some("assistant") {
                return m.clone();
            }
            let content = m.get("content");
            if content.map_or(true, |c| !c.is_array()) {
                return m.clone();
            }
            let blocks = content.unwrap().as_array().unwrap();
            let has_thinking = blocks
                .iter()
                .any(|b| b.get("type").and_then(|v| v.as_str()) == Some("thinking"));
            if has_thinking {
                m.clone()
            } else {
                let mut new_content = vec![json!({
                    "type": "thinking",
                    "thinking": "...",
                    "signature": "placeholder"
                })];
                new_content.extend(blocks.iter().cloned());
                let mut mm = m.clone();
                mm["content"] = Value::Array(new_content);
                mm
            }
        })
        .collect()
}

fn _keep_claude_block(b: &Value) -> bool {
    if let Some(obj) = b.as_object() {
        if obj.get("type").and_then(|v| v.as_str()) == Some("thinking") {
            return obj.get("signature").is_some();
        }
    }
    true
}

fn _drop_unsigned_thinking(messages: &[Value]) -> Vec<Value> {
    messages
        .iter()
        .map(|m| {
            let mut mm = m.clone();
            if let Some(arr) = mm.get("content").and_then(|v| v.as_array()).cloned() {
                let filtered: Vec<Value> = arr.into_iter().filter(_keep_claude_block).collect();
                mm["content"] = Value::Array(filtered);
            }
            mm
        })
        .collect()
}

fn _record_usage(usage: &Value, api_mode: &str) {
    if usage.is_null() {
        return;
    }
    // Store for frontend display
    if let Ok(mut guard) = GLOBAL_LAST_USAGE.lock() {
        *guard = Some(usage.clone());
    }
    match api_mode {
        "responses" => {
            let cached = usage
                .get("input_tokens_details")
                .and_then(|d| d.get("cached_tokens"))
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let inp = usage
                .get("input_tokens")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            debug!("[Cache] input={inp} cached={cached}");
        }
        "messages" => {
            let ci = usage
                .get("cache_creation_input_tokens")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let cr = usage
                .get("cache_read_input_tokens")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let inp = usage
                .get("input_tokens")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            debug!("[Cache] input={inp} creation={ci} read={cr}");
        }
        _ => {
            let cached = usage
                .get("prompt_tokens_details")
                .and_then(|d| d.get("cached_tokens"))
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let inp = usage
                .get("prompt_tokens")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            debug!("[Cache] input={inp} cached={cached}");
        }
    }
}

/// Global last usage for frontend polling (set by SSE parsers, read by web API).
static GLOBAL_LAST_USAGE: std::sync::LazyLock<std::sync::Mutex<Option<Value>>> =
    std::sync::LazyLock::new(|| std::sync::Mutex::new(None));

/// Expose last LLM usage to web layer.
pub fn take_last_usage() -> Option<Value> {
    GLOBAL_LAST_USAGE.lock().ok().and_then(|mut g| g.take())
}

// ── SSE Parsers ───────────────────────────────────────────────────────────

/// Parses Anthropic SSE stream. Yields text chunks via sender, returns content_blocks.
async fn parse_claude_sse(
    mut lines: impl futures::Stream<Item = Result<bytes::Bytes, reqwest::Error>> + Unpin,
    tx: &tokio::sync::mpsc::Sender<ChunkResult>,
) -> Vec<Value> {
    use futures::StreamExt;
    let mut content_blocks: Vec<Value> = Vec::new();
    let mut current_block: Option<Value> = None;
    let mut tool_json_buf = String::new();
    let mut stop_reason: Option<String> = None;
    let mut got_message_stop = false;
    let mut warn: Option<String> = None;
    let mut line_buffer = String::new();

    'outer: while let Some(Ok(bytes)) = lines.next().await {
        line_buffer.push_str(&String::from_utf8_lossy(&bytes));
        for line in drain_complete_sse_lines(&mut line_buffer) {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            if !line.starts_with("data:") {
                continue;
            }
            let data_str = line[5..].trim();
            if data_str == "[DONE]" {
                break 'outer;
            }
            let evt: Value = match serde_json::from_str(data_str) {
                Ok(v) => v,
                Err(e) => {
                    warn!(
                        "[SSE] JSON parse error: {}, line: {}",
                        e,
                        &data_str[..data_str.len().min(200)]
                    );
                    continue;
                }
            };
            let evt_type = evt.get("type").and_then(|v| v.as_str()).unwrap_or("");

            match evt_type {
                "message_start" => {
                    _record_usage(
                        evt.get("message")
                            .and_then(|m| m.get("usage"))
                            .unwrap_or(&Value::Null),
                        "messages",
                    );
                }
                "content_block_start" => {
                    let block = evt.get("content_block").unwrap_or(&Value::Null);
                    let bt = block.get("type").and_then(|v| v.as_str()).unwrap_or("");
                    match bt {
                        "text" => {
                            current_block = Some(json!({"type": "text", "text": ""}));
                        }
                        "thinking" => {
                            current_block =
                                Some(json!({"type": "thinking", "thinking": "", "signature": ""}));
                        }
                        "tool_use" => {
                            current_block = Some(json!({
                                "type": "tool_use",
                                "id": block.get("id").and_then(|v| v.as_str()).unwrap_or(""),
                                "name": block.get("name").and_then(|v| v.as_str()).unwrap_or(""),
                                "input": {}
                            }));
                            tool_json_buf.clear();
                        }
                        _ => {}
                    }
                }
                "content_block_delta" => {
                    let delta = evt.get("delta").unwrap_or(&Value::Null);
                    let dt = delta.get("type").and_then(|v| v.as_str()).unwrap_or("");
                    match dt {
                        "text_delta" => {
                            let text = delta.get("text").and_then(|v| v.as_str()).unwrap_or("");
                            if let Some(ref mut cb) = current_block {
                                if cb.get("type").and_then(|v| v.as_str()) == Some("text") {
                                    if let Some(existing) = cb.get("text").and_then(|v| v.as_str())
                                    {
                                        cb["text"] = Value::String(format!("{}{}", existing, text));
                                    } else {
                                        cb["text"] = Value::String(text.to_string());
                                    }
                                }
                            }
                            if !text.is_empty() {
                                let _ = tx.send(Ok(text.to_string())).await;
                            }
                        }
                        "thinking_delta" => {
                            if let Some(ref mut cb) = current_block {
                                if cb.get("type").and_then(|v| v.as_str()) == Some("thinking") {
                                    let th = delta
                                        .get("thinking")
                                        .and_then(|v| v.as_str())
                                        .unwrap_or("");
                                    let existing =
                                        cb.get("thinking").and_then(|v| v.as_str()).unwrap_or("");
                                    cb["thinking"] = Value::String(format!("{}{}", existing, th));
                                }
                            }
                        }
                        "signature_delta" => {
                            if let Some(ref mut cb) = current_block {
                                if cb.get("type").and_then(|v| v.as_str()) == Some("thinking") {
                                    let sig = delta
                                        .get("signature")
                                        .and_then(|v| v.as_str())
                                        .unwrap_or("");
                                    let existing =
                                        cb.get("signature").and_then(|v| v.as_str()).unwrap_or("");
                                    cb["signature"] = Value::String(format!("{}{}", existing, sig));
                                }
                            }
                        }
                        "input_json_delta" => {
                            tool_json_buf.push_str(
                                delta
                                    .get("partial_json")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or(""),
                            );
                        }
                        _ => {}
                    }
                }
                "content_block_stop" => {
                    if let Some(mut cb) = current_block.take() {
                        if cb.get("type").and_then(|v| v.as_str()) == Some("tool_use") {
                            if !tool_json_buf.is_empty() {
                                cb["input"] = serde_json::from_str(&tool_json_buf)
                                    .unwrap_or_else(|_| json!({"_raw": tool_json_buf}));
                            }
                        }
                        content_blocks.push(cb);
                    }
                }
                "message_delta" => {
                    let delta = evt.get("delta").unwrap_or(&Value::Null);
                    stop_reason = delta
                        .get("stop_reason")
                        .and_then(|v| v.as_str())
                        .map(String::from)
                        .or(stop_reason);
                    let out_usage = evt.get("usage").unwrap_or(&Value::Null);
                    let out_tokens = out_usage
                        .get("output_tokens")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0);
                    if out_tokens > 0 {
                        debug!(
                            "[Output] tokens={} stop_reason={:?}",
                            out_tokens, stop_reason
                        );
                    }
                }
                "message_stop" => {
                    got_message_stop = true;
                }
                "error" => {
                    let err = evt.get("error").unwrap_or(&Value::Null);
                    let emsg = if err.as_object().is_some() {
                        err.get("message")
                            .and_then(|v| v.as_str())
                            .unwrap_or("unknown error")
                            .to_string()
                    } else {
                        err.to_string()
                    };
                    warn = Some(format!("\n\n!!!Error: SSE {emsg}"));
                    break 'outer;
                }
                _ => {}
            }
        }
    }

    if warn.is_none() {
        if !got_message_stop && stop_reason.is_none() {
            warn = Some("\n\n[!!! Stream interrupted, incomplete response !!!]".into());
        } else if stop_reason.as_deref() == Some("max_tokens") {
            warn = Some("\n\n[!!! Response truncated: max_tokens !!!]".into());
        }
    }
    if let Some(mut cb) = current_block.take() {
        if cb.get("type").and_then(|v| v.as_str()) == Some("tool_use") {
            if !tool_json_buf.is_empty() {
                cb["input"] = serde_json::from_str(&tool_json_buf)
                    .unwrap_or_else(|_| json!({"_raw": tool_json_buf}));
            }
        }
        content_blocks.push(cb);
    }
    if let Some(w) = warn {
        warn!("{}", w.trim());
        content_blocks.push(json!({"type": "text", "text": w.clone()}));
        let _ = tx.send(Ok(w)).await;
    }
    content_blocks
}

/// Parses OpenAI-compatible SSE stream. Yields text chunks via sender, returns content_blocks.
async fn parse_openai_sse(
    mut lines: impl futures::Stream<Item = Result<bytes::Bytes, reqwest::Error>> + Unpin,
    tx: &tokio::sync::mpsc::Sender<ChunkResult>,
    api_mode: &str,
) -> Vec<Value> {
    use futures::StreamExt;
    let mut content_text = String::new();
    let mut line_buffer = String::new();

    if api_mode == "responses" {
        let mut seen_delta = false;
        let mut fc_buf: HashMap<usize, Value> = HashMap::new();
        let mut current_fc_idx: Option<usize> = None;
        let mut message_buf: HashMap<usize, String> = HashMap::new();

        'outer_responses: while let Some(Ok(bytes)) = lines.next().await {
            line_buffer.push_str(&String::from_utf8_lossy(&bytes));
            for line in drain_complete_sse_lines(&mut line_buffer) {
                let line = line.trim();
                if line.is_empty() || !line.starts_with("data:") {
                    continue;
                }
                let data_str = line[5..].trim();
                if data_str == "[DONE]" {
                    break 'outer_responses;
                }
                let evt: Value = match serde_json::from_str(data_str) {
                    Ok(v) => v,
                    Err(_) => continue,
                };
                let etype = evt.get("type").and_then(|v| v.as_str()).unwrap_or("");
                match etype {
                    "response.output_text.delta" => {
                        let delta = evt.get("delta").and_then(|v| v.as_str()).unwrap_or("");
                        if !delta.is_empty() {
                            seen_delta = true;
                            content_text.push_str(delta);
                            let _ = tx.send(Ok(delta.to_string())).await;
                        }
                    }
                    "response.output_text.done" => {
                        if !seen_delta {
                            let text = evt.get("text").and_then(|v| v.as_str()).unwrap_or("");
                            if !text.is_empty() {
                                content_text.push_str(text);
                                let _ = tx.send(Ok(text.to_string())).await;
                            }
                        }
                    }
                    "response.output_item.added" => {
                        let item = evt.get("item").unwrap_or(&Value::Null);
                        let idx = evt
                            .get("output_index")
                            .and_then(|v| v.as_u64())
                            .unwrap_or(0) as usize;
                        if item.get("type").and_then(|v| v.as_str()) == Some("function_call") {
                            fc_buf.insert(
                                idx,
                                json!({
                                    "id": item.get("call_id").or(item.get("id")).and_then(|v| v.as_str()).unwrap_or(""),
                                    "name": item.get("name").and_then(|v| v.as_str()).unwrap_or(""),
                                    "args": ""
                                }),
                            );
                            current_fc_idx = Some(idx);
                        } else if item.get("type").and_then(|v| v.as_str()) == Some("message") {
                            let text = extract_responses_message_text(item);
                            if !text.is_empty() {
                                message_buf.insert(idx, text);
                            }
                        }
                    }
                    "response.output_item.done" => {
                        let item = evt.get("item").unwrap_or(&Value::Null);
                        let idx = evt
                            .get("output_index")
                            .and_then(|v| v.as_u64())
                            .unwrap_or(0) as usize;
                        match item.get("type").and_then(|v| v.as_str()).unwrap_or("") {
                            "message" => {
                                let text = extract_responses_message_text(item);
                                if !text.is_empty() {
                                    message_buf.insert(idx, text);
                                }
                            }
                            "function_call" => {
                                let entry = fc_buf.entry(idx).or_insert_with(|| {
                                    json!({
                                        "id": item.get("call_id").or(item.get("id")).and_then(|v| v.as_str()).unwrap_or(""),
                                        "name": item.get("name").and_then(|v| v.as_str()).unwrap_or(""),
                                        "args": ""
                                    })
                                });
                                if let Some(args) = item.get("arguments").and_then(|v| v.as_str()) {
                                    entry["args"] = Value::String(args.to_string());
                                }
                            }
                            _ => {}
                        }
                    }
                    "response.function_call_arguments.delta" => {
                        let idx = evt
                            .get("output_index")
                            .and_then(|v| v.as_u64())
                            .unwrap_or(current_fc_idx.unwrap_or(0) as u64)
                            as usize;
                        if let Some(fc) = fc_buf.get_mut(&idx) {
                            let delta = evt.get("delta").and_then(|v| v.as_str()).unwrap_or("");
                            let existing = fc.get("args").and_then(|v| v.as_str()).unwrap_or("");
                            fc["args"] = Value::String(format!("{}{}", existing, delta));
                        }
                    }
                    "response.function_call_arguments.done" => {
                        let idx = evt
                            .get("output_index")
                            .and_then(|v| v.as_u64())
                            .unwrap_or(current_fc_idx.unwrap_or(0) as u64)
                            as usize;
                        if let Some(fc) = fc_buf.get_mut(&idx) {
                            if let Some(args) = evt.get("arguments").and_then(|v| v.as_str()) {
                                fc["args"] = Value::String(args.to_string());
                            }
                        }
                    }
                    "error" => {
                        let err = evt.get("error").unwrap_or(&Value::Null);
                        let emsg = if err.as_object().is_some() {
                            err.get("message")
                                .and_then(|v| v.as_str())
                                .unwrap_or("unknown error")
                                .to_string()
                        } else {
                            err.to_string()
                        };
                        if !emsg.is_empty() {
                            content_text.push_str(&format!("!!!Error: {emsg}"));
                            let _ = tx.send(Ok(format!("!!!Error: {emsg}"))).await;
                        }
                        break 'outer_responses;
                    }
                    "response.completed" => {
                        if let Some(output) = evt
                            .get("response")
                            .and_then(|r| r.get("output"))
                            .and_then(|v| v.as_array())
                        {
                            for (idx, item) in output.iter().enumerate() {
                                match item.get("type").and_then(|v| v.as_str()).unwrap_or("") {
                                    "message" => {
                                        let text = extract_responses_message_text(item);
                                        if !text.is_empty() {
                                            message_buf.insert(idx, text);
                                        }
                                    }
                                    "function_call" => {
                                        fc_buf.entry(idx).or_insert_with(|| {
                                            json!({
                                                "id": item.get("call_id").or(item.get("id")).and_then(|v| v.as_str()).unwrap_or(""),
                                                "name": item.get("name").and_then(|v| v.as_str()).unwrap_or(""),
                                                "args": item.get("arguments").and_then(|v| v.as_str()).unwrap_or("")
                                            })
                                        });
                                    }
                                    _ => {}
                                }
                            }
                        }
                        _record_usage(
                            evt.get("response")
                                .and_then(|r| r.get("usage"))
                                .unwrap_or(&Value::Null),
                            api_mode,
                        );
                        break 'outer_responses;
                    }
                    _ => {}
                }
            }
        }

        let mut sorted_msg_idxs: Vec<usize> = message_buf.keys().copied().collect();
        sorted_msg_idxs.sort();
        let completed_text = sorted_msg_idxs
            .into_iter()
            .filter_map(|idx| message_buf.get(&idx).cloned())
            .filter(|text| !text.is_empty())
            .collect::<Vec<_>>()
            .join("\n");
        if !completed_text.is_empty()
            && (content_text.is_empty() || completed_text.len() > content_text.len())
        {
            content_text = completed_text;
        }

        let mut blocks: Vec<Value> = Vec::new();
        if !content_text.is_empty() {
            blocks.push(json!({"type": "text", "text": content_text}));
        }
        let mut sorted_idxs: Vec<usize> = fc_buf.keys().copied().collect();
        sorted_idxs.sort();
        for idx in sorted_idxs {
            let fc = &fc_buf[&idx];
            let raw_args = fc.get("args").and_then(|v| v.as_str()).unwrap_or("");
            let inps = try_parse_tool_args(raw_args);
            for (i, inp) in inps.iter().enumerate() {
                let bid = fc
                    .get("id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let bid = if inps.len() > 1 {
                    if bid.is_empty() {
                        format!("split_{}", i)
                    } else {
                        format!("{}_{}", bid, i)
                    }
                } else {
                    bid
                };
                blocks.push(json!({
                    "type": "tool_use",
                    "id": bid,
                    "name": fc.get("name").and_then(|v| v.as_str()).unwrap_or(""),
                    "input": inp
                }));
            }
        }
        return blocks;
    }

    // chat_completions mode
    let mut tc_buf: HashMap<usize, Value> = HashMap::new();
    let mut reasoning_text = String::new();

    'outer_chat: while let Some(Ok(bytes)) = lines.next().await {
        line_buffer.push_str(&String::from_utf8_lossy(&bytes));
        for line in drain_complete_sse_lines(&mut line_buffer) {
            let line = line.trim();
            if line.is_empty() || !line.starts_with("data:") {
                continue;
            }
            let data_str = line[5..].trim();
            if data_str == "[DONE]" {
                break 'outer_chat;
            }
            let evt: Value = match serde_json::from_str(data_str) {
                Ok(v) => v,
                Err(_) => continue,
            };
            let choices = evt
                .get("choices")
                .and_then(|c| c.as_array())
                .cloned()
                .unwrap_or_default();
            let ch = choices.first().cloned().unwrap_or(Value::Null);
            let delta = ch.get("delta").cloned().unwrap_or(Value::Null);

            if let Some(rc) = delta.get("reasoning_content").and_then(|v| v.as_str()) {
                reasoning_text.push_str(rc);
            }
            if let Some(text) = delta.get("content").and_then(|v| v.as_str()) {
                content_text.push_str(text);
                let _ = tx.send(Ok(text.to_string())).await;
            }
            for tc in delta
                .get("tool_calls")
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default()
            {
                let idx = tc.get("index").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                let has_name = tc
                    .get("function")
                    .and_then(|f| f.get("name"))
                    .and_then(|v| v.as_str())
                    .map(|n| !n.is_empty())
                    .unwrap_or(false);
                if !tc_buf.contains_key(&idx) {
                    if has_name || tc_buf.is_empty() {
                        tc_buf.insert(
                            idx,
                            json!({"id": tc.get("id").and_then(|v| v.as_str()).unwrap_or(""), "name": "", "args": ""}),
                        );
                    } else {
                        let max_idx = *tc_buf.keys().max().unwrap_or(&0);
                        tc_buf.insert(
                            max_idx,
                            json!({"id": tc.get("id").and_then(|v| v.as_str()).unwrap_or(""), "name": "", "args": ""}),
                        );
                    }
                }
                let entry_idx = if !tc_buf.contains_key(&idx) {
                    *tc_buf.keys().max().unwrap_or(&0)
                } else {
                    idx
                };
                if let Some(entry) = tc_buf.get_mut(&entry_idx) {
                    if has_name {
                        if let Some(name) = tc
                            .get("function")
                            .and_then(|f| f.get("name"))
                            .and_then(|v| v.as_str())
                        {
                            entry["name"] = Value::String(name.to_string());
                        }
                    }
                    if let Some(args) = tc
                        .get("function")
                        .and_then(|f| f.get("arguments"))
                        .and_then(|v| v.as_str())
                    {
                        let existing = entry.get("args").and_then(|v| v.as_str()).unwrap_or("");
                        entry["args"] = Value::String(format!("{}{}", existing, args));
                    }
                    if let Some(id) = tc.get("id").and_then(|v| v.as_str()) {
                        let current_id = entry.get("id").and_then(|v| v.as_str()).unwrap_or("");
                        if current_id.is_empty() && !id.is_empty() {
                            entry["id"] = Value::String(id.to_string());
                        }
                    }
                }
            }
            if let Some(usage) = evt.get("usage") {
                _record_usage(usage, api_mode);
            }
        }
    }

    let mut blocks: Vec<Value> = Vec::new();
    if !reasoning_text.is_empty() {
        blocks.push(json!({"type": "thinking", "thinking": reasoning_text}));
    }
    if !content_text.is_empty() {
        blocks.push(json!({"type": "text", "text": content_text}));
    }
    let mut sorted_idxs: Vec<usize> = tc_buf.keys().copied().collect();
    sorted_idxs.sort();
    for idx in sorted_idxs {
        let tc = &tc_buf[&idx];
        let raw_args = tc.get("args").and_then(|v| v.as_str()).unwrap_or("");
        let inps = try_parse_tool_args(raw_args);
        for (i, inp) in inps.iter().enumerate() {
            let bid = tc
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let bid = if inps.len() > 1 {
                if bid.is_empty() {
                    format!("split_{}", i)
                } else {
                    format!("{}_{}", bid, i)
                }
            } else {
                bid
            };
            blocks.push(json!({
                "type": "tool_use",
                "id": bid,
                "name": tc.get("name").and_then(|v| v.as_str()).unwrap_or(""),
                "input": inp
            }));
        }
    }
    blocks
}

// ── BaseSession trait ────────────────────────────────────────────────────

#[async_trait]
pub trait BaseSession: Send + Sync {
    fn name(&self) -> &str;
    fn model(&self) -> &str;
    fn history(&self) -> &Arc<Mutex<Vec<Value>>>;

    /// Issue a raw streaming request and return (stream, content_blocks).
    /// The stream yields text chunks; content_blocks are the parsed final result.
    async fn raw_ask(
        &self,
        messages: Vec<Value>,
        tx: tokio::sync::mpsc::Sender<ChunkResult>,
    ) -> Vec<Value>;

    /// Build messages for the API from raw history entries.
    fn make_messages(&self, raw_list: &[Value]) -> Vec<Value>;

    fn context_win(&self) -> usize;
}

/// Free function: high-level chat that adds user message, trims, and streams.
pub async fn session_chat(
    session: Arc<dyn BaseSession>,
    prompt: &str,
) -> (ChunkStream, Arc<Mutex<Option<Vec<Value>>>>) {
    let (tx, rx) = tokio::sync::mpsc::channel::<ChunkResult>(256);
    let result = Arc::new(Mutex::new(Option::<Vec<Value>>::None));
    let result_for_spawn = result.clone();
    let session_c = session.clone();
    let prompt_o = prompt.to_string();

    tokio::spawn(async move {
        let s = session_c;
        {
            let mut hist = s.history().lock().await;
            hist.push(json!({"role": "user", "content": [{"type": "text", "text": &prompt_o}]}));
            trim_messages_history(&mut hist, s.context_win());
        }
        let messages = {
            let hist = s.history().lock().await;
            s.make_messages(&hist)
        };
        let blocks = s.raw_ask(messages, tx.clone()).await;
        {
            let mut hist = s.history().lock().await;
            if !blocks.iter().any(|b| {
                b.get("text")
                    .and_then(|v| v.as_str())
                    .map(|s| s.starts_with("!!!Error:"))
                    .unwrap_or(false)
            }) {
                let text_blocks: Vec<Value> = blocks
                    .iter()
                    .filter(|b| b.get("type").and_then(|v| v.as_str()) == Some("text"))
                    .cloned()
                    .collect();
                hist.push(json!({"role": "assistant", "content": text_blocks}));
            }
        }
        *result_for_spawn.lock().await = Some(blocks);
    });

    (ChunkStream { rx }, result)
}

// ── ClaudeSession ────────────────────────────────────────────────────────

pub struct ClaudeSession {
    pub config: LlmConfig,
    pub client: Client,
    pub history: Arc<Mutex<Vec<Value>>>,
    pub name: String,
}

impl ClaudeSession {
    pub fn new(cfg: LlmConfig) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(300))
            .danger_accept_invalid_certs(!cfg.verify)
            .build()
            .unwrap_or_default();
        let name = cfg.name.clone();
        let history = Arc::new(Mutex::new(Vec::new()));
        Self {
            config: cfg,
            client,
            history,
            name,
        }
    }

    fn max_tokens(&self) -> usize {
        self.config.max_tokens.unwrap_or(8192)
    }

    fn apply_claude_thinking(&self, payload: &mut Value) {
        if let Some(ref tt) = self.config.thinking_type {
            let thinking_type = tt.to_lowercase();
            if thinking_type == "enabled" {
                if let Some(budget) = self.config.thinking_budget_tokens {
                    payload["thinking"] = json!({
                        "type": "enabled",
                        "budget_tokens": budget
                    });
                } else {
                    warn!("thinking_type='enabled' requires thinking_budget_tokens, ignored.");
                }
            } else {
                payload["thinking"] = json!({"type": thinking_type});
            }
        }
        if let Some(ref effort) = self.config.reasoning_effort {
            let effort_val = match effort.to_lowercase().as_str() {
                "low" => "low",
                "medium" => "medium",
                "high" => "high",
                "xhigh" => "max",
                _ => {
                    warn!(
                        "reasoning_effort '{effort}' is unsupported for Claude output_config.effort, ignored."
                    );
                    return;
                }
            };
            payload["output_config"] = json!({"effort": effort_val});
        }
    }

    fn build_headers(&self) -> Vec<(String, String)> {
        vec![
            ("x-api-key".into(), self.config.apikey.clone()),
            ("Content-Type".into(), "application/json".into()),
            ("anthropic-version".into(), ANTHROPIC_VERSION.into()),
            ("anthropic-beta".into(), PROMPT_CACHE_BETA.into()),
        ]
    }
}

#[async_trait]
impl BaseSession for ClaudeSession {
    fn name(&self) -> &str {
        &self.name
    }
    fn model(&self) -> &str {
        &self.config.model.as_str()
    }
    fn history(&self) -> &Arc<Mutex<Vec<Value>>> {
        &self.history
    }
    fn context_win(&self) -> usize {
        self.config.context_win
    }

    fn make_messages(&self, raw_list: &[Value]) -> Vec<Value> {
        let msgs: Vec<Value> = raw_list
            .iter()
            .map(|m| {
                let role = m.get("role").and_then(|r| r.as_str()).unwrap_or("user");
                let content = m.get("content").cloned().unwrap_or(Value::Null);
                json!({"role": role, "content": content})
            })
            .collect();
        let msgs = _drop_unsigned_thinking(&msgs);
        let mut msgs: Vec<Value> = msgs
            .into_iter()
            .map(|m| {
                let mut mm = m;
                if let Some(arr) = mm.get("content").and_then(|v| v.as_array()).cloned() {
                    mm["content"] = Value::Array(arr);
                }
                mm
            })
            .collect();
        _stamp_claude_cache(&mut msgs);
        msgs
    }

    async fn raw_ask(
        &self,
        messages: Vec<Value>,
        tx: tokio::sync::mpsc::Sender<ChunkResult>,
    ) -> Vec<Value> {
        let mut payload = json!({
            "model": self.config.model,
            "messages": messages,
            "max_tokens": self.max_tokens(),
            "stream": true
        });
        if self.config.temperature != 1.0 {
            payload["temperature"] = json!(self.config.temperature);
        }
        self.apply_claude_thinking(&mut payload);
        if !self.config.extra_sys_prompt.is_empty() {
            payload["system"] = json!([{
                "type": "text",
                "text": self.config.extra_sys_prompt,
                "cache_control": {"type": "persistent"}
            }]);
        }

        let url = auto_make_url(&self.config.apibase, "messages");
        let mut req = self.client.post(&url).json(&payload);
        for (k, v) in self.build_headers() {
            req = req.header(k, v);
        }

        match req.send().await {
            Ok(resp) => {
                if resp.status() != 200 {
                    let status = resp.status();
                    let body = resp.text().await.unwrap_or_default();
                    let err = format!("!!!Error: HTTP {status} {}", &body[..body.len().min(500)]);
                    let _ = tx.send(Ok(err.clone())).await;
                    return vec![json!({"type": "text", "text": err})];
                }
                let stream = resp.bytes_stream();
                parse_claude_sse(stream, &tx).await
            }
            Err(e) => {
                let err = format!("!!!Error: {e}");
                let _ = tx.send(Ok(err.clone())).await;
                vec![json!({"type": "text", "text": err})]
            }
        }
    }
}

// ── OaiSession (LLMSession) ─────────────────────────────────────────────

pub struct OaiSession {
    pub config: LlmConfig,
    pub client: Client,
    pub history: Arc<Mutex<Vec<Value>>>,
    pub name: String,
}

impl OaiSession {
    pub fn new(cfg: LlmConfig) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(300))
            .danger_accept_invalid_certs(!cfg.verify)
            .build()
            .unwrap_or_default();
        let name = cfg.name.clone();
        let history = Arc::new(Mutex::new(Vec::new()));
        Self {
            config: cfg,
            client,
            history,
            name,
        }
    }
}

#[async_trait]
impl BaseSession for OaiSession {
    fn name(&self) -> &str {
        &self.name
    }
    fn model(&self) -> &str {
        &self.config.model.as_str()
    }
    fn history(&self) -> &Arc<Mutex<Vec<Value>>> {
        &self.history
    }
    fn context_win(&self) -> usize {
        self.config.context_win
    }

    fn make_messages(&self, raw_list: &[Value]) -> Vec<Value> {
        _msgs_claude2oai(raw_list)
    }

    async fn raw_ask(
        &self,
        messages: Vec<Value>,
        tx: tokio::sync::mpsc::Sender<ChunkResult>,
    ) -> Vec<Value> {
        let ml = self.config.model.to_lowercase();
        let api_mode = &self.config.api_mode;

        let headers: Vec<(String, String)> = vec![
            (
                "Authorization".into(),
                format!("Bearer {}", self.config.apikey),
            ),
            ("Content-Type".into(), "application/json".into()),
            ("Accept".into(), "text/event-stream".into()),
        ];

        let (url, mut payload) = if api_mode == "responses" {
            let u = auto_make_url(&self.config.apibase, "responses");
            let input = _to_responses_input(&messages);
            let mut p = json!({
                "model": self.config.model,
                "input": input,
                "stream": true,
                "prompt_cache_key": *RESP_CACHE_KEY
            });
            if !self.config.extra_sys_prompt.is_empty() {
                p["instructions"] = Value::String(self.config.extra_sys_prompt.clone());
            }
            if let Some(ref effort) = self.config.reasoning_effort {
                p["reasoning"] = json!({"effort": effort});
            }
            if let Some(mt) = self.config.max_tokens {
                p["max_output_tokens"] = json!(mt);
            }
            (u, p)
        } else {
            let u = auto_make_url(&self.config.apibase, "chat/completions");
            let mut msgs = messages.clone();
            if !self.config.extra_sys_prompt.is_empty() {
                msgs.insert(
                    0,
                    json!({"role": "system", "content": self.config.extra_sys_prompt}),
                );
            }
            _stamp_oai_cache_markers(&mut msgs, &self.config.model);
            let mut p = json!({
                "model": self.config.model,
                "messages": msgs,
                "stream": true,
                "stream_options": {"include_usage": true}
            });
            if self.config.temperature != 1.0 {
                p["temperature"] = json!(self.config.temperature);
            }
            if let Some(mt) = self.config.max_tokens {
                let key = if ml.starts_with("gpt-5")
                    || ml.starts_with("o1")
                    || ml.starts_with("o2")
                    || ml.starts_with("o3")
                    || ml.starts_with("o4")
                {
                    "max_completion_tokens"
                } else {
                    "max_tokens"
                };
                p[key] = json!(mt);
            }
            if let Some(effort) = normalize_chat_reasoning_effort(&self.config) {
                p["reasoning_effort"] = json!(effort);
            }
            (u, p)
        };

        if let Some(ref tier) = self.config.service_tier {
            payload["service_tier"] = json!(tier);
        }

        let max_retries = self.config.max_retries;
        let mut last_err = String::new();

        for attempt in 0..=max_retries {
            let mut req = self.client.post(&url).json(&payload);
            for (k, v) in &headers {
                req = req.header(k.as_str(), v.as_str());
            }
            req = req.timeout(Duration::from_secs(self.config.read_timeout.max(5)));

            match req.send().await {
                Ok(resp) => {
                    let status = resp.status();
                    if status.is_server_error() || RETRYABLE_STATUSES.contains(&status.as_u16()) {
                        if attempt < max_retries {
                            let delay = Duration::from_secs_f64(
                                (0.5f64).max(1.5 * (2u32.pow(attempt as u32) as f64)),
                            );
                            warn!(
                                "[LLM Retry] HTTP {}, retry in {:.1}s ({}/{})",
                                status,
                                delay.as_secs_f64(),
                                attempt + 1,
                                max_retries + 1
                            );
                            tokio::time::sleep(delay).await;
                            continue;
                        }
                    }
                    if status.is_client_error() || status.is_server_error() {
                        let body = resp.text().await.unwrap_or_default();
                        last_err =
                            format!("!!!Error: HTTP {} {}", status, &body[..body.len().min(500)]);
                        let _ = tx.send(Ok(last_err.clone())).await;
                        return vec![json!({"type": "text", "text": last_err})];
                    }
                    let stream = resp.bytes_stream();
                    return parse_openai_sse(stream, &tx, api_mode).await;
                }
                Err(e) => {
                    if attempt < max_retries {
                        let delay_s = 0.5f64.max(1.5 * (2u64.pow(attempt as u32) as f64));
                        let delay = Duration::from_secs_f64(delay_s);
                        warn!(
                            "[LLM Retry] {}, retry in {:.1}s ({}/{})",
                            e,
                            delay.as_secs_f64(),
                            attempt + 1,
                            max_retries + 1
                        );
                        tokio::time::sleep(delay).await;
                        continue;
                    }
                    last_err = format!("!!!Error: {e}");
                    let _ = tx.send(Ok(last_err.clone())).await;
                    return vec![json!({"type": "text", "text": last_err})];
                }
            }
        }

        let _ = tx.send(Ok(last_err.clone())).await;
        vec![json!({"type": "text", "text": last_err})]
    }
}

fn _to_responses_input(messages: &[Value]) -> Value {
    let mut result: Vec<Value> = Vec::new();
    let mut pending: Vec<String> = Vec::new();

    for msg in messages {
        let role = msg
            .get("role")
            .and_then(|r| r.as_str())
            .unwrap_or("user")
            .to_lowercase();

        if role == "tool" {
            let cid = msg
                .get("tool_call_id")
                .and_then(|v| v.as_str())
                .map(String::from)
                .or_else(|| pending.pop())
                .unwrap_or_else(|| {
                    format!(
                        "call_{}",
                        Uuid::new_v4()
                            .to_string()
                            .chars()
                            .take(8)
                            .collect::<String>()
                    )
                });
            result.push(json!({
                "type": "function_call_output",
                "call_id": cid,
                "output": msg.get("content").and_then(|v| v.as_str()).unwrap_or("")
            }));
            continue;
        }

        let mut effective_role = role.clone();
        if effective_role != "user"
            && effective_role != "assistant"
            && effective_role != "developer"
        {
            effective_role = "user".into();
        }
        if effective_role == "system" {
            effective_role = "developer".into();
        }

        let content = msg
            .get("content")
            .cloned()
            .unwrap_or(Value::String(String::new()));
        let text_type = if effective_role == "assistant" {
            "output_text"
        } else {
            "input_text"
        };

        let mut parts: Vec<Value> = Vec::new();
        if let Some(s) = content.as_str() {
            if !s.is_empty() {
                parts.push(json!({"type": text_type, "text": s}));
            }
        } else if let Some(arr) = content.as_array() {
            for part in arr {
                if !part.is_object() {
                    continue;
                }
                let pt = part.get("type").and_then(|v| v.as_str()).unwrap_or("");
                match pt {
                    "text" => {
                        if let Some(txt) = part.get("text").and_then(|v| v.as_str()) {
                            if !txt.is_empty() {
                                parts.push(json!({"type": text_type, "text": txt}));
                            }
                        }
                    }
                    "image_url" => {
                        let url = part
                            .get("image_url")
                            .and_then(|iu| iu.get("url"))
                            .and_then(|v| v.as_str())
                            .unwrap_or("");
                        if !url.is_empty() && effective_role != "assistant" {
                            parts.push(json!({"type": "input_image", "image_url": url}));
                        }
                    }
                    _ => {}
                }
            }
        }
        if parts.is_empty() {
            let fallback = if content.is_array() {
                "[empty]".to_string()
            } else {
                content.as_str().unwrap_or(&content.to_string()).to_string()
            };
            parts.push(json!({"type": text_type, "text": fallback}));
        }
        result.push(json!({"role": effective_role, "content": parts}));
        pending.clear();

        if let Some(tool_calls) = msg.get("tool_calls").and_then(|v| v.as_array()) {
            for tc in tool_calls {
                let f = tc.get("function");
                let cid = tc
                    .get("id")
                    .and_then(|v| v.as_str())
                    .map(String::from)
                    .unwrap_or_else(|| {
                        format!(
                            "call_{}",
                            Uuid::new_v4()
                                .to_string()
                                .chars()
                                .take(8)
                                .collect::<String>()
                        )
                    });
                pending.push(cid.clone());
                result.push(json!({
                    "type": "function_call",
                    "call_id": cid,
                    "name": f.and_then(|f| f.get("name")).and_then(|v| v.as_str()).unwrap_or(""),
                    "arguments": f.and_then(|f| f.get("arguments")).and_then(|v| v.as_str()).unwrap_or("")
                }));
            }
        }
    }
    Value::Array(result)
}

// ── NativeClaudeSession ─────────────────────────────────────────────────

pub struct NativeClaudeSession {
    pub config: LlmConfig,
    pub client: Client,
    pub history: Arc<Mutex<Vec<Value>>>,
    pub name: String,
    pub tools: Arc<Mutex<Option<Vec<ToolSchema>>>>,
    pub system: Arc<Mutex<String>>,
    device_id: String,
    account_uuid: String,
    session_id: String,
}

impl NativeClaudeSession {
    pub fn new(cfg: LlmConfig) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(300))
            .danger_accept_invalid_certs(!cfg.verify)
            .build()
            .unwrap_or_default();
        let name = cfg.name.clone();
        let raw_uuid = || Uuid::new_v4().to_string().replace('-', "");
        let device_id = format!("{}{}", raw_uuid(), &raw_uuid()[..32]);
        Self {
            config: cfg,
            client,
            history: Arc::new(Mutex::new(Vec::new())),
            name,
            tools: Arc::new(Mutex::new(None)),
            system: Arc::new(Mutex::new(String::new())),
            device_id,
            account_uuid: Uuid::new_v4().to_string(),
            session_id: Uuid::new_v4().to_string(),
        }
    }

    fn max_tokens(&self) -> usize {
        self.config.max_tokens.unwrap_or(8192)
    }

    fn build_headers(&self) -> Vec<(String, String)> {
        let mut beta_parts = vec![
            CLAUDE_CODE_BETA,
            INTERTHINK_BETA,
            REDACT_BETA,
            CACHE_SCOPE_BETA,
        ];
        let model = self.config.model.clone();
        if model.to_lowercase().contains("[1m]") {
            beta_parts.insert(1, CONTEXT_1M_BETA);
        }
        let mut headers = vec![
            ("Content-Type".into(), "application/json".into()),
            ("anthropic-version".into(), ANTHROPIC_VERSION.into()),
            (
                "anthropic-dangerous-direct-browser-access".into(),
                "true".into(),
            ),
            (
                "user-agent".into(),
                "claude-cli/2.1.113 (external, cli)".into(),
            ),
            ("x-app".into(), "cli".into()),
            ("anthropic-beta".into(), beta_parts.join(",")),
        ];

        if self.config.apikey.starts_with("sk-ant-") {
            headers.push(("x-api-key".into(), self.config.apikey.clone()));
        } else {
            headers.push((
                "authorization".into(),
                format!("Bearer {}", self.config.apikey),
            ));
        }
        headers
    }

    pub async fn set_tools(&self, tools: Vec<ToolSchema>) {
        *self.tools.lock().await = Some(tools);
    }

    pub async fn set_system(&self, sys: String) {
        *self.system.lock().await = sys;
    }
}

#[async_trait]
impl BaseSession for NativeClaudeSession {
    fn name(&self) -> &str {
        &self.name
    }
    fn model(&self) -> &str {
        &self.config.model.as_str()
    }
    fn history(&self) -> &Arc<Mutex<Vec<Value>>> {
        &self.history
    }
    fn context_win(&self) -> usize {
        self.config.context_win
    }

    fn make_messages(&self, raw_list: &[Value]) -> Vec<Value> {
        let msgs: Vec<Value> = raw_list
            .iter()
            .map(|m| {
                let role = m.get("role").and_then(|r| r.as_str()).unwrap_or("user");
                let content = m.get("content").cloned().unwrap_or(Value::Null);
                json!({"role": role, "content": content})
            })
            .collect();
        let msgs = _drop_unsigned_thinking(&msgs);
        let msgs = _fix_messages(&msgs);
        let mut msgs = _ensure_thinking_blocks(&msgs, &self.config.model);
        _stamp_claude_cache(&mut msgs);
        msgs
    }

    async fn raw_ask(
        &self,
        messages: Vec<Value>,
        tx: tokio::sync::mpsc::Sender<ChunkResult>,
    ) -> Vec<Value> {
        let ml = self.config.model.to_lowercase();
        let model = if ml.contains("[1m]") || ml.contains("[1M]") {
            self.config.model.replace("[1m]", "").replace("[1M]", "")
        } else {
            self.config.model.clone()
        };
        let mut payload = json!({
            "model": model.trim(),
            "messages": messages,
            "max_tokens": self.max_tokens(),
            "stream": true
        });
        if self.config.temperature != 1.0 {
            payload["temperature"] = json!(self.config.temperature);
        }

        Self::apply_claude_thinking_inner(&self.config, &mut payload);

        payload["metadata"] = json!({
            "user_id": serde_json::to_string(&json!({
                "device_id": self.device_id,
                "account_uuid": self.account_uuid,
                "session_id": self.session_id
            })).unwrap_or_default()
        });

        let tools_guard = self.tools.lock().await;
        if let Some(ref tools) = *tools_guard {
            let mut claude_tools = openai_tools_to_claude(tools);
            if let Some(last) = claude_tools.last_mut() {
                if let Some(obj) = last.as_object_mut() {
                    obj.insert("cache_control".into(), json!({"type": "ephemeral"}));
                }
            }
            payload["tools"] = Value::Array(claude_tools);
        } else {
            warn!("[ERROR] No tools provided for this session.");
        }
        drop(tools_guard);

        let system_guard = self.system.lock().await;
        if !system_guard.is_empty() {
            payload["system"] = json!([{"type": "text", "text": &*system_guard}]);
        }
        drop(system_guard);

        let url = format!(
            "{}?beta=true",
            auto_make_url(&self.config.apibase, "messages")
        );

        let mut req = self.client.post(&url).json(&payload);
        for (k, v) in self.build_headers() {
            req = req.header(k, v);
        }

        match req.send().await {
            Ok(resp) => {
                if resp.status() != 200 {
                    let status = resp.status();
                    let body = resp.text().await.unwrap_or_default();
                    let err = format!("!!!Error: HTTP {status} {}", &body[..body.len().min(500)]);
                    let _ = tx.send(Ok(err.clone())).await;
                    return vec![json!({"type": "text", "text": err})];
                }
                let stream = resp.bytes_stream();
                parse_claude_sse(stream, &tx).await
            }
            Err(e) => {
                let err = format!("!!!Error: {e}");
                let _ = tx.send(Ok(err.clone())).await;
                vec![json!({"type": "text", "text": err})]
            }
        }
    }
}

impl NativeClaudeSession {
    fn apply_claude_thinking_inner(cfg: &LlmConfig, payload: &mut Value) {
        if let Some(ref tt) = cfg.thinking_type {
            let thinking_type = tt.to_lowercase();
            if thinking_type == "enabled" {
                if let Some(budget) = cfg.thinking_budget_tokens {
                    payload["thinking"] = json!({
                        "type": "enabled",
                        "budget_tokens": budget
                    });
                } else {
                    warn!("thinking_type='enabled' requires thinking_budget_tokens, ignored.");
                }
            } else {
                payload["thinking"] = json!({"type": thinking_type});
            }
        }
        if let Some(ref effort) = cfg.reasoning_effort {
            let effort_val = match effort.to_lowercase().as_str() {
                "low" => "low",
                "medium" => "medium",
                "high" => "high",
                "xhigh" => "max",
                _ => {
                    warn!("reasoning_effort '{effort}' is unsupported for Claude output_config.effort, ignored.");
                    return;
                }
            };
            payload["output_config"] = json!({"effort": effort_val});
        }
    }
}

// ── NativeOaiSession ────────────────────────────────────────────────────

pub struct NativeOaiSession {
    pub inner: OaiSession,
}

impl NativeOaiSession {
    pub fn new(cfg: LlmConfig) -> Self {
        Self {
            inner: OaiSession::new(cfg),
        }
    }
}

#[async_trait]
impl BaseSession for NativeOaiSession {
    fn name(&self) -> &str {
        self.inner.name()
    }
    fn model(&self) -> &str {
        self.inner.model()
    }
    fn history(&self) -> &Arc<Mutex<Vec<Value>>> {
        self.inner.history()
    }
    fn context_win(&self) -> usize {
        self.inner.context_win()
    }

    fn make_messages(&self, raw_list: &[Value]) -> Vec<Value> {
        let msgs: Vec<Value> = raw_list
            .iter()
            .map(|m| {
                let role = m.get("role").and_then(|r| r.as_str()).unwrap_or("user");
                let content = m.get("content").cloned().unwrap_or(Value::Null);
                json!({"role": role, "content": content})
            })
            .collect();
        let msgs = _fix_messages(&msgs);
        let msgs = _ensure_thinking_blocks(&msgs, &self.inner.config.model);
        _msgs_claude2oai(&msgs)
    }

    async fn raw_ask(
        &self,
        messages: Vec<Value>,
        tx: tokio::sync::mpsc::Sender<ChunkResult>,
    ) -> Vec<Value> {
        self.inner.raw_ask(messages, tx).await
    }
}

// ── ToolClient ────────────────────────────────────────────────────────────

pub struct ToolClient {
    pub backend: Arc<dyn BaseSession>,
    pub auto_save_tokens: bool,
    pub last_tools: String,
    pub total_cd_tokens: usize,
}

impl ToolClient {
    pub fn new(backend: Arc<dyn BaseSession>, auto_save_tokens: bool) -> Self {
        Self {
            backend,
            auto_save_tokens,
            last_tools: String::new(),
            total_cd_tokens: 0,
        }
    }

    pub async fn chat(
        &mut self,
        messages: Vec<Value>,
        tools: &[ToolSchema],
    ) -> (ChunkStream, Arc<Mutex<Option<Vec<Value>>>>, Vec<Value>) {
        let full_prompt = self.build_protocol_prompt(&messages, tools);
        debug!("Full prompt length: {} chars", full_prompt.len());
        let (stream, result) = session_chat(self.backend.clone(), &full_prompt).await;
        // Parse will happen after stream consumption; return blocks for caller
        (stream, result, messages)
    }

    fn build_protocol_prompt(&mut self, messages: &[Value], tools: &[ToolSchema]) -> String {
        let system_content = messages
            .iter()
            .find(|m| {
                m.get("role")
                    .and_then(|r| r.as_str())
                    .map(|r| r == "system")
                    .unwrap_or(false)
            })
            .and_then(|m| m.get("content").and_then(|v| v.as_str()))
            .unwrap_or("");
        let history_msgs: Vec<&Value> = messages
            .iter()
            .filter(|m| {
                m.get("role")
                    .and_then(|r| r.as_str())
                    .map(|r| r != "system")
                    .unwrap_or(true)
            })
            .collect();
        let tool_instruction = self.prepare_tool_instruction(tools);
        let mut prompt = String::new();
        if !system_content.is_empty() {
            prompt.push_str(&format!("{}\n", system_content));
        }
        prompt.push_str(&tool_instruction);

        for m in history_msgs {
            let role = if m.get("role").and_then(|r| r.as_str()).unwrap_or("") == "user" {
                "USER"
            } else {
                "ASSISTANT"
            };
            prompt.push_str(&format!("=== {} ===\n", role));
            if let Some(trs) = m.get("tool_results").and_then(|v| v.as_array()) {
                for tr in trs {
                    if let Some(content) = tr.get("content").and_then(|v| v.as_str()) {
                        prompt.push_str(&format!("<tool_result>{}</tool_result>\n", content));
                    }
                }
            }
            let c = m
                .get("content")
                .cloned()
                .unwrap_or(Value::String(String::new()));
            prompt.push_str(&format!("{}\n", c.as_str().unwrap_or(&c.to_string())));
        }
        prompt.push_str("=== ASSISTANT ===\n");
        prompt
    }

    fn prepare_tool_instruction(&mut self, tools: &[ToolSchema]) -> String {
        if tools.is_empty() {
            return String::new();
        }
        let tools_json = serde_json::to_string(tools).unwrap_or_default();
        let is_en = std::env::var("GA_LANG").unwrap_or_default().to_lowercase() == "en";

        let header = if is_en {
            "### Interaction Protocol (must follow strictly, always in effect)\n\
             Follow these steps to think and act:\n\
             1. **Think**: Analyze the current situation and strategy inside `<thinking>` tags.\n\
             2. **Summarize**: Output a minimal one-line (<30 words) physical snapshot in `<summary>`: \
             new info from last tool result + current tool call intent. This goes into long-term working memory. \
             Must contain real information, no filler.\n\
             3. **Act**: If you need to call tools, output one or more **<tool_use> blocks** after your reply, then stop.\n"
        } else {
            "### 交互协议 (必须严格遵守，持续有效)\n\
             请按照以下步骤思考并行动：\n\
             1. **思考**: 在 `<thinking>` 标签中先进行思考，分析现状和策略。\n\
             2. **总结**: 在 `<summary>` 中输出*极为简短*的高度概括的单行（<30字）物理快照，\
             包括上次工具调用结果产生的新信息+本次工具调用意图。此内容将进入长期工作记忆，\
             记录关键信息，严禁输出无实际信息增量的描述。\n\
             3. **行动**: 如需调用工具，请在回复正文之后输出一个（或多个）**<tool_use>块**，然后结束。\n"
        };

        if self.auto_save_tokens && self.last_tools == tools_json {
            let cached = if is_en {
                "\n### Tools: still active, **ready to call**. Protocol unchanged.\n"
            } else {
                "\n### 工具库状态：持续有效（code_run/file_read等），**可正常调用**。调用协议沿用。\n"
            };
            return cached.to_string();
        }

        self.total_cd_tokens = 0;
        self.last_tools = tools_json.clone();

        format!(
            "{}\nFormat: ```<tool_use>{{\"name\": \"tool_name\", \"arguments\": {{...}}}}</tool_use>```\n\n\
             ### Tools (mounted, always in effect):\n{}\n",
            header, tools_json
        )
    }

    pub fn parse_mixed_response(&mut self, raw_text: &str) -> LlmResponse {
        let mut remaining = raw_text.to_string();
        let mut thinking = String::new();

        if let Some(caps) = THINK_TAG_RE.captures(&remaining) {
            thinking = caps
                .get(1)
                .map(|m| m.as_str().to_string())
                .unwrap_or_default();
            remaining = THINK_TAG_RE.replace_all(&remaining, "").to_string();
        }

        let mut tool_calls: Vec<ToolCall> = Vec::new();
        let mut json_strs: Vec<String> = Vec::new();
        let mut errors: Vec<String> = Vec::new();

        // (?s) enables dot-all mode so `.` matches newlines — required for multi-line <tool_use> blocks
        let tool_re = Regex::new(
            r"(?s)<(?:tool_use|tool_call)>([\s\S]{15,}?)</_?(?:tool_use|tool_call)>",
        )
        .unwrap();
        let tool_all: Vec<String> = tool_re
            .captures_iter(&remaining)
            .map(|c| c[1].to_string().trim().to_string())
            .collect();

        if !tool_all.is_empty() {
            for s in tool_all {
                if s.starts_with('{') && s.ends_with('}') {
                    json_strs.push(s);
                }
            }
            remaining = tool_re.replace_all(&remaining, "").to_string();
        } else if remaining.contains("<tool_use>") {
            // Fallback: extract content between <tool_use> and </tool_use> (or end of string)
            let after_tag = remaining.split("<tool_use>").last().unwrap_or("");
            let weak = if let Some(close_pos) = after_tag
                .find("</tool_use>")
                .or_else(|| after_tag.find("</_tool_use>"))
            {
                after_tag[..close_pos].trim().to_string()
            } else {
                after_tag.trim().to_string()
            };
            if weak.starts_with('{') && weak.ends_with('}') {
                json_strs.push(weak.clone());
            } else if let Some(end) = weak.find("```") {
                let candidate = weak[..end].trim().to_string();
                if candidate.ends_with('}') {
                    json_strs.push(candidate);
                }
            }
            if !weak.is_empty() {
                let tool_body = after_tag
                    .split("</tool_use>")
                    .next()
                    .unwrap_or(after_tag.split("</_tool_use>").next().unwrap_or(""));
                remaining = remaining.replace(&format!("<tool_use>{}", tool_body), "");
                remaining = remaining.replace("</tool_use>", "");
                remaining = remaining.replace("</_tool_use>", "");
            }
        } else if remaining.contains("\"name\":") && remaining.contains("\"arguments\":") {
            let json_re = Regex::new("(?s)\\{.*\"name\".*\\}").unwrap();
            if let Some(caps) = json_re.captures(&remaining) {
                let s = caps.get(0).unwrap().as_str().to_string();
                json_strs.push(s.clone());
                remaining = remaining.replace(&s, "").trim().to_string();
            }
        }

        if remaining.contains("<｜｜DSML｜｜tool_calls>") {
            tool_calls.extend(parse_dsml_tool_calls(&remaining));
            remaining = strip_dsml_protocol_tags(&remaining);
        }

        for json_str in &json_strs {
            match tryparse_json(json_str) {
                Ok(data) => {
                    let func_name = data
                        .get("name")
                        .or(data.get("function"))
                        .or(data.get("tool"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let args = data
                        .get("arguments")
                        .or(data.get("args"))
                        .or(data.get("params"))
                        .or(data.get("parameters"))
                        .cloned()
                        .unwrap_or(data.clone());
                    if !func_name.is_empty() {
                        tool_calls.push(ToolCall {
                            id: String::new(),
                            name: func_name.to_string(),
                            arguments: args,
                        });
                    }
                }
                Err(e) => {
                    let err_msg =
                        format!("[Warn] Failed to parse tool_use JSON: {} - {}", json_str, e);
                    errors.push(err_msg.clone());
                    self.last_tools.clear();
                }
            }
        }

        if tool_calls.is_empty() {
            for e in &errors {
                warn!("{}", e);
            }
        }

        let content = sanitize_protocol_text(&remaining);

        let has_tools = !tool_calls.is_empty();
        LlmResponse {
            thinking,
            content,
            tool_calls,
            raw: raw_text.to_string(),
            stop_reason: if has_tools {
                "tool_use".into()
            } else {
                "end_turn".into()
            },
            usage: None,
        }
    }
}

// ── NativeToolClient ─────────────────────────────────────────────────────

pub struct NativeToolClient {
    pub backend: Arc<NativeClaudeSession>,
    pending_tool_ids: Vec<String>,
}

impl NativeToolClient {
    const THINKING_PROMPT_EN: &'static str = "\
### Action Protocol (always in effect)\n\
The reply body should first include a minimal one-line (<30 words) physical snapshot in <summary></summary>: \
new info from last result + current intent. This goes into long-term working memory.\n\
**If the user's request is not yet complete, tool calls are required!**\n";

    const THINKING_PROMPT_ZH: &'static str = "\
### 行动规范（持续有效）\n\
每次回复（含工具调用轮）都先在回复文字中包含一个<summary></summary> 中输出极简单行（<30字）物理快照：上次结果新信息+本次意图。此内容进入长期工作记忆。\n\
**若用户需求未完成，必须进行工具调用！**\n";

    pub fn new(backend: Arc<NativeClaudeSession>) -> Self {
        Self {
            backend,
            pending_tool_ids: Vec::new(),
        }
    }

    pub fn thinking_prompt() -> &'static str {
        if std::env::var("GA_LANG").unwrap_or_default().to_lowercase() == "en" {
            Self::THINKING_PROMPT_EN
        } else {
            Self::THINKING_PROMPT_ZH
        }
    }

    pub async fn chat(
        &mut self,
        messages: Vec<Value>,
        tools: &[ToolSchema],
    ) -> (ChunkStream, Arc<Mutex<Option<Vec<Value>>>>) {
        if !tools.is_empty() {
            self.backend.set_tools(tools.to_vec()).await;
        }

        let mut combined_content: Vec<Value> = Vec::new();
        let mut tool_results: Vec<Value> = Vec::new();
        let mut extra_sys = String::new();

        for msg in &messages {
            let role = msg.get("role").and_then(|r| r.as_str()).unwrap_or("");
            let c = msg
                .get("content")
                .cloned()
                .unwrap_or(Value::String(String::new()));

            if role == "system" {
                extra_sys = c.as_str().unwrap_or("").to_string();
                continue;
            }

            if let Some(s) = c.as_str() {
                combined_content.push(json!({"type": "text", "text": s}));
            } else if let Some(arr) = c.as_array() {
                combined_content.extend(arr.clone());
            }

            if role == "user" {
                if let Some(trs) = msg.get("tool_results").and_then(|v| v.as_array()) {
                    tool_results.extend(trs.clone());
                }
            }
        }

        self.set_system(extra_sys).await;

        let tr_id_set: std::collections::HashSet<String> = tool_results
            .iter()
            .filter_map(|tr| {
                tr.get("tool_use_id")
                    .and_then(|v| v.as_str())
                    .map(String::from)
            })
            .collect();

        let mut tool_result_blocks: Vec<Value> = Vec::new();
        for tr in &tool_results {
            let tid = tr.get("tool_use_id").and_then(|v| v.as_str()).unwrap_or("");
            if !tid.is_empty() {
                tool_result_blocks.push(json!({
                    "type": "tool_result",
                    "tool_use_id": tid,
                    "content": tr.get("content").and_then(|v| v.as_str()).unwrap_or("")
                }));
            } else {
                let tc = tr.get("content").and_then(|v| v.as_str()).unwrap_or("");
                combined_content.insert(
                    0,
                    json!({"type": "text", "text": format!("<tool_result>{}</tool_result>", tc)}),
                );
            }
        }

        for tid in &self.pending_tool_ids {
            if !tr_id_set.contains(tid) {
                tool_result_blocks.push(json!({
                    "type": "tool_result",
                    "tool_use_id": tid,
                    "content": ""
                }));
            }
        }
        self.pending_tool_ids.clear();

        let mut merged_content: Vec<Value> = tool_result_blocks;
        merged_content.extend(combined_content);
        let merged = json!({
            "role": "user",
            "content": merged_content
        });

        // Use session_chat via Arc<dyn BaseSession>
        let prompt_str = serde_json::to_string(&merged).unwrap_or_default();
        let backend: Arc<dyn BaseSession> = self.backend.clone();
        session_chat(backend, &prompt_str).await
    }

    async fn set_system(&mut self, extra_system: String) {
        let combined = if extra_system.is_empty() {
            Self::thinking_prompt().to_string()
        } else {
            format!("{}\n\n{}", extra_system, Self::thinking_prompt())
        };
        debug!("Updated system prompt, length {} chars.", combined.len());
        self.backend.set_system(combined).await;
    }
}

// ── MixinSession ─────────────────────────────────────────────────────────

pub struct MixinSession {
    pub name: String,
    pub cur_idx: Arc<Mutex<usize>>,
    pub switched_at: Arc<Mutex<f64>>,
    pub spring_sec: f64,
    pub max_retries: usize,
    pub base_delay: f64,
}

impl MixinSession {
    pub fn new(
        all_sessions: &[Arc<dyn BaseSession>],
        indices: &[usize],
        max_retries: usize,
        base_delay: f64,
        spring_sec: f64,
    ) -> Self {
        let name = indices
            .iter()
            .map(|&i| all_sessions[i].name().to_string())
            .collect::<Vec<_>>()
            .join("|");

        Self {
            name,
            cur_idx: Arc::new(Mutex::new(0)),
            switched_at: Arc::new(Mutex::new(0.0)),
            spring_sec,
            max_retries,
            base_delay,
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }
}

// ── Response helpers ─────────────────────────────────────────────────────

pub fn blocks_to_response(blocks: &[Value]) -> LlmResponse {
    let thinking = blocks
        .iter()
        .filter_map(|b| {
            if b.get("type").and_then(|v| v.as_str()) == Some("thinking") {
                b.get("thinking").and_then(|v| v.as_str())
            } else {
                None
            }
        })
        .collect::<Vec<_>>()
        .join("\n");

    let content = blocks
        .iter()
        .filter_map(|b| {
            if b.get("type").and_then(|v| v.as_str()) == Some("text") {
                b.get("text").and_then(|v| v.as_str())
            } else {
                None
            }
        })
        .collect::<Vec<_>>()
        .join("\n");

    let tool_calls: Vec<ToolCall> = blocks
        .iter()
        .filter_map(|b| {
            if b.get("type").and_then(|v| v.as_str()) == Some("tool_use") {
                Some(ToolCall {
                    id: b
                        .get("id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    name: b
                        .get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    arguments: b.get("input").cloned().unwrap_or(Value::Null),
                })
            } else {
                None
            }
        })
        .collect();

    let stop_reason = if tool_calls.is_empty() {
        "end_turn".into()
    } else {
        "tool_use".into()
    };

    LlmResponse {
        thinking,
        content,
        tool_calls,
        raw: serde_json::to_string(blocks).unwrap_or_default(),
        stop_reason,
        usage: None,
    }
}

pub fn parse_text_tool_calls(content: &str) -> (Vec<ToolCall>, String) {
    let mut tcs = Vec::new();
    let remaining = content.to_string();

    // Try JSON array format: [{"type":"tool_use", "name":..., "input":...}]
    for marker in &["[{\"type\":\"tool_use\"", "[{\"type\": \"tool_use\""] {
        if let Some(pos) = remaining.find(marker) {
            if remaining.ends_with("}]") {
                if let Ok(raw) = serde_json::from_str::<Vec<Value>>(&remaining[pos..]) {
                    tcs = raw
                        .iter()
                        .filter(|b| b.get("type").and_then(|v| v.as_str()) == Some("tool_use"))
                        .map(|b| ToolCall {
                            id: b
                                .get("id")
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                                .to_string(),
                            name: b
                                .get("name")
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                                .to_string(),
                            arguments: b.get("input").cloned().unwrap_or(Value::Null),
                        })
                        .collect();
                    return (tcs, sanitize_protocol_text(&remaining[..pos]));
                }
            }
        }
    }

    // Try XML tags — (?s) enables dot-all so multi-line blocks are matched
    let tool_re = Regex::new(
        r"(?s)<(?:tool_use|tool_call)>([\s\S]{15,}?)</_?(?:tool_use|tool_call)>",
    )
    .unwrap();
    for caps in tool_re.captures_iter(&remaining) {
        let s = caps.get(1).unwrap().as_str().trim();
        if let Ok(d) = tryparse_json(s) {
            let name = d.get("name").and_then(|v| v.as_str()).unwrap_or("");
            let args = d
                .get("arguments")
                .or(d.get("args"))
                .or(d.get("input"))
                .cloned()
                .unwrap_or(Value::Null);
            if !name.is_empty() {
                tcs.push(ToolCall {
                    id: String::new(),
                    name: name.to_string(),
                    arguments: args,
                });
            }
        }
    }

    let legacy_tool_re = Regex::new(
        r"(?s)<([a-z][a-z0-9_]*_[a-z0-9_]*)>([\s\S]{2,}?)</([a-z][a-z0-9_]*_[a-z0-9_]*)>",
    )
    .unwrap();
    for caps in legacy_tool_re.captures_iter(&remaining) {
        let open = caps.get(1).unwrap().as_str();
        let close = caps.get(3).unwrap().as_str();
        if open != close || !is_legacy_tool_tag(open) {
            continue;
        }
        let s = caps.get(2).unwrap().as_str().trim();
        if let Ok(d) = tryparse_json(s) {
            let args = d
                .get("arguments")
                .or(d.get("args"))
                .or(d.get("input"))
                .cloned()
                .unwrap_or(d);
            tcs.push(ToolCall {
                id: String::new(),
                name: open.to_string(),
                arguments: args,
            });
        }
    }

    tcs.extend(parse_dsml_tool_calls(&remaining));

    let remaining = tool_re.replace_all(&remaining, "").to_string();
    (tcs, sanitize_protocol_text(&remaining))
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;
    use tokio::sync::mpsc;

    #[test]
    fn test_auto_make_url() {
        assert_eq!(
            auto_make_url("https://api.anthropic.com", "messages"),
            "https://api.anthropic.com/v1/messages"
        );
        assert_eq!(
            auto_make_url("https://api.anthropic.com/v1", "messages"),
            "https://api.anthropic.com/v1/messages"
        );
    }

    #[test]
    fn test_tryparse_json() {
        let v = tryparse_json(r#"{"name":"test"}"#).unwrap();
        assert_eq!(v["name"], "test");

        let v = tryparse_json(
            r#"```json
{"name":"test"}
```"#,
        )
        .unwrap();
        assert_eq!(v["name"], "test");
    }

    #[test]
    fn test_sanitize_protocol_text_removes_internal_tags() {
        let raw = "<summary>Inspect README</summary>\nAnswer body\n<tool_use>{\"name\":\"file_read\",\"arguments\":{\"path\":\"README.md\"}}</tool_use>";
        assert_eq!(sanitize_protocol_text(raw), "Answer body");
    }

    #[test]
    fn test_parse_text_tool_calls_strips_summary_from_visible_content() {
        let raw = "<summary>Read local README</summary>\nVisible answer\n<tool_use>{\"name\":\"file_read\",\"arguments\":{\"path\":\"README.md\"}}</tool_use>";
        let (tool_calls, content) = parse_text_tool_calls(raw);
        assert_eq!(tool_calls.len(), 1);
        assert_eq!(content, "Visible answer");
    }

    #[test]
    fn test_sanitize_protocol_text_removes_unclosed_tool_block() {
        let raw = "Working\n<tool_use>{\"name\":\"file_read\",\"arguments\":{\"path\":\"README.md\"}}";
        assert_eq!(sanitize_protocol_text(raw), "Working");
    }

    #[test]
    fn test_sanitize_protocol_text_removes_malformed_tool_close_tag() {
        let raw = "Working\n<tool_use>{\"name\":\"file_read\",\"arguments\":{\"path\":\"README.md\"}}</_tool_use>";
        assert_eq!(sanitize_protocol_text(raw), "Working");
    }

    #[test]
    fn test_sanitize_protocol_text_removes_legacy_tool_and_file_content_tags() {
        let raw = "Visible answer\n<file_write>{\"path\":\"README.md\",\"mode\":\"overwrite\"}</file_write>\n<file_content>Hello</file_content>";
        assert_eq!(sanitize_protocol_text(raw), "Visible answer");
    }

    #[test]
    fn test_parse_text_tool_calls_supports_legacy_tool_tags() {
        let raw = "Visible answer\n<file_write>{\"path\":\"README.md\",\"mode\":\"overwrite\"}</file_write>\n<file_content>Hello</file_content>";
        let (tool_calls, content) = parse_text_tool_calls(raw);
        assert_eq!(tool_calls.len(), 1);
        assert_eq!(tool_calls[0].name, "file_write");
        assert_eq!(tool_calls[0].arguments["path"], "README.md");
        assert_eq!(content, "Visible answer");
    }

    #[test]
    fn test_sanitize_protocol_text_removes_dsml_tool_calls() {
        let raw = "Visible answer\n<｜｜DSML｜｜tool_calls><｜｜DSML｜｜invoke name=\"code_run\"><｜｜DSML｜｜parameter name=\"command\" string=\"true\">echo ok</｜｜DSML｜｜parameter></｜｜DSML｜｜invoke></｜｜DSML｜｜tool_calls>";
        assert_eq!(sanitize_protocol_text(raw), "Visible answer");
    }

    #[test]
    fn test_parse_text_tool_calls_supports_dsml_tool_calls() {
        let raw = "Visible answer\n<｜｜DSML｜｜tool_calls>\n<｜｜DSML｜｜invoke name=\"code_run\">\n<｜｜DSML｜｜parameter name=\"command\" string=\"true\">echo ok</｜｜DSML｜｜parameter>\n<｜｜DSML｜｜parameter name=\"timeout\" string=\"false\">1000</｜｜DSML｜｜parameter>\n</｜｜DSML｜｜invoke>\n</｜｜DSML｜｜tool_calls>";
        let (tool_calls, content) = parse_text_tool_calls(raw);
        assert_eq!(tool_calls.len(), 1);
        assert_eq!(tool_calls[0].name, "code_run");
        assert_eq!(tool_calls[0].arguments["command"], "echo ok");
        assert_eq!(tool_calls[0].arguments["timeout"], 1000);
        assert_eq!(content, "Visible answer");
    }

    #[test]
    fn test_parse_text_tool_calls_supports_malformed_tool_close_tag() {
        let raw = "Visible answer\n<tool_use>{\"name\":\"file_read\",\"arguments\":{\"path\":\"README.md\"}}</_tool_use>";
        let (tool_calls, content) = parse_text_tool_calls(raw);
        assert_eq!(tool_calls.len(), 1);
        assert_eq!(tool_calls[0].name, "file_read");
        assert_eq!(tool_calls[0].arguments["path"], "README.md");
        assert_eq!(content, "Visible answer");
    }

    #[test]
    fn test_compress_history_tags_handles_history_and_key_info_blocks() {
        let mut messages = vec![json!({
            "role": "assistant",
            "content": "<history>older context</history>\n<key_info>secret</key_info>\n<thinking level=\"high\">step</thinking>"
        })];

        compress_history_tags(&mut messages, 0, 400, true);

        let content = messages[0]["content"].as_str().unwrap_or("");
        assert!(content.contains("<history>[...]</history>"));
        assert!(content.contains("<key_info>[...]</key_info>"));
        assert!(content.contains("<thinking>step</thinking>"));
    }

    #[test]
    fn test_auto_make_url_terminator() {
        assert_eq!(
            auto_make_url("https://api.anthropic.com/v1/$", "messages"),
            "https://api.anthropic.com/v1"
        );
    }

    #[test]
    fn test_extract_responses_message_text() {
        let item = json!({
            "type": "message",
            "content": [
                {"type": "output_text", "text": "Hel"},
                {"type": "output_text", "text": "lo!"}
            ]
        });
        assert_eq!(extract_responses_message_text(&item), "Hello!");
    }

    #[tokio::test]
    async fn test_parse_openai_sse_responses_prefers_completed_message_text() {
        let events = vec![
            Ok(Bytes::from_static(b"data: {\"type\":\"response.output_text.done\",\"text\":\"!\"}\n\n")),
            Ok(Bytes::from_static(b"data: {\"type\":\"response.output_item.done\",\"output_index\":0,\"item\":{\"type\":\"message\",\"role\":\"assistant\",\"content\":[{\"type\":\"output_text\",\"text\":\"Hello!\"}]}}\n\n")),
            Ok(Bytes::from_static(b"data: {\"type\":\"response.completed\",\"response\":{\"usage\":{},\"output\":[{\"type\":\"message\",\"role\":\"assistant\",\"content\":[{\"type\":\"output_text\",\"text\":\"Hello!\"}]}]}}\n\n")),
            Ok(Bytes::from_static(b"data: [DONE]\n")),
        ];
        let stream = futures::stream::iter(events);
        let (tx, mut rx) = mpsc::channel(8);

        let blocks = parse_openai_sse(stream, &tx, "responses").await;
        drop(tx);

        let response = blocks_to_response(&blocks);
        assert_eq!(response.content, "Hello!");

        let mut streamed = String::new();
        while let Some(item) = rx.recv().await {
            if let Ok(text) = item {
                streamed.push_str(&text);
            }
        }
        assert_eq!(streamed, "!");
    }

    #[tokio::test]
    async fn test_parse_openai_sse_chat_completions_handles_multiple_events_per_chunk() {
        let chunk = concat!(
            "data: {\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"content\":\"\"},\"finish_reason\":null}]}\n\n",
            "data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"Hello\"},\"finish_reason\":null}]}\n\n",
            "data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"!\"},\"finish_reason\":null}],\"usage\":{}}\n\n",
            "data: [DONE]\n"
        );
        let events = vec![Ok(Bytes::from(chunk.as_bytes().to_vec()))];
        let stream = futures::stream::iter(events);
        let (tx, mut rx) = mpsc::channel(8);

        let blocks = parse_openai_sse(stream, &tx, "chat_completions").await;
        drop(tx);

        let response = blocks_to_response(&blocks);
        assert_eq!(response.content, "Hello!");

        let mut streamed = String::new();
        while let Some(item) = rx.recv().await {
            if let Ok(text) = item {
                streamed.push_str(&text);
            }
        }
        assert_eq!(streamed, "Hello!");
    }

    #[test]
    fn test_trim_messages_history_drops_irrelevant_old_turns() {
        let mut history = Vec::new();
        for idx in 0..6 {
            history.push(json!({"role": "user", "content": format!("old topic deploy pipeline batch {}", idx)}));
            history.push(json!({"role": "assistant", "content": format!("deploy notes and release checklist {}", idx)}));
        }
        for idx in 0..4 {
            history.push(json!({"role": "user", "content": format!("auth middleware token refresh bug fix {}", idx)}));
            history.push(json!({"role": "assistant", "content": format!("auth fix details for refresh token flow {}", idx)}));
        }

        trim_messages_history(&mut history, 128_000);

        let joined = history
            .iter()
            .map(extract_message_text)
            .collect::<Vec<_>>()
            .join("\n");
        assert!(joined.contains("auth middleware token refresh bug fix"));
        assert!(!joined.contains("old topic deploy pipeline batch 0"));
        assert!(!joined.contains("old topic deploy pipeline batch 1"));
    }

    #[test]
    fn test_trim_messages_history_keeps_relevant_older_turns() {
        let mut history = Vec::new();
        for idx in 0..5 {
            history.push(json!({"role": "user", "content": format!("legacy deploy docs topic {}", idx)}));
            history.push(json!({"role": "assistant", "content": format!("deploy response {}", idx)}));
        }
        history.push(json!({"role": "user", "content": "parser panic on markdown table rendering"}));
        history.push(json!({"role": "assistant", "content": "parser investigation and stack trace"}));
        for idx in 0..4 {
            history.push(json!({"role": "user", "content": format!("parser fix for markdown renderer {}", idx)}));
            history.push(json!({"role": "assistant", "content": format!("parser code patch {}", idx)}));
        }

        trim_messages_history(&mut history, 128_000);

        let joined = history
            .iter()
            .map(extract_message_text)
            .collect::<Vec<_>>()
            .join("\n");
        assert!(joined.contains("parser panic on markdown table rendering"));
        assert!(joined.contains("parser fix for markdown renderer 3"));
        assert!(!joined.contains("legacy deploy docs topic 0"));
    }

    #[test]
    fn test_trim_messages_history_with_empty_focus_keeps_only_recent_context() {
        let mut history = Vec::new();
        for idx in 0..8 {
            history.push(json!({"role": "user", "content": format!("topic cluster {}", idx)}));
            history.push(json!({"role": "assistant", "content": format!("response {}", idx)}));
        }
        history.push(json!({"role": "user", "content": "ok"}));
        history.push(json!({"role": "assistant", "content": "ack"}));
        history.push(json!({"role": "user", "content": "yes"}));
        history.push(json!({"role": "assistant", "content": "continuing"}));

        trim_messages_history(&mut history, 128_000);

        let joined = history
            .iter()
            .map(extract_message_text)
            .collect::<Vec<_>>()
            .join("\n");
        assert!(!joined.contains("topic cluster 0"));
        assert!(joined.contains("topic cluster 7"));
        assert!(joined.contains("continuing"));
        assert!(history.len() < 20);
    }
}
