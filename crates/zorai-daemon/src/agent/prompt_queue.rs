use super::*;
use anyhow::Result;
use std::sync::Arc;
use tokio::sync::mpsc;
use zorai_protocol::QueuedPromptRecord;

pub(crate) fn spawn_prompt_queue_startup_flush(engine: Arc<AgentEngine>) {
    tokio::spawn(async move {
        match engine.history.list_queued_prompts(None).await {
            Ok(prompts) => {
                let mut seen = std::collections::HashSet::new();
                for prompt in prompts {
                    if seen.insert(prompt.thread_id.clone()) {
                        engine.wake_prompt_queue(&prompt.thread_id);
                    }
                }
            }
            Err(error) => {
                tracing::warn!(%error, "failed to restore prompt queue on startup");
            }
        }
    });
}

pub(crate) fn spawn_prompt_queue_worker(
    engine: Arc<AgentEngine>,
    mut wake_rx: mpsc::UnboundedReceiver<String>,
) {
    tokio::spawn(async move {
        while let Some(thread_id) = wake_rx.recv().await {
            if let Err(error) = engine.dispatch_next_queued_prompt(&thread_id).await {
                tracing::warn!(
                    thread_id = %thread_id,
                    %error,
                    "failed to dispatch queued prompt"
                );
            }
        }
    });
}

impl AgentEngine {
    pub(crate) fn wake_prompt_queue(&self, thread_id: &str) {
        let _ = self.prompt_queue_wake_tx.send(thread_id.to_string());
    }

    pub(crate) async fn is_thread_streaming(&self, thread_id: &str) -> bool {
        self.stream_cancellations
            .lock()
            .await
            .contains_key(thread_id)
    }

    async fn broadcast_prompt_queue(&self, thread_id: &str) -> Result<Vec<QueuedPromptRecord>> {
        let prompts = self.history.list_queued_prompts(Some(thread_id)).await?;
        let _ = self.event_tx.send(AgentEvent::PromptQueueUpdate {
            thread_id: thread_id.to_string(),
            prompts: prompts.clone(),
        });
        Ok(prompts)
    }

    pub(crate) async fn enqueue_prompt(
        &self,
        thread_id: &str,
        content: &str,
        content_blocks_json: Option<&str>,
        prompt_id: Option<&str>,
    ) -> Result<Vec<QueuedPromptRecord>> {
        if thread_id.trim().is_empty() {
            anyhow::bail!("thread_id is required to enqueue a prompt");
        }
        if content.trim().is_empty() && content_blocks_json.is_none() {
            anyhow::bail!("queued prompt content is empty");
        }
        self.history
            .enqueue_prompt(thread_id, content, content_blocks_json, prompt_id)
            .await?;
        let prompts = self.broadcast_prompt_queue(thread_id).await?;
        self.wake_prompt_queue(thread_id);
        Ok(prompts)
    }

    pub(crate) async fn list_prompt_queue(
        &self,
        thread_id: Option<&str>,
    ) -> Result<Vec<QueuedPromptRecord>> {
        self.history.list_queued_prompts(thread_id).await
    }

    pub(crate) async fn update_queued_prompt(
        &self,
        thread_id: &str,
        prompt_id: &str,
        content: &str,
        content_blocks_json: Option<&str>,
    ) -> Result<Vec<QueuedPromptRecord>> {
        if !self
            .history
            .update_queued_prompt(thread_id, prompt_id, content, content_blocks_json)
            .await?
        {
            anyhow::bail!("queued prompt {prompt_id} was not found");
        }
        self.broadcast_prompt_queue(thread_id).await
    }

    pub(crate) async fn cancel_queued_prompt(
        &self,
        thread_id: &str,
        prompt_id: &str,
    ) -> Result<Vec<QueuedPromptRecord>> {
        if self
            .history
            .delete_queued_prompt(thread_id, prompt_id)
            .await?
            .is_none()
        {
            anyhow::bail!("queued prompt {prompt_id} was not found");
        }
        self.broadcast_prompt_queue(thread_id).await
    }

    pub(crate) async fn send_queued_prompt_now(
        self: &Arc<Self>,
        thread_id: &str,
        prompt_id: &str,
    ) -> Result<Vec<QueuedPromptRecord>> {
        let Some(prompt) = self
            .history
            .delete_queued_prompt(thread_id, prompt_id)
            .await?
        else {
            anyhow::bail!("queued prompt {prompt_id} was not found");
        };
        let remaining = self.broadcast_prompt_queue(thread_id).await?;
        let (placeholder_generation, _, _) = self.begin_stream_cancellation(thread_id).await;
        let engine = Arc::clone(self);
        let thread_id = thread_id.to_string();
        tokio::spawn(async move {
            if let Err(error) = Box::pin(engine.send_message_with_session_surface_and_target(
                Some(&thread_id),
                None,
                &prompt.content,
                prompt.content_blocks_json.as_deref(),
                None,
                None,
            ))
            .await
            {
                tracing::warn!(
                    thread_id = %thread_id,
                    %error,
                    "failed to send queued prompt now"
                );
                engine
                    .finish_stream_cancellation(&thread_id, placeholder_generation)
                    .await;
            }
        });
        Ok(remaining)
    }

    pub(crate) async fn deliver_queued_prompts_after_tool_results(
        &self,
        thread_id: &str,
    ) -> Vec<String> {
        let last_is_tool_result = self
            .threads
            .read()
            .await
            .get(thread_id)
            .and_then(|thread| thread.messages.last())
            .is_some_and(|message| message.role == MessageRole::Tool);
        if !last_is_tool_result {
            return Vec::new();
        }
        let mut delivered = Vec::new();
        loop {
            let prompt = match self.history.dequeue_next_prompt(thread_id).await {
                Ok(Some(prompt)) => prompt,
                Ok(None) => break,
                Err(error) => {
                    tracing::warn!(thread_id, %error, "failed to dequeue prompt for mid-turn delivery");
                    break;
                }
            };
            let blocks = match Self::parse_content_blocks_json(prompt.content_blocks_json.as_deref())
            {
                Ok(blocks) => blocks,
                Err(error) => {
                    tracing::warn!(thread_id, prompt_id = %prompt.id, %error, "queued prompt has invalid content blocks; delivering text only");
                    Vec::new()
                }
            };
            let blocks = crate::agent::thread_crud::store_user_images_in_inventory(
                self.history.data_root(),
                thread_id,
                &now_millis().to_string(),
                &blocks,
            )
            .await;
            let message = AgentMessage::user_with_blocks(&prompt.content, blocks, now_millis());
            let message_id = message.id.clone();
            {
                let mut threads = self.threads.write().await;
                let Some(thread) = threads.get_mut(thread_id) else {
                    break;
                };
                thread.messages.push(message);
                thread.updated_at = now_millis();
            }
            if let Err(error) = self
                .record_operator_message(thread_id, &prompt.content, false)
                .await
            {
                tracing::warn!(thread_id, %error, "failed to record mid-turn queued prompt");
            }
            let _ = self.event_tx.send(AgentEvent::QueuedPromptDelivered {
                thread_id: thread_id.to_string(),
                message_id: message_id.clone(),
                prompt,
            });
            delivered.push(message_id);
        }
        if !delivered.is_empty() {
            self.persist_thread_by_id(thread_id).await;
            let _ = self.broadcast_prompt_queue(thread_id).await;
        }
        delivered
    }

    pub(crate) async fn dispatch_next_queued_prompt(&self, thread_id: &str) -> Result<()> {
        if self.is_thread_streaming(thread_id).await {
            return Ok(());
        }
        let Some(prompt) = self.history.dequeue_next_prompt(thread_id).await? else {
            return Ok(());
        };
        let _ = self.broadcast_prompt_queue(thread_id).await;
        if let Err(error) = Box::pin(self.send_message_with_session_surface_and_target(
            Some(thread_id),
            None,
            &prompt.content,
            prompt.content_blocks_json.as_deref(),
            None,
            None,
        ))
        .await
        {
            tracing::warn!(
                thread_id = %thread_id,
                prompt_id = %prompt.id,
                %error,
                "failed to auto-dispatch queued prompt"
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::history::prompt_queue::MAX_QUEUED_PROMPTS;

    #[test]
    fn queue_cap_matches_tui_bound() {
        assert_eq!(MAX_QUEUED_PROMPTS, 500);
    }

    async fn engine_with_last_message(
        thread_id: &str,
        last: crate::agent::AgentMessage,
    ) -> std::sync::Arc<crate::agent::AgentEngine> {
        use crate::agent::*;
        let root = tempfile::tempdir().expect("tempdir should succeed");
        let manager = crate::session_manager::SessionManager::new_test(root.path()).await;
        let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
        engine.threads.write().await.insert(
            thread_id.to_string(),
            AgentThread {
                id: thread_id.to_string(),
                agent_name: None,
                title: "Queue".to_string(),
                messages: vec![AgentMessage::user("start the refactor", 1), last],
                pinned: false,
                upstream_thread_id: None,
                upstream_transport: None,
                upstream_provider: None,
                upstream_model: None,
                upstream_assistant_id: None,
                created_at: 1,
                updated_at: 1,
                total_input_tokens: 0,
                total_output_tokens: 0,
            },
        );
        engine
    }

    #[tokio::test]
    async fn queued_prompt_is_delivered_after_tool_result_without_waiting_for_turn_end() {
        use crate::agent::*;
        let thread_id = "thread-queue-mid-turn";
        let mut tool_result = AgentMessage::user("file contents", 2);
        tool_result.role = MessageRole::Tool;
        tool_result.tool_call_id = Some("call-1".to_string());
        let engine = engine_with_last_message(thread_id, tool_result).await;
        engine
            .enqueue_prompt(thread_id, "stop, use the other crate", None, None)
            .await
            .expect("enqueue should succeed");
        let mut events = engine.subscribe();

        let delivered = engine
            .deliver_queued_prompts_after_tool_results(thread_id)
            .await;

        assert_eq!(delivered.len(), 1);
        let threads = engine.threads.read().await;
        let last = threads[thread_id].messages.last().expect("message");
        assert_eq!(last.role, MessageRole::User);
        assert_eq!(last.content, "stop, use the other crate");
        assert_eq!(last.id, delivered[0]);
        drop(threads);
        assert!(
            engine
                .list_prompt_queue(Some(thread_id))
                .await
                .expect("list should succeed")
                .is_empty(),
            "a delivered prompt must not be sent again when the turn ends"
        );
        let mut saw_delivery_event = false;
        while let Ok(event) = events.try_recv() {
            if matches!(event, AgentEvent::QueuedPromptDelivered { ref message_id, .. } if *message_id == delivered[0]) {
                saw_delivery_event = true;
            }
        }
        assert!(saw_delivery_event, "the UI needs an event to show the delivered prompt mid-turn");
    }

    #[tokio::test]
    async fn queued_prompt_waits_when_last_message_is_not_a_tool_result() {
        use crate::agent::*;
        let thread_id = "thread-queue-no-tool";
        let engine =
            engine_with_last_message(thread_id, AgentMessage::user("another user note", 2)).await;
        engine
            .enqueue_prompt(thread_id, "later", None, None)
            .await
            .expect("enqueue should succeed");

        let delivered = engine
            .deliver_queued_prompts_after_tool_results(thread_id)
            .await;

        assert!(
            delivered.is_empty(),
            "prompts are only injected right after tool results, where the model is about to be re-asked"
        );
        assert_eq!(
            engine
                .list_prompt_queue(Some(thread_id))
                .await
                .expect("list should succeed")
                .len(),
            1
        );
    }
}
