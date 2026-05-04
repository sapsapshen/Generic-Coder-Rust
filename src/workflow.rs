//! Workflow system — pipeline of agent modes (work/plan/review)
//! and drag-and-drop workflow builder support.

use serde::{Deserialize, Serialize};

/// Agent execution modes
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AgentMode {
    #[serde(rename = "work")]
    Work,
    #[serde(rename = "plan")]
    Plan,
    #[serde(rename = "review")]
    Review,
}

impl AgentMode {
    /// Max turns for this mode
    pub fn max_turns(self) -> usize {
        match self {
            Self::Work => 70,
            Self::Plan => 100,
            Self::Review => 50,
        }
    }

    /// Mode-specific system prompt prefix injected at start of each mode
    pub fn system_prompt_prefix(self) -> &'static str {
        match self {
            Self::Work => "\n## Mode: WORK\nYou are in execution mode. Implement the task using tools. Be thorough and precise.\n",
            Self::Plan => "\n## Mode: PLAN\nYou are in planning mode. DO NOT write or modify code. Explore the codebase, design an approach, identify risks, and present a clear plan. Use ask_user to get approval before any implementation.\n",
            Self::Review => "\n## Mode: REVIEW\nYou are in code review mode. Audit existing code for: correctness, error handling, security issues, performance problems, and style violations. Report findings with file:line references. Suggest concrete fixes. Do not implement changes unless explicitly asked.\n",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "work" => Some(Self::Work),
            "plan" => Some(Self::Plan),
            "review" => Some(Self::Review),
            _ => None,
        }
    }
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
        // Mark current node as completed
        if let Some(node) = self.nodes.get_mut(self.current_node) {
            node.completed = true;
        }
        self.current_node += 1;
        if self.current_node >= self.nodes.len() {
            self.active = false;
            self.current_node = 0;
            return false; // no more nodes
        }
        true // has next node
    }

    pub fn reset(&mut self) {
        self.active = false;
        self.current_node = 0;
        for node in &mut self.nodes {
            node.completed = false;
        }
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
