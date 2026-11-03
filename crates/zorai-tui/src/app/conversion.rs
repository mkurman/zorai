#[path = "conversion_parts/convert_thread_to_convert_todo_with_fallback_step.rs"]
mod convert_thread_to_convert_todo_with_fallback_step;

#[path = "conversion_parts/convert_work_context_to_copy_to_clipboard.rs"]
mod convert_work_context_to_copy_to_clipboard;

pub(crate) use convert_thread_to_convert_todo_with_fallback_step::*;
pub(crate) use convert_work_context_to_copy_to_clipboard::*;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::chat;

    #[test]
    fn convert_thread_preserves_operator_question_metadata() {
        let thread = crate::wire::AgentThread {
            id: "thread-1".into(),
            title: "Thread".into(),
            messages: vec![crate::wire::AgentMessage {
                role: crate::wire::MessageRole::Assistant,
                content: "Approve this slice?\na - proceed".into(),
                is_operator_question: true,
                operator_question_id: Some("oq-1".into()),
                operator_question_answer: Some("a".into()),
                ..Default::default()
            }],
            ..Default::default()
        };

        let converted = convert_thread(thread);
        let message = &converted.messages[0];

        assert!(message.is_operator_question);
        assert_eq!(message.operator_question_id.as_deref(), Some("oq-1"));
        assert_eq!(message.operator_question_answer.as_deref(), Some("a"));
    }

    #[test]
    fn convert_thread_preserves_image_content_blocks() {
        let thread = crate::wire::AgentThread {
            id: "thread-1".into(),
            title: "Thread".into(),
            messages: vec![crate::wire::AgentMessage {
                role: crate::wire::MessageRole::Assistant,
                content: "Generated image.".into(),
                content_blocks: vec![crate::wire::AgentContentBlock::Image {
                    url: Some("file:///tmp/thread-files/generated.png".into()),
                    data_url: None,
                    mime_type: Some("image/png".into()),
                }],
                ..Default::default()
            }],
            ..Default::default()
        };

        let converted = convert_thread(thread);
        let message = &converted.messages[0];

        assert!(matches!(
            message.content_blocks.first(),
            Some(chat::AgentContentBlock::Image {
                url: Some(url),
                mime_type: Some(mime_type),
                ..
            }) if url == "file:///tmp/thread-files/generated.png" && mime_type == "image/png"
        ));
    }

    #[test]
    fn thread_with_generated_video_block_deserializes_and_converts() {
        let wire_thread: crate::wire::AgentThread = serde_json::from_str(
            r#"{
                "id": "thread-video",
                "title": "Video",
                "messages": [
                    {
                        "role": "user",
                        "content": "drone shot"
                    },
                    {
                        "role": "assistant",
                        "content": "Generated video.",
                        "content_blocks": [
                            {
                                "type": "video",
                                "url": "file:///tmp/media/video-1.mp4",
                                "mime_type": "video/mp4"
                            }
                        ]
                    }
                ]
            }"#,
        )
        .expect("thread with a video content block should deserialize");

        let converted = convert_thread(wire_thread);
        let message = &converted.messages[1];

        assert_eq!(message.content, "Generated video.");
        assert!(matches!(
            message.content_blocks.first(),
            Some(chat::AgentContentBlock::Video {
                url: Some(url),
                data_url: None,
                mime_type: Some(mime_type),
            }) if url == "file:///tmp/media/video-1.mp4" && mime_type == "video/mp4"
        ));
    }

    #[test]
    fn thread_handoff_state_wire_conversion_preserves_full_ordered_state() {
        let wire_thread: crate::wire::AgentThread = serde_json::from_str(
            r#"{
                "id": "thread-detail-handoff",
                "title": "Thread detail handoff",
                "thread_handoff_state": {
                    "origin_agent_id": "swarog",
                    "active_agent_id": "weles",
                    "responder_stack": [
                        {
                            "agent_id": "swarog",
                            "agent_name": "Svarog",
                            "entered_at": 10,
                            "linked_thread_id": null
                        },
                        {
                            "agent_id": "weles",
                            "agent_name": "Weles",
                            "entered_at": 20,
                            "linked_thread_id": "thread-weles"
                        }
                    ],
                    "pending_approval_id": null
                }
            }"#,
        )
        .expect("thread-detail handoff payload should deserialize");

        let converted = convert_thread(wire_thread);
        let handoff = converted
            .thread_handoff_state
            .as_ref()
            .expect("thread detail should retain handoff state");

        assert_eq!(handoff.origin_agent_id, "swarog");
        assert_eq!(handoff.active_agent_id, "weles");
        assert_eq!(handoff.responder_stack.len(), 2);

        let svarog = &handoff.responder_stack[0];
        assert_eq!(svarog.agent_id, "swarog");
        assert_eq!(svarog.agent_name, "Svarog");
        assert_eq!(svarog.entered_at, 10);
        assert_eq!(svarog.linked_thread_id, None);

        let weles = &handoff.responder_stack[1];
        assert_eq!(weles.agent_id, "weles");
        assert_eq!(weles.agent_name, "Weles");
        assert_eq!(weles.entered_at, 20);
        assert_eq!(weles.linked_thread_id.as_deref(), Some("thread-weles"));
        assert_eq!(handoff.pending_approval_id, None);
    }

    #[test]
    fn thread_handoff_state_wire_conversion_accepts_legacy_thread_without_state() {
        let wire_thread: crate::wire::AgentThread = serde_json::from_str(
            r#"{
                "id": "legacy-thread",
                "title": "Legacy thread"
            }"#,
        )
        .expect("legacy thread payload should deserialize");

        let converted = convert_thread(wire_thread);

        assert_eq!(converted.thread_handoff_state, None);
    }

    #[test]
    fn copy_to_clipboard_keeps_owner_alive_after_write() {
        reset_last_copied_text();

        copy_to_clipboard("hello");

        assert_eq!(last_copied_text().as_deref(), Some("hello"));
        assert!(
            test_clipboard_owner_held(),
            "clipboard owner should stay alive after copy so Linux clipboard managers can read it"
        );
    }
}
