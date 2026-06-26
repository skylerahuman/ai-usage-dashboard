use crate::model::{Aggregated, Provider, ProviderStatus, ProviderUsage, UsageWindow, WindowKey};
use crate::tokens::{TokenSummary, TokenWindow};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph, Wrap};
use ratatui::Frame;
use std::time::Instant;

pub fn render(frame: &mut Frame, state: &Aggregated, tokens: Option<&TokenSummary>) {
    let has_tokens = tokens.is_some();
    let mut constraints = vec![Constraint::Length(3)];
    if has_tokens {
        constraints.push(Constraint::Length(8));
    }
    for p in &state.providers {
        let height = if p.status.is_error() { 3 } else { 6 };
        constraints.push(Constraint::Length(height));
    }
    if state.providers.is_empty() {
        constraints.push(Constraint::Min(6));
    }
    constraints.push(Constraint::Length(2));

    let root = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(frame.area());

    let mut idx = 0;
    render_header(frame, root[idx], state);
    idx += 1;
    if let Some(t) = tokens {
        render_tokens(frame, root[idx], t);
        idx += 1;
    }
    render_providers(frame, &root[idx..], state);
    idx += state.providers.len().max(1);
    if idx < root.len() {
        render_footer(frame, root[idx], state);
    }
}

fn render_header(frame: &mut Frame, area: Rect, state: &Aggregated) {
    let last = state
        .last_refresh
        .map(|i| format!("{} ago", fmt_duration(i.elapsed())))
        .unwrap_or_else(|| "never".into());
    let next = state
        .next_refresh
        .map(|i| {
            if i > Instant::now() { fmt_duration(i - Instant::now()) } else { "now".into() }
        })
        .unwrap_or_else(|| "n/a".into());
    let title = Line::from(vec![
        Span::styled("ai-usage-dashboard", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
        Span::raw("  ·  sorted by 5h % used  ·  refreshed "),
        Span::styled(last, Style::default().fg(Color::Green)),
        Span::raw("  ·  next "),
        Span::styled(next, Style::default().fg(Color::Yellow)),
    ]);
    frame.render_widget(Paragraph::new(title).block(Block::default().borders(Borders::ALL)), area);
}

fn render_tokens(frame: &mut Frame, area: Rect, summary: &TokenSummary) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(Line::from(vec![
            Span::styled(" tokens ", Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD)),
            Span::styled(format!(" {} ", summary.window.label()), Style::default().fg(Color::DarkGray)),
        ]));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if summary.rows.is_empty() {
        frame.render_widget(Paragraph::new("(no usage found in pi sessions)"), inner);
        return;
    }

    let header = Line::from(vec![
        Span::styled(format!("{:<20}", "model"), Style::default().add_modifier(Modifier::BOLD)),
        Span::styled(format!("{:>10}", "msgs"), Style::default().add_modifier(Modifier::BOLD)),
        Span::styled(format!("{:>14}", "input"), Style::default().add_modifier(Modifier::BOLD)),
        Span::styled(format!("{:>14}", "output"), Style::default().add_modifier(Modifier::BOLD)),
        Span::styled(format!("{:>14}", "cached"), Style::default().add_modifier(Modifier::BOLD)),
        Span::styled(format!("{:>14}", "total"), Style::default().add_modifier(Modifier::BOLD)),
        Span::styled(format!("{:>12}", "cost"), Style::default().add_modifier(Modifier::BOLD)),
    ]);
    let mut lines: Vec<Line> = vec![header];
    for r in &summary.rows {
        let provider_color = match r.provider {
            Provider::Zai => Color::Cyan,
            Provider::Minimax => Color::Green,
            Provider::Codex => Color::Yellow,
        };
        let is_placeholder = r.total == 0 && r.msgs == 0;
        let style = if is_placeholder {
            Style::default().fg(Color::DarkGray).add_modifier(Modifier::ITALIC)
        } else {
            Style::default().fg(provider_color)
        };
        let suffix = if is_placeholder { "  (no sessions yet)" } else { "" };
        lines.push(Line::from(vec![
            Span::styled(format!("{:<20}", truncate(&r.model, 20)), style),
            Span::styled(format!("{:>10}", r.msgs), style),
            Span::styled(format!("{:>14}", fmt_num(r.input)), style),
            Span::styled(format!("{:>14}", fmt_num(r.output)), style),
            Span::styled(format!("{:>14}", fmt_num(r.cache_read)), style),
            Span::styled(format!("{:>14}", fmt_num(r.total)), style),
            Span::styled(format!("{:>12}", format!("${:.4}", r.cost)), style),
            Span::styled(suffix.to_string(), Style::default().fg(Color::DarkGray)),
        ]));
    }
    let body = Paragraph::new(lines);
    frame.render_widget(body, inner);
}

fn render_providers(frame: &mut Frame, areas: &[Rect], state: &Aggregated) {
    let providers = state.sorted_by_usage();
    if providers.is_empty() {
        if let Some(area) = areas.first() {
            frame.render_widget(Paragraph::new("No providers configured").block(Block::default().borders(Borders::ALL)), *area);
        }
        return;
    }
    for (p, area) in providers.into_iter().zip(areas.iter()) {
        render_provider(frame, *area, p);
    }
}

fn render_provider(frame: &mut Frame, area: Rect, p: &ProviderUsage) {
    let status_style = match &p.status {
        ProviderStatus::Live => Style::default().fg(Color::Green),
        ProviderStatus::Stale => Style::default().fg(Color::Yellow),
        ProviderStatus::CredentialsMissing => Style::default().fg(Color::DarkGray),
        ProviderStatus::Error { .. } => Style::default().fg(Color::Red),
    };

    // Compact 1-line rendering for errored providers (no live data to show).
    if p.status.is_error() {
        let border = if matches!(p.status, ProviderStatus::Error { .. }) {
            Style::default().fg(Color::Red)
        } else {
            Style::default().fg(Color::DarkGray)
        };
        let message = match &p.status {
            ProviderStatus::Error { message } => message.clone(),
            ProviderStatus::CredentialsMissing => p.notes.first().cloned().unwrap_or_else(|| "no credentials".into()),
            _ => "stale".into(),
        };
        let line = Line::from(vec![
            Span::styled(format!(" {} ", p.label), Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            Span::styled(format!(" {} ", p.status.chip()), status_style),
            Span::styled(format!(" {}  ", p.provider.source()), Style::default().fg(Color::DarkGray)),
            Span::styled(message, Style::default().fg(Color::Red)),
        ]);
        let block = Block::default().borders(Borders::ALL).border_style(border).title_bottom(Line::from(""));
        let para = Paragraph::new(line);
        frame.render_widget(para.block(block), area);
        return;
    }

    let block = Block::default()
        .borders(Borders::ALL)
        .title(Line::from(vec![
            Span::styled(format!(" {} ", p.label), Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            Span::styled(format!(" {} ", p.status.chip()), status_style),
            Span::styled(format!(" {} ", p.provider.source()), Style::default().fg(Color::DarkGray)),
        ]));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let mut windows = p.windows.clone();
    windows.sort_by_key(|w| match w.key { WindowKey::FiveHour => 0, WindowKey::Weekly => 1, WindowKey::Additional(_) => 2 });
    let has_5h = windows.iter().any(|w| matches!(w.key, WindowKey::FiveHour));
    let has_weekly = windows.iter().any(|w| matches!(w.key, WindowKey::Weekly));
    let extras: Vec<_> = windows.iter().filter(|w| matches!(w.key, WindowKey::Additional(_))).collect();

    let mut row_constraints: Vec<Constraint> = Vec::new();
    if has_5h { row_constraints.push(Constraint::Length(2)); }
    if has_weekly { row_constraints.push(Constraint::Length(2)); }
    for _ in 0..extras.len().min(2) {
        row_constraints.push(Constraint::Length(1));
    }
    row_constraints.push(Constraint::Min(1));
    if row_constraints.is_empty() {
        frame.render_widget(Paragraph::new("(no usage windows reported)"), inner);
        return;
    }

    let rows = Layout::default().direction(Direction::Vertical).constraints(row_constraints).split(inner);

    let mut row_i = 0;
    for w in &windows {
        if row_i >= rows.len() { break; }
        match w.key {
            WindowKey::FiveHour => { render_window_row(frame, rows[row_i], w); row_i += 1; }
            WindowKey::Weekly => { render_window_row(frame, rows[row_i], w); row_i += 1; }
            WindowKey::Additional(_) => {
                render_extra_row(frame, rows[row_i], w); row_i += 1;
            }
        }
    }

    let mut notes: Vec<ListItem> = p.notes.iter().take(3).map(|n| ListItem::new(n.clone())).collect();
    if notes.is_empty() {
        // Healthy provider: don't show the “no token/credit counts” fallback.
        // Only show that text when something is actually missing on an otherwise-empty row.
    }
    if row_i < rows.len() {
        frame.render_widget(List::new(notes), rows[row_i]);
    }
}

fn render_window_row(frame: &mut Frame, area: Rect, w: &UsageWindow) {
    let pct = w.used_percent.unwrap_or(0.0).clamp(0.0, 100.0);
    let color = if pct >= 90.0 { Color::Red } else if pct >= 70.0 { Color::Yellow } else { Color::Green };
    let row = Layout::default().direction(Direction::Vertical).constraints([Constraint::Length(1), Constraint::Length(1)]).split(area);

    // Top: label + percentage (left), reset countdown (right)
    let left = Line::from(vec![
        Span::styled(format!(" {} ", w.label), Style::default().add_modifier(Modifier::BOLD)),
        Span::styled(format!("{:>5.1}%", pct), Style::default().fg(color).add_modifier(Modifier::BOLD)),
        Span::raw("  "),
        Span::styled(format!("{}", raw_counts(w)), Style::default().fg(Color::DarkGray)),
    ]);
    let right = Line::from(Span::styled(reset_text(w), Style::default().fg(Color::DarkGray)));
    let title_line = line_with_right_span(area.width, left, right);
    frame.render_widget(Paragraph::new(title_line), row[0]);

    // Bottom: a manual bar using block chars, so it always fits the width
    let bar_width = area.width as usize;
    let filled = ((pct / 100.0) * bar_width as f64).round() as usize;
    let empty = bar_width.saturating_sub(filled);
    let bar = format!("{}{}", "█".repeat(filled), "░".repeat(empty));
    let bar_line = Line::from(Span::styled(bar, Style::default().fg(color)));
    frame.render_widget(Paragraph::new(bar_line), row[1]);
}

fn render_extra_row(frame: &mut Frame, area: Rect, w: &UsageWindow) {
    let pct = w.used_percent.unwrap_or(0.0).clamp(0.0, 100.0);
    let color = if pct >= 90.0 { Color::Red } else if pct >= 70.0 { Color::Yellow } else { Color::Blue };
    let line = Line::from(vec![
        Span::styled(format!(" {} ", w.label), Style::default().add_modifier(Modifier::BOLD)),
        Span::styled(format!("{:>5.1}%", pct), Style::default().fg(color)),
        Span::raw("  "),
        Span::styled(reset_text(w), Style::default().fg(Color::DarkGray)),
    ]);
    frame.render_widget(Paragraph::new(line), area);
}

fn line_with_right_span<'a>(width: u16, left: Line<'a>, right: Line<'a>) -> Line<'a> {
    let left_len: usize = left.spans.iter().map(|s| s.content.chars().count()).sum();
    let right_len: usize = right.spans.iter().map(|s| s.content.chars().count()).sum();
    let total = width as usize;
    if total <= left_len + right_len + 1 {
        return left;
    }
    let gap = total - left_len - right_len;
    let mut spans: Vec<Span<'_>> = left.spans.to_vec();
    spans.push(Span::raw(" ".repeat(gap)));
    spans.extend(right.spans.iter().cloned());
    Line::from(spans)
}

fn raw_counts(w: &UsageWindow) -> String {
    // Hide counts when both are 0 or missing — just noise.
    match (w.used_raw, w.total_raw) {
        (Some(u), Some(t)) if t > 0 && u > 0 => format!("{}/{}", fmt_num(u), fmt_num(t)),
        (Some(u), Some(t)) if t > 0 => format!("0/{}", fmt_num(t)),
        (Some(u), _) if u > 0 => fmt_num(u),
        _ => "".into(),
    }
}

fn reset_text(w: &UsageWindow) -> String {
    let Some(reset) = w.reset_at else { return "reset n/a".into(); };
    let now = chrono::Utc::now().timestamp();
    if reset <= now { "resetting now".into() } else { format!("resets in {}", fmt_secs(reset - now)) }
}

fn render_footer(frame: &mut Frame, area: Rect, state: &Aggregated) {
    let auth = state.auth_source.as_deref().unwrap_or("env only / no auth.json");
    let line = format!("[r] refresh  [q] quit  ·  auth: {}  ·  --since <dur> for token window", auth);
    frame.render_widget(Paragraph::new(line).wrap(Wrap { trim: true }), area);
}

fn fmt_duration(d: std::time::Duration) -> String { fmt_secs(d.as_secs() as i64) }

fn fmt_secs(secs: i64) -> String {
    if secs < 60 { format!("{}s", secs.max(0)) }
    else if secs < 3600 { format!("{}m {}s", secs / 60, secs % 60) }
    else if secs < 86400 { format!("{}h {}m", secs / 3600, (secs % 3600) / 60) }
    else { format!("{}d {}h", secs / 86400, (secs % 86400) / 3600) }
}

fn fmt_num(n: i64) -> String {
    let abs = n.abs();
    if abs >= 1_000_000_000 { format!("{:.2}B", n as f64 / 1_000_000_000.0) }
    else if abs >= 1_000_000 { format!("{:.2}M", n as f64 / 1_000_000.0) }
    else if abs >= 1_000 { format!("{:.1}K", n as f64 / 1_000.0) }
    else { n.to_string() }
}

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n { s.to_string() } else { s.chars().take(n - 1).collect::<String>() + "…" }
}

pub fn _phantom_window(_: TokenWindow) {} // keep TokenWindow reachable from main