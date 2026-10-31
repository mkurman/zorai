use super::*;
use crate::agent::AgentEngine;
use anyhow::Result;
use std::sync::Arc;
use zorai_protocol::{ClientMessage, DaemonMessage};

async fn send_prompt_queue(
    framed: &mut ConnectionWriter,
    thread_id: Option<String>,
    prompts: Vec<zorai_protocol::QueuedPromptRecord>,
) -> Result<bool> {
    framed
        .send(DaemonMessage::AgentPromptQueue { thread_id, prompts })
        .await?;
    Ok(true)
}

async fn send_queue_error(framed: &mut ConnectionWriter, message: String) -> Result<bool> {
    framed.send(DaemonMessage::Error { message }).await?;
    Ok(true)
}

pub(crate) async fn dispatch_part11(
    msg: &ClientMessage,
    agent: &Arc<AgentEngine>,
    framed: &mut ConnectionWriter,
) -> Result<bool> {
    if !matches!(
        msg,
        ClientMessage::AgentEnqueuePrompt { .. }
            | ClientMessage::AgentListPromptQueue { .. }
            | ClientMessage::AgentUpdateQueuedPrompt { .. }
            | ClientMessage::AgentCancelQueuedPrompt { .. }
            | ClientMessage::AgentSendQueuedPromptNow { .. }
            | ClientMessage::AgentSetPromptQueueHold { .. }
    ) {
        return Ok(false);
    }
    let msg = msg.clone();
    match msg {
        ClientMessage::AgentEnqueuePrompt {
            thread_id,
            content,
            content_blocks_json,
            prompt_id,
        } => match agent
            .enqueue_prompt(
                &thread_id,
                &content,
                content_blocks_json.as_deref(),
                prompt_id.as_deref(),
            )
            .await
        {
            Ok(prompts) => send_prompt_queue(framed, Some(thread_id), prompts).await,
            Err(error) => {
                let prompts = agent
                    .list_prompt_queue(Some(thread_id.as_str()))
                    .await
                    .unwrap_or_default();
                framed
                    .send(DaemonMessage::AgentPromptQueueError {
                        thread_id: Some(thread_id),
                        message: error.to_string(),
                        prompts,
                    })
                    .await?;
                Ok(true)
            }
        },
        ClientMessage::AgentListPromptQueue { thread_id } => {
            match agent.list_prompt_queue(thread_id.as_deref()).await {
                Ok(prompts) => send_prompt_queue(framed, thread_id, prompts).await,
                Err(error) => send_queue_error(framed, error.to_string()).await,
            }
        }
        ClientMessage::AgentUpdateQueuedPrompt {
            thread_id,
            prompt_id,
            content,
            content_blocks_json,
        } => match agent
            .update_queued_prompt(
                &thread_id,
                &prompt_id,
                &content,
                content_blocks_json.as_deref(),
            )
            .await
        {
            Ok(prompts) => send_prompt_queue(framed, Some(thread_id), prompts).await,
            Err(error) => send_queue_error(framed, error.to_string()).await,
        },
        ClientMessage::AgentCancelQueuedPrompt {
            thread_id,
            prompt_id,
        } => match agent.cancel_queued_prompt(&thread_id, &prompt_id).await {
            Ok(prompts) => send_prompt_queue(framed, Some(thread_id), prompts).await,
            Err(error) => send_queue_error(framed, error.to_string()).await,
        },
        ClientMessage::AgentSendQueuedPromptNow {
            thread_id,
            prompt_id,
        } => match agent.send_queued_prompt_now(&thread_id, &prompt_id).await {
            Ok(prompts) => send_prompt_queue(framed, Some(thread_id), prompts).await,
            Err(error) => send_queue_error(framed, error.to_string()).await,
        },
        ClientMessage::AgentSetPromptQueueHold { thread_id, held } => {
            match agent.set_prompt_queue_hold(&thread_id, held).await {
                Ok(prompts) => send_prompt_queue(framed, Some(thread_id), prompts).await,
                Err(error) => send_queue_error(framed, error.to_string()).await,
            }
        }
        _ => Ok(false),
    }
}
