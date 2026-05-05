//! Status bar rendering for Generic Coder TUI.

use ratatui::{
    prelude::*,
    widgets::*,
};

use crate::app::App;

/// Draw the bottom status bar
pub fn draw(
    frame: &mut Frame,
    area: Rect,
    app: &App,
    bg: Color,
    border_color: Color,
    accent: Color,
    text: Color,
    text_dim: Color,
    _ok: Color,
    _err: Color,
) {
    let block = Block::default()
        .borders(Borders::TOP)
        .border_style(Style::default().fg(border_color))
        .style(Style::default().bg(bg));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let status_color = if app.is_running { accent } else { text };
    let status_text = if app.status_msg.is_empty() {
        "Ready"
    } else {
        &app.status_msg
    };

    let chunks = Layout::horizontal([
        Constraint::Percentage(50),
        Constraint::Percentage(50),
    ])
    .split(inner);

    // Left: status message
    frame.render_widget(
        Paragraph::new(Span::styled(status_text, Style::default().fg(status_color))),
        chunks[0],
    );

    // Right: key hints
    let mode_hint = match app.current_mode {
        generic_coder::workflow::AgentMode::Work => "Work",
        generic_coder::workflow::AgentMode::Plan => "Plan",
        generic_coder::workflow::AgentMode::Review => "Review",
    };

    let right_text = format!(
        "{} | Ctrl+S:Settings | Ctrl+W:Sidebar | Ctrl+Q:Quit",
        mode_hint
    );
    frame.render_widget(
        Paragraph::new(Span::styled(right_text, Style::default().fg(text_dim)))
            .alignment(Alignment::Right),
        chunks[1],
    );
}
