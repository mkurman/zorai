use super::*;

fn help_modal_lines(theme: &ThemeTokens) -> Vec<Line<'static>> {
    vec![
        Line::raw(""),
        Line::from(Span::styled("  Navigation", theme.accent_primary)),
        Line::from(vec![
            Span::styled("  Tab / Shift+Tab  ", theme.fg_active),
            Span::styled("Cycle focus: Chat → Sidebar → Input", theme.fg_dim),
        ]),
        Line::from(vec![
            Span::styled("  Ctrl+P           ", theme.fg_active),
            Span::styled("Open command palette (except chat focus)", theme.fg_dim),
        ]),
        Line::from(vec![
            Span::styled("  Ctrl+T           ", theme.fg_active),
            Span::styled("Open thread picker", theme.fg_dim),
        ]),
        Line::from(vec![
            Span::styled("  Ctrl+Q           ", theme.fg_active),
            Span::styled("Open queued messages", theme.fg_dim),
        ]),
        Line::from(vec![
            Span::styled("  Ctrl+N           ", theme.fg_active),
            Span::styled("Open notifications", theme.fg_dim),
        ]),
        Line::from(vec![
            Span::styled("  Ctrl+A           ", theme.fg_active),
            Span::styled("Open approvals center", theme.fg_dim),
        ]),
        Line::from(vec![
            Span::styled("  /participants    ", theme.fg_active),
            Span::styled("Open thread participants modal", theme.fg_dim),
        ]),
        Line::from(vec![
            Span::styled("  /handoff          ", theme.fg_active),
            Span::styled("Open thread handoff modal", theme.fg_dim),
        ]),
        Line::from(vec![
            Span::styled("  /handoff <agent>  ", theme.fg_active),
            Span::styled("Hand off current thread", theme.fg_dim),
        ]),
        Line::from(vec![
            Span::styled("  /handoff return   ", theme.fg_active),
            Span::styled("Return to previous responder", theme.fg_dim),
        ]),
        Line::from(vec![
            Span::styled("  /gateway          ", theme.fg_active),
            Span::styled("Continue thread on a gateway chat", theme.fg_dim),
        ]),
        Line::from(vec![
            Span::styled("  Ctrl+G           ", theme.fg_active),
            Span::styled("Toggle Goal Runner view", theme.fg_dim),
        ]),
        Line::from(vec![
            Span::styled("  /                ", theme.fg_active),
            Span::styled("Open command palette (from any focus)", theme.fg_dim),
        ]),
        Line::raw(""),
        Line::from(Span::styled("  Chat (when focused)", theme.accent_primary)),
        Line::from(vec![
            Span::styled("  ↑ / ↓            ", theme.fg_active),
            Span::styled("Select message", theme.fg_dim),
        ]),
        Line::from(vec![
            Span::styled("  PgUp / PgDn      ", theme.fg_active),
            Span::styled("Scroll chat", theme.fg_dim),
        ]),
        Line::from(vec![
            Span::styled("  Ctrl+D / Ctrl+U  ", theme.fg_active),
            Span::styled("Half-page scroll", theme.fg_dim),
        ]),
        Line::from(vec![
            Span::styled("  Home / End       ", theme.fg_active),
            Span::styled("Scroll to top / bottom", theme.fg_dim),
        ]),
        Line::from(vec![
            Span::styled("  r                ", theme.fg_active),
            Span::styled("Toggle reasoning on selected message", theme.fg_dim),
        ]),
        Line::from(vec![
            Span::styled("  e / Enter        ", theme.fg_active),
            Span::styled("Toggle tool call expansion", theme.fg_dim),
        ]),
        Line::from(vec![
            Span::styled("  c                ", theme.fg_active),
            Span::styled("Copy selected message to clipboard", theme.fg_dim),
        ]),
        Line::from(vec![
            Span::styled("  Ctrl+P           ", theme.fg_active),
            Span::styled(
                "Speak selected/latest assistant message; pause/resume while playing (mpv)",
                theme.fg_dim,
            ),
        ]),
        Line::from(vec![
            Span::styled("  Ctrl+S           ", theme.fg_active),
            Span::styled(
                "Stop active speech playback when audio is playing",
                theme.fg_dim,
            ),
        ]),
        Line::from(vec![
            Span::styled("  Esc              ", theme.fg_active),
            Span::styled("Clear selection", theme.fg_dim),
        ]),
        Line::raw(""),
        Line::from(Span::styled("  Goal Runner", theme.accent_primary)),
        Line::from(vec![
            Span::styled("  t                ", theme.fg_active),
            Span::styled("Toggle live todos section", theme.fg_dim),
        ]),
        Line::from(vec![
            Span::styled("  l                ", theme.fg_active),
            Span::styled("Toggle timeline section", theme.fg_dim),
        ]),
        Line::from(vec![
            Span::styled("  f                ", theme.fg_active),
            Span::styled("Toggle files section", theme.fg_dim),
        ]),
        Line::from(vec![
            Span::styled("  Ctrl+S           ", theme.fg_active),
            Span::styled("Pause/resume selected goal run", theme.fg_dim),
        ]),
        Line::from(vec![
            Span::styled("  a                ", theme.fg_active),
            Span::styled("Open goal actions", theme.fg_dim),
        ]),
        Line::from(vec![
            Span::styled("  r                ", theme.fg_active),
            Span::styled("Accept the goal while awaiting review", theme.fg_dim),
        ]),
        Line::from(vec![
            Span::styled("  Ctrl+R           ", theme.fg_active),
            Span::styled("Hard reject the goal while awaiting review", theme.fg_dim),
        ]),
        Line::from(vec![
            Span::styled("  Shift+R          ", theme.fg_active),
            Span::styled("Refresh open goal metadata from the daemon", theme.fg_dim),
        ]),
        Line::raw(""),
        Line::from(Span::styled("  Input", theme.accent_primary)),
        Line::from(vec![
            Span::styled("  Enter            ", theme.fg_active),
            Span::styled("Send message", theme.fg_dim),
        ]),
        Line::from(vec![
            Span::styled("  Ctrl+Enter      ", theme.fg_active),
            Span::styled("Insert newline", theme.fg_dim),
        ]),
        Line::from(vec![
            Span::styled("  ← → ↑ ↓         ", theme.fg_active),
            Span::styled("Move cursor in textarea", theme.fg_dim),
        ]),
        Line::from(vec![
            Span::styled("  Ctrl+Backspace   ", theme.fg_active),
            Span::styled("Delete word backwards", theme.fg_dim),
        ]),
        Line::from(vec![
            Span::styled("  Ctrl+W           ", theme.fg_active),
            Span::styled("Delete word backwards", theme.fg_dim),
        ]),
        Line::from(vec![
            Span::styled("  Ctrl+U           ", theme.fg_active),
            Span::styled("Clear input line", theme.fg_dim),
        ]),
        Line::from(vec![
            Span::styled("  Ctrl+Z           ", theme.fg_active),
            Span::styled("Undo", theme.fg_dim),
        ]),
        Line::from(vec![
            Span::styled("  Ctrl+Y           ", theme.fg_active),
            Span::styled("Redo", theme.fg_dim),
        ]),
        Line::from(vec![
            Span::styled("  Ctrl+L           ", theme.fg_active),
            Span::styled("Start/stop voice recording", theme.fg_dim),
        ]),
        Line::raw(""),
        Line::from(Span::styled("  Streaming", theme.accent_primary)),
        Line::from(vec![
            Span::styled("  Esc              ", theme.fg_active),
            Span::styled("Show stop prompt (first press)", theme.fg_dim),
        ]),
        Line::from(vec![
            Span::styled("  Esc Esc          ", theme.fg_active),
            Span::styled("Force stop stream (double press within 2s)", theme.fg_dim),
        ]),
        Line::from(vec![
            Span::styled("  Queue modal      ", theme.fg_active),
            Span::styled("↑↓ select  ←→ action  Enter run", theme.fg_dim),
        ]),
        Line::raw(""),
        Line::from(Span::styled("  Error", theme.accent_primary)),
        Line::from(vec![
            Span::styled("  Ctrl+E           ", theme.fg_active),
            Span::styled("Open last error viewer", theme.fg_dim),
        ]),
        Line::raw(""),
        Line::from(Span::styled("  Commands (/)", theme.accent_primary)),
        Line::from(vec![
            Span::styled("  /settings        ", theme.fg_active),
            Span::styled("Open settings panel", theme.fg_dim),
        ]),
        Line::from(vec![
            Span::styled("  /provider        ", theme.fg_active),
            Span::styled("Switch Svarog's provider", theme.fg_dim),
        ]),
        Line::from(vec![
            Span::styled("  /model           ", theme.fg_active),
            Span::styled("Switch Svarog's model", theme.fg_dim),
        ]),
        Line::from(vec![
            Span::styled("  /effort          ", theme.fg_active),
            Span::styled("Set Svarog's reasoning effort", theme.fg_dim),
        ]),
        Line::from(vec![
            Span::styled("  /context [n]     ", theme.fg_active),
            Span::styled("Set the current context window", theme.fg_dim),
        ]),
        Line::from(vec![
            Span::styled("  /thread [agent]  ", theme.fg_active),
            Span::styled("Pick a thread or preselect an agent source", theme.fg_dim),
        ]),
        Line::from(vec![
            Span::styled("  /new [agent]     ", theme.fg_active),
            Span::styled("New conversation (defaults to Svarog)", theme.fg_dim),
        ]),
        Line::from(vec![
            Span::styled("  /goal            ", theme.fg_active),
            Span::styled("Open goal picker / create goal", theme.fg_dim),
        ]),
        Line::from(vec![
            Span::styled("  /new-goal        ", theme.fg_active),
            Span::styled("Open new goal composer", theme.fg_dim),
        ]),
        Line::from(vec![
            Span::styled("  /attach <path>   ", theme.fg_active),
            Span::styled("Attach file to message", theme.fg_dim),
        ]),
        Line::from(vec![
            Span::styled("  /image <prompt>  ", theme.fg_active),
            Span::styled("Generate an image into the current thread", theme.fg_dim),
        ]),
        Line::from(vec![
            Span::styled("  /view            ", theme.fg_active),
            Span::styled("Cycle transcript mode", theme.fg_dim),
        ]),
        Line::from(vec![
            Span::styled("  /status          ", theme.fg_active),
            Span::styled("Show zorai status", theme.fg_dim),
        ]),
        Line::from(vec![
            Span::styled("  /statistics      ", theme.fg_active),
            Span::styled("Open historical statistics modal", theme.fg_dim),
        ]),
        Line::from(vec![
            Span::styled("  /notifications   ", theme.fg_active),
            Span::styled("Open notifications center", theme.fg_dim),
        ]),
        Line::from(vec![
            Span::styled("  /approvals       ", theme.fg_active),
            Span::styled("Open approvals center", theme.fg_dim),
        ]),
        Line::from(vec![
            Span::styled("  /compact         ", theme.fg_active),
            Span::styled("Force compact current thread", theme.fg_dim),
        ]),
        Line::from(vec![
            Span::styled("  /help            ", theme.fg_active),
            Span::styled("This help screen", theme.fg_dim),
        ]),
        Line::from(vec![
            Span::styled("  /explain         ", theme.fg_active),
            Span::styled("Explain latest goal-run decision", theme.fg_dim),
        ]),
        Line::from(vec![
            Span::styled("  /diverge         ", theme.fg_active),
            Span::styled("Prepare divergent session command", theme.fg_dim),
        ]),
        Line::from(vec![
            Span::styled("  /diverge-start   ", theme.fg_active),
            Span::styled("Start divergent session", theme.fg_dim),
        ]),
        Line::from(vec![
            Span::styled("  /diverge-get     ", theme.fg_active),
            Span::styled("Fetch divergent session payload", theme.fg_dim),
        ]),
        Line::from(vec![
            Span::styled("  /quit            ", theme.fg_active),
            Span::styled("Exit TUI", theme.fg_dim),
        ]),
        Line::raw(""),
        Line::from(Span::styled(
            "  Use Up/Down, PgUp/PgDn, Home/End to scroll",
            theme.fg_dim,
        )),
        Line::from(Span::styled("  Press Esc to close", theme.fg_dim)),
    ]
}

pub(super) fn help_modal_text() -> String {
    help_modal_lines(&ThemeTokens::default())
        .into_iter()
        .map(|line| line.to_string())
        .collect::<Vec<_>>()
        .join("\n")
}

pub(super) fn render_help_modal(frame: &mut Frame, area: Rect, scroll: usize, theme: &ThemeTokens) {
    use ratatui::widgets::{Block, BorderType, Borders, Paragraph, Wrap};

    let block = Block::default()
        .title(" KEYBOARD SHORTCUTS ")
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .border_style(theme.accent_secondary);

    let inner = block.inner(area);
    frame.render_widget(Clear, area);
    frame.render_widget(block, area);

    let lines = help_modal_lines(theme);

    let paragraph = Paragraph::new(lines)
        .scroll((scroll.min(u16::MAX as usize) as u16, 0))
        .wrap(Wrap { trim: false });
    frame.render_widget(paragraph, inner);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn help_modal_lists_status_command() {
        let text = help_modal_lines(&ThemeTokens::default())
            .iter()
            .map(|line| line.to_string())
            .collect::<Vec<_>>()
            .join("\n");

        assert!(text.contains("/status"));
        assert!(text.contains("Show zorai status"));
    }

    #[test]
    fn help_modal_lists_notifications_and_approvals_commands() {
        let text = help_modal_lines(&ThemeTokens::default())
            .iter()
            .map(|line| line.to_string())
            .collect::<Vec<_>>()
            .join("\n");

        assert!(text.contains("/notifications"));
        assert!(text.contains("/approvals"));
    }

    #[test]
    fn help_modal_lists_compact_command() {
        let text = help_modal_lines(&ThemeTokens::default())
            .iter()
            .map(|line| line.to_string())
            .collect::<Vec<_>>()
            .join("\n");

        assert!(text.contains("/compact"));
        assert!(text.contains("Force compact current thread"));
    }

    #[test]
    fn help_modal_lists_goal_command_not_goals() {
        let text = help_modal_lines(&ThemeTokens::default())
            .iter()
            .map(|line| line.to_string())
            .collect::<Vec<_>>()
            .join("\n");

        assert!(text.contains("/goal"));
        assert!(!text.contains("/goals"));
    }
}

#[cfg(test)]
mod handoff_discoverability_tests {
    use super::*;

    #[test]
    fn handoff_discoverability_help_lists_modal_push_and_return_forms() {
        let theme = ThemeTokens::default();
        let text = help_modal_lines(&theme)
            .into_iter()
            .map(|line| line.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(text.contains("/handoff"));
        assert!(text.contains("Open thread handoff modal"));
        assert!(text.contains("/handoff <agent>"));
        assert!(text.contains("Hand off current thread"));
        assert!(text.contains("/handoff return"));
        assert!(text.contains("Return to previous responder"));
    }
}
