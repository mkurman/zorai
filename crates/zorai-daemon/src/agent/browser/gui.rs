use super::*;

const GUI_BROWSER_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(45);

pub(super) async fn gui_browser_request(
    agent: &AgentEngine,
    thread_id: &str,
    op: &BrowserOp,
) -> Result<Value> {
    let request_id = format!("br_{}", uuid::Uuid::new_v4());
    let (response_tx, response_rx) = tokio::sync::oneshot::channel();
    agent
        .browser
        .gui_pending
        .lock()
        .await
        .insert(request_id.clone(), response_tx);
    let _ = agent.event_tx.send(AgentEvent::BrowserRequest {
        thread_id: thread_id.to_string(),
        request_id: request_id.clone(),
        op: serde_json::to_value(op)?,
    });
    let outcome = tokio::time::timeout(GUI_BROWSER_TIMEOUT, response_rx).await;
    agent.browser.gui_pending.lock().await.remove(&request_id);
    match outcome {
        Ok(Ok(Ok(value))) => Ok(value),
        Ok(Ok(Err(message))) => Err(anyhow::anyhow!(message)),
        Ok(Err(_)) => Err(anyhow::anyhow!("the Zorai browser request was dropped")),
        Err(_) => Err(anyhow::anyhow!(
            "the Zorai browser did not respond within {}s; the desktop app may be closed",
            GUI_BROWSER_TIMEOUT.as_secs()
        )),
    }
}

impl AgentEngine {
    pub async fn answer_browser_request(
        &self,
        request_id: &str,
        ok: bool,
        result_json: &str,
    ) -> bool {
        let Some(response_tx) = self.browser.gui_pending.lock().await.remove(request_id) else {
            return false;
        };
        let outcome = if ok {
            serde_json::from_str::<Value>(result_json)
                .map_err(|error| format!("invalid browser result: {error}"))
        } else {
            Err(serde_json::from_str::<String>(result_json)
                .unwrap_or_else(|_| result_json.to_string()))
        };
        response_tx.send(outcome).is_ok()
    }
}
