use super::*;
use crate::providers::context::is_known_default_url;
use zorai_shared::providers::{
    MINIMAX_PROVIDER, PROVIDER_ID_ALIBABA_CODING_PLAN, PROVIDER_ID_ANTHROPIC, PROVIDER_ID_ARCEE,
    PROVIDER_ID_CHUTES, PROVIDER_ID_DEEPSEEK, PROVIDER_ID_ELEVENLABS, PROVIDER_ID_GITHUB_COPILOT,
    PROVIDER_ID_HERMES_AGENT_API, PROVIDER_ID_KIMI, PROVIDER_ID_KIMI_CODING_PLAN,
    PROVIDER_ID_MINIMAX_CODING_PLAN, PROVIDER_ID_NVIDIA, PROVIDER_ID_OPENAI, PROVIDER_ID_POOLSIDE,
    PROVIDER_ID_XAI, PROVIDER_ID_Z_AI, PROVIDER_ID_Z_AI_CODING_PLAN, QWEN_PROVIDER,
};

#[test]
fn provider_count_matches_builtin_catalog() {
    assert_eq!(PROVIDERS.len(), 35);
}

#[test]
fn shared_provider_refs_match_tui_catalog() {
    let provider = find_by_id(QWEN_PROVIDER.id).unwrap();
    assert_eq!(provider.id, QWEN_PROVIDER.id);
    assert!(uses_fixed_anthropic_messages(
        MINIMAX_PROVIDER.id,
        "MiniMax-M2.7"
    ));
}

#[test]
fn github_copilot_defaults_to_gpt_54_and_keeps_gpt_55_selectable() {
    let provider = find_by_id(PROVIDER_ID_GITHUB_COPILOT).unwrap();
    assert_eq!(provider.default_model, "gpt-5.4");
    assert_eq!(
        default_model_for_provider_auth(PROVIDER_ID_GITHUB_COPILOT, "github_copilot"),
        "gpt-5.4"
    );
    let models = known_models_for_provider_auth(PROVIDER_ID_GITHUB_COPILOT, "github_copilot");
    assert_eq!(
        models.first().map(|model| model.id.as_str()),
        Some("gpt-5.4")
    );
    assert_eq!(
        models.first().and_then(|model| model.context_window),
        Some(400_000)
    );
    assert!(models.iter().any(|model| model.id == "gpt-5.5"));
}

#[test]
fn find_by_id_works() {
    let p = find_by_id(QWEN_PROVIDER.id).unwrap();
    assert_eq!(p.name, "Qwen");
    assert_eq!(
        p.default_base_url,
        "https://dashscope-intl.aliyuncs.com/compatible-mode/v1"
    );
}

#[test]
fn alibaba_coding_plan_uses_openai_compatible_base_url() {
    let p = find_by_id(PROVIDER_ID_ALIBABA_CODING_PLAN).unwrap();
    assert_eq!(
        p.default_base_url,
        "https://coding-intl.dashscope.aliyuncs.com/v1"
    );
    assert_eq!(p.default_model, "qwen3.6-plus");
}

#[test]
fn anthropic_message_providers_are_detected() {
    assert!(uses_fixed_anthropic_messages(
        MINIMAX_PROVIDER.id,
        "MiniMax-M2.7"
    ));
    assert!(uses_fixed_anthropic_messages(
        PROVIDER_ID_ANTHROPIC,
        "claude-opus-4-7"
    ));
    assert!(!uses_fixed_anthropic_messages(QWEN_PROVIDER.id, "qwen-max"));
    assert!(!uses_fixed_anthropic_messages(
        PROVIDER_ID_ALIBABA_CODING_PLAN,
        "qwen3.6-plus"
    ));
    assert!(!uses_fixed_anthropic_messages(
        PROVIDER_ID_GITHUB_COPILOT,
        "claude-sonnet-4.6"
    ));
}

#[test]
fn find_by_name_works() {
    let p = find_by_name("OpenAI / ChatGPT").unwrap();
    assert_eq!(p.id, PROVIDER_ID_OPENAI);
}

#[test]
fn is_known_default_url_returns_true() {
    assert!(is_known_default_url("https://api.openai.com/v1"));
    assert!(!is_known_default_url("https://custom.example.com/v1"));
}

#[test]
fn known_models_openai_non_empty() {
    let models = known_models_for_provider(PROVIDER_ID_OPENAI);
    assert!(!models.is_empty());
    assert_eq!(
        default_model_for_provider_auth(PROVIDER_ID_OPENAI, "api_key"),
        "gpt-5.5"
    );
    assert_eq!(
        models.first().map(|model| model.id.as_str()),
        Some("gpt-5.5")
    );
    assert!(models.iter().any(|m| m.id == "gpt-6-astra"));
    assert!(models.iter().any(|m| m.id == "gpt-5.6-terra"));
    assert!(models.iter().any(|m| m.id == "gpt-5.6-luna"));
    assert!(models.iter().any(|m| m.id == "gpt-5.5"));
    assert!(models.iter().any(|m| m.id == "gpt-5.4"));
    assert_eq!(
        known_context_window_for(PROVIDER_ID_OPENAI, "gpt-5.3-codex-spark"),
        Some(128_000)
    );
    assert_eq!(
        known_context_window_for(PROVIDER_ID_OPENAI, "gpt-6-astra"),
        Some(1_050_000)
    );
}

#[test]
fn known_models_openai_chatgpt_subscription_is_restricted() {
    let models = known_models_for_provider_auth(PROVIDER_ID_OPENAI, "chatgpt_subscription");
    assert_eq!(
        default_model_for_provider_auth(PROVIDER_ID_OPENAI, "chatgpt_subscription"),
        "gpt-5.5"
    );
    assert_eq!(
        models.first().map(|model| model.id.as_str()),
        Some("gpt-5.5")
    );
    assert!(models.iter().any(|m| m.id == "gpt-6-astra"));
    assert!(models.iter().any(|m| m.id == "gpt-5.6-sol-pro"));
    assert!(models.iter().any(|m| m.id == "gpt-5.6-terra"));
    assert!(models.iter().any(|m| m.id == "gpt-5.6-luna"));
    assert!(models.iter().any(|m| m.id == "gpt-5.5"));
    assert!(models.iter().any(|m| m.id == "gpt-5.4"));
    assert!(models.iter().any(|m| m.id == "gpt-5.3-codex-spark"));
    assert!(!models.iter().any(|m| m.id == "gpt-4o"));
    assert!(!models.iter().any(|m| m.id == "o3"));
}

#[test]
fn xai_static_catalog_uses_current_grok_defaults() {
    let provider = find_by_id(PROVIDER_ID_XAI).unwrap();
    assert_eq!(provider.default_model, "grok-4.6");
    assert_eq!(
        default_model_for_provider_auth(PROVIDER_ID_XAI, "api_key"),
        "grok-4.6"
    );
    let models = known_models_for_provider(PROVIDER_ID_XAI);
    assert_eq!(
        models.first().map(|model| model.id.as_str()),
        Some("grok-4.6")
    );
    assert_eq!(
        known_context_window_for(PROVIDER_ID_XAI, "grok-4.6"),
        Some(500_000)
    );
    assert_eq!(
        known_context_window_for(PROVIDER_ID_XAI, "grok-4.3"),
        Some(1_000_000)
    );
    assert!(models.iter().any(|model| model.id == "grok-4"));
    assert!(models.iter().any(|model| model.id == "grok-build-0.1"));
    assert!(models.iter().any(|model| model.id == "grok-code-fast-1"));
}

#[test]
fn elevenlabs_static_catalog_uses_audio_defaults() {
    let provider = find_by_id(PROVIDER_ID_ELEVENLABS).unwrap();
    assert_eq!(provider.name, "ElevenLabs");
    assert_eq!(provider.default_base_url, "https://api.elevenlabs.io");
    assert_eq!(provider.default_model, "scribe_v2");
    assert_eq!(
        default_model_for_provider_auth(PROVIDER_ID_ELEVENLABS, "api_key"),
        "scribe_v2"
    );
    let models = known_models_for_provider(PROVIDER_ID_ELEVENLABS);
    assert_eq!(
        models
            .iter()
            .map(|model| model.id.as_str())
            .collect::<Vec<_>>(),
        vec!["scribe_v2", "eleven_multilingual_v2"]
    );
}

#[test]
fn minimax_static_catalog_uses_current_models_and_fetch_support() {
    for provider_id in [MINIMAX_PROVIDER.id, PROVIDER_ID_MINIMAX_CODING_PLAN] {
        assert!(supports_model_fetch_for(provider_id));
        let models = known_models_for_provider(provider_id);
        assert!(models.iter().any(|model| model.id == "MiniMax-M3"));
        assert!(models
            .iter()
            .any(|model| model.id == "MiniMax-M2.7-highspeed"));
        assert!(models
            .iter()
            .any(|model| model.id == "MiniMax-M2.1-highspeed"));
        assert!(models.iter().any(|model| model.id == "MiniMax-M2"));
        assert_eq!(
            known_context_window_for(provider_id, "MiniMax-M3"),
            Some(1_000_000)
        );
        assert_eq!(
            known_context_window_for(provider_id, "MiniMax-M2.7"),
            Some(205_000)
        );
    }
}

#[test]
fn known_models_unknown_returns_empty() {
    let models = known_models_for_provider("nonexistent");
    assert!(models.is_empty());
}

#[test]
fn kimi_static_catalog_preserves_provider_default_model() {
    let models = known_models_for_provider(PROVIDER_ID_KIMI);
    assert!(models.iter().any(|model| model.id == "moonshot-v1-32k"));
    assert_eq!(
        default_model_for_provider_auth(PROVIDER_ID_KIMI, "api_key"),
        "moonshot-v1-32k"
    );
}

#[test]
fn kimi_coding_static_catalog_preserves_provider_default_model() {
    let models = known_models_for_provider(PROVIDER_ID_KIMI_CODING_PLAN);
    assert!(models.iter().any(|model| model.id == "k3"));
    assert!(models.iter().any(|model| model.id == "kimi-for-coding"));
    assert!(models
        .iter()
        .any(|model| model.id == "kimi-k2-turbo-preview"));
    assert_eq!(
        default_model_for_provider_auth(PROVIDER_ID_KIMI_CODING_PLAN, "api_key"),
        "k3"
    );
}

#[test]
fn z_ai_static_catalog_preserves_provider_default_model() {
    let models = known_models_for_provider(PROVIDER_ID_Z_AI);
    assert!(models.iter().any(|model| model.id == "glm-4-plus"));
    assert!(models.iter().any(|model| model.id == "glm-5.3-flash"));
    assert!(models.iter().any(|model| model.id == "glm-4-air"));
    assert!(!models.iter().any(|model| model.id == "glm-4.7"));
    assert!(!models.iter().any(|model| model.id == "glm-4.7-air"));
    assert!(!models.iter().any(|model| model.id == "glm-4.7-flash"));
    assert_eq!(
        default_model_for_provider_auth(PROVIDER_ID_Z_AI, "api_key"),
        "glm-4-plus"
    );
}

#[test]
fn z_ai_coding_static_catalog_uses_current_supported_models() {
    let models = known_models_for_provider(PROVIDER_ID_Z_AI_CODING_PLAN);
    assert_eq!(
        models.first().map(|model| model.id.as_str()),
        Some("glm-5.3")
    );
    assert!(models.iter().any(|model| model.id == "glm-5.3-flash"));
    assert!(models.iter().any(|model| model.id == "glm-5-turbo"));
    assert!(models.iter().any(|model| model.id == "glm-4.7"));
    assert!(models.iter().any(|model| model.id == "glm-5"));
    assert!(models.iter().any(|model| model.id == "glm-4-flash"));
    assert!(!models.iter().any(|model| model.id == "glm-4.7-air"));
    assert!(!models.iter().any(|model| model.id == "glm-4.7-flash"));
    assert_eq!(
        default_model_for_provider_auth(PROVIDER_ID_Z_AI_CODING_PLAN, "api_key"),
        "glm-5.3"
    );
}

#[test]
fn alibaba_coding_static_catalog_preserves_provider_default_model() {
    let models = known_models_for_provider(PROVIDER_ID_ALIBABA_CODING_PLAN);
    assert!(models.iter().any(|model| model.id == "qwen3.6-plus"));
    assert!(models.iter().any(|model| model.id == "qwen3.8-flash"));
    assert!(!models.iter().any(|model| model.id == "qwen3.5-plus"));
    assert_eq!(
        default_model_for_provider_auth(PROVIDER_ID_ALIBABA_CODING_PLAN, "api_key"),
        "qwen3.6-plus"
    );
}

#[test]
fn anthropic_provider_uses_expected_defaults() {
    let provider = find_by_id(PROVIDER_ID_ANTHROPIC).unwrap();
    assert_eq!(provider.name, "Anthropic");
    assert_eq!(provider.default_base_url, "https://api.anthropic.com");
    assert_eq!(provider.default_model, "claude-opus-4-7");
    assert_eq!(provider.default_auth_source, "api_key");
    assert_eq!(provider.supported_auth_sources, API_KEY_ONLY_AUTH_SOURCES);
    assert_eq!(provider.default_transport, "chat_completions");
    assert_eq!(provider.supported_transports, CHAT_ONLY_TRANSPORTS);
    assert_eq!(
        known_context_window_for(PROVIDER_ID_ANTHROPIC, "claude-opus-4-7"),
        Some(1_000_000)
    );
    assert_eq!(
        known_context_window_for(PROVIDER_ID_ANTHROPIC, "claude-haiku-4-5-20251001"),
        Some(200_000)
    );
    assert!(!supports_model_fetch_for(PROVIDER_ID_ANTHROPIC));
    let models = known_models_for_provider(PROVIDER_ID_ANTHROPIC);
    assert_eq!(models.len(), 13);
    assert!(models.iter().any(|model| model.id == "claude-opus-4-7"));
    assert!(models
        .iter()
        .any(|model| model.id == "claude-sonnet-4-5-20250929"));
}

#[test]
fn github_copilot_supports_browser_and_token_auth() {
    let provider = find_by_id(PROVIDER_ID_GITHUB_COPILOT).unwrap();
    assert_eq!(provider.default_auth_source, "github_copilot");
    assert_eq!(provider.supported_auth_sources, GITHUB_COPILOT_AUTH_SOURCES);
    assert_eq!(provider.default_transport, "responses");
    assert_eq!(
        provider.supported_transports,
        RESPONSES_CHAT_AND_ANTHROPIC_TRANSPORTS
    );
}

#[test]
fn known_models_github_copilot_matches_static_catalog() {
    let models = known_models_for_provider_auth(PROVIDER_ID_GITHUB_COPILOT, "github_copilot");
    assert!(models.len() > 10);
    assert!(models.iter().any(|model| model.id == "gpt-4.1"));
    assert!(models.iter().any(|model| model.id == "gpt-5.4-mini"));
    assert!(models.iter().any(|model| model.id == "claude-sonnet-4.6"));
    assert!(models.iter().any(|model| model.id == "raptor-mini"));
    assert!(models.iter().any(|model| model.id == "goldeneye"));
}

#[test]
fn custom_model_name_can_resolve_known_provider_context_window() {
    assert_eq!(
        resolve_context_window_for_provider_auth(
            PROVIDER_ID_GITHUB_COPILOT,
            "github_copilot",
            "totally-custom-runtime-id",
            "Raptor mini (Preview)",
        ),
        Some(264_000)
    );
}

#[test]
fn unknown_custom_model_uses_264k_default_context_window() {
    assert_eq!(default_custom_model_context_window(), 264_000);
}

#[test]
fn arcee_provider_uses_expected_defaults() {
    let provider = find_by_id(PROVIDER_ID_ARCEE).unwrap();
    assert_eq!(provider.name, "Arcee");
    assert_eq!(provider.default_base_url, "https://api.arcee.ai/api/v1");
    assert_eq!(provider.default_model, "trinity-large-thinking");
    assert_eq!(provider.default_auth_source, "api_key");
    assert_eq!(provider.supported_auth_sources, API_KEY_ONLY_AUTH_SOURCES);
    assert_eq!(provider.default_transport, "chat_completions");
    assert_eq!(provider.supported_transports, CHAT_ONLY_TRANSPORTS);
    assert_eq!(
        known_context_window_for(PROVIDER_ID_ARCEE, "trinity-large-thinking"),
        Some(256_000)
    );
    assert!(supports_model_fetch_for(PROVIDER_ID_ARCEE));
}

#[test]
fn nvidia_provider_uses_expected_defaults() {
    let provider = find_by_id(PROVIDER_ID_NVIDIA).unwrap();
    assert_eq!(provider.name, "NVIDIA");
    assert_eq!(
        provider.default_base_url,
        "https://integrate.api.nvidia.com/v1"
    );
    assert_eq!(provider.default_model, "minimaxai/minimax-m2.7");
    assert_eq!(provider.default_auth_source, "api_key");
    assert_eq!(provider.supported_auth_sources, API_KEY_ONLY_AUTH_SOURCES);
    assert_eq!(provider.default_transport, "chat_completions");
    assert_eq!(provider.supported_transports, CHAT_ONLY_TRANSPORTS);
    assert_eq!(
        known_context_window_for(PROVIDER_ID_NVIDIA, "minimaxai/minimax-m2.7"),
        Some(205_000)
    );
    assert!(supports_model_fetch_for(PROVIDER_ID_NVIDIA));
}

#[test]
fn poolside_provider_uses_expected_defaults() {
    let provider = find_by_id(PROVIDER_ID_POOLSIDE).unwrap();
    assert_eq!(provider.name, "Poolside");
    assert_eq!(
        provider.default_base_url,
        "https://inference.poolside.ai/v1"
    );
    assert_eq!(provider.default_model, "poolside/laguna-m.1");
    assert_eq!(provider.default_auth_source, "api_key");
    assert_eq!(provider.supported_auth_sources, API_KEY_ONLY_AUTH_SOURCES);
    assert_eq!(provider.default_transport, "chat_completions");
    assert_eq!(provider.supported_transports, CHAT_ONLY_TRANSPORTS);
    assert_eq!(
        known_context_window_for(PROVIDER_ID_POOLSIDE, "poolside/laguna-m.1"),
        Some(256_000)
    );
    assert!(supports_model_fetch_for(PROVIDER_ID_POOLSIDE));
}

#[test]
fn chutes_provider_uses_expected_defaults() {
    let provider = find_by_id(PROVIDER_ID_CHUTES).unwrap();
    assert_eq!(provider.name, "Chutes");
    assert_eq!(provider.default_base_url, "https://llm.chutes.ai/v1");
    assert_eq!(provider.default_model, "deepseek-ai/DeepSeek-R1");
    assert_eq!(provider.default_auth_source, "api_key");
    assert_eq!(provider.supported_auth_sources, API_KEY_ONLY_AUTH_SOURCES);
    assert_eq!(provider.default_transport, "chat_completions");
    assert_eq!(provider.supported_transports, CHAT_ONLY_TRANSPORTS);
    assert_eq!(
        known_context_window_for(PROVIDER_ID_CHUTES, "deepseek-ai/DeepSeek-R1"),
        Some(128_000)
    );
    assert!(supports_model_fetch_for(PROVIDER_ID_CHUTES));
    assert_eq!(
        default_model_for_provider_auth(PROVIDER_ID_CHUTES, "api_key"),
        "deepseek-ai/DeepSeek-R1"
    );
}

#[test]
fn deepseek_provider_uses_expected_defaults() {
    let provider = find_by_id(PROVIDER_ID_DEEPSEEK).unwrap();
    assert_eq!(provider.name, "DeepSeek");
    assert_eq!(provider.default_base_url, "https://api.deepseek.com");
    assert_eq!(provider.default_model, "deepseek-v4-pro");
    assert_eq!(provider.default_auth_source, "api_key");
    assert_eq!(provider.supported_auth_sources, API_KEY_ONLY_AUTH_SOURCES);
    assert_eq!(provider.default_transport, "chat_completions");
    assert_eq!(provider.supported_transports, CHAT_ONLY_TRANSPORTS);
    assert_eq!(
        known_context_window_for(PROVIDER_ID_DEEPSEEK, "deepseek-v4-pro"),
        Some(1_048_576)
    );
    assert!(supports_model_fetch_for(PROVIDER_ID_DEEPSEEK));
    let models = known_models_for_provider(PROVIDER_ID_DEEPSEEK);
    assert_eq!(models.len(), 2);
    assert_eq!(
        models.first().map(|model| model.id.as_str()),
        Some("deepseek-v4-pro")
    );
    assert_eq!(
        default_model_for_provider_auth(PROVIDER_ID_DEEPSEEK, "api_key"),
        "deepseek-v4-pro"
    );
}

#[test]
fn xiaomi_mimo_platform_provider_uses_expected_defaults() {
    let provider = find_by_id("xiaomi-mimo").unwrap();
    assert_eq!(provider.name, "Xiaomi MiMo");
    assert_eq!(provider.default_base_url, "https://api.xiaomimimo.com/v1");
    assert_eq!(provider.default_model, "mimo-v2.5-pro");
    assert_eq!(provider.default_auth_source, "api_key");
    assert_eq!(provider.supported_auth_sources, API_KEY_ONLY_AUTH_SOURCES);
    assert_eq!(provider.default_transport, "chat_completions");
    assert_eq!(
        provider.supported_transports,
        &["chat_completions", "anthropic_messages"]
    );
    assert_eq!(
        known_context_window_for("xiaomi-mimo", "mimo-v2.5-pro"),
        Some(1_000_000)
    );
    assert!(!supports_model_fetch_for("xiaomi-mimo"));
    let models = known_models_for_provider("xiaomi-mimo");
    assert_eq!(models.len(), 10);
    assert!(models.iter().any(|model| model.id == "mimo-v2.5-pro"));
}

#[test]
fn xiaomi_mimo_provider_uses_expected_defaults() {
    let provider = find_by_id("xiaomi-mimo-token-plan").unwrap();
    assert_eq!(provider.name, "Xiaomi MiMo Token Plan");
    assert_eq!(
        provider.default_base_url,
        "https://token-plan-ams.xiaomimimo.com/v1"
    );
    assert_eq!(provider.default_model, "mimo-v2.5-pro");
    assert_eq!(provider.default_auth_source, "api_key");
    assert_eq!(provider.supported_auth_sources, API_KEY_ONLY_AUTH_SOURCES);
    assert_eq!(provider.default_transport, "chat_completions");
    assert_eq!(
        provider.supported_transports,
        &["chat_completions", "anthropic_messages"]
    );
    assert_eq!(
        known_context_window_for("xiaomi-mimo-token-plan", "mimo-v2-pro"),
        Some(1_000_000)
    );
    assert_eq!(
        known_context_window_for("xiaomi-mimo-token-plan", "mimo-v2-omni"),
        Some(256_000)
    );
    assert_eq!(
        known_context_window_for("xiaomi-mimo-token-plan", "mimo-v2.5-pro"),
        Some(1_000_000)
    );
    assert_eq!(
        known_context_window_for("xiaomi-mimo-token-plan", "mimo-v2.5-pro-ultraspeed"),
        Some(1_000_000)
    );
    assert_eq!(
        known_context_window_for("xiaomi-mimo-token-plan", "mimo-v2.5"),
        Some(1_000_000)
    );
    assert_eq!(
        known_context_window_for("xiaomi-mimo-token-plan", "mimo-v2.5-asr"),
        Some(128_000)
    );
    assert_eq!(
        known_context_window_for("xiaomi-mimo-token-plan", "mimo-v2.5-tts"),
        Some(128_000)
    );
    assert!(!supports_model_fetch_for("xiaomi-mimo-token-plan"));
    let models = known_models_for_provider("xiaomi-mimo-token-plan");
    assert_eq!(models.len(), 10);
    assert!(models.iter().any(|model| model.id == "mimo-v2-pro"));
    assert!(models.iter().any(|model| model.id == "mimo-v2-omni"));
    assert!(models.iter().any(|model| model.id == "mimo-v2.5-pro"));
    assert!(models
        .iter()
        .any(|model| model.id == "mimo-v2.5-pro-ultraspeed"));
    assert!(models.iter().any(|model| model.id == "mimo-v2.5"));
    assert!(models.iter().any(|model| model.id == "mimo-v2.5-asr"));
    assert!(models.iter().any(|model| model.id == "mimo-v2.5-tts"));
    assert!(models
        .iter()
        .any(|model| model.id == "mimo-v2.5-tts-voiceclone"));
    assert!(models
        .iter()
        .any(|model| model.id == "mimo-v2.5-tts-voicedesign"));
    assert!(models.iter().any(|model| model.id == "mimo-v2-tts"));
}

#[test]
fn nous_portal_provider_uses_expected_defaults() {
    let provider = find_by_id("nous-portal").unwrap();
    assert_eq!(provider.name, "Nous Portal");
    assert_eq!(
        provider.default_base_url,
        "https://inference-api.nousresearch.com/v1"
    );
    assert_eq!(provider.default_model, "nousresearch/hermes-4-70b");
    assert_eq!(provider.default_auth_source, "api_key");
    assert_eq!(provider.supported_auth_sources, API_KEY_ONLY_AUTH_SOURCES);
    assert_eq!(provider.default_transport, "chat_completions");
    assert_eq!(provider.supported_transports, CHAT_ONLY_TRANSPORTS);
    assert_eq!(
        known_context_window_for("nous-portal", "nousresearch/hermes-4-70b"),
        Some(131_072)
    );
    assert_eq!(
        known_context_window_for("nous-portal", "nousresearch/hermes-4-405b"),
        Some(131_072)
    );
    assert!(supports_model_fetch_for("nous-portal"));
    let models = known_models_for_provider("nous-portal");
    assert_eq!(models.len(), 4);
    assert!(models
        .iter()
        .any(|model| model.id == "nousresearch/hermes-4-70b"));
    assert!(models
        .iter()
        .any(|model| model.id == "nousresearch/hermes-4-405b"));
}

#[test]
fn hermes_agent_api_provider_uses_documented_openai_compatible_defaults() {
    let provider = find_by_id(PROVIDER_ID_HERMES_AGENT_API).unwrap();
    assert_eq!(provider.name, "Hermes Agent API");
    assert_eq!(provider.default_base_url, "http://localhost:8642/v1");
    assert_eq!(provider.default_model, "hermes-agent");
    assert_eq!(provider.default_auth_source, "api_key");
    assert_eq!(provider.supported_auth_sources, API_KEY_ONLY_AUTH_SOURCES);
    assert_eq!(provider.default_transport, "chat_completions");
    assert_eq!(provider.supported_transports, CHAT_ONLY_TRANSPORTS);
    assert!(supports_model_fetch_for(PROVIDER_ID_HERMES_AGENT_API));
    let models = known_models_for_provider(PROVIDER_ID_HERMES_AGENT_API);
    assert_eq!(models.len(), 1);
    assert_eq!(
        models.first().map(|model| model.id.as_str()),
        Some("hermes-agent")
    );
    assert_eq!(
        default_model_for_provider_auth(PROVIDER_ID_HERMES_AGENT_API, "api_key"),
        "hermes-agent"
    );
    assert_eq!(
        known_context_window_for(PROVIDER_ID_HERMES_AGENT_API, "hermes-agent"),
        Some(128_000)
    );
}
