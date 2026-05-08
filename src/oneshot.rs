//! One Shot — Fully autonomous brainstorming-driven agent execution.
//!
//! Outer-inner loop: brainstorming cycles generate branching options,
//! the agent selects the best one and executes it. Roadblocks trigger
//! re-brainstorming. Stops when brainstorming is exhausted (no new options).

use serde::Serialize;
use serde_json::Value;
use std::collections::HashSet;
use std::sync::{Arc, RwLock};
use tokio::sync::mpsc;

use crate::agent::{agent_runner_loop, AgentHandler, LlmClient};
use crate::types::ToolSchema;

// ── Configuration ─────────────────────────────────────────────────────────────

pub struct OneShotConfig {
    pub max_brainstorm_cycles: u32,
    pub max_turns_per_direction: usize,
}

impl Default for OneShotConfig {
    fn default() -> Self {
        Self {
            max_brainstorm_cycles: 10,
            max_turns_per_direction: 40,
        }
    }
}

// ── Event types for frontend streaming ────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "oneshot_event")]
pub enum OneShotEvent {
    #[serde(rename = "oneshot_brainstorm_start")]
    BrainstormingStart { cycle: u32 },
    #[serde(rename = "oneshot_options")]
    OptionsGenerated {
        cycle: u32,
        options: Vec<String>,
        selected: String,
        rationale: String,
    },
    #[serde(rename = "oneshot_exhausted")]
    BrainstormingExhausted { cycle: u32, reason: String },
    #[serde(rename = "oneshot_executing")]
    ExecutingDirection { cycle: u32, direction: String },
    #[serde(rename = "oneshot_roadblock")]
    RoadblockDetected { cycle: u32, reason: String },
    #[serde(rename = "oneshot_done")]
    Done { summary: String },
}

// ── Parsed brainstorm output ──────────────────────────────────────────────────

#[derive(Debug, Clone)]
struct BrainstormOption {
    direction: String,
    novelty_score: f64,
}

#[derive(Debug, Clone)]
struct BrainstormResult {
    exhausted: bool,
    exhausted_reason: Option<String>,
    options: Vec<BrainstormOption>,
    selected: Option<String>,
    rationale: Option<String>,
}

// ── Main entry point ──────────────────────────────────────────────────────────

pub async fn run_one_shot_task(
    client: Arc<tokio::sync::RwLock<dyn LlmClient>>,
    user_input: String,
    handler: Arc<RwLock<AgentHandler>>,
    tools_schema: Vec<ToolSchema>,
    display_tx: mpsc::Sender<Value>,
    sys_prompt: String,
    verbose: bool,
) {
    let config = OneShotConfig::default();
    let source = "oneshot".to_string();

    let mut seen_options: HashSet<String> = HashSet::new();
    let mut all_output = String::new();

    // Build the base prompt with brainstorming skill reference
    let base_prompt = format!(
        "{}\n\n## One Shot Autonomous Mode\n\
         You are in fully autonomous One Shot mode. The user's task is below.\n\
         At each decision point or roadblock, read the brainstorming skill at\n\
         skills/brainstorming/README.md and follow its protocol to generate\n\
         branching options, select the best one, and continue.\n\
         NEVER ask the user for input — proceed autonomously.\n\
         \n\
         ## Important Protocol\n\
         - When you need to decide between approaches, invoke brainstorming\n\
         - Output brainstorming results as structured JSON (see skill README)\n\
         - If you hit a roadblock, respond with ROADBLOCK: <reason>\n\
         - When the task is fully complete, respond with DONE\n\
         - If brainstorming produces no new options, respond with EXHAUSTED",
        sys_prompt
    );

    for cycle in 1..=config.max_brainstorm_cycles {
        // ── Phase 1: Brainstorm ──────────────────────────────────────────
        send_oneshot_event(&display_tx, &OneShotEvent::BrainstormingStart { cycle }).await;

        let seen_list: Vec<String> = seen_options.iter().cloned().collect();
        let brainstorm_prompt = format!(
            "## Brainstorming Cycle {cycle}\n\n\
             Original task: {user_input}\n\n\
             Previously tried directions (DO NOT repeat these):\n{}\n\n\
             Accumulated output so far:\n{}\n\n\
             Read skills/brainstorming/README.md and follow the brainstorming protocol.\n\
             Generate branching options for what to do next.\n\
             Output the structured JSON result as specified in the skill README.\n\
             If no viable new options exist, output {{\"exhausted\": true, \"reason\": \"...\"}}",
            if seen_list.is_empty() {
                "(none yet)".to_string()
            } else {
                seen_list
                    .iter()
                    .enumerate()
                    .map(|(i, s)| format!("  {}. {}", i + 1, s))
                    .collect::<Vec<_>>()
                    .join("\n")
            },
            if all_output.is_empty() {
                "(just starting)".to_string()
            } else {
                all_output
                    .chars()
                    .rev()
                    .take(3000)
                    .collect::<String>()
                    .chars()
                    .rev()
                    .collect()
            },
        );

        // Use agent_runner_loop with 5 turns to let the LLM read the skill file + produce JSON
        let (brain_tx, mut brain_rx) = mpsc::channel::<String>(256);
        let brain_display = display_tx.clone();

        let _brain_stream = tokio::spawn(async move {
            let mut text = String::new();
            while let Some(chunk) = brain_rx.recv().await {
                text.push_str(&chunk);
                let _ = brain_display
                    .send(serde_json::json!({
                        "next": &chunk,
                        "source": "oneshot"
                    }))
                    .await;
            }
            text
        });

        let brainstorm_result = agent_runner_loop(
            client.clone(),
            base_prompt.clone(),
            brainstorm_prompt,
            handler.clone(),
            tools_schema.clone(),
            5,
            verbose,
            brain_tx,
            None,
        )
        .await;

        // Extract the final output from the brainstorm turn
        let brain_output = match &brainstorm_result {
            Ok(payload) => payload
                .get("final_output")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            Err(_) => String::new(),
        };

        // ── Phase 2: Parse brainstorm output ─────────────────────────────
        let parsed = parse_brainstorm_output(&brain_output);

        if parsed.exhausted {
            let reason = parsed
                .exhausted_reason
                .unwrap_or_else(|| "No new options available".to_string());
            send_oneshot_event(
                &display_tx,
                &OneShotEvent::BrainstormingExhausted {
                    cycle,
                    reason: reason.clone(),
                },
            )
            .await;
            all_output.push_str(&format!(
                "\n\n## Brainstorming Exhausted (cycle {cycle})\n{reason}\n"
            ));
            break;
        }

        if parsed.options.is_empty() {
            // No options but not explicitly exhausted — treat as exhausted
            send_oneshot_event(
                &display_tx,
                &OneShotEvent::BrainstormingExhausted {
                    cycle,
                    reason: "No options generated by brainstorming".to_string(),
                },
            )
            .await;
            break;
        }

        let selected = parsed.selected.unwrap_or_else(|| {
            // Fallback: pick highest novelty
            parsed
                .options
                .iter()
                .max_by(|a, b| {
                    a.novelty_score
                        .partial_cmp(&b.novelty_score)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .map(|o| o.direction.clone())
                .unwrap_or_else(|| parsed.options[0].direction.clone())
        });

        let rationale = parsed.rationale.unwrap_or_default();

        // Mark selected and all options as seen (to prevent cycling)
        mark_seen(&mut seen_options, &selected);
        for opt in &parsed.options {
            mark_seen(&mut seen_options, &opt.direction);
        }

        let option_summaries: Vec<String> = parsed
            .options
            .iter()
            .map(|o| format!("{} (novelty: {:.2})", o.direction, o.novelty_score))
            .collect();

        send_oneshot_event(
            &display_tx,
            &OneShotEvent::OptionsGenerated {
                cycle,
                options: option_summaries,
                selected: selected.clone(),
                rationale: rationale.clone(),
            },
        )
        .await;

        // ── Phase 3: Execute selected direction ──────────────────────────
        send_oneshot_event(
            &display_tx,
            &OneShotEvent::ExecutingDirection {
                cycle,
                direction: selected.clone(),
            },
        )
        .await;

        let exec_prompt = format!(
            "## Execute Direction (cycle {cycle})\n\n\
             Original task: {user_input}\n\n\
             Selected direction: {selected}\n\
             Rationale: {rationale}\n\n\
             Execute this direction now. Use tools as needed.\n\
             Do NOT ask the user for anything — work autonomously.\n\
             If you hit a roadblock you cannot overcome, respond with:\n\
             ROADBLOCK: <description of what blocked you and why>\n\
             If you complete this direction successfully, respond with:\n\
             DONE\n\n\
             Output from previous directions:\n{all_output}",
            all_output = all_output
                .chars()
                .rev()
                .take(2000)
                .collect::<String>()
                .chars()
                .rev()
                .collect::<String>(),
        );

        let (exec_tx, mut exec_rx) = mpsc::channel::<String>(256);
        let exec_display = display_tx.clone();

        let _exec_stream = tokio::spawn(async move {
            let mut text = String::new();
            while let Some(chunk) = exec_rx.recv().await {
                text.push_str(&chunk);
                let _ = exec_display
                    .send(serde_json::json!({
                        "next": &chunk,
                        "source": "oneshot"
                    }))
                    .await;
            }
            text
        });

        let exec_result = agent_runner_loop(
            client.clone(),
            base_prompt.clone(),
            exec_prompt,
            handler.clone(),
            tools_schema.clone(),
            config.max_turns_per_direction,
            verbose,
            exec_tx,
            None,
        )
        .await;

        let exec_output = match &exec_result {
            Ok(payload) => payload
                .get("final_output")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            Err(e) => format!("Execution error: {e:#}"),
        };

        // Check for sentinel keywords
        if exec_output.contains("ROADBLOCK:") || exec_output.contains("ROADBLOCK：") {
            let reason = exec_output
                .lines()
                .find(|l| l.contains("ROADBLOCK"))
                .unwrap_or("Unknown roadblock")
                .to_string();
            send_oneshot_event(
                &display_tx,
                &OneShotEvent::RoadblockDetected {
                    cycle,
                    reason: reason.chars().take(200).collect(),
                },
            )
            .await;
            all_output.push_str(&format!(
                "\n## Cycle {cycle} — Roadblock\n{}\n",
                &exec_output
            ));
            // Continue to next brainstorm cycle
            continue;
        }

        if exec_output.contains("DONE") {
            all_output.push_str(&exec_output);
            break;
        }

        // No sentinel — just accumulate and continue
        all_output.push_str(&exec_output);
    }

    // ── Completion ─────────────────────────────────────────────────────────────
    send_oneshot_event(
        &display_tx,
        &OneShotEvent::Done {
            summary: all_output
                .chars()
                .rev()
                .take(500)
                .collect::<String>()
                .chars()
                .rev()
                .collect(),
        },
    )
    .await;

    let _ = display_tx
        .send(serde_json::json!({
            "done": all_output,
            "source": source,
        }))
        .await;
}

// ── Helpers ───────────────────────────────────────────────────────────────────

async fn send_oneshot_event(display_tx: &mpsc::Sender<Value>, event: &OneShotEvent) {
    let json = serde_json::to_value(event).unwrap_or_default();
    let _ = display_tx
        .send(serde_json::json!({
            "acp": json,
            "source": "oneshot",
        }))
        .await;
}

fn parse_brainstorm_output(output: &str) -> BrainstormResult {
    // Strategy 1: Find JSON in markdown code fence
    let json_str = if let Some(start) = output.find("```json") {
        let inner = &output[start + 7..];
        if let Some(end) = inner.find("```") {
            inner[..end].trim()
        } else {
            inner.trim()
        }
    } else if let Some(start) = output.find("```") {
        let inner = &output[start + 3..];
        if let Some(end) = inner.find("```") {
            inner[..end].trim()
        } else {
            inner.trim()
        }
    } else if let Some(start) = output.find('{') {
        let end = output.rfind('}').unwrap_or(output.len() - 1);
        &output[start..=end]
    } else {
        ""
    };

    // Try to parse as JSON
    if !json_str.is_empty() {
        if let Ok(val) = serde_json::from_str::<Value>(json_str) {
            // Check for exhausted
            if val
                .get("exhausted")
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
            {
                return BrainstormResult {
                    exhausted: true,
                    exhausted_reason: val
                        .get("reason")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string()),
                    options: vec![],
                    selected: None,
                    rationale: None,
                };
            }

            // Parse options
            let options: Vec<BrainstormOption> = val
                .get("options")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|opt| {
                            let direction =
                                opt.get("direction").and_then(|v| v.as_str())?.to_string();
                            let novelty = opt
                                .get("novelty_score")
                                .and_then(|v| v.as_f64())
                                .unwrap_or(0.5);
                            Some(BrainstormOption {
                                direction,
                                novelty_score: novelty,
                            })
                        })
                        .collect()
                })
                .unwrap_or_default();

            let selected = val
                .get("selected")
                .and_then(|v| v.get("direction"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            let rationale = val
                .get("selected")
                .and_then(|v| v.get("rationale"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            return BrainstormResult {
                exhausted: false,
                exhausted_reason: None,
                options,
                selected,
                rationale,
            };
        }
    }

    // Strategy 2: Check for EXHAUSTED keyword in raw output
    if output.to_uppercase().contains("EXHAUSTED") {
        return BrainstormResult {
            exhausted: true,
            exhausted_reason: Some("LLM declared exhaustion".to_string()),
            options: vec![],
            selected: None,
            rationale: None,
        };
    }

    // Strategy 3: No structured output found
    BrainstormResult {
        exhausted: false,
        exhausted_reason: None,
        options: vec![],
        selected: None,
        rationale: None,
    }
}

fn mark_seen(seen: &mut HashSet<String>, direction: &str) {
    let normalized = direction.trim().to_lowercase();
    if normalized.is_empty() {
        return;
    }
    seen.insert(normalized.clone());

    // Also insert short substrings as fuzzy dedup (prevents near-duplicates)
    // Insert first 40 chars as a fingerprint
    let fingerprint: String = normalized.chars().take(40).collect();
    if fingerprint.len() >= 10 {
        seen.insert(fingerprint);
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_exhausted() {
        let output = r#"```json
{"exhausted": true, "reason": "All approaches tried"}
```"#;
        let result = parse_brainstorm_output(output);
        assert!(result.exhausted);
        assert_eq!(
            result.exhausted_reason.as_deref(),
            Some("All approaches tried")
        );
    }

    #[test]
    fn test_parse_options() {
        let output = r#"```json
{
  "cycle": 1,
  "options": [
    {"direction": "Use Rust", "pros": ["Fast"], "cons": ["Complex"], "feasibility": 0.9, "impact": 0.8, "risk": 0.2, "novelty_score": 0.85},
    {"direction": "Use Python", "pros": ["Easy"], "cons": ["Slow"], "feasibility": 0.95, "impact": 0.5, "risk": 0.1, "novelty_score": 0.4}
  ],
  "selected": {"direction": "Use Rust", "rationale": "Better performance"}
}
```"#;
        let result = parse_brainstorm_output(output);
        assert!(!result.exhausted);
        assert_eq!(result.options.len(), 2);
        assert_eq!(result.options[0].direction, "Use Rust");
        assert!((result.options[0].novelty_score - 0.85).abs() < 0.001);
        assert_eq!(result.selected.as_deref(), Some("Use Rust"));
        assert_eq!(result.rationale.as_deref(), Some("Better performance"));
    }

    #[test]
    fn test_parse_exhausted_keyword() {
        let output = "I've tried everything. EXHAUSTED - no more options available.";
        let result = parse_brainstorm_output(output);
        assert!(result.exhausted);
    }

    #[test]
    fn test_mark_seen_normalizes() {
        let mut seen = HashSet::new();
        mark_seen(&mut seen, "  Use Rust Binary  ");
        assert!(seen.contains("use rust binary"));
    }

    #[test]
    fn test_mark_seen_fingerprint() {
        let mut seen = HashSet::new();
        mark_seen(
            &mut seen,
            "Implement as a Rust binary with clap argument parsing for CLI interface",
        );
        // Should contain both the full normalized string and the fingerprint
        assert!(seen.len() >= 2);
    }
}
