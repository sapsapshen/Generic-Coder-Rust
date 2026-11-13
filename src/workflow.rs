//! Workflow system — pipeline of agent modes (ask/plan/build/review)
//! and drag-and-drop workflow builder support.

use serde::{Deserialize, Serialize};

/// Agent execution modes.
///
/// `Build` is the only mode that may change files or run commands.
/// The historical wire value `"work"` still deserializes as `Build`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AgentMode {
    #[serde(rename = "ask")]
    Ask,
    #[serde(rename = "plan")]
    Plan,
    #[serde(rename = "build", alias = "work")]
    Build,
    #[serde(rename = "review")]
    Review,
}

impl AgentMode {
    /// Max turns for this mode.
    pub fn max_turns(self) -> usize {
        match self {
            Self::Ask => 70,
            Self::Plan => 100,
            Self::Build => 70,
            Self::Review => 50,
        }
    }

    /// Value returned by the HTTP API and stored in workflow JSON.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ask => "ask",
            Self::Plan => "plan",
            Self::Build => "build",
            Self::Review => "review",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Ask => "Ask",
            Self::Plan => "Plan",
            Self::Build => "Build",
            Self::Review => "Review",
        }
    }

    /// Ask, Plan, and Review inspect the workspace. Build may change it.
    pub fn allows_mutation(self) -> bool {
        matches!(self, Self::Build)
    }

    /// Mode-specific system prompt prefix injected at start of each mode.
    pub fn system_prompt_prefix(self) -> &'static str {
        match self {
            Self::Ask => "\n## Mode: ASK\nYou are in ask mode. Answer questions and explain code. Do not create, modify, or delete files, and do not run commands. Use read-only tools only when you need to inspect the workspace.\n",
            Self::Plan => "\n## Mode: PLAN\nYou are in planning mode. DO NOT write or modify code. Explore the codebase, design an approach, identify risks, and present a clear plan. Use ask_user to get approval before any implementation.\n",
            Self::Build => "\n## Mode: BUILD\nYou are in execution mode. Implement the task using tools. Be thorough and precise.\n",
            Self::Review => "\n## Mode: REVIEW\nYou are in code review mode. Audit existing code for: correctness, error handling, security issues, performance problems, and style violations. Report findings with file:line references. Suggest concrete fixes. Do not implement changes unless explicitly asked.\n",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "ask" => Some(Self::Ask),
            "plan" => Some(Self::Plan),
            "build" | "work" => Some(Self::Build),
            "review" => Some(Self::Review),
            _ => None,
        }
    }
}

/// Tools that create, modify, delete, or execute something outside the chat.
pub fn tool_mutates(name: &str) -> bool {
    if matches!(
        name,
        "code_run"
            | "run_tests"
            | "lsp_get_diagnostics"
            | "workspace_open"
            | "file_patch"
            | "file_write"
            | "file_revert"
            | "remote_exec"
            | "remote_file_write"
            | "mcp_call_tool"
    ) {
        return true;
    }
    let Some(action) = name.strip_prefix("computer_") else {
        return false;
    };
    !matches!(
        action,
        "screenshot"
            | "zoom"
            | "cursor_position"
            | "list_granted_applications"
            | "read_clipboard"
            | "wait"
    )
}

/// A single node in the workflow pipeline
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowNode {
    pub mode: AgentMode,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub completed: bool,
}

/// The workflow pipeline — a sequence of mode nodes
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Workflow {
    pub nodes: Vec<WorkflowNode>,
    #[serde(default)]
    pub active: bool,
    #[serde(default)]
    pub current_node: usize,
}

#[derive(Debug, Deserialize)]
pub struct WorkflowPayload {
    pub nodes: Vec<WorkflowNodePayload>,
}

#[derive(Debug, Deserialize)]
pub struct WorkflowNodePayload {
    pub mode: String,
    #[serde(default)]
    pub label: String,
}

/// Validation result
pub struct Validation {
    pub valid: bool,
    pub reason: Option<String>,
}

impl Workflow {
    pub fn validate(nodes: &[WorkflowNode]) -> Validation {
        if nodes.is_empty() {
            return Validation {
                valid: true,
                reason: None,
            };
        }
        if nodes.len() > 3 {
            return Validation {
                valid: false,
                reason: Some("Maximum 3 nodes per workflow".into()),
            };
        }
        for i in 0..nodes.len().saturating_sub(1) {
            if nodes[i].mode == nodes[i + 1].mode {
                return Validation {
                    valid: false,
                    reason: Some(format!(
                        "Cannot have consecutive identical modes at position {}",
                        i + 1
                    )),
                };
            }
        }
        Validation {
            valid: true,
            reason: None,
        }
    }

    pub fn current_mode(&self) -> Option<AgentMode> {
        if !self.active || self.nodes.is_empty() {
            return None;
        }
        self.nodes.get(self.current_node).map(|n| n.mode)
    }

    pub fn advance(&mut self) -> bool {
        if !self.active || self.nodes.is_empty() {
            return false;
        }
        if let Some(node) = self.nodes.get_mut(self.current_node) {
            node.completed = true;
        }
        self.current_node += 1;
        if self.current_node >= self.nodes.len() {
            self.active = false;
            self.current_node = 0;
            return false;
        }
        true
    }

    pub fn reset(&mut self) {
        self.active = false;
        self.current_node = 0;
        self.nodes.clear();
    }

    pub fn set_active(&mut self, nodes: Vec<WorkflowNode>) {
        self.nodes = nodes;
        self.active = !self.nodes.is_empty();
        self.current_node = 0;
        for node in &mut self.nodes {
            node.completed = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mode_wire_values_round_trip() {
        for mode in [
            AgentMode::Ask,
            AgentMode::Plan,
            AgentMode::Build,
            AgentMode::Review,
        ] {
            assert_eq!(AgentMode::from_str(mode.as_str()), Some(mode));
            let json = serde_json::to_string(&mode).unwrap();
            assert_eq!(json, format!("\"{}\"", mode.as_str()));
            let parsed: AgentMode = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed, mode);
        }
    }

    #[test]
    fn work_alias_deserializes_as_build() {
        assert_eq!(AgentMode::from_str("work"), Some(AgentMode::Build));
        assert_eq!(AgentMode::from_str(" WORK "), Some(AgentMode::Build));
        let parsed: AgentMode = serde_json::from_str("\"work\"").unwrap();
        assert_eq!(parsed, AgentMode::Build);
        assert_eq!(parsed.as_str(), "build");
    }

    #[test]
    fn only_build_may_mutate() {
        assert!(!AgentMode::Ask.allows_mutation());
        assert!(!AgentMode::Plan.allows_mutation());
        assert!(AgentMode::Build.allows_mutation());
        assert!(!AgentMode::Review.allows_mutation());
        assert!(tool_mutates("file_write"));
        assert!(tool_mutates("code_run"));
        assert!(tool_mutates("run_tests"));
        assert!(tool_mutates("lsp_get_diagnostics"));
        assert!(tool_mutates("workspace_open"));
        assert!(tool_mutates("computer_left_click"));
        assert!(!tool_mutates("file_read"));
        assert!(!tool_mutates("computer_screenshot"));
        assert!(!tool_mutates("computer_wait"));
    }

    #[test]
    fn turn_budgets_match_mode() {
        assert_eq!(AgentMode::Ask.max_turns(), 70);
        assert_eq!(AgentMode::Build.max_turns(), 70);
        assert_eq!(AgentMode::Plan.max_turns(), 100);
        assert_eq!(AgentMode::Review.max_turns(), 50);
    }

    #[test]
    fn workflow_rejects_consecutive_duplicates_and_accepts_ask_then_build() {
        let ask = WorkflowNode {
            mode: AgentMode::Ask,
            label: String::new(),
            completed: false,
        };
        let build = WorkflowNode {
            mode: AgentMode::Build,
            label: String::new(),
            completed: false,
        };
        assert!(Workflow::validate(&[ask.clone(), build]).valid);
        assert!(!Workflow::validate(&[ask.clone(), ask]).valid);
    }
}
