//! ACP — Agent Communication Protocol for multi-agent collaboration.
//!
//! Orchestrator decomposes tasks → Specialist agents execute steps sequentially.
//! Each specialist has an independent context window and role-specific system prompt.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::mpsc;

use crate::agent::{agent_runner_loop, AgentHandler, LlmClient};
use crate::error_memory::ErrorSeverity;
use crate::types::ToolSchema;

// ── Role system prompts ──────────────────────────────────────────────────────

const ORCHESTRATOR_PROMPT: &str = r#"You are the **Orchestrator Agent**. Your job is to decompose the user's task into a sequential execution plan and assign each step to a specialist agent role.

Available specialist roles:
- **searcher** — Explore codebases, search files, read code, understand project structure
- **planner** — Design implementation approach, identify risks, create step-by-step plans (NO code writing)
- **coder** — Implement solutions, write/modify code, run tests, fix bugs
- **reviewer** — Audit code for correctness, errors, security, performance, style issues

Output ONLY a JSON object (no markdown fences, no explanation):

{
  "task_summary": "one-line summary of the task",
  "steps": [
    {
      "step_id": 1,
      "role": "searcher",
      "description": "Search the codebase for ..."
    },
    {
      "step_id": 2,
      "role": "coder",
      "description": "Implement the changes to ..."
    },
    {
      "step_id": 3,
      "role": "reviewer",
      "description": "Review the implementation for ..."
    }
  ]
}

Rules:
- Maximum 5 steps
- First step is usually "searcher" to understand the codebase
- Do NOT include the orchestrator as a step — you are the orchestrator
- Each step_description must be concrete and actionable
- Steps must be sequential (each step's output feeds the next)"#;

const SEARCHER_PROMPT: &str = r#"## Mode: ACP Searcher
You are a **Code Searcher**. Your job is to explore and understand the codebase.

What you do:
- Search for files, patterns, and symbols using workspace_list, content_search, file_read
- Understand project structure, dependencies, and key abstractions
- Report findings clearly with file paths and line numbers

What you do NOT do:
- Do NOT write or modify any code
- Do NOT propose solutions — just report what exists

Output format: after exploration, give a structured report of your findings."#;

const PLANNER_PROMPT: &str = r#"## Mode: ACP Planner
You are a **Planner**. Your job is to design the implementation approach.

What you do:
- Read the codebase findings from the previous step
- Design a concrete implementation plan with specific files and line numbers
- Identify risks, edge cases, and dependencies
- Present a clear step-by-step plan

What you do NOT do:
- Do NOT write or modify any code — only design

Output format: a numbered implementation plan with file paths, function names, and expected changes."#;

const CODER_PROMPT: &str = r#"## Mode: ACP Coder
You are a **Coder**. Your job is to implement the plan using tools.

What you do:
- Read the plan from the previous step
- Implement changes using file_read, file_write, file_patch, code_run
- Be thorough — handle edge cases, add error handling
- Verify your changes work

What you do NOT do:
- Do NOT deviate from the plan without good reason
- Do NOT refactor unrelated code

Output: implement all changes and verify they work. Report what you changed with file:line references."#;

const REVIEWER_PROMPT: &str = r#"## Mode: ACP Reviewer
You are a **Code Reviewer**. Your job is to audit the implementation.

What you do:
- Read the implemented changes
- Check for: correctness, error handling, security issues, performance problems, style consistency
- Report findings with file:line references
- Suggest concrete fixes for any issues found

What you do NOT do:
- Do NOT implement changes yourself — only audit

Output: a review report with PASS/FAIL items, each referencing specific file:line locations."#;

// ── Types ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum AgentRole {
    Orchestrator,
    Searcher,
    Planner,
    Coder,
    Reviewer,
}

impl AgentRole {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "orchestrator" => Self::Orchestrator,
            "searcher" => Self::Searcher,
            "planner" => Self::Planner,
            "coder" => Self::Coder,
            "reviewer" => Self::Reviewer,
            _ => Self::Coder, // default fallback
        }
    }

    pub fn system_prompt(&self) -> &'static str {
        match self {
            Self::Orchestrator => ORCHESTRATOR_PROMPT,
            Self::Searcher => SEARCHER_PROMPT,
            Self::Planner => PLANNER_PROMPT,
            Self::Coder => CODER_PROMPT,
            Self::Reviewer => REVIEWER_PROMPT,
        }
    }

    pub fn emoji(&self) -> &'static str {
        match self {
            Self::Orchestrator => "\u{1F3AF}", // 🎯
            Self::Searcher => "\u{1F50D}",     // 🔍
            Self::Planner => "\u{1F4CB}",      // 📋
            Self::Coder => "\u{1F4BB}",        // 💻
            Self::Reviewer => "\u{1F512}",     // 🔒
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum AcpStepStatus {
    Pending,
    Running,
    Done,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AcpStep {
    pub step_id: usize,
    pub role: AgentRole,
    pub description: String,
    #[serde(default)]
    pub input: String,
    #[serde(default)]
    pub output: Option<String>,
    #[serde(default = "default_status")]
    pub status: AcpStepStatus,
}

fn default_status() -> AcpStepStatus {
    AcpStepStatus::Pending
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum AcpPlanStatus {
    Planning,
    Executing,
    Done,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AcpPlan {
    pub task_summary: String,
    pub steps: Vec<AcpStep>,
    #[serde(default = "planning_status")]
    pub status: AcpPlanStatus,
}

fn planning_status() -> AcpPlanStatus {
    AcpPlanStatus::Planning
}

// ── ACP event types for frontend streaming ───────────────────────────────────

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "acp_event")]
pub enum AcpEvent {
    #[serde(rename = "acp_plan")]
    Plan { plan: AcpPlan },
    #[serde(rename = "acp_step_start")]
    StepStart {
        step_id: usize,
        role: AgentRole,
        description: String,
    },
    #[serde(rename = "acp_step_done")]
    StepDone {
        step_id: usize,
        role: AgentRole,
        summary: String,
    },
    #[serde(rename = "acp_step_failed")]
    StepFailed {
        step_id: usize,
        role: AgentRole,
        error: String,
    },
    #[serde(rename = "acp_done")]
    Done { summary: String },
}

// ── Main entry point ─────────────────────────────────────────────────────────

/// Run a full ACP multi-agent task. Called from GenericAgent.run_task().
pub async fn run_acp_task(
    client: Arc<tokio::sync::RwLock<dyn LlmClient>>,
    user_input: String,
    handler: Arc<RwLock<AgentHandler>>,
    tools_schema: Vec<ToolSchema>,
    display_tx: mpsc::Sender<Value>,
    verbose: bool,
) {
    let source = "acp".to_string();

    // Wrap display_tx in an adapter to accept String for agent_runner_loop
    let (output_tx, mut output_rx) = mpsc::channel::<String>(256);
    let display_tx_clone = display_tx.clone();

    // Stream output from agent_runner_loop back to display_tx
    let _stream_task = tokio::spawn(async move {
        let mut full = String::new();
        let mut last: usize = 0;
        while let Some(chunk) = output_rx.recv().await {
            full.push_str(&chunk);
            if full.len() - last > 50 || chunk.contains("LLM Running") {
                let _ = display_tx_clone
                    .send(serde_json::json!({
                        "next": &full[last..],
                        "source": "acp"
                    }))
                    .await;
                last = full.len();
            }
        }
        if last < full.len() {
            let _ = display_tx_clone
                .send(serde_json::json!({
                    "next": &full[last..],
                    "source": "acp"
                }))
                .await;
        }
        full
    });

    // ── Phase 1: Orchestration ──────────────────────────────────────────
    send_acp_event(
        &display_tx,
        &AcpEvent::Plan {
            plan: AcpPlan {
                task_summary: "Analyzing task...".into(),
                steps: vec![],
                status: AcpPlanStatus::Planning,
            },
        },
    )
    .await;

    let orchestrator_input = format!(
        "Decompose the following user request into a multi-agent execution plan:\n\n{}",
        user_input
    );

    let orch_result = agent_runner_loop(
        client.clone(),
        ORCHESTRATOR_PROMPT.to_string(),
        orchestrator_input,
        handler.clone(),
        tools_schema.clone(),
        10, // orchestrator gets 10 turns max
        verbose,
        output_tx.clone(),
        None,
    )
    .await;

    // Collect streamed output so far (orchestrator's thinking)
    drop(output_tx);

    // Parse the orchestrator's JSON response
    let plan = match parse_acp_plan(&orch_result) {
        Ok(p) => p,
        Err(e) => {
            send_acp_event(
                &display_tx,
                &AcpEvent::StepFailed {
                    step_id: 0,
                    role: AgentRole::Orchestrator,
                    error: format!("Failed to parse plan: {e}"),
                },
            )
            .await;
            let _ = display_tx
                .send(serde_json::json!({
                    "done": format!("Multi-agent planning failed: {e}\n\nFalling back to single-agent mode. Please try without multi-agent enabled."),
                    "source": "acp"
                }))
                .await;
            return;
        }
    };

    // Send the parsed plan to frontend
    send_acp_event(&display_tx, &AcpEvent::Plan { plan: plan.clone() }).await;

    // ── Phase 2: Sequential specialist execution ────────────────────────
    let mut previous_output = String::new();
    let mut all_results = String::new();
    let total_steps = plan.steps.len();

    for (idx, step) in plan.steps.iter().enumerate() {
        // Create a fresh output_tx for each specialist
        let (step_output_tx, mut step_output_rx) = mpsc::channel::<String>(256);
        let step_display = display_tx.clone();

        // Stream specialist output to display
        let step_stream = tokio::spawn(async move {
            let mut full = String::new();
            let mut last: usize = 0;
            while let Some(chunk) = step_output_rx.recv().await {
                full.push_str(&chunk);
                if full.len() - last > 50 {
                    let _ = step_display
                        .send(serde_json::json!({
                            "next": &full[last..],
                            "source": "acp"
                        }))
                        .await;
                    last = full.len();
                }
            }
            if last < full.len() {
                let _ = step_display
                    .send(serde_json::json!({
                        "next": &full[last..],
                        "source": "acp"
                    }))
                    .await;
            }
            full
        });

        // Send step_start event
        send_acp_event(
            &display_tx,
            &AcpEvent::StepStart {
                step_id: step.step_id,
                role: step.role.clone(),
                description: step.description.clone(),
            },
        )
        .await;

        // Build specialist's input with context from previous steps
        let specialist_input = if previous_output.is_empty() {
            format!(
                "Task from user: {}\n\nYour job (step {}/{}): {}",
                user_input,
                idx + 1,
                total_steps,
                step.description
            )
        } else {
            format!(
                "Task from user: {}\n\nPrevious step output:\n{}\n\nYour job (step {}/{}): {}",
                user_input,
                previous_output,
                idx + 1,
                total_steps,
                step.description
            )
        };

        // Execute the specialist
        let result = agent_runner_loop(
            client.clone(),
            step.role.system_prompt().to_string(),
            specialist_input,
            handler.clone(),
            tools_schema.clone(),
            30, // specialists get 30 turns max
            verbose,
            step_output_tx,
            None,
        )
        .await;

        // Collect output
        let specialist_output = step_stream.await.unwrap_or_default();

        match result {
            Ok(ref payload) => {
                let summary = payload
                    .get("final_output")
                    .and_then(|v| v.as_str())
                    .unwrap_or(&specialist_output);
                let summary = summary.chars().take(1000).collect::<String>();

                send_acp_event(
                    &display_tx,
                    &AcpEvent::StepDone {
                        step_id: step.step_id,
                        role: step.role.clone(),
                        summary: summary.clone(),
                    },
                )
                .await;

                previous_output = summary;
                all_results.push_str(&format!(
                    "\n## {} {} (Step {})\n\n{}\n",
                    step.role.emoji(),
                    role_display_name(&step.role),
                    step.step_id,
                    &specialist_output
                ));
            }
            Err(e) => {
                let err_msg = format!("{:#}", e);
                handler.read().unwrap().record_error(
                    &format!("acp_{}", role_key(&step.role)),
                    &err_msg,
                    ErrorSeverity::Critical,
                    serde_json::json!({"step_id": step.step_id, "role": role_key(&step.role)}),
                );

                send_acp_event(
                    &display_tx,
                    &AcpEvent::StepFailed {
                        step_id: step.step_id,
                        role: step.role.clone(),
                        error: err_msg.clone(),
                    },
                )
                .await;

                all_results.push_str(&format!(
                    "\n## {} {} (Step {}) — FAILED\n\n{}\n",
                    step.role.emoji(),
                    role_display_name(&step.role),
                    step.step_id,
                    err_msg
                ));
                break;
            }
        }
    }

    // ── Phase 3: Completion ─────────────────────────────────────────────
    send_acp_event(
        &display_tx,
        &AcpEvent::Done {
            summary: plan.task_summary.clone(),
        },
    )
    .await;

    let _ = display_tx
        .send(serde_json::json!({
            "done": all_results,
            "source": source,
        }))
        .await;
}

// ── Helpers ──────────────────────────────────────────────────────────────────

async fn send_acp_event(display_tx: &mpsc::Sender<Value>, event: &AcpEvent) {
    let json = serde_json::to_value(event).unwrap_or_default();
    let _ = display_tx
        .send(serde_json::json!({
            "acp": json,
            "source": "acp",
        }))
        .await;
}

fn parse_acp_plan(
    result: &Result<HashMap<String, Value>, anyhow::Error>,
) -> Result<AcpPlan, String> {
    let payload = result.as_ref().map_err(|e| format!("{:#}", e))?;

    let output = payload
        .get("final_output")
        .and_then(|v| v.as_str())
        .or_else(|| payload.get("message").and_then(|v| v.as_str()))
        .unwrap_or("");

    // Extract JSON from the response — LLMs sometimes wrap in markdown fences
    let json_str = if let Some(start) = output.find("```json") {
        let inner = &output[start + 7..];
        if let Some(end) = inner.find("```") {
            &inner[..end]
        } else {
            inner
        }
    } else if let Some(start) = output.find("```") {
        let inner = &output[start + 3..];
        if let Some(end) = inner.find("```") {
            &inner[..end]
        } else {
            inner
        }
    } else if let Some(start) = output.find('{') {
        // Return from first { to last }
        let end = output.rfind('}').unwrap_or(output.len() - 1);
        &output[start..=end]
    } else {
        return Err("No JSON found in orchestrator output".into());
    };

    let json_str = json_str.trim();

    let mut plan: AcpPlan = serde_json::from_str(json_str).map_err(|e| {
        format!(
            "Failed to parse plan JSON: {e}\nRaw: {}",
            &json_str[..json_str.len().min(500)]
        )
    })?;

    // Validate and fill in defaults
    if plan.steps.is_empty() {
        return Err("Plan has no steps".into());
    }
    if plan.steps.len() > 5 {
        plan.steps.truncate(5); // hard cap
    }

    // Ensure step IDs are sequential
    for (i, step) in plan.steps.iter_mut().enumerate() {
        step.step_id = i + 1;
        step.status = AcpStepStatus::Pending;
    }

    plan.status = AcpPlanStatus::Executing;

    Ok(plan)
}

fn role_display_name(role: &AgentRole) -> &'static str {
    match role {
        AgentRole::Orchestrator => "Orchestrator",
        AgentRole::Searcher => "Searcher",
        AgentRole::Planner => "Planner",
        AgentRole::Coder => "Coder",
        AgentRole::Reviewer => "Reviewer",
    }
}

fn role_key(role: &AgentRole) -> String {
    role_display_name(role).to_lowercase()
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_role_from_str() {
        assert_eq!(AgentRole::from_str("searcher"), AgentRole::Searcher);
        assert_eq!(AgentRole::from_str("CODER"), AgentRole::Coder);
        assert_eq!(AgentRole::from_str("Planner"), AgentRole::Planner);
        assert_eq!(AgentRole::from_str("unknown"), AgentRole::Coder); // fallback
    }

    #[test]
    fn test_role_prompts_not_empty() {
        for role in &[
            AgentRole::Orchestrator,
            AgentRole::Searcher,
            AgentRole::Planner,
            AgentRole::Coder,
            AgentRole::Reviewer,
        ] {
            assert!(!role.system_prompt().is_empty());
            assert!(!role.emoji().is_empty());
        }
    }

    #[test]
    fn test_parse_acp_plan() {
        let json = serde_json::json!({
            "task_summary": "Add error handling",
            "steps": [
                {"step_id": 1, "role": "searcher", "description": "Find all endpoints"},
                {"step_id": 2, "role": "coder", "description": "Add try/catch"},
                {"step_id": 3, "role": "reviewer", "description": "Review changes"},
            ]
        })
        .to_string();

        // Simulate an orchestrator result payload
        let mut map = HashMap::new();
        map.insert("final_output".to_string(), Value::String(json));
        let result: Result<HashMap<String, Value>, anyhow::Error> = Ok(map);

        let plan = parse_acp_plan(&result).unwrap();
        assert_eq!(plan.task_summary, "Add error handling");
        assert_eq!(plan.steps.len(), 3);
        assert_eq!(plan.steps[0].role, AgentRole::Searcher);
        assert_eq!(plan.steps[2].role, AgentRole::Reviewer);
    }

    #[test]
    fn test_parse_acp_plan_with_markdown_fence() {
        let json = r#"```json
{"task_summary": "Test", "steps": [{"step_id": 1, "role": "coder", "description": "Do it"}]}
```"#;
        let mut map = HashMap::new();
        map.insert("final_output".to_string(), Value::String(json.to_string()));
        let result: Result<HashMap<String, Value>, anyhow::Error> = Ok(map);

        let plan = parse_acp_plan(&result).unwrap();
        assert_eq!(plan.steps.len(), 1);
        assert_eq!(plan.steps[0].role, AgentRole::Coder);
    }

    #[test]
    fn test_parse_acp_plan_truncates_to_5() {
        let steps: Vec<_> = (1..=7)
            .map(|i| serde_json::json!({"step_id": i, "role": "coder", "description": format!("Step {i}")}))
            .collect();
        let json = serde_json::json!({"task_summary": "T", "steps": steps}).to_string();

        let mut map = HashMap::new();
        map.insert("final_output".to_string(), Value::String(json));
        let result: Result<HashMap<String, Value>, anyhow::Error> = Ok(map);

        let plan = parse_acp_plan(&result).unwrap();
        assert_eq!(plan.steps.len(), 5);
    }

    #[test]
    fn test_parse_acp_plan_no_json() {
        let mut map = HashMap::new();
        map.insert(
            "final_output".to_string(),
            Value::String("I'm sorry, I cannot help with that.".into()),
        );
        let result: Result<HashMap<String, Value>, anyhow::Error> = Ok(map);
        assert!(parse_acp_plan(&result).is_err());
    }
}
