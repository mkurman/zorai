use super::super::chat_with_messages;
use super::super::*;
use crate::state::chat::{AgentMessage, ChatAction, ChatState, MessageRole, RetryPhase};
use crate::theme::ThemeTokens;
use ratatui::layout::Rect;
#[test]
fn create_file_tool_row_uses_filename_when_path_is_missing() {
    let chat = chat_with_messages(vec![AgentMessage {
        role: MessageRole::Tool,
        tool_name: Some("create_file".into()),
        tool_arguments: Some(
            serde_json::json!({
                "cwd": "/tmp/project/src",
                "filename": "types.rs",
                "path": "",
                "content": "pub struct Demo;"
            })
            .to_string(),
        ),
        tool_status: Some("done".into()),
        content: "created".into(),
        ..Default::default()
    }]);

    let (lines, _) = build_rendered_lines(&chat, &ThemeTokens::default(), 80, 0, false);
    let tool_line = lines
        .iter()
        .find(|line| {
            line.message_index == Some(0) && matches!(line.kind, RenderedLineKind::ToolToggle)
        })
        .expect("tool row should be rendered");
    let text = rendered_line_plain_text(tool_line);

    assert!(text.contains("Create File"));
    assert!(
        text.contains("[types.rs]"),
        "expected filename chip, got: {text}"
    );
    assert!(
        !text.contains("[]"),
        "unexpected empty filename chip: {text}"
    );
}

#[test]
fn apply_file_patch_tool_row_uses_filename_chip() {
    let chat = chat_with_messages(vec![AgentMessage {
        role: MessageRole::Tool,
        tool_name: Some("apply_file_patch".into()),
        tool_arguments: Some(
            serde_json::json!({
                "path": "/tmp/demo.txt",
                "edits": [
                    {
                        "old_text": "old",
                        "new_text": "new"
                    }
                ]
            })
            .to_string(),
        ),
        tool_status: Some("done".into()),
        content: "patched".into(),
        ..Default::default()
    }]);

    let (lines, _) = build_rendered_lines(&chat, &ThemeTokens::default(), 80, 0, false);
    let tool_line = lines
        .iter()
        .find(|line| {
            line.message_index == Some(0) && matches!(line.kind, RenderedLineKind::ToolToggle)
        })
        .expect("tool row should be rendered");
    let text = rendered_line_plain_text(tool_line);

    assert!(text.contains("Apply File Patch"));
    assert!(text.contains("[demo.txt]"));
    assert!(!text.contains("[/tmp/demo.txt]"));
}

#[test]
fn invalid_tool_arguments_do_not_create_file_chip() {
    let chat = chat_with_messages(vec![AgentMessage {
        role: MessageRole::Tool,
        tool_name: Some("read_file".into()),
        tool_arguments: Some("{not json}".into()),
        tool_status: Some("done".into()),
        content: "file contents".into(),
        ..Default::default()
    }]);

    let (lines, _) = build_rendered_lines(&chat, &ThemeTokens::default(), 80, 0, false);
    let tool_line = lines
        .iter()
        .find(|line| {
            line.message_index == Some(0) && matches!(line.kind, RenderedLineKind::ToolToggle)
        })
        .expect("tool row should be rendered");
    let text = rendered_line_plain_text(tool_line);

    assert!(text.contains("Read File"));
    assert!(!text.contains("[/tmp/demo.txt]"));
}

#[test]
fn hit_test_returns_tool_file_path_target() {
    let chat = chat_with_messages(vec![AgentMessage {
        role: MessageRole::Tool,
        tool_name: Some("read_file".into()),
        tool_arguments: Some(r#"{"path":"/tmp/demo.txt"}"#.into()),
        tool_status: Some("done".into()),
        content: "file contents".into(),
        ..Default::default()
    }]);

    let area = Rect::new(0, 0, 100, 6);
    let (inner, visible) = visible_rendered_lines(area, &chat, &ThemeTokens::default(), 0, false)
        .expect("chat should produce visible lines");
    let tool_row = visible
        .iter()
        .position(|line| {
            line.message_index == Some(0) && matches!(line.kind, RenderedLineKind::ToolToggle)
        })
        .expect("tool row should be visible");
    let hit_line = &visible[tool_row];
    let (plain, content_start, _) = rendered_line_content_bounds(hit_line);
    let chip_col = plain
        .find("[demo.txt]")
        .expect("path chip should be rendered on the tool row");

    let chip_hit = hit_test(
        area,
        &chat,
        &ThemeTokens::default(),
        0,
        Position::new(inner.x + chip_col as u16 + 1, inner.y + tool_row as u16),
    );
    let row_hit = hit_test(
        area,
        &chat,
        &ThemeTokens::default(),
        0,
        Position::new(
            inner.x + content_start as u16 + 4,
            inner.y + tool_row as u16,
        ),
    );

    assert_eq!(
        chip_hit,
        Some(ChatHitTarget::ToolFilePath { message_index: 0 })
    );
    assert_eq!(row_hit, Some(ChatHitTarget::Message(0)));
}

#[test]
fn hit_test_returns_tool_file_path_target_for_read_skill() {
    let chat = chat_with_messages(vec![AgentMessage {
        role: MessageRole::Tool,
        tool_name: Some("read_skill".into()),
        tool_arguments: Some(r#"{"skill":"systematic-debugging"}"#.into()),
        tool_status: Some("done".into()),
        content: serde_json::json!({
            "skills_root": "/tmp/skills",
            "path": "development/superpowers/systematic-debugging/SKILL.md",
            "content": "skill contents",
            "truncated": false,
            "total_lines": 12
        })
        .to_string(),
        ..Default::default()
    }]);

    let area = Rect::new(0, 0, 100, 6);
    let (inner, visible) = visible_rendered_lines(area, &chat, &ThemeTokens::default(), 0, false)
        .expect("chat should produce visible lines");
    let tool_row = visible
        .iter()
        .position(|line| {
            line.message_index == Some(0) && matches!(line.kind, RenderedLineKind::ToolToggle)
        })
        .expect("tool row should be visible");
    let hit_line = &visible[tool_row];
    let (plain, _, _) = rendered_line_content_bounds(hit_line);
    let chip_col = plain
        .find("[SKILL.md]")
        .expect("skill path chip should be rendered on the tool row");

    let chip_hit = hit_test(
        area,
        &chat,
        &ThemeTokens::default(),
        0,
        Position::new(inner.x + chip_col as u16 + 1, inner.y + tool_row as u16),
    );

    assert_eq!(
        chip_hit,
        Some(ChatHitTarget::ToolFilePath { message_index: 0 })
    );
}

#[test]
fn hit_test_returns_tool_file_path_target_for_read_guideline() {
    let chat = chat_with_messages(vec![AgentMessage {
        role: MessageRole::Tool,
        tool_name: Some("read_guideline".into()),
        tool_arguments: Some(r#"{"guideline":"coding-task"}"#.into()),
        tool_status: Some("done".into()),
        content: "Guideline coding-task.md:\n\n# Coding Task\n".into(),
        ..Default::default()
    }]);

    let area = Rect::new(0, 0, 100, 6);
    let (inner, visible) = visible_rendered_lines(area, &chat, &ThemeTokens::default(), 0, false)
        .expect("chat should produce visible lines");
    let tool_row = visible
        .iter()
        .position(|line| {
            line.message_index == Some(0) && matches!(line.kind, RenderedLineKind::ToolToggle)
        })
        .expect("tool row should be visible");
    let hit_line = &visible[tool_row];
    let (plain, _, _) = rendered_line_content_bounds(hit_line);
    let chip_col = plain
        .find("[coding-task.md]")
        .expect("guideline path chip should be rendered on the tool row");

    let chip_hit = hit_test(
        area,
        &chat,
        &ThemeTokens::default(),
        0,
        Position::new(inner.x + chip_col as u16 + 1, inner.y + tool_row as u16),
    );

    assert_eq!(
        chip_hit,
        Some(ChatHitTarget::ToolFilePath { message_index: 0 })
    );
}

#[test]
fn hit_test_returns_tool_file_path_target_for_tool_output_preview_metadata() {
    let preview_path =
        std::env::temp_dir().join(format!("web_search-preview-{}.txt", uuid::Uuid::new_v4()));
    let chat = chat_with_messages(vec![AgentMessage {
        role: MessageRole::Tool,
        tool_name: Some("web_search".into()),
        tool_status: Some("done".into()),
        tool_output_preview_path: Some(preview_path.display().to_string()),
        content: "Tool result saved to preview file\n- tool: web_search".into(),
        ..Default::default()
    }]);

    let area = Rect::new(0, 0, 120, 6);
    let (inner, visible) = visible_rendered_lines(area, &chat, &ThemeTokens::default(), 0, false)
        .expect("chat should produce visible lines");
    let tool_row = visible
        .iter()
        .position(|line| {
            line.message_index == Some(0) && matches!(line.kind, RenderedLineKind::ToolToggle)
        })
        .expect("tool row should be visible");
    let hit_line = &visible[tool_row];
    let (plain, _, _) = rendered_line_content_bounds(hit_line);
    let chip_col = plain
        .find('[')
        .expect("preview path chip should be rendered on the tool row");

    let chip_hit = hit_test(
        area,
        &chat,
        &ThemeTokens::default(),
        0,
        Position::new(inner.x + chip_col as u16 + 1, inner.y + tool_row as u16),
    );

    assert_eq!(
        chip_hit,
        Some(ChatHitTarget::ToolFilePath { message_index: 0 })
    );
}

#[test]
fn hit_test_returns_message_image_target_for_assistant_image_attachment() {
    use base64::Engine as _;

    let image_path =
        std::env::temp_dir().join(format!("zorai-inline-image-{}.png", uuid::Uuid::new_v4()));
    std::fs::write(
        &image_path,
        base64::engine::general_purpose::STANDARD
            .decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO0pGfcAAAAASUVORK5CYII=")
            .expect("fixture PNG should decode"),
    )
    .expect("fixture PNG should write");

    let chat = chat_with_messages(vec![AgentMessage {
        role: MessageRole::Assistant,
        content_blocks: vec![crate::state::chat::AgentContentBlock::Image {
            url: Some(format!("file://{}", image_path.display())),
            data_url: None,
            mime_type: Some("image/png".into()),
        }],
        ..Default::default()
    }]);

    let area = Rect::new(0, 0, 100, 12);
    let (inner, visible) = visible_rendered_lines(area, &chat, &ThemeTokens::default(), 0, false)
        .expect("chat should produce visible lines");
    let image_row = visible
        .iter()
        .position(|line| {
            line.message_index == Some(0) && matches!(line.kind, RenderedLineKind::ImageAttachment)
        })
        .expect("image attachment row should be visible");

    let hit = hit_test(
        area,
        &chat,
        &ThemeTokens::default(),
        0,
        Position::new(inner.x.saturating_add(2), inner.y + image_row as u16),
    );

    assert_eq!(hit, Some(ChatHitTarget::MessageImage { message_index: 0 }));
}

#[test]
fn streaming_append_preserves_locked_viewport() {
    let mut chat = ChatState::new();
    chat.reduce(ChatAction::ThreadCreated {
        thread_id: "t1".into(),
        title: "Test".into(),
    });
    let initial = (1..=40)
        .map(|idx| format!("line {idx}"))
        .collect::<Vec<_>>()
        .join("\n");
    chat.reduce(ChatAction::Delta {
        thread_id: "t1".into(),
        content: initial,
    });

    let inner_height = 8usize;
    let inner_width = 80usize;
    chat.reduce(ChatAction::ScrollChat(3));
    let (before_lines, before_ranges) =
        build_rendered_lines(&chat, &ThemeTokens::default(), inner_width, 0, false);
    let before_scroll = resolved_scroll(&chat, before_lines.len(), inner_height, &before_ranges);
    let (_, before_start, _) =
        visible_window_bounds(before_lines.len(), inner_height, before_scroll);

    chat.reduce(ChatAction::Delta {
        thread_id: "t1".into(),
        content: "\nline 41".into(),
    });

    let (after_lines, after_ranges) =
        build_rendered_lines(&chat, &ThemeTokens::default(), inner_width, 0, false);
    let after_scroll = resolved_scroll(&chat, after_lines.len(), inner_height, &after_ranges);
    let (_, after_start, _) = visible_window_bounds(after_lines.len(), inner_height, after_scroll);

    assert_eq!(
        after_start, before_start,
        "locked viewport should stay anchored while new streamed lines append"
    );
}

#[test]
fn hit_test_full_mode_reasoning_header_only_selects_message() {
    let mut chat = chat_with_messages(vec![AgentMessage {
        role: MessageRole::Assistant,
        content: "Answer".into(),
        reasoning: Some("Think".into()),
        ..Default::default()
    }]);
    chat.reduce(ChatAction::SetTranscriptMode(TranscriptMode::Full));

    let hit = hit_test(
        Rect::new(0, 0, 80, 6),
        &chat,
        &ThemeTokens::default(),
        0,
        Position::new(2, 2),
    );

    assert_eq!(hit, Some(ChatHitTarget::Message(0)));
}

#[test]
fn waiting_retry_row_shows_yes_countdown_and_no_action() {
    let mut chat = ChatState::new();
    chat.reduce(ChatAction::ThreadCreated {
        thread_id: "t1".into(),
        title: "Test".into(),
    });
    chat.reduce(ChatAction::SetRetryStatus {
        thread_id: "t1".into(),
        phase: RetryPhase::Waiting,
        attempt: 3,
        max_retries: 3,
        delay_ms: 30_000,
        failure_class: "rate_limit".into(),
        message: "429 Too Many Requests".into(),
        received_at_tick: 0,
    });

    let (lines, _) = build_rendered_lines(&chat, &ThemeTokens::default(), 80, 20, false);
    let action_line = lines
        .iter()
        .find(|line| matches!(line.kind, RenderedLineKind::RetryAction))
        .expect("retry action line should be rendered");
    let action_text = rendered_line_plain_text(action_line);
    assert!(action_text.contains("[Yes 29s]"));
    assert!(action_text.contains("[No]"));

    let area = Rect::new(0, 0, 80, 8);
    let (inner, visible) = visible_rendered_lines(area, &chat, &ThemeTokens::default(), 20, false)
        .expect("retry state should render visible lines");
    let retry_row = visible
        .iter()
        .position(|line| matches!(line.kind, RenderedLineKind::RetryAction))
        .expect("retry action row should be visible");
    let hit_line = &visible[retry_row];
    let (_, content_start, _) = rendered_line_content_bounds(hit_line);
    let yes_width = UnicodeWidthStr::width("[Yes 29s]");
    let no_x = inner.x + (content_start + yes_width + 2) as u16;
    let no_y = inner.y + retry_row as u16;

    let hit = hit_test(
        area,
        &chat,
        &ThemeTokens::default(),
        20,
        Position::new(no_x, no_y),
    );
    assert_eq!(hit, Some(ChatHitTarget::RetryStop));

    let yes_x = inner.x + (content_start + 1) as u16;
    let yes_hit = hit_test(
        area,
        &chat,
        &ThemeTokens::default(),
        20,
        Position::new(yes_x, no_y),
    );
    assert_eq!(yes_hit, Some(ChatHitTarget::RetryStartNow));
}
