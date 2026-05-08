//! Status bar rendering for Generic Coder TUI.

use ratatui::{prelude::*, widgets::*};

use crate::app::App;

const DEEPSEEK_PRICING: &[(&str, f64, f64, f64)] = &[
    ("deepseek-v4-pro", 0.003625, 0.435, 0.87),
    ("deepseek-v4-flash", 0.0028, 0.14, 0.28),
    ("deepseek-reasoner", 0.003625, 0.435, 0.87),
    ("deepseek-chat", 0.0028, 0.14, 0.28),
];

fn estimate_cost(model: &str, prompt: u64, completion: u64, cached: u64) -> Option<String> {
    let lm = model.to_lowercase();
    let (_, cache_hit, cache_miss, out) = DEEPSEEK_PRICING
        .iter()
        .find(|(name, ..)| lm.contains(name))?;
    let miss = prompt.saturating_sub(cached) as f64;
    let cost = (cached as f64 / 1_000_000.0) * cache_hit
        + (miss / 1_000_000.0) * cache_miss
        + (completion as f64 / 1_000_000.0) * out;
    Some(if cost < 0.001 {
        "<$0.001".into()
    } else {
        format!("${:.4}", cost)
    })
}

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

    let chunks =
        Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)]).split(inner);

    // Left: status message + token/cost after last task
    let active_model = app
        .auto_route
        .as_ref()
        .map(|route| route.model.as_str())
        .unwrap_or(&app.model_label);
    let left_text = if let Some((pt, ct, ca)) = app.last_usage {
        let cost_str = estimate_cost(active_model, pt, ct, ca)
            .map(|c| format!(" {c}"))
            .unwrap_or_default();
        let session_cost = estimate_cost(
            active_model,
            app.session_usage.prompt_tokens,
            app.session_usage.completion_tokens,
            app.session_usage.cached_tokens,
        )
        .map(|c| format!(" session:{c}"))
        .unwrap_or_default();
        format!("{status_text}  ↑{pt} ↓{ct}💾{ca}{cost_str}{session_cost}")
    } else {
        status_text.to_string()
    };

    frame.render_widget(
        Paragraph::new(Span::styled(left_text, Style::default().fg(status_color))),
        chunks[0],
    );

    // Right: mode | effort | yolo | key hints
    let mode_hint = match app.current_mode {
        generic_coder::workflow::AgentMode::Work => "Work",
        generic_coder::workflow::AgentMode::Plan => "Plan",
        generic_coder::workflow::AgentMode::Review => "Review",
    };

    let effort_hint = app
        .auto_route
        .as_ref()
        .and_then(|route| route.reasoning_effort.as_deref())
        .or(app.reasoning_effort.as_deref())
        .unwrap_or("default");
    let yolo_hint = if app.yolo_enabled { " | YOLO⚡" } else { "" };
    let auto_hint = if app.auto_model_enabled {
        " | Auto"
    } else {
        ""
    };

    let right_text = format!(
        "{} | Effort:{}{}{} | Ctrl+S:Settings | Ctrl+W:Sidebar | Ctrl+Q:Quit",
        mode_hint, effort_hint, auto_hint, yolo_hint
    );
    frame.render_widget(
        Paragraph::new(Span::styled(right_text, Style::default().fg(text_dim)))
            .alignment(Alignment::Right),
        chunks[1],
    );
}
