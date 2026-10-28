use super::super::chat_with_messages;
use super::super::*;
use crate::state::chat::{AgentMessage, ChatState, MessageRole};
use crate::theme::ThemeTokens;
use ratatui::layout::Rect;
#[test]
fn chat_handles_empty_state() {
    let chat = ChatState::new();
    assert!(chat.active_thread().is_none());
    assert!(chat.streaming_content().is_empty());
}

#[test]
fn hit_test_selects_clicked_message_row() {
    let chat = chat_with_messages(vec![
        AgentMessage {
            role: MessageRole::User,
            content: "first".into(),
            ..Default::default()
        },
        AgentMessage {
            role: MessageRole::User,
            content: "second".into(),
            ..Default::default()
        },
    ]);

    let hit = hit_test(
        Rect::new(0, 0, 80, 6),
        &chat,
        &ThemeTokens::default(),
        0,
        Position::new(2, 4),
    );

    assert_eq!(hit, Some(ChatHitTarget::Message(1)));
}

#[test]
fn hit_test_marks_reasoning_header_as_toggle() {
    let chat = chat_with_messages(vec![AgentMessage {
        role: MessageRole::Assistant,
        content: "Answer".into(),
        reasoning: Some("Think".into()),
        ..Default::default()
    }]);

    let area = Rect::new(0, 0, 80, 5);
    let (inner, visible) = visible_rendered_lines(area, &chat, &ThemeTokens::default(), 0, false)
        .expect("chat should produce visible lines");
    let header_row = visible
        .iter()
        .position(|line| {
            line.message_index == Some(0) && matches!(line.kind, RenderedLineKind::ReasoningToggle)
        })
        .expect("reasoning header should be visible");
    let hit_line = &visible[header_row];
    let (_, content_start, _) = rendered_line_content_bounds(hit_line);

    let hit = hit_test(
        area,
        &chat,
        &ThemeTokens::default(),
        0,
        Position::new(inner.x + content_start as u16, inner.y + header_row as u16),
    );

    assert_eq!(hit, Some(ChatHitTarget::ReasoningToggle(0)));
}

#[test]
fn hit_test_reasoning_header_body_selects_message_instead_of_toggling() {
    let chat = chat_with_messages(vec![AgentMessage {
        role: MessageRole::Assistant,
        content: "Answer".into(),
        reasoning: Some("Think".into()),
        ..Default::default()
    }]);

    let area = Rect::new(0, 0, 80, 5);
    let (inner, visible) = visible_rendered_lines(area, &chat, &ThemeTokens::default(), 0, false)
        .expect("chat should produce visible lines");
    let header_row = visible
        .iter()
        .position(|line| {
            line.message_index == Some(0) && matches!(line.kind, RenderedLineKind::ReasoningToggle)
        })
        .expect("reasoning header should be visible");
    let hit_line = &visible[header_row];
    let (_, content_start, _) = rendered_line_content_bounds(hit_line);

    let hit = hit_test(
        area,
        &chat,
        &ThemeTokens::default(),
        0,
        Position::new(
            inner.x + (content_start + 3) as u16,
            inner.y + header_row as u16,
        ),
    );

    assert_eq!(hit, Some(ChatHitTarget::Message(0)));
}

#[test]
fn hit_test_marks_tool_header_as_toggle() {
    let chat = chat_with_messages(vec![AgentMessage {
        role: MessageRole::Tool,
        tool_name: Some("bash_command".into()),
        tool_status: Some("done".into()),
        content: "ok".into(),
        ..Default::default()
    }]);

    let hit = hit_test(
        Rect::new(0, 0, 80, 4),
        &chat,
        &ThemeTokens::default(),
        0,
        Position::new(2, 3),
    );

    assert_eq!(hit, Some(ChatHitTarget::ToolToggle(0)));
}

#[test]
fn hit_test_tool_header_body_selects_message_instead_of_toggling() {
    let chat = chat_with_messages(vec![AgentMessage {
        role: MessageRole::Tool,
        tool_name: Some("bash_command".into()),
        tool_status: Some("done".into()),
        content: "ok".into(),
        ..Default::default()
    }]);

    let area = Rect::new(0, 0, 80, 4);
    let (inner, visible) = visible_rendered_lines(area, &chat, &ThemeTokens::default(), 0, false)
        .expect("chat should produce visible lines");
    let header_row = visible
        .iter()
        .position(|line| {
            line.message_index == Some(0) && matches!(line.kind, RenderedLineKind::ToolToggle)
        })
        .expect("tool header should be visible");
    let hit_line = &visible[header_row];
    let (plain, content_start, _) = rendered_line_content_bounds(hit_line);
    let gear_offset = plain
        .find("⌨")
        .expect("terminal emoji icon should be rendered for bash_command");

    let hit = hit_test(
        area,
        &chat,
        &ThemeTokens::default(),
        0,
        Position::new(
            inner.x + (content_start + gear_offset) as u16,
            inner.y + header_row as u16,
        ),
    );

    assert_eq!(hit, Some(ChatHitTarget::Message(0)));
}

#[test]
fn read_file_tool_row_renders_clickable_path_chip() {
    let chat = chat_with_messages(vec![AgentMessage {
        role: MessageRole::Tool,
        tool_name: Some("read_file".into()),
        tool_arguments: Some(r#"{"path":"/tmp/demo.txt"}"#.into()),
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
    assert!(text.contains("[demo.txt]"));
    assert!(!text.contains("[/tmp/demo.txt]"));
}

#[test]
fn tool_row_renders_toggle_chevron_before_tool_name() {
    let chat = chat_with_messages(vec![AgentMessage {
        role: MessageRole::Tool,
        tool_name: Some("read_file".into()),
        tool_arguments: Some(r#"{"path":"/tmp/demo.txt"}"#.into()),
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

    assert!(
        text.contains("▶"),
        "expected collapsed chevron, got: {text}"
    );
    assert!(text.contains("📄"), "expected file emoji icon, got: {text}");
}

#[test]
fn edit_tool_row_renders_clickable_path_chip() {
    let chat = chat_with_messages(vec![AgentMessage {
        role: MessageRole::Tool,
        tool_name: Some("write_file".into()),
        tool_arguments: Some(r#"{"path":"/tmp/demo.txt"}"#.into()),
        tool_status: Some("done".into()),
        content: "written".into(),
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

    assert!(text.contains("Write File"));
    assert!(text.contains("[demo.txt]"));
    assert!(!text.contains("[/tmp/demo.txt]"));
}

#[test]
fn read_skill_tool_row_renders_skill_name_chip() {
    let chat = chat_with_messages(vec![AgentMessage {
        role: MessageRole::Tool,
        tool_name: Some("read_skill".into()),
        tool_arguments: Some(r#"{"skill":"systematic-debugging"}"#.into()),
        tool_status: Some("done".into()),
        content: "skill contents".into(),
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

    assert!(text.contains("Read Skill"));
    assert!(text.contains("[systematic-debugging]"));
}

#[test]
fn read_skill_tool_row_renders_clickable_file_chip_from_result_path() {
    let skills_root = "/tmp/skills";
    let relative_path = "development/superpowers/systematic-debugging/SKILL.md";
    let message = AgentMessage {
        role: MessageRole::Tool,
        tool_name: Some("read_skill".into()),
        tool_arguments: Some(r#"{"skill":"systematic-debugging"}"#.into()),
        tool_status: Some("done".into()),
        content: serde_json::json!({
            "skills_root": skills_root,
            "path": relative_path,
            "content": "skill contents",
            "truncated": false,
            "total_lines": 12
        })
        .to_string(),
        ..Default::default()
    };
    let chip = tool_file_chip(&message).expect("read_skill should expose a file chip");
    assert_eq!(
        chip.path,
        format!("{skills_root}/{relative_path}"),
        "read_skill should resolve the preview path from the tool result"
    );

    let chat = chat_with_messages(vec![message]);
    let (lines, _) = build_rendered_lines(&chat, &ThemeTokens::default(), 100, 0, false);
    let tool_line = lines
        .iter()
        .find(|line| {
            line.message_index == Some(0) && matches!(line.kind, RenderedLineKind::ToolToggle)
        })
        .expect("tool row should be rendered");
    let text = rendered_line_plain_text(tool_line);

    assert!(text.contains("Read Skill"));
    assert!(
        text.contains("[SKILL.md]"),
        "expected file chip, got: {text}"
    );
    assert!(
        !text.contains("[systematic-debugging]"),
        "read_skill row should not render a duplicate skill chip when the file chip is present: {text}"
    );
}

#[test]
fn read_skill_file_chip_falls_back_to_daemon_result_header() {
    let message = AgentMessage {
        role: MessageRole::Tool,
        tool_name: Some("read_skill".into()),
        tool_arguments: Some(r#"{"skill":"systematic-debugging"}"#.into()),
        tool_status: Some("done".into()),
        content: "Skill development/superpowers/systematic-debugging/SKILL.md [systematic-debugging | default | uses=1 | success=100% | tags=none]:\n\nskill contents".into(),
        ..Default::default()
    };

    let chip =
        tool_file_chip(&message).expect("daemon-style read_skill output should expose a file chip");
    assert_eq!(
        chip.path,
        zorai_protocol::zorai_skills_dir()
            .join("development/superpowers/systematic-debugging/SKILL.md")
            .display()
            .to_string()
    );
    assert_eq!(chip.label, "SKILL.md");
}

#[test]
fn read_guideline_tool_row_renders_clickable_file_chip_from_result_header() {
    let message = AgentMessage {
        role: MessageRole::Tool,
        tool_name: Some("read_guideline".into()),
        tool_arguments: Some(r#"{"guideline":"coding-task"}"#.into()),
        tool_status: Some("done".into()),
        content: "Guideline coding-task.md:\n\n# Coding Task\n".into(),
        ..Default::default()
    };
    let chip =
        tool_file_chip(&message).expect("daemon-style read_guideline output should expose a chip");
    assert_eq!(
        chip.path,
        zorai_protocol::zorai_guidelines_dir()
            .join("coding-task.md")
            .display()
            .to_string()
    );
    assert_eq!(chip.label, "coding-task.md");

    let chat = chat_with_messages(vec![message]);
    let (lines, _) = build_rendered_lines(&chat, &ThemeTokens::default(), 100, 0, false);
    let tool_line = lines
        .iter()
        .find(|line| {
            line.message_index == Some(0) && matches!(line.kind, RenderedLineKind::ToolToggle)
        })
        .expect("tool row should be rendered");
    let text = rendered_line_plain_text(tool_line);

    assert!(text.contains("Read Guideline"));
    assert!(
        text.contains("[coding-task.md]"),
        "expected guideline file chip, got: {text}"
    );
}

#[test]
fn tool_file_path_chip_prefers_tool_output_preview_path_metadata() {
    let preview_path =
        std::env::temp_dir().join(format!("bash_command-preview-{}.txt", uuid::Uuid::new_v4()));
    let message = AgentMessage {
        role: MessageRole::Tool,
        tool_name: Some("bash_command".into()),
        tool_status: Some("done".into()),
        tool_output_preview_path: Some(preview_path.display().to_string()),
        content: "Tool result saved to preview file\n- tool: bash_command".into(),
        ..Default::default()
    };

    let chip = tool_file_chip(&message).expect("preview-backed tool result should expose a chip");
    assert_eq!(chip.path, preview_path.display().to_string());
    assert_eq!(
        chip.label,
        preview_path
            .file_name()
            .and_then(|value| value.to_str())
            .expect("preview file should have a basename")
    );

    let chat = chat_with_messages(vec![message]);
    let (lines, _) = build_rendered_lines(&chat, &ThemeTokens::default(), 100, 0, false);
    let tool_line = lines
        .iter()
        .find(|line| {
            line.message_index == Some(0) && matches!(line.kind, RenderedLineKind::ToolToggle)
        })
        .expect("tool row should be rendered");
    let text = rendered_line_plain_text(tool_line);

    assert!(text.contains("bash_command"));
    assert!(text.contains(&format!("[{}]", chip.label)));
}

#[test]
fn all_file_mutation_tool_rows_use_filename_chip() {
    let cases = [
        (
            "create_file",
            serde_json::json!({
                "path": "/tmp/demo.txt",
                "content": "hello"
            })
            .to_string(),
        ),
        (
            "append_to_file",
            serde_json::json!({
                "path": "/tmp/demo.txt",
                "content": "hello"
            })
            .to_string(),
        ),
        (
            "replace_in_file",
            serde_json::json!({
                "path": "/tmp/demo.txt",
                "old_text": "old",
                "new_text": "new"
            })
            .to_string(),
        ),
        (
            "apply_file_patch",
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
        (
            "apply_patch",
            serde_json::json!({
                "input": "*** Begin Patch\n*** Update File: /tmp/demo.txt\n@@\n-old\n+new\n*** End Patch"
            })
            .to_string(),
        ),
    ];

    for (tool_name, tool_arguments) in cases {
        let chat = chat_with_messages(vec![AgentMessage {
            role: MessageRole::Tool,
            tool_name: Some(tool_name.into()),
            tool_arguments: Some(tool_arguments),
            tool_status: Some("done".into()),
            content: "ok".into(),
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

        assert!(
            text.contains(&crate::widgets::message::compact_tool_display_name(
                tool_name
            )),
            "expected tool name in row: {text}"
        );
        assert!(
            text.contains("[demo.txt]"),
            "expected filename chip for {tool_name}, got: {text}"
        );
        assert!(
            !text.contains("[/tmp/demo.txt]"),
            "unexpected full path chip for {tool_name}, got: {text}"
        );
    }
}
