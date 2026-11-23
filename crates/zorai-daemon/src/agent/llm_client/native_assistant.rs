use super::*;
pub(crate) async fn run_native_assistant(
    client: &reqwest::Client,
    provider: &str,
    config: &ProviderConfig,
    messages: &[ApiMessage],
    upstream_thread_id: Option<&str>,
    copilot_initiator: CopilotInitiator,
    force_connection_close: bool,
    tx: &mpsc::Sender<Result<CompletionChunk>>,
) -> Result<()> {
    let definition = get_provider_definition(provider).ok_or_else(|| {
        anyhow::anyhow!("native assistant transport is not defined for provider '{provider}'")
    })?;
    if definition.native_transport_kind.is_none() {
        return Err(transport_incompatibility_error(
            provider,
            "provider does not expose a native assistant API",
        ));
    }
    if config.assistant_id.trim().is_empty() {
        return Err(transport_incompatibility_error(
            provider,
            "native assistant requires assistant_id",
        ));
    }
    let base_url = build_native_assistant_base_url(provider, config).ok_or_else(|| {
        anyhow::anyhow!("native assistant base URL is not configured for provider '{provider}'")
    })?;
    let user_text = messages
        .iter()
        .rev()
        .find(|message| message.role == "user")
        .and_then(api_message_to_text)
        .filter(|text| !text.trim().is_empty())
        .ok_or_else(|| anyhow::anyhow!("native assistant requires a user message"))?;

    let thread_id = match upstream_thread_id.filter(|value| !value.trim().is_empty()) {
        Some(existing) => existing.to_string(),
        None => {
            let url = format!("{base_url}/threads");
            let response = build_openai_auth_request(
                client,
                &url,
                provider,
                config,
                copilot_initiator,
                force_connection_close,
            )
            .body("{}".to_string())
            .send()
            .await?;
            if !response.status().is_success() {
                let status = response.status();
                let retry_after_ms = extract_retry_after_ms(Some(response.headers()), "");
                let text = response
                    .text()
                    .await
                    .unwrap_or_default()
                    .chars()
                    .take(240)
                    .collect::<String>();
                let is_compatibility_error = matches!(
                    status,
                    reqwest::StatusCode::BAD_REQUEST
                        | reqwest::StatusCode::NOT_FOUND
                        | reqwest::StatusCode::METHOD_NOT_ALLOWED
                        | reqwest::StatusCode::UNPROCESSABLE_ENTITY
                );
                if is_compatibility_error {
                    return Err(transport_incompatibility_error(
                        provider,
                        format!("native assistant thread creation failed ({status}): {text}"),
                    ));
                }
                return Err(classify_http_failure_with_retry_after(
                    status,
                    provider,
                    &text,
                    retry_after_ms.or_else(|| extract_retry_after_ms(None, &text)),
                ));
            }
            let payload: serde_json::Value = response.json().await?;
            payload
                .get("id")
                .and_then(|value| value.as_str())
                .map(ToOwned::to_owned)
                .ok_or_else(|| {
                    anyhow::anyhow!("native assistant thread creation returned no thread id")
                })?
        }
    };

    let message_url = format!("{base_url}/threads/{thread_id}/messages");
    let add_message_body = serde_json::json!({
        "role": "user",
        "content": user_text,
    });
    let add_message_response = build_openai_auth_request(
        client,
        &message_url,
        provider,
        config,
        copilot_initiator,
        force_connection_close,
    )
    .body(add_message_body.to_string())
    .send()
    .await?;
    if !add_message_response.status().is_success() {
        let status = add_message_response.status();
        let retry_after_ms = extract_retry_after_ms(Some(add_message_response.headers()), "");
        let text = add_message_response
            .text()
            .await
            .unwrap_or_default()
            .chars()
            .take(240)
            .collect::<String>();
        let is_compatibility_error = matches!(
            status,
            reqwest::StatusCode::BAD_REQUEST
                | reqwest::StatusCode::NOT_FOUND
                | reqwest::StatusCode::METHOD_NOT_ALLOWED
                | reqwest::StatusCode::UNPROCESSABLE_ENTITY
        );
        if is_compatibility_error {
            return Err(transport_incompatibility_error(
                provider,
                format!("native assistant message append failed ({status}): {text}"),
            ));
        }
        return Err(classify_http_failure_with_retry_after(
            status,
            provider,
            &text,
            retry_after_ms.or_else(|| extract_retry_after_ms(None, &text)),
        ));
    }

    let run_url = format!("{base_url}/threads/{thread_id}/runs");
    let run_body = serde_json::json!({
        "assistant_id": config.assistant_id,
    });
    let run_response = build_openai_auth_request(
        client,
        &run_url,
        provider,
        config,
        copilot_initiator,
        force_connection_close,
    )
    .body(run_body.to_string())
    .send()
    .await?;
    if !run_response.status().is_success() {
        let status = run_response.status();
        let retry_after_ms = extract_retry_after_ms(Some(run_response.headers()), "");
        let text = run_response
            .text()
            .await
            .unwrap_or_default()
            .chars()
            .take(240)
            .collect::<String>();
        let is_compatibility_error = matches!(
            status,
            reqwest::StatusCode::BAD_REQUEST
                | reqwest::StatusCode::NOT_FOUND
                | reqwest::StatusCode::METHOD_NOT_ALLOWED
                | reqwest::StatusCode::UNPROCESSABLE_ENTITY
        );
        if is_compatibility_error {
            return Err(transport_incompatibility_error(
                provider,
                format!("native assistant run creation failed ({status}): {text}"),
            ));
        }
        return Err(classify_http_failure_with_retry_after(
            status,
            provider,
            &text,
            retry_after_ms.or_else(|| extract_retry_after_ms(None, &text)),
        ));
    }
    let run_payload: serde_json::Value = run_response.json().await?;
    let run_id = run_payload
        .get("id")
        .and_then(|value| value.as_str())
        .ok_or_else(|| anyhow::anyhow!("native assistant run creation returned no run id"))?
        .to_string();

    let mut input_tokens = 0u64;
    let mut output_tokens = 0u64;
    let run_status_url = format!("{base_url}/threads/{thread_id}/runs/{run_id}");
    for _ in 0..180u32 {
        tokio::time::sleep(Duration::from_millis(1000)).await;
        let status_response = maybe_force_connection_close(
            apply_openai_auth_headers(
                client.get(&run_status_url),
                provider,
                config,
                copilot_initiator,
            ),
            force_connection_close,
        )
        .send()
        .await?;
        if !status_response.status().is_success() {
            let status = status_response.status();
            let retry_after_ms = extract_retry_after_ms(Some(status_response.headers()), "");
            let text = status_response
                .text()
                .await
                .unwrap_or_default()
                .chars()
                .take(240)
                .collect::<String>();
            return Err(classify_http_failure_with_retry_after(
                status,
                provider,
                &text,
                retry_after_ms.or_else(|| extract_retry_after_ms(None, &text)),
            ));
        }
        let run_status: serde_json::Value = status_response.json().await?;
        if let Some(usage) = run_status.get("usage") {
            input_tokens = usage
                .get("prompt_tokens")
                .or_else(|| usage.get("input_tokens"))
                .and_then(|value| value.as_u64())
                .unwrap_or(input_tokens);
            output_tokens = usage
                .get("completion_tokens")
                .or_else(|| usage.get("output_tokens"))
                .and_then(|value| value.as_u64())
                .unwrap_or(output_tokens);
        }
        match run_status
            .get("status")
            .and_then(|value| value.as_str())
            .unwrap_or_default()
        {
            "queued" | "in_progress" => continue,
            "completed" => {
                let content = fetch_native_assistant_message(
                    client,
                    provider,
                    config,
                    &base_url,
                    &thread_id,
                    copilot_initiator,
                    force_connection_close,
                )
                .await?;
                let _ = tx
                    .send(Ok(CompletionChunk::Done {
                        reasoning_tokens: None,
                        content,
                        reasoning: None,
                        input_tokens,
                        output_tokens,
                        cost_usd: None,
                        stop_reason: None,
                        stop_sequence: None,
                        response_id: None,
                        request_id: None,
                        upstream_model: None,
                        upstream_role: None,
                        upstream_message_type: None,
                        upstream_container: None,
                        upstream_message: None,
                        provider_final_result: None,
                        upstream_thread_id: Some(thread_id),
                        cache_creation_input_tokens: None,
                        cache_read_input_tokens: None,
                        server_tool_use: None,
                    }))
                    .await;
                return Ok(());
            }
            "requires_action" => {
                return Err(anyhow::anyhow!(
                    "native assistant requires external tool action, which zorai does not proxy yet"
                ));
            }
            "failed" | "cancelled" | "expired" => {
                let details = run_status
                    .get("last_error")
                    .and_then(|value| value.get("message"))
                    .and_then(|value| value.as_str())
                    .unwrap_or("native assistant run failed");
                return Err(anyhow::anyhow!("{details}"));
            }
            other => {
                return Err(anyhow::anyhow!(
                    "native assistant run entered unexpected status '{other}'"
                ));
            }
        }
    }

    Err(anyhow::anyhow!(
        "native assistant run timed out while waiting for completion"
    ))
}
