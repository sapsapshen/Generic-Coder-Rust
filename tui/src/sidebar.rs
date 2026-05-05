//! Left sidebar rendering for Generic Coder TUI.

use ratatui::{
    prelude::*,
    widgets::*,
};

use crate::app::App;

pub fn draw(
    frame: &mut Frame,
    area: Rect,
    app: &App,
    bg: Color,
    panel_bg: Color,
    border_color: Color,
    accent: Color,
    text: Color,
    text_dim: Color,
    ok: Color,
) {
    let block = Block::default()
        .borders(Borders::RIGHT)
        .border_style(Style::default().fg(border_color))
        .style(Style::default().bg(panel_bg));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let sections = Layout::vertical([
        Constraint::Length(3),  // Brand
        Constraint::Length(6),  // Modes
        Constraint::Length(4),  // Context
        Constraint::Min(0),     // Workspace tree
        Constraint::Length(3),  // Status
    ])
    .split(inner);

    // ── Brand ────────────────────────────────────────
    let brand_lines = vec![
        Line::from(Span::styled("Generic Coder", Style::default().fg(accent).add_modifier(Modifier::BOLD))),
        Line::from(Span::styled("Autonomous cockpit", Style::default().fg(text_dim))),
    ];
    frame.render_widget(Paragraph::new(brand_lines), sections[0]);

    // ── Mode buttons ──────────────────────────────────
    let mode_chunks = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(2),
        Constraint::Length(2),
    ])
    .split(sections[1]);

    frame.render_widget(
        Paragraph::new("MODE").style(Style::default().fg(text_dim).add_modifier(Modifier::BOLD)),
        mode_chunks[0],
    );

    // F1 Work
    let work_style = if matches!(app.current_mode, generic_coder::workflow::AgentMode::Work) {
        Style::default().fg(bg).bg(accent).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(text)
    };
    let work_text = format!(
        " F1 Work  {}",
        if app.multi_agent_enabled {
            "[MA]"
        } else if app.one_shot_enabled {
            "[OS]"
        } else {
            ""
        }
    );
    frame.render_widget(Paragraph::new(work_text).style(work_style), mode_chunks[1]);

    // Toggles row
    let toggles = format!(
        "{} {}{}",
        if app.multi_agent_enabled {
            "[F4 MA:ON]"
        } else {
            "[F4 MA:OFF]"
        },
        if app.one_shot_enabled {
            " [F5 OS:ON]"
        } else {
            " [F5 OS:OFF]"
        },
        if app.is_running { " ⚡" } else { "" },
    );
    frame.render_widget(
        Paragraph::new(toggles).style(Style::default().fg(text_dim)),
        mode_chunks[2],
    );

    // ── Context ───────────────────────────────────────
    let ctx_chunks = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(3),
    ])
    .split(sections[2]);

    frame.render_widget(
        Paragraph::new("AGENT CONTEXT").style(Style::default().fg(text_dim).add_modifier(Modifier::BOLD)),
        ctx_chunks[0],
    );

    let mode_label = match app.current_mode {
        generic_coder::workflow::AgentMode::Work => "Work",
        generic_coder::workflow::AgentMode::Plan => "Plan",
        generic_coder::workflow::AgentMode::Review => "Review",
    };
    let ctx_text = vec![
        Line::from(Span::styled(format!("  Mode:   {mode_label}"), Style::default().fg(text))),
        Line::from(Span::styled(format!("  Model:  {}", app.model_label), Style::default().fg(text_dim))),
        Line::from(Span::styled(
            format!("  Turns:  {} max", mode_label_turns(app)),
            Style::default().fg(text_dim),
        )),
    ];
    frame.render_widget(Paragraph::new(ctx_text), ctx_chunks[1]);

    // ── Workspace tree ────────────────────────────────
    let tree_block = Block::default()
        .title(" Workspace ")
        .title_alignment(Alignment::Left)
        .borders(Borders::TOP)
        .border_style(Style::default().fg(border_color));
    let tree_inner = tree_block.inner(sections[3]);
    frame.render_widget(tree_block, sections[3]);

    if app.workspace_tree.is_empty() {
        frame.render_widget(
            Paragraph::new("No workspace open").style(Style::default().fg(text_dim)),
            tree_inner,
        );
    } else {
        let visible_count = tree_inner.height as usize;
        let start = app.workspace_tree_scroll;
        let end = (start + visible_count).min(app.workspace_tree.len());
        let visible: Vec<&crate::app::FileEntry> = app.workspace_tree[start..end].iter().collect();

        let tree_lines: Vec<Line> = visible
            .iter()
            .map(|entry| {
                let indent = "  ".repeat(entry.depth);
                let icon = if entry.is_dir { "📁" } else { "📄" };
                let entry_style = if entry.is_dir {
                    Style::default().fg(accent).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(text_dim)
                };
                Line::from(Span::styled(
                    format!("{indent}{icon} {}", entry.name),
                    entry_style,
                ))
            })
            .collect();
        frame.render_widget(Paragraph::new(tree_lines), tree_inner);
    }

    // ── Status dot ────────────────────────────────────
    let status_color = if app.is_running { ok } else { text_dim };
    let status_text = if app.is_running {
        "● Running"
    } else {
        "○ Idle"
    };
    frame.render_widget(
        Paragraph::new(Span::styled(status_text, Style::default().fg(status_color))),
        sections[4],
    );
}

fn mode_label_turns(app: &App) -> &'static str {
    match app.current_mode {
        generic_coder::workflow::AgentMode::Work => "70",
        generic_coder::workflow::AgentMode::Plan => "100",
        generic_coder::workflow::AgentMode::Review => "50",
    }
}
