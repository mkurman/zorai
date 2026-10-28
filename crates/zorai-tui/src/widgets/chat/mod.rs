use ratatui::prelude::*;
use unicode_width::UnicodeWidthStr;

#[cfg(test)]
use crate::state::chat::{AgentMessage, ChatHitTarget, ChatState, RetryPhase, TranscriptMode};
#[cfg(test)]
use crate::theme::ThemeTokens;
#[cfg(test)]
use ratatui::style::Color;

const MESSAGE_PADDING_X: usize = 2;
const MESSAGE_PADDING_Y: usize = 1;
const TOGGLE_BUTTON_HIT_WIDTH: usize = 2;
const SCROLLBAR_WIDTH: u16 = 1;

fn message_padding_y(
    msg: &crate::state::chat::AgentMessage,
    mode: crate::state::chat::TranscriptMode,
) -> usize {
    if mode == crate::state::chat::TranscriptMode::Compact
        && msg.role == crate::state::chat::MessageRole::Tool
        && msg.tool_name.is_some()
    {
        0
    } else {
        MESSAGE_PADDING_Y
    }
}

fn message_margin_y(msg: &crate::state::chat::AgentMessage) -> usize {
    usize::from(msg.role == crate::state::chat::MessageRole::User)
}

#[cfg(test)]
thread_local! {
    static BUILD_RENDERED_LINES_CALLS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    static BUILD_TRANSCRIPT_METRICS_CALLS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    static ASSISTANT_RESPONDER_LABELS_CALLS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

mod build_rendered_lines_to_build_visible_window_from_snapshot_to_apply;
mod render_streaming_markdown_to_message_block_style_to_message_action;
mod resolved_scroll_to_highlight_line_range_to_selected_text_to_selection;
mod selection_point_from_snapshot_to_render;

pub(crate) use build_rendered_lines_to_build_visible_window_from_snapshot_to_apply::*;
pub(crate) use render_streaming_markdown_to_message_block_style_to_message_action::*;
pub(crate) use resolved_scroll_to_highlight_line_range_to_selected_text_to_selection::*;
pub(crate) use selection_point_from_snapshot_to_render::*;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::chat::{AgentThread, ChatAction, MessageRole};

    fn chat_with_messages(messages: Vec<AgentMessage>) -> ChatState {
        let mut chat = ChatState::new();
        chat.reduce(ChatAction::ThreadCreated {
            thread_id: "t1".into(),
            title: "Test".into(),
        });
        chat.reduce(ChatAction::ThreadDetailReceived(AgentThread {
            id: "t1".into(),
            title: "Test".into(),
            messages,
            ..Default::default()
        }));
        chat
    }

    #[path = "chat_handles_empty_state_to_all_file_mutation_tool_rows_use_filename.rs"]
    mod chat_handles_empty_state_to_all_file_mutation_tool_rows_use_filename;
    #[path = "compaction_artifact_lines_use_standard_message_left_padding_to_concierge.rs"]
    mod compaction_artifact_lines_use_standard_message_left_padding_to_concierge;

    #[test]
    fn assistant_markdown_is_not_rendered_twice_for_line_classification() {
        let chat = chat_with_messages(vec![AgentMessage {
            role: MessageRole::Assistant,
            content: "**alpha beta**\n\n- gamma\n- delta".into(),
            ..Default::default()
        }]);

        crate::widgets::message::reset_markdown_render_call_count();
        let _ = build_rendered_lines(&chat, &ThemeTokens::default(), 80, 0, false);

        assert_eq!(
            crate::widgets::message::markdown_render_call_count(),
            1,
            "assistant markdown should be rendered once and classified from the rendered output"
        );
    }

    #[test]
    fn older_history_pending_renders_loading_row() {
        let mut chat = ChatState::new();
        chat.reduce(ChatAction::ThreadDetailReceived(AgentThread {
            id: "t1".into(),
            title: "Test".into(),
            total_message_count: 3,
            loaded_message_start: 1,
            loaded_message_end: 3,
            messages: vec![
                AgentMessage {
                    role: MessageRole::Assistant,
                    content: "second".into(),
                    ..Default::default()
                },
                AgentMessage {
                    role: MessageRole::Assistant,
                    content: "third".into(),
                    ..Default::default()
                },
            ],
            ..Default::default()
        }));
        chat.reduce(ChatAction::SelectThread("t1".into()));
        chat.mark_active_thread_older_page_pending(true, 0, 6);

        let (_, lines) = visible_rendered_lines(
            Rect::new(0, 0, 60, 10),
            &chat,
            &ThemeTokens::default(),
            1,
            false,
        )
        .expect("chat should render");
        let plain = lines
            .into_iter()
            .map(|line| {
                line.line
                    .spans
                    .into_iter()
                    .map(|span| span.content)
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n");

        assert!(
            plain.contains("Loading previous messages"),
            "pending older-page fetch should be visible in the chat transcript: {plain:?}"
        );
    }

    #[test]
    fn rendered_window_survives_overestimated_message_metrics() {
        let chat = chat_with_messages(vec![AgentMessage {
            role: MessageRole::Assistant,
            content: "short answer".into(),
            ..Default::default()
        }]);
        let metrics = TranscriptMetrics {
            total_lines: 80,
            message_line_ranges: vec![(0, 80)],
            responder_labels: vec![Some(zorai_protocol::AGENT_NAME_SWAROG.to_string())],
        };

        let lines = build_rendered_line_window(
            &chat,
            &ThemeTokens::default(),
            80,
            0,
            false,
            0,
            60,
            &metrics,
        );

        assert_eq!(
            lines.len(),
            60,
            "virtual window should preserve one row per requested transcript row"
        );
        let plain = |line: &RenderedChatLine| {
            line.line
                .spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect::<String>()
        };
        assert_eq!(
            lines
                .iter()
                .filter(|line| plain(line).contains(zorai_protocol::AGENT_NAME_SWAROG))
                .count(),
            1,
            "overestimated metrics must not duplicate the responder header"
        );
        assert_eq!(
            lines
                .iter()
                .filter(|line| plain(line).contains("short answer"))
                .count(),
            1,
            "overestimated metrics must not duplicate message content"
        );
        assert!(
            lines[10..]
                .iter()
                .all(|line| matches!(line.kind, RenderedLineKind::Padding)
                    && line.message_index == Some(0)),
            "overestimated metric gaps should render as padding still attributed to the message"
        );
    }

    #[test]
    fn post_fetch_scroll_refresh_reuses_responder_labels_from_snapshot_metrics() {
        let mut chat = ChatState::new();
        chat.reduce(ChatAction::ThreadDetailReceived(AgentThread {
            id: "t1".into(),
            title: "Test".into(),
            total_message_count: 640,
            loaded_message_start: 0,
            loaded_message_end: 640,
            messages: (0..640)
                .map(|index| AgentMessage {
                    id: Some(format!("msg-{index}")),
                    role: MessageRole::Assistant,
                    content: format!("message {index}\nmore detail {index}"),
                    ..Default::default()
                })
                .collect(),
            created_at: 1,
            ..Default::default()
        }));
        chat.reduce(ChatAction::SelectThread("t1".into()));

        let area = Rect::new(0, 0, 80, 24);
        let snapshot = build_selection_snapshot(area, &chat, &ThemeTokens::default(), 0, false)
            .expect("large loaded transcript should build a snapshot");

        reset_assistant_responder_labels_call_count();
        chat.reduce(ChatAction::ScrollChat(240));
        let refreshed = refresh_cached_snapshot_window(
            &snapshot,
            area,
            &chat,
            &ThemeTokens::default(),
            0,
            false,
        );

        assert!(
            refreshed.is_some(),
            "scroll refresh should rebuild the visible window from cached transcript metrics"
        );
        assert_eq!(
            assistant_responder_labels_call_count(),
            0,
            "post-fetch scroll refresh should not recompute responder labels for every loaded message"
        );
    }

    #[test]
    fn scrollbar_pointer_math_reuses_cached_snapshot_metrics() {
        let mut chat = ChatState::new();
        chat.reduce(ChatAction::ThreadDetailReceived(AgentThread {
            id: "t1".into(),
            title: "Test".into(),
            total_message_count: 640,
            loaded_message_start: 0,
            loaded_message_end: 640,
            messages: (0..640)
                .map(|index| AgentMessage {
                    id: Some(format!("msg-{index}")),
                    role: MessageRole::Assistant,
                    content: format!("message {index}\nmore detail {index}"),
                    ..Default::default()
                })
                .collect(),
            created_at: 1,
            ..Default::default()
        }));
        chat.reduce(ChatAction::SelectThread("t1".into()));

        let area = Rect::new(0, 0, 80, 24);
        let snapshot = build_selection_snapshot(area, &chat, &ThemeTokens::default(), 0, false)
            .expect("large loaded transcript should build a snapshot");

        reset_build_transcript_metrics_call_count();
        let layout = scrollbar_layout_from_cached_snapshot(&snapshot, &chat, 0, false)
            .expect("cached snapshot should expose scrollbar geometry");
        let target = scrollbar_scroll_offset_for_pointer_from_cached_snapshot(
            &snapshot,
            &chat,
            0,
            false,
            layout.thumb.y.saturating_add(3),
            1,
        )
        .expect("cached snapshot should map pointer rows to scroll offsets");

        assert!(target <= layout.max_scroll);
        assert_eq!(
            build_transcript_metrics_call_count(),
            0,
            "scrollbar hover/drag math should not rebuild transcript metrics when a snapshot exists"
        );
    }

    #[test]
    fn scrollbar_does_not_include_unloaded_older_messages() {
        let mut chat = ChatState::new();
        chat.reduce(ChatAction::ThreadDetailReceived(AgentThread {
            id: "t1".into(),
            title: "Test".into(),
            total_message_count: 100,
            loaded_message_start: 98,
            loaded_message_end: 100,
            messages: (98..100)
                .map(|index| AgentMessage {
                    id: Some(format!("msg-{index}")),
                    role: MessageRole::Assistant,
                    content: format!("message {index}"),
                    ..Default::default()
                })
                .collect(),
            created_at: 1,
            ..Default::default()
        }));
        chat.reduce(ChatAction::SelectThread("t1".into()));

        let area = Rect::new(0, 0, 80, 24);
        let snapshot = build_selection_snapshot(area, &chat, &ThemeTokens::default(), 0, false)
            .expect("loaded messages should build a snapshot");

        assert!(
            scrollbar_layout_from_cached_snapshot(&snapshot, &chat, 0, false).is_none(),
            "chat scrollbar must describe rendered rows only; unloaded older rows are fetched by scroll state, not painted as fake rows"
        );
    }

    #[test]
    fn intersecting_message_range_finds_only_visible_blocks() {
        let ranges = vec![(0, 4), (4, 9), (9, 15), (15, 21), (21, 30)];

        assert_eq!(intersecting_message_range(&ranges, 0, 1), 0..1);
        assert_eq!(intersecting_message_range(&ranges, 5, 16), 1..4);
        assert_eq!(intersecting_message_range(&ranges, 30, 35), 5..5);
        assert_eq!(intersecting_message_range(&ranges, 12, 12), 0..0);
    }

    #[test]
    fn compaction_artifact_metrics_match_rendered_notice_lines() {
        let msg = AgentMessage {
            role: MessageRole::System,
            content: "Pre-compaction context: ~92,000 / 200,000 tokens (threshold 160,000)\nTrigger: token-threshold\nStrategy: rule based".into(),
            message_kind: "compaction_artifact".into(),
            compaction_payload: Some("Preserved project facts and pending tasks.".into()),
            ..Default::default()
        };
        let theme = ThemeTokens::default();
        let width = 72usize;
        let expanded = std::collections::HashSet::new();
        let expanded_tools = std::collections::HashSet::new();

        let rendered = crate::widgets::message::message_to_lines(
            &msg,
            0,
            TranscriptMode::Compact,
            &theme,
            padded_content_width(width),
            &expanded,
            &expanded_tools,
        )
        .len();
        let estimated = estimated_message_content_line_count(
            &msg,
            0,
            TranscriptMode::Compact,
            &theme,
            padded_content_width(width),
            &expanded,
            &expanded_tools,
            false,
        );

        assert_eq!(
            estimated, rendered,
            "compaction notice metrics must match rendered rows so windowed rendering does not duplicate button/content rows"
        );
    }

    #[test]
    fn operator_question_metrics_match_rendered_question_lines() {
        let msg = AgentMessage {
            role: MessageRole::Assistant,
            content: "Approve this slice?\nA - proceed\nB - revise".into(),
            is_operator_question: true,
            operator_question_id: Some("oq-1".into()),
            ..Default::default()
        };
        let theme = ThemeTokens::default();
        let width = 72usize;
        let expanded = std::collections::HashSet::new();
        let expanded_tools = std::collections::HashSet::new();

        let rendered = crate::widgets::message::message_to_lines(
            &msg,
            0,
            TranscriptMode::Compact,
            &theme,
            padded_content_width(width),
            &expanded,
            &expanded_tools,
        )
        .len();
        let estimated = estimated_message_content_line_count(
            &msg,
            0,
            TranscriptMode::Compact,
            &theme,
            padded_content_width(width),
            &expanded,
            &expanded_tools,
            false,
        );

        assert_eq!(
            estimated, rendered,
            "operator question metrics must match rendered rows so responder headers and option rows are not repeated"
        );
    }
}
