use super::*;
use zorai_shared::providers::{PROVIDER_ID_GEMINI, PROVIDER_ID_OPENAI, PROVIDER_ID_XAI};

const OPENAI_CLIENT_SECRETS_URL: &str = "https://api.openai.com/v1/realtime/client_secrets";
const XAI_CLIENT_SECRETS_URL: &str = "https://api.x.ai/v1/realtime/client_secrets";
const CLIENT_SECRET_TTL_SECONDS: u64 = 600;
const GEMINI_AUTH_TOKENS_URL: &str = "https://generativelanguage.googleapis.com/v1beta/auth_tokens";

fn arg<'a>(args: &'a serde_json::Value, key: &str) -> Option<&'a str> {
    args.get(key)
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

pub(crate) fn realtime_client_secret_request(
    provider_id: &str,
    model: &str,
    voice: Option<&str>,
    instructions: Option<&str>,
) -> Result<(&'static str, serde_json::Value)> {
    match provider_id {
        PROVIDER_ID_OPENAI => {
            let mut session = serde_json::json!({ "type": "realtime", "model": model });
            if let Some(voice) = voice {
                session["audio"] = serde_json::json!({ "output": { "voice": voice } });
            }
            if let Some(instructions) = instructions {
                session["instructions"] = serde_json::Value::String(instructions.to_string());
            }
            Ok((
                OPENAI_CLIENT_SECRETS_URL,
                serde_json::json!({
                    "expires_after": { "anchor": "created_at", "seconds": CLIENT_SECRET_TTL_SECONDS },
                    "session": session,
                }),
            ))
        }
        PROVIDER_ID_XAI => Ok((
            XAI_CLIENT_SECRETS_URL,
            serde_json::json!({
                "expires_after": { "seconds": CLIENT_SECRET_TTL_SECONDS },
                "session": { "model": model },
            }),
        )),
        other => anyhow::bail!("realtime voice is not supported for provider '{other}'"),
    }
}

pub(crate) fn gemini_live_token_request(
    model: &str,
    now: chrono::DateTime<chrono::Utc>,
) -> (&'static str, serde_json::Value) {
    let model = if model.starts_with("models/") {
        model.to_string()
    } else {
        format!("models/{model}")
    };
    let at = |seconds: i64| {
        (now + chrono::Duration::seconds(seconds))
            .to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
    };
    (
        GEMINI_AUTH_TOKENS_URL,
        serde_json::json!({
            "uses": 1,
            "expireTime": at(CLIENT_SECRET_TTL_SECONDS as i64 * 3),
            "newSessionExpireTime": at(120),
            "liveConnectConstraints": { "model": model },
        }),
    )
}

pub(crate) fn realtime_client_secret_value(
    payload: &serde_json::Value,
) -> Result<(String, Option<i64>)> {
    let value = payload
        .get("value")
        .or_else(|| payload.pointer("/client_secret/value"))
        .or_else(|| payload.get("name"))
        .and_then(|value| value.as_str())
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| {
            anyhow::anyhow!("realtime client secret response did not include a value")
        })?;
    let expires_at = payload
        .get("expires_at")
        .or_else(|| payload.pointer("/client_secret/expires_at"))
        .and_then(|value| value.as_i64());
    Ok((value.to_string(), expires_at))
}

pub(crate) async fn execute_realtime_client_secret(
    args: &serde_json::Value,
    agent: &AgentEngine,
) -> Result<String> {
    let provider_id =
        arg(args, "provider").ok_or_else(|| anyhow::anyhow!("missing 'provider' argument"))?;
    let model = arg(args, "model").ok_or_else(|| anyhow::anyhow!("missing 'model' argument"))?;
    let voice = arg(args, "voice");
    let gemini = provider_id == PROVIDER_ID_GEMINI;
    let (url, body) = if gemini {
        gemini_live_token_request(model, chrono::Utc::now())
    } else {
        realtime_client_secret_request(provider_id, model, voice, arg(args, "instructions"))?
    };
    let config = agent.get_config().await;
    let provider_config = agent.resolve_sub_agent_provider_config(&config, provider_id)?;
    if provider_config.auth_source != crate::agent::types::AuthSource::ApiKey
        || provider_config.api_key.trim().is_empty()
    {
        anyhow::bail!("realtime voice needs an API key for '{provider_id}'; add one in Settings → Providers (subscription logins are not accepted by realtime APIs)");
    }
    let request = agent.http_client.post(url);
    let request = if gemini {
        request.header("x-goog-api-key", provider_config.api_key.trim())
    } else {
        request.bearer_auth(provider_config.api_key.trim())
    };
    let response = request.json(&body).send().await?;
    if !response.status().is_success() {
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        anyhow::bail!("realtime session request failed: HTTP {status} {text}");
    }
    let payload: serde_json::Value = response.json().await?;
    let (value, expires_at) = realtime_client_secret_value(&payload)?;
    Ok(serde_json::json!({
        "provider": provider_id,
        "model": model,
        "voice": voice,
        "value": value,
        "expires_at": expires_at,
    })
    .to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn openai_secret_request_carries_model_voice_and_instructions_in_a_realtime_session() {
        let (url, body) = realtime_client_secret_request(
            "openai",
            "gpt-realtime-2.1",
            Some("marin"),
            Some("Be brief."),
        )
        .unwrap();
        assert_eq!(url, OPENAI_CLIENT_SECRETS_URL);
        assert_eq!(body["session"]["type"], "realtime");
        assert_eq!(body["session"]["model"], "gpt-realtime-2.1");
        assert_eq!(body["session"]["audio"]["output"]["voice"], "marin");
        assert_eq!(body["session"]["instructions"], "Be brief.");
        assert_eq!(body["expires_after"]["anchor"], "created_at");
    }

    #[test]
    fn xai_secret_request_only_pins_the_model_because_voice_is_set_over_the_socket() {
        let (url, body) =
            realtime_client_secret_request("xai", "grok-voice-latest", Some("eve"), None).unwrap();
        assert_eq!(url, XAI_CLIENT_SECRETS_URL);
        assert_eq!(
            body["session"],
            serde_json::json!({ "model": "grok-voice-latest" })
        );
        assert_eq!(body["expires_after"]["seconds"], 600);
    }

    #[test]
    fn gemini_token_is_single_use_scoped_to_the_live_model_with_short_start_window() {
        let now = chrono::DateTime::parse_from_rfc3339("2026-11-27T12:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc);
        let (url, body) = gemini_live_token_request("gemini-3.8-live", now);
        assert_eq!(url, GEMINI_AUTH_TOKENS_URL);
        assert_eq!(body["uses"], 1);
        assert_eq!(
            body["liveConnectConstraints"]["model"],
            "models/gemini-3.8-live"
        );
        assert_eq!(body["newSessionExpireTime"], "2026-11-27T12:02:00Z");
        assert_eq!(body["expireTime"], "2026-11-27T12:30:00Z");
        assert_eq!(
            realtime_client_secret_value(&serde_json::json!({ "name": "auth_tokens/abc" }))
                .unwrap()
                .0,
            "auth_tokens/abc"
        );
    }

    #[test]
    fn unsupported_providers_are_rejected_instead_of_sending_keys_elsewhere() {
        assert!(realtime_client_secret_request("openrouter", "x", None, None).is_err());
    }

    #[test]
    fn reads_the_secret_from_top_level_or_nested_client_secret() {
        assert_eq!(
            realtime_client_secret_value(&serde_json::json!({ "value": "ek_1", "expires_at": 9 }))
                .unwrap(),
            ("ek_1".to_string(), Some(9))
        );
        assert_eq!(
            realtime_client_secret_value(
                &serde_json::json!({ "client_secret": { "value": "ek_2" } })
            )
            .unwrap()
            .0,
            "ek_2"
        );
        assert!(realtime_client_secret_value(&serde_json::json!({})).is_err());
    }
}
