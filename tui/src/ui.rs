//! UI rendering for Generic Coder TUI using Ratatui.

use ratatui::{
    prelude::*,
    widgets::*,
};

use crate::app::{App, ChatMessage, Dialog, SettingsTab};
use crate::sidebar;
use crate::status;

/// Entry point: called on every frame
pub fn draw(frame: &mut Frame, app: &App) {
    let area = frame.area();

    // ── Color palette ────────────────────────────────
    let bg = Color::Rgb(13, 17, 23);
    let panel_bg = Color::Rgb(22, 27, 34);
    let border_color = Color::Rgb(48, 54, 61);
    let accent = Color::Rgb(217, 141, 106);
    let text = Color::Rgb(230, 237, 243);
    let text_dim = Color::Rgb(139, 148, 158);
    let ok = Color::Rgb(63, 185, 80);
    let err = Color::Rgb(248, 81, 73);

    // ── Full-screen background ────────────────────────
    frame.render_widget(
        Block::new().style(Style::default().bg(bg)),
        area,
    );

    // ── Layout: sidebar | main ────────────────────────
    let has_sidebar = app.sidebar_width > 0;
    let layout = if has_sidebar {
        let chunks = Layout::horizontal([
            Constraint::Length(app.sidebar_width),
            Constraint::Min(0),
        ])
        .split(area);
        (Some(chunks[0]), chunks[1])
    } else {
        (None, area)
    };

    // ── Sidebar ────────────────────────────────────────
    if let Some(sidebar_area) = layout.0 {
        sidebar::draw(frame, sidebar_area, app, bg, panel_bg, border_color, accent, text, text_dim, ok);
    }

    // ── Main area: chat + input + status bar ──────────
    let main_area = layout.1;
    let main_chunks = Layout::vertical([
        Constraint::Min(3),
        Constraint::Length(3),
        Constraint::Length(1),
    ])
    .split(main_area);

    // ── Chat messages ──────────────────────────────────
    let chat_block = Block::default()
        .title(" Chat ")
        .title_alignment(Alignment::Left)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color))
        .style(Style::default().bg(panel_bg));
    let chat_inner = chat_block.inner(main_chunks[0]);
    frame.render_widget(chat_block, main_chunks[0]);
    render_messages(frame, chat_inner, app, bg, panel_bg, accent, text, text_dim, ok, err);

    // ── Input line ─────────────────────────────────────
    let is_insert = matches!(app.input_mode, crate::event::InputMode::Insert);
    let input_bg = if is_insert {
        Color::Rgb(30, 34, 42)
    } else {
        bg
    };
    let input_block = Block::default()
        .title(if is_insert { " Input (Esc to cancel) " } else { " Press Enter to type " })
        .title_alignment(Alignment::Left)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(if is_insert { accent } else { border_color }))
        .style(Style::default().bg(input_bg));
    let input_inner = input_block.inner(main_chunks[1]);
    frame.render_widget(input_block, main_chunks[1]);

    // Render input text with cursor
    let display_text = if app.input.is_empty() && !is_insert {
        "Type your message or /command...".to_string()
    } else if app.input.is_empty() {
        String::new()
    } else {
        app.input.clone()
    };

    let input_style = if is_insert {
        Style::default().fg(text)
    } else {
        Style::default().fg(text_dim)
    };

    if is_insert && !display_text.is_empty() && app.input_cursor < display_text.len() {
        let before = &display_text[..app.input_cursor.min(display_text.len())];
        let cursor_char = &display_text[app.input_cursor.min(display_text.len())..(app.input_cursor + 1).min(display_text.len())];
        let after = if app.input_cursor + 1 < display_text.len() {
            &display_text[app.input_cursor + 1..]
        } else {
            ""
        };

        let before_span = Span::styled(before, input_style);
        let cursor_span = Span::styled(
            if cursor_char.is_empty() { " " } else { cursor_char },
            Style::default().fg(bg).bg(accent),
        );
        let after_span = Span::styled(after, input_style);

        frame.render_widget(
            Paragraph::new(Line::from(vec![before_span, cursor_span, after_span]))
                .scroll((0, 0)),
            input_inner,
        );
    } else {
        frame.render_widget(
            Paragraph::new(display_text).style(input_style),
            input_inner,
        );
    }

    // ── Status bar ─────────────────────────────────────
    status::draw(frame, main_chunks[2], app, bg, border_color, accent, text, text_dim, ok, err);

    // ── Dialogs ────────────────────────────────────────
    match &app.dialog {
        Dialog::Settings(tab) => {
            render_settings_dialog(frame, area, app, *tab, bg, panel_bg, border_color, accent, text, text_dim, ok);
        }
        Dialog::Sessions => {
            render_sessions_dialog(frame, area, app, bg, panel_bg, border_color, accent, text, text_dim, ok);
        }
        Dialog::Help => {
            render_help_dialog(frame, area, bg, panel_bg, border_color, accent, text, ok);
        }
        Dialog::None => {}
    }
}

/// Render chat messages in reverse chronological order (newest first up top)
fn push_wrapped_line(lines: &mut Vec<Line<'static>>, text: &str, max_w: usize, style: Style) {
    if max_w == 0 {
        lines.push(Line::from(Span::styled(text.to_string(), style)));
        return;
    }
    if text.len() > max_w {
        for chunk in text.chars().collect::<Vec<char>>().chunks(max_w) {
            let s: String = chunk.iter().collect();
            lines.push(Line::from(Span::styled(s, style)));
        }
    } else {
        lines.push(Line::from(Span::styled(text.to_string(), style)));
    }
}

fn render_messages(
    frame: &mut Frame,
    area: Rect,
    app: &App,
    _bg: Color,
    _panel_bg: Color,
    accent: Color,
    text: Color,
    text_dim: Color,
    ok: Color,
    _err: Color,
) {
    let msg_count = app.messages.len();
    if msg_count == 0 {
        let welcome = vec![
            Line::from(Span::styled(
                "╔══════════════════════════════════════════╗",
                Style::default().fg(text_dim),
            )),
            Line::from(Span::styled(
                "║  Welcome to Generic Coder TUI            ║",
                Style::default().fg(accent),
            )),
            Line::from(Span::styled(
                "║                                          ║",
                Style::default().fg(text_dim),
            )),
            Line::from(Span::styled(
                "║  Enter     Type & send a message         ║",
                Style::default().fg(text_dim),
            )),
            Line::from(Span::styled(
                "║  F1-F3     Switch Work/Plan/Review mode  ║",
                Style::default().fg(text_dim),
            )),
            Line::from(Span::styled(
                "║  F4        Toggle Multi-Agent            ║",
                Style::default().fg(text_dim),
            )),
            Line::from(Span::styled(
                "║  F5        Toggle One Shot               ║",
                Style::default().fg(text_dim),
            )),
            Line::from(Span::styled(
                "║  F6        Toggle YOLO mode ⚡           ║",
                Style::default().fg(text_dim),
            )),
            Line::from(Span::styled(
                "║  F7        Toggle Auto model routing    ║",
                Style::default().fg(text_dim),
            )),
            Line::from(Span::styled(
                "║  Shift+Tab Cycle reasoning effort        ║",
                Style::default().fg(text_dim),
            )),
            Line::from(Span::styled(
                "║  F8        Toggle Git Changes            ║",
                Style::default().fg(text_dim),
            )),
            Line::from(Span::styled(
                "║  Ctrl+S    Open Settings                 ║",
                Style::default().fg(text_dim),
            )),
            Line::from(Span::styled(
                "║  Ctrl+Q    Quit                          ║",
                Style::default().fg(text_dim),
            )),
            Line::from(Span::styled(
                "║  /help     Show all commands             ║",
                Style::default().fg(text_dim),
            )),
            Line::from(Span::styled(
                "╚══════════════════════════════════════════╝",
                Style::default().fg(text_dim),
            )),
        ];
        frame.render_widget(Paragraph::new(welcome), area);
        return;
    }

    // Render from scroll_offset upward
    let max_y = area.height as usize;
    let start = app.scroll_offset;
    let _end = (start + max_y).min(msg_count);
    let visible: Vec<&ChatMessage> = app.messages.iter().rev().skip(start).take(max_y).collect();

    let mut lines: Vec<Line> = Vec::new();
    for msg in visible.iter().rev() {
        let role_color = if msg.role == "user" { accent } else { ok };
        let role_prefix = if msg.role == "user" { "▶ You" } else { "● Agent" };
        lines.push(Line::from(vec![
            Span::styled(role_prefix, Style::default().fg(role_color).add_modifier(Modifier::BOLD)),
            if msg.streaming {
                Span::styled(" (streaming...)", Style::default().fg(text_dim).add_modifier(Modifier::ITALIC))
            } else {
                Span::raw("")
            },
        ]));

        // Render ACP state if present
        if let Some(acp) = &msg.acp {
            if let Some(plan) = &acp.plan {
                if let Some(steps) = plan.get("steps").and_then(|v| v.as_array()) {
                    lines.push(Line::from(Span::styled(
                        "┌─ ACP Plan ────────────────────",
                        Style::default().fg(Color::Rgb(156, 39, 176)),
                    )));
                    for step in steps {
                        let role = step.get("role").and_then(|v| v.as_str()).unwrap_or("?");
                        let desc = step.get("description").and_then(|v| v.as_str()).unwrap_or("");
                        lines.push(Line::from(vec![
                            Span::styled("│ ", Style::default().fg(text_dim)),
                            Span::styled(format!("[{}] ", role.to_uppercase()), Style::default().fg(accent).add_modifier(Modifier::BOLD)),
                            Span::styled(desc.to_string(), Style::default().fg(text)),
                        ]));
                    }
                    lines.push(Line::from(Span::styled(
                        "└───────────────────────────────",
                        Style::default().fg(text_dim),
                    )));
                }
            }
            if acp.active_step >= 0 {
                lines.push(Line::from(Span::styled(
                    format!("⚡ ACP Step {} executing...", acp.active_step),
                    Style::default().fg(accent).add_modifier(Modifier::ITALIC),
                )));
            }
            if acp.done {
                lines.push(Line::from(Span::styled(
                    "✓ ACP complete",
                    Style::default().fg(ok),
                )));
            }
        }

        // Message content: detect and render <thinking>...</thinking> blocks
        let content = &msg.content;
        let max_w = area.width.saturating_sub(4) as usize;
        let thinking_re_str = "<thinking>";
        let mut remaining = content.as_str();
        while !remaining.is_empty() {
            if let Some(think_start) = remaining.find(thinking_re_str) {
                // Render text before <thinking>
                let before = &remaining[..think_start];
                for line in before.lines() {
                    push_wrapped_line(&mut lines, line, max_w, Style::default().fg(text));
                }
                remaining = &remaining[think_start + "<thinking>".len()..];
                if let Some(think_end) = remaining.find("</thinking>") {
                    // Render thinking block header
                    lines.push(Line::from(vec![
                        Span::styled("💭 Reasoning: ", Style::default().fg(Color::Rgb(180, 150, 60)).add_modifier(Modifier::ITALIC)),
                    ]));
                    let think_content = &remaining[..think_end];
                    for line in think_content.lines() {
                        push_wrapped_line(&mut lines, line, max_w.saturating_sub(2), 
                            Style::default().fg(Color::Rgb(120, 120, 100)).add_modifier(Modifier::ITALIC));
                    }
                    lines.push(Line::from(Span::styled(
                        "   ─ end reasoning ─",
                        Style::default().fg(Color::Rgb(100, 100, 80)),
                    )));
                    remaining = &remaining[think_end + "</thinking>".len()..];
                } else {
                    // Unclosed thinking block (streaming) — render as dim
                    for line in remaining.lines() {
                        push_wrapped_line(&mut lines, line, max_w.saturating_sub(2),
                            Style::default().fg(Color::Rgb(120, 120, 100)).add_modifier(Modifier::ITALIC));
                    }
                    remaining = "";
                }
            } else {
                for line in remaining.lines() {
                    push_wrapped_line(&mut lines, line, max_w, Style::default().fg(text));
                }
                remaining = "";
            }
        }
        lines.push(Line::from(Span::styled(
            "─".repeat(area.width.saturating_sub(2) as usize),
            Style::default().fg(text_dim),
        )));
    }

    frame.render_widget(Paragraph::new(lines), area);
}

// ── Settings Dialog ────────────────────────────────────────────────

fn render_settings_dialog(
    frame: &mut Frame,
    area: Rect,
    app: &App,
    tab: SettingsTab,
    bg: Color,
    panel_bg: Color,
    _border_color: Color,
    accent: Color,
    text: Color,
    text_dim: Color,
    _ok: Color,
) {
    let popup_area = centered_rect(area, 70, 70);
    frame.render_widget(Clear, popup_area);

    let block = Block::default()
        .title(" Settings ")
        .title_alignment(Alignment::Center)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(accent))
        .style(Style::default().bg(panel_bg));
    frame.render_widget(block.clone(), popup_area);

    let inner = block.inner(popup_area);
    let chunks = Layout::vertical([
        Constraint::Length(2),
        Constraint::Min(0),
    ])
    .split(inner);

    // Tab bar
    let tabs = ["Model", "Workspace", "Interface", "Skills"];
    let current_idx = match tab {
        SettingsTab::Model => 0,
        SettingsTab::Workspace => 1,
        SettingsTab::Interface => 2,
        SettingsTab::Skills => 3,
        _ => 0,
    };

    let tab_spans: Vec<Span> = tabs
        .iter()
        .enumerate()
        .map(|(i, name)| {
            if i == current_idx {
                Span::styled(
                    format!(" [{}] ", name),
                    Style::default().fg(bg).bg(accent).add_modifier(Modifier::BOLD),
                )
            } else {
                Span::styled(
                    format!("  {}  ", name),
                    Style::default().fg(text_dim),
                )
            }
        })
        .collect();
    frame.render_widget(
        Paragraph::new(Line::from(tab_spans)).alignment(Alignment::Center),
        chunks[0],
    );

    // Tab content
    match tab {
        SettingsTab::Model => render_model_settings(frame, chunks[1], app, text, text_dim, accent),
        SettingsTab::Workspace => render_workspace_settings(frame, chunks[1], app, text, text_dim, accent),
        SettingsTab::Interface => render_interface_settings(frame, chunks[1], app, text, text_dim),
        SettingsTab::Skills => render_skills_settings(frame, chunks[1], app, text, text_dim, accent),
        _ => {}
    }
}

fn render_model_settings(
    frame: &mut Frame,
    area: Rect,
    app: &App,
    text: Color,
    text_dim: Color,
    _accent: Color,
) {
    let lines = vec![
        Line::from(Span::styled("Model Settings", Style::default().fg(text).add_modifier(Modifier::BOLD))),
        Line::from(""),
        Line::from(Span::styled(
            format!("Configured models: {}", app.models.len()),
            Style::default().fg(text),
        )),
        Line::from(""),
        Line::from(Span::styled("Use the web UI or edit ~/.genericagent/ui_llm_config.json", Style::default().fg(text_dim))),
        Line::from(Span::styled("to add/remove model configurations.", Style::default().fg(text_dim))),
        Line::from(""),
        Line::from(Span::styled("Current model:", Style::default().fg(text))),
        Line::from(Span::styled(
            format!("  {}", app.model_label),
            Style::default().fg(text).add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(Span::styled("Press Ctrl+Left/Right to switch models", Style::default().fg(text_dim))),
    ];
    frame.render_widget(Paragraph::new(lines), area);
}

fn render_workspace_settings(
    frame: &mut Frame,
    area: Rect,
    app: &App,
    text: Color,
    text_dim: Color,
    _accent: Color,
) {
    let ws_path = if app.workspace_path.is_empty() {
        "Not set".to_string()
    } else {
        app.workspace_path.clone()
    };
    let lines = vec![
        Line::from(Span::styled("Workspace Settings", Style::default().fg(text).add_modifier(Modifier::BOLD))),
        Line::from(""),
        Line::from(Span::styled("Active workspace:", Style::default().fg(text))),
        Line::from(Span::styled(
            format!("  Name: {}", app.workspace_name),
            Style::default().fg(text),
        )),
        Line::from(Span::styled(format!("  Path: {ws_path}"), Style::default().fg(text_dim))),
        Line::from(""),
        Line::from(Span::styled("Use /path in chat input to set workspace path.", Style::default().fg(text_dim))),
    ];
    frame.render_widget(Paragraph::new(lines), area);
}

fn render_interface_settings(
    frame: &mut Frame,
    area: Rect,
    app: &App,
    text: Color,
    text_dim: Color,
) {
    let lines = vec![
        Line::from(Span::styled("Interface Settings", Style::default().fg(text).add_modifier(Modifier::BOLD))),
        Line::from(""),
        Line::from(Span::styled(format!("Theme: {}", app.theme_name), Style::default().fg(text))),
        Line::from(Span::styled(format!("Sidebar: {} (Ctrl+W to toggle)", if app.sidebar_width > 0 { "Visible" } else { "Hidden" }), Style::default().fg(text_dim))),
        Line::from(""),
        Line::from(Span::styled("Keyboard Shortcuts:", Style::default().fg(text).add_modifier(Modifier::BOLD))),
        Line::from(Span::styled("  F1-F3    Mode: Work / Plan / Review", Style::default().fg(text_dim))),
        Line::from(Span::styled("  F4       Toggle Multi-Agent", Style::default().fg(text_dim))),
        Line::from(Span::styled("  F5       Toggle One Shot", Style::default().fg(text_dim))),
        Line::from(Span::styled("  F8       Toggle Git Changes", Style::default().fg(text_dim))),
        Line::from(Span::styled("  Ctrl+W   Toggle Sidebar", Style::default().fg(text_dim))),
        Line::from(Span::styled("  Ctrl+S   Settings", Style::default().fg(text_dim))),
        Line::from(Span::styled("  Ctrl+Q   Quit", Style::default().fg(text_dim))),
    ];
    frame.render_widget(Paragraph::new(lines), area);
}

fn render_skills_settings(
    frame: &mut Frame,
    area: Rect,
    app: &App,
    text: Color,
    text_dim: Color,
    accent: Color,
) {
    let mut lines = vec![
        Line::from(Span::styled("Installed Skills", Style::default().fg(text).add_modifier(Modifier::BOLD))),
        Line::from(""),
    ];
    if app.skills_list.is_empty() {
        lines.push(Line::from(Span::styled("No skills installed.", Style::default().fg(text_dim))));
    } else {
        for skill in &app.skills_list {
            let status = if skill.enabled { "✓" } else { "✗" };
            let status_color = if skill.enabled { Color::Rgb(63, 185, 80) } else { Color::Rgb(248, 81, 73) };
            lines.push(Line::from(vec![
                Span::styled(format!(" {status} "), Style::default().fg(status_color).add_modifier(Modifier::BOLD)),
                Span::styled(&skill.name, Style::default().fg(accent).add_modifier(Modifier::BOLD)),
                Span::styled(format!(" v{}", skill.version), Style::default().fg(text_dim)),
            ]));
            lines.push(Line::from(Span::styled(
                format!("    {}", skill.description),
                Style::default().fg(text_dim),
            )));
        }
    }
    frame.render_widget(Paragraph::new(lines), area);
}

// ── Sessions Dialog ────────────────────────────────────────────────

fn render_sessions_dialog(
    frame: &mut Frame,
    area: Rect,
    app: &App,
    _bg: Color,
    panel_bg: Color,
    border_color: Color,
    accent: Color,
    text: Color,
    text_dim: Color,
    _ok: Color,
) {
    let popup_area = centered_rect(area, 68, 58);
    frame.render_widget(Clear, popup_area);

    let block = Block::default()
        .title(" Sessions ")
        .title_alignment(Alignment::Center)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(accent))
        .style(Style::default().bg(panel_bg));
    frame.render_widget(block.clone(), popup_area);

    let inner = block.inner(popup_area);
    let sections = Layout::vertical([Constraint::Min(6), Constraint::Length(8)]).split(inner);
    if app.sessions.is_empty() {
        frame.render_widget(
            Paragraph::new("No saved sessions yet.\n\nSessions now persist locally for both TUI and GUI.\nStart a task, then use Enter to restore, F to fork, or D to delete a saved row.")
                .style(Style::default().fg(text_dim))
                .alignment(Alignment::Center),
            sections[0],
        );
    } else {
        let items: Vec<ListItem> = app
            .sessions
            .iter()
            .enumerate()
            .map(|(i, s)| {
                let style = if i == app.sessions_cursor {
                    Style::default().fg(border_color).bg(accent)
                } else {
                    Style::default().fg(text)
                };
                let prefix = if i == app.sessions_cursor { ">" } else { " " };
                ListItem::new(format!(
                    "{} #{}  {}  rounds={}  checkpoints={}  {}",
                    prefix, s.index, s.preview, s.rounds, s.checkpoint_count, s.time
                ))
                    .style(style)
            })
            .collect();
        let mut list_state = ratatui::widgets::ListState::default();
        list_state.select(Some(app.sessions_cursor));
        frame.render_stateful_widget(
            List::new(items).highlight_style(Style::default().fg(border_color).bg(accent)),
            sections[0],
            &mut list_state,
        );
    }

    let details_block = Block::default()
        .title(" Target ")
        .borders(Borders::TOP)
        .border_style(Style::default().fg(border_color));
    let details_inner = details_block.inner(sections[1]);
    frame.render_widget(details_block, sections[1]);

    let selected_target = app.selected_sessions_dialog_target();
    let checkpoints = app.selected_sessions_dialog_checkpoints();
    let target_label = selected_target
        .map(|(session_index, checkpoint_index)| {
            checkpoint_index
                .map(|checkpoint| format!("session #{session_index} @ checkpoint {checkpoint}"))
                .unwrap_or_else(|| format!("session #{session_index} latest state"))
        })
        .unwrap_or_else(|| "no session selected".into());
    let checkpoint_summary = if checkpoints.is_empty() {
        vec![Line::from(Span::styled("No restore points yet.", Style::default().fg(text_dim)))]
    } else {
        checkpoints
            .iter()
            .take(3)
            .enumerate()
            .map(|(index, checkpoint)| {
                let selected = app.sessions_checkpoint_cursor == index + 1;
                let style = if selected {
                    Style::default().fg(accent).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(text_dim)
                };
                Line::from(Span::styled(
                    format!(
                        "{} checkpoint {} · {} · {} rounds",
                        if selected { ">" } else { " " },
                        checkpoint.index,
                        checkpoint.preview,
                        checkpoint.rounds
                    ),
                    style,
                ))
            })
            .collect()
    };
    let mut detail_lines = vec![
        Line::from(Span::styled(format!("Target: {target_label}"), Style::default().fg(text))),
        Line::from(Span::styled("Left/Right picks latest state or a checkpoint. Enter restores. F forks. D deletes the session.", Style::default().fg(text_dim))),
        Line::from(Span::styled("Recent restore points:", Style::default().fg(text).add_modifier(Modifier::BOLD))),
    ];
    detail_lines.extend(checkpoint_summary);
    frame.render_widget(Paragraph::new(detail_lines), details_inner);
}

// ── Help Dialog ────────────────────────────────────────────────────

fn render_help_dialog(
    frame: &mut Frame,
    area: Rect,
    _bg: Color,
    panel_bg: Color,
    _border_color: Color,
    accent: Color,
    text: Color,
    _ok: Color,
) {
    let popup_area = centered_rect(area, 60, 70);
    frame.render_widget(Clear, popup_area);

    let block = Block::default()
        .title(" Help ")
        .title_alignment(Alignment::Center)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(accent))
        .style(Style::default().bg(panel_bg));
    frame.render_widget(block.clone(), popup_area);

    let inner = block.inner(popup_area);
    let help_text = vec![
        Line::from(Span::styled("Generic Coder TUI — Hotkeys", Style::default().fg(accent).add_modifier(Modifier::BOLD))),
        Line::from(""),
        Line::from(Span::styled("Chat", Style::default().fg(text).add_modifier(Modifier::BOLD))),
        Line::from(Span::styled("  Enter     Send message", Style::default().fg(text))),
        Line::from(Span::styled("  Esc       Stop generation", Style::default().fg(text))),
        Line::from(Span::styled("  ↑/↓       Scroll chat history", Style::default().fg(text))),
        Line::from(Span::styled("  PgUp/Dn   Fast scroll", Style::default().fg(text))),
        Line::from(Span::styled("  Tab       Autocomplete file path", Style::default().fg(text))),
        Line::from(""),
        Line::from(Span::styled("Modes & Toggles", Style::default().fg(text).add_modifier(Modifier::BOLD))),
        Line::from(Span::styled("  F1        Work mode (70 turns)", Style::default().fg(text))),
        Line::from(Span::styled("  F2        Plan mode (100 turns)", Style::default().fg(text))),
        Line::from(Span::styled("  F3        Review mode (50 turns)", Style::default().fg(text))),
        Line::from(Span::styled("  F4        Toggle Multi-Agent", Style::default().fg(text))),
        Line::from(Span::styled("  F5        Toggle One Shot", Style::default().fg(text))),
        Line::from(Span::styled("  F6        Toggle YOLO", Style::default().fg(text))),
        Line::from(Span::styled("  F7        Toggle Auto model routing", Style::default().fg(text))),
        Line::from(Span::styled("  Shift+Tab Cycle reasoning effort", Style::default().fg(text))),
        Line::from(""),
        Line::from(Span::styled("Navigation", Style::default().fg(text).add_modifier(Modifier::BOLD))),
        Line::from(Span::styled("  Ctrl+W    Toggle sidebar", Style::default().fg(text))),
        Line::from(Span::styled("  F8        Toggle Git changes panel", Style::default().fg(text))),
        Line::from(Span::styled("  Ctrl+S    Open settings", Style::default().fg(text))),
        Line::from(Span::styled("  Ctrl+R    Open sessions", Style::default().fg(text))),
        Line::from(""),
        Line::from(Span::styled("Commands", Style::default().fg(text).add_modifier(Modifier::BOLD))),
        Line::from(Span::styled("  /new      New session", Style::default().fg(text))),
        Line::from(Span::styled("  /clear    Clear chat", Style::default().fg(text))),
        Line::from(Span::styled("  /help     Show this help", Style::default().fg(text))),
        Line::from(Span::styled("  /stop     Stop generation", Style::default().fg(text))),
        Line::from(Span::styled("  /refresh  Refresh workspace", Style::default().fg(text))),
        Line::from(Span::styled("  /auto     Toggle auto model routing", Style::default().fg(text))),
        Line::from(Span::styled("  /profiles Show DeepSeek provider presets", Style::default().fg(text))),
        Line::from(Span::styled("  /preset <id> Apply a DeepSeek preset", Style::default().fg(text))),
        Line::from(Span::styled("  /continue <session[@checkpoint]>", Style::default().fg(text))),
        Line::from(Span::styled("  /fork <session[@checkpoint]>", Style::default().fg(text))),
        Line::from(Span::styled("  /delete <session>", Style::default().fg(text))),
        Line::from(""),
        Line::from(Span::styled("System", Style::default().fg(text).add_modifier(Modifier::BOLD))),
        Line::from(Span::styled("  Sessions dialog: ←/→ target checkpoint, Enter restore, F fork, D delete", Style::default().fg(text))),
        Line::from(Span::styled("  Ctrl+Q    Quit", Style::default().fg(text))),
        Line::from(""),
        Line::from(Span::styled("Press any key to close this dialog", Style::default().fg(text).add_modifier(Modifier::ITALIC))),
    ];
    frame.render_widget(Paragraph::new(help_text), inner);
}

// ── Utility ────────────────────────────────────────────────────────

fn centered_rect(area: Rect, percent_x: u16, percent_y: u16) -> Rect {
    let width = (area.width * percent_x / 100).min(area.width);
    let height = (area.height * percent_y / 100).min(area.height);
    let x = (area.width - width) / 2;
    let y = (area.height - height) / 2;
    Rect::new(area.x + x, area.y + y, width, height)
}
