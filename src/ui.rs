use crate::model::{Aggregated, Provider, ProviderStatus, ProviderUsage, UsageWindow, WindowKey};
use crate::tokens::{TokenSummary, TokenWindow};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, LineGauge, List, ListItem, Paragraph, Wrap};
use ratatui::Frame;
use std::time::Instant;

pub fn render(frame: &mut Frame, state: &Aggregated, tokens: Option<&TokenSummary>, token_scroll: usize) {
    let has_tokens = tokens.is_some();
    let mut constraints = vec![Constraint::Length(3)];
    if has_tokens {
        constraints.push(Constraint::Length(8));
    }
    for p in &state.providers {
        let height = if p.status.is_error() {
            3
        } else {
            // 2 rows per window (bar + meta) + 1 row for notes + 2 rows for borders.
            let n = p.windows.len().max(1) as u16;
            (n * 2 + 1 + 2).min(10)
        };
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
        render_tokens(frame, root[idx], t, token_scroll);
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

fn render_tokens(frame: &mut Frame, area: Rect, summary: &TokenSummary, scroll: usize) {
    // Compute inner area first so the title can show the current scroll position.
    let temp_block = Block::default().borders(Borders::ALL);
    let inner = temp_block.inner(area);
    let visible = inner.height.saturating_sub(1) as usize;
    let end_visible = (scroll + visible).min(summary.rows.len());

    let block = Block::default()
        .borders(Borders::ALL)
        .title(Line::from(vec![
            Span::styled(" tokens ", Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD)),
            Span::styled(format!(" {} ", summary.window.label()), Style::default().fg(Color::DarkGray)),
            if summary.rows.len() > 1 {
                Span::styled(format!("  [{}–{} of {}]", scroll + 1, end_visible, summary.rows.len()), Style::default().fg(Color::DarkGray))
            } else {
                Span::raw("")
            },
        ]));
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

    // Visible window: reserve 1 row for the header.
    let visible = inner.height.saturating_sub(1) as usize;
    let end = (scroll + visible).min(summary.rows.len());
    let start = scroll.min(end);

    for r in &summary.rows[start..end] {
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
        let block = Block::default().borders(Borders::ALL).border_style(border);
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

    // Two rows per window: the bar line (LineGauge), then a meta line (counts + reset).
    let mut windows = p.windows.clone();
    windows.sort_by_key(|w| match w.key { WindowKey::FiveHour => 0, WindowKey::Weekly => 1, WindowKey::Additional(_) => 2 });

    if windows.is_empty() {
        frame.render_widget(Paragraph::new("(no usage windows reported)"), inner);
        return;
    }

    let n = windows.len() as u16;
    let mut constraints: Vec<Constraint> = Vec::new();
    for _ in 0..n {
        constraints.push(Constraint::Length(1)); // bar
        constraints.push(Constraint::Length(1)); // meta line
    }
    constraints.push(Constraint::Min(0)); // notes
    let rows = Layout::default().direction(Direction::Vertical).constraints(constraints).split(inner);

    for (i, w) in windows.iter().enumerate() {
        let bar_row = rows.get((i as u16 * 2) as usize).copied();
        let meta_row = rows.get((i as u16 * 2 + 1) as usize).copied();
        if let Some(r) = bar_row { render_line_gauge(frame, r, w); }
        if let Some(r) = meta_row { render_window_meta(frame, r, w); }
    }

    let notes: Vec<ListItem> = p.notes.iter().take(2).map(|n| ListItem::new(n.clone())).collect();
    let notes_area = rows.get((n * 2) as usize).copied().unwrap_or(inner);
    if !notes.is_empty() && notes_area.height > 0 {
        frame.render_widget(List::new(notes), notes_area);
    }
}

fn render_line_gauge(frame: &mut Frame, area: Rect, w: &UsageWindow) {
    let pct = w.used_percent.unwrap_or(0.0).clamp(0.0, 100.0);
    let color = if pct >= 90.0 { Color::Red } else if pct >= 70.0 { Color::Yellow } else { Color::Green };
    // Label is short: just the percentage. The bar fills the rest of the line.
    // LineGauge centers the label at the position determined by `ratio`, so
    // for a 45% bar, the label appears at ~45% across the line.
    let label = format!("{}  {:>5.1}%  ", w.label, pct);
    let gauge = LineGauge::default()
        .ratio((pct / 100.0).clamp(0.0, 1.0))
        .filled_style(Style::default().fg(Color::Black).bg(color))
        .unfilled_style(Style::default().fg(Color::Black).bg(Color::DarkGray))
        .line_set(ratatui::symbols::line::THICK)
        .label(label);
    frame.render_widget(gauge, area);
}

fn render_window_meta(frame: &mut Frame, area: Rect, w: &UsageWindow) {
    // Right side: counts + reset time, dim.
    let line = Line::from(vec![
        Span::styled(raw_counts(w), Style::default().fg(Color::DarkGray)),
        Span::raw("  "),
        Span::styled(reset_text(w), Style::default().fg(Color::DarkGray)),
    ]);
    frame.render_widget(Paragraph::new(line), area);
}

fn raw_counts(w: &UsageWindow) -> String {
    // Hide counts when both are 0 or missing — just noise.
    match (w.used_raw, w.total_raw) {
        (Some(u), Some(t)) if t > 0 && u > 0 => format!("{}/{}", fmt_num(u), fmt_num(t)),
        (Some(_), Some(t)) if t > 0 => format!("0/{}", fmt_num(t)),
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
    let line = format!("[r] refresh  [j/k] scroll  [q] quit  ·  auth: {}  ·  --since <dur> for token window", auth);
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