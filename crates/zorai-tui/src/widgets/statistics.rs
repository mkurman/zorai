use chrono::{Local, TimeZone};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::prelude::{Frame, Position, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph, Wrap};

use crate::state::statistics::StatisticsTab;
use crate::theme::ThemeTokens;

pub enum StatisticsHitTarget {
    Tab(StatisticsTab),
    Window(zorai_protocol::AgentStatisticsWindow),
}

pub fn format_statistics_body(
    snapshot: &zorai_protocol::AgentStatisticsSnapshot,
    tab: StatisticsTab,
) -> String {
    match tab {
        StatisticsTab::Overview => format_overview(snapshot),
        StatisticsTab::Providers => format_providers(snapshot),
        StatisticsTab::Models => format_models(snapshot),
        StatisticsTab::Rankings => format_rankings(snapshot),
    }
}

pub fn render(
    frame: &mut Frame,
    area: Rect,
    snapshot: Option<&zorai_protocol::AgentStatisticsSnapshot>,
    loading: bool,
    error: Option<&str>,
    active_tab: StatisticsTab,
    active_window: zorai_protocol::AgentStatisticsWindow,
    scroll: usize,
    theme: &ThemeTokens,
) {
    let block = Block::default()
        .title(" STATISTICS ")
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .border_style(theme.accent_primary);
    let inner = block.inner(area);

    frame.render_widget(Clear, area);
    frame.render_widget(block, area);

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .split(inner);

    frame.render_widget(Paragraph::new(tab_line(active_tab, theme)), layout[0]);
    frame.render_widget(Paragraph::new(window_line(active_window, theme)), layout[1]);

    let body = if loading {
        "Loading historical statistics...".to_string()
    } else if let Some(error) = error {
        format!("Statistics request failed\n=========================\n{error}")
    } else if let Some(snapshot) = snapshot {
        format_statistics_body(snapshot, active_tab)
    } else {
        "No statistics available.".to_string()
    };

    frame.render_widget(
        Paragraph::new(body)
            .wrap(Wrap { trim: false })
            .scroll((scroll.min(u16::MAX as usize) as u16, 0)),
        layout[2],
    );

    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("Esc", theme.fg_active),
            Span::styled(" close  ", theme.fg_dim),
            Span::styled("h/l", theme.fg_active),
            Span::styled(" tabs  ", theme.fg_dim),
            Span::styled("[/]", theme.fg_active),
            Span::styled(" filters  ", theme.fg_dim),
            Span::styled("j/k", theme.fg_active),
            Span::styled(" scroll", theme.fg_dim),
        ])),
        layout[3],
    );
}

pub fn hit_test(area: Rect, position: Position) -> Option<StatisticsHitTarget> {
    if area.width <= 2 || area.height <= 2 {
        return None;
    }
    let inner = Rect {
        x: area.x + 1,
        y: area.y + 1,
        width: area.width.saturating_sub(2),
        height: area.height.saturating_sub(2),
    };

    if position.y == inner.y {
        for (tab, rect) in tab_regions(inner) {
            if contains(rect, position) {
                return Some(StatisticsHitTarget::Tab(tab));
            }
        }
    }

    if position.y == inner.y.saturating_add(1) {
        for (window, rect) in window_regions(inner) {
            if contains(rect, position) {
                return Some(StatisticsHitTarget::Window(window));
            }
        }
    }

    None
}

fn contains(rect: Rect, position: Position) -> bool {
    position.x >= rect.x
        && position.x < rect.x.saturating_add(rect.width)
        && position.y >= rect.y
        && position.y < rect.y.saturating_add(rect.height)
}

fn tab_line(active_tab: StatisticsTab, theme: &ThemeTokens) -> Line<'static> {
    let mut spans = Vec::new();
    for (index, tab) in StatisticsTab::ALL.iter().copied().enumerate() {
        if index > 0 {
            spans.push(Span::raw(" "));
        }
        spans.push(Span::styled(
            format!(" {} ", tab.label()),
            chip_style(tab == active_tab, theme, theme.accent_primary),
        ));
    }
    Line::from(spans)
}

fn window_line(
    active_window: zorai_protocol::AgentStatisticsWindow,
    theme: &ThemeTokens,
) -> Line<'static> {
    let mut spans = vec![Span::styled(" Window ", theme.fg_dim)];
    for (index, window) in statistics_windows().iter().copied().enumerate() {
        if index > 0 {
            spans.push(Span::raw(" "));
        }
        spans.push(Span::styled(
            format!(" {} ", window_label(window)),
            chip_style(window == active_window, theme, theme.accent_secondary),
        ));
    }
    Line::from(spans)
}

fn chip_style(active: bool, theme: &ThemeTokens, accent: Style) -> Style {
    if active {
        accent
    } else {
        theme.fg_dim
    }
}

fn tab_regions(inner: Rect) -> Vec<(StatisticsTab, Rect)> {
    let mut x = inner.x;
    let mut regions = Vec::new();
    for tab in StatisticsTab::ALL {
        let width = (tab.label().len() + 2) as u16;
        regions.push((
            tab,
            Rect {
                x,
                y: inner.y,
                width,
                height: 1,
            },
        ));
        x = x.saturating_add(width + 1);
    }
    regions
}

fn window_regions(inner: Rect) -> Vec<(zorai_protocol::AgentStatisticsWindow, Rect)> {
    let mut x = inner.x + " Window ".len() as u16;
    let mut regions = Vec::new();
    for window in statistics_windows() {
        let width = (window_label(window).len() + 2) as u16;
        regions.push((
            window,
            Rect {
                x,
                y: inner.y.saturating_add(1),
                width,
                height: 1,
            },
        ));
        x = x.saturating_add(width + 1);
    }
    regions
}

fn statistics_windows() -> [zorai_protocol::AgentStatisticsWindow; 4] {
    [
        zorai_protocol::AgentStatisticsWindow::Today,
        zorai_protocol::AgentStatisticsWindow::Last7Days,
        zorai_protocol::AgentStatisticsWindow::Last30Days,
        zorai_protocol::AgentStatisticsWindow::All,
    ]
}

fn window_label(window: zorai_protocol::AgentStatisticsWindow) -> &'static str {
    match window {
        zorai_protocol::AgentStatisticsWindow::Today => "Today",
        zorai_protocol::AgentStatisticsWindow::Last7Days => "7d",
        zorai_protocol::AgentStatisticsWindow::Last30Days => "30d",
        zorai_protocol::AgentStatisticsWindow::All => "All",
    }
}

fn format_overview(snapshot: &zorai_protocol::AgentStatisticsSnapshot) -> String {
    let mut body = String::new();
    body.push_str("Totals\n");
    body.push_str("------\n");
    body.push_str(&format!(
        "Input tokens:      {}\nOutput tokens:     {}\nTotal tokens:      {}\nTotal cost:        ${:.6}\nProviders:         {}\nModels:            {}\nGenerated at:      {}\n",
        format_statistics_token_count(snapshot.totals.input_tokens),
        format_statistics_token_count(snapshot.totals.output_tokens),
        format_statistics_token_count(snapshot.totals.total_tokens),
        snapshot.totals.cost_usd,
        snapshot.totals.provider_count,
        snapshot.totals.model_count,
        format_generated_at(snapshot.generated_at),
    ));
    body.push('\n');
    if snapshot.has_incomplete_cost_history {
        body.push_str("Warning: historical cost is incomplete for this window. Older rows without stored cost are counted as $0.\n\n");
    }

    body.push_str("Top Models By Tokens\n");
    body.push_str("--------------------\n");
    for (index, row) in snapshot.top_models_by_tokens.iter().enumerate() {
        body.push_str(&format!(
            "{}. {}/{}  {} tok  ${:.6}\n",
            index + 1,
            row.provider,
            row.model,
            format_statistics_token_value(row.total_tokens),
            row.cost_usd,
        ));
    }
    body.push('\n');
    body.push_str("Top Models By Cost\n");
    body.push_str("------------------\n");
    for (index, row) in snapshot.top_models_by_cost.iter().enumerate() {
        body.push_str(&format!(
            "{}. {}/{}  ${:.6}  {} tok\n",
            index + 1,
            row.provider,
            row.model,
            row.cost_usd,
            format_statistics_token_value(row.total_tokens),
        ));
    }
    body
}

fn format_providers(snapshot: &zorai_protocol::AgentStatisticsSnapshot) -> String {
    let mut body =
        String::from("Provider                 In           Out         Total        Cost\n");
    body.push_str("------------------------------------------------------------------\n");
    for row in &snapshot.providers {
        body.push_str(&format!(
            "{:<22} {:>12} {:>12} {:>12}  ${:>9.6}\n",
            row.provider,
            format_statistics_token_count(row.input_tokens),
            format_statistics_token_count(row.output_tokens),
            format_statistics_token_count(row.total_tokens),
            row.cost_usd,
        ));
    }
    body
}

fn format_models(snapshot: &zorai_protocol::AgentStatisticsSnapshot) -> String {
    let mut body = String::from(
        "Provider         Model                         In           Out         Total        Cost\n",
    );
    body.push_str(
        "---------------------------------------------------------------------------------------\n",
    );
    for row in &snapshot.models {
        body.push_str(&format!(
            "{:<16} {:<28} {:>12} {:>12} {:>12}  ${:>9.6}\n",
            row.provider,
            row.model,
            format_statistics_token_count(row.input_tokens),
            format_statistics_token_count(row.output_tokens),
            format_statistics_token_count(row.total_tokens),
            row.cost_usd,
        ));
    }
    body
}

fn format_rankings(snapshot: &zorai_protocol::AgentStatisticsSnapshot) -> String {
    let mut body = String::from("Top 5 By Total Tokens\n");
    body.push_str("---------------------\n");
    for (index, row) in snapshot.top_models_by_tokens.iter().enumerate() {
        body.push_str(&format!(
            "{}. {:<16} {:<24} {:>12}  ${:>9.6}\n",
            index + 1,
            row.provider,
            row.model,
            format_statistics_token_count(row.total_tokens),
            row.cost_usd,
        ));
    }
    body.push('\n');
    body.push_str("Top 5 By Cost\n");
    body.push_str("-------------\n");
    for (index, row) in snapshot.top_models_by_cost.iter().enumerate() {
        body.push_str(&format!(
            "{}. {:<16} {:<24} ${:>9.6}  {:>12}\n",
            index + 1,
            row.provider,
            row.model,
            row.cost_usd,
            format_statistics_token_count(row.total_tokens),
        ));
    }
    body
}

fn format_statistics_token_count(tokens: u64) -> String {
    format!("{} tok", format_statistics_token_value(tokens))
}

fn format_statistics_token_value(tokens: u64) -> String {
    const TOKEN_UNITS: [&str; 6] = ["", "k", "M", "B", "T", "P"];

    if tokens < 1_000 {
        return tokens.to_string();
    }

    let mut value = tokens as f64;
    let mut unit_index = 0usize;

    while value >= 999.995 && unit_index + 1 < TOKEN_UNITS.len() {
        value /= 1_000.0;
        unit_index += 1;
    }

    format!("{value:.2}{}", TOKEN_UNITS[unit_index])
}

fn format_generated_at(timestamp_ms: u64) -> String {
    let Some(timestamp_ms) = i64::try_from(timestamp_ms).ok() else {
        return timestamp_ms.to_string();
    };
    let Some(local_time) = Local.timestamp_millis_opt(timestamp_ms).single() else {
        return timestamp_ms.to_string();
    };
    local_time.format("%Y-%m-%d %H:%M:%S %Z").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_snapshot() -> zorai_protocol::AgentStatisticsSnapshot {
        zorai_protocol::AgentStatisticsSnapshot {
            window: zorai_protocol::AgentStatisticsWindow::All,
            generated_at: 1_704_198_896_000,
            has_incomplete_cost_history: false,
            totals: zorai_protocol::AgentStatisticsTotals {
                input_tokens: 1_234,
                output_tokens: 5_678_901,
                total_tokens: 9_876_543_210,
                cost_usd: 12.34,
                provider_count: 2,
                model_count: 3,
            },
            providers: vec![zorai_protocol::ProviderStatisticsRow {
                provider: "openai".to_string(),
                input_tokens: 12_345,
                output_tokens: 67_890,
                total_tokens: 80_235,
                cost_usd: 0.42,
            }],
            models: vec![zorai_protocol::ModelStatisticsRow {
                provider: "openai".to_string(),
                model: "gpt-5.4".to_string(),
                input_tokens: 1_234_567,
                output_tokens: 8_765_432,
                total_tokens: 9_999_999,
                cost_usd: 1.23,
            }],
            top_models_by_tokens: vec![zorai_protocol::ModelStatisticsRow {
                provider: "github-copilot".to_string(),
                model: "gpt-5.4".to_string(),
                input_tokens: 10,
                output_tokens: 20,
                total_tokens: 31_077_560,
                cost_usd: 0.0,
            }],
            top_models_by_cost: vec![zorai_protocol::ModelStatisticsRow {
                provider: "alibaba-coding-plan".to_string(),
                model: "qwen3.6-plus".to_string(),
                input_tokens: 10,
                output_tokens: 20,
                total_tokens: 36_022_724,
                cost_usd: 0.5,
            }],
            daily: vec![],
            sessions: vec![],
            session_total: 0,
            session_limit: 25,
            session_offset: 0,
            content: Default::default(),
        }
    }

    #[test]
    fn overview_formats_compact_tokens_and_local_timestamp() {
        let snapshot = sample_snapshot();
        let expected_generated_at = Local
            .timestamp_millis_opt(snapshot.generated_at as i64)
            .single()
            .expect("valid local timestamp")
            .format("%Y-%m-%d %H:%M:%S %Z")
            .to_string();

        let body = format_statistics_body(&snapshot, StatisticsTab::Overview);

        assert!(body.contains("Input tokens:      1.23k tok"));
        assert!(body.contains("Output tokens:     5.68M tok"));
        assert!(body.contains("Total tokens:      9.88B tok"));
        assert!(body.contains("31.08M tok"));
        assert!(body.contains("36.02M tok"));
        assert!(!body.contains(&snapshot.generated_at.to_string()));
        assert!(body.contains(&format!("Generated at:      {expected_generated_at}")));
    }

    #[test]
    fn providers_tab_formats_compact_token_columns() {
        let snapshot = sample_snapshot();

        let body = format_statistics_body(&snapshot, StatisticsTab::Providers);

        assert!(body.contains("12.35k tok"));
        assert!(body.contains("67.89k tok"));
        assert!(body.contains("80.23k tok"));
    }

    #[test]
    fn models_tab_formats_compact_token_columns() {
        let snapshot = sample_snapshot();

        let body = format_statistics_body(&snapshot, StatisticsTab::Models);

        assert!(body.contains("1.23M tok"));
        assert!(body.contains("8.77M tok"));
        assert!(body.contains("10.00M tok"));
    }

    #[test]
    fn rankings_tab_formats_compact_token_columns() {
        let snapshot = sample_snapshot();

        let body = format_statistics_body(&snapshot, StatisticsTab::Rankings);

        assert!(body.contains("31.08M tok"));
        assert!(body.contains("36.02M tok"));
    }

    #[test]
    fn statistics_token_value_keeps_small_counts_plain() {
        assert_eq!(format_statistics_token_value(999), "999");
    }
}
