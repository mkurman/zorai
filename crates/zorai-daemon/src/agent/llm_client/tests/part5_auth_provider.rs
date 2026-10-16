use super::part5_support::*;
use super::*;
use crate::test_support::EnvGuard;
use tempfile::tempdir;
#[tokio::test]
async fn provider_auth_states_respect_codex_helper_state() {
    let _lock = crate::agent::provider_auth_store::provider_auth_test_env_lock();
    let _guard = EnvGuard::new(&["ZORAI_PROVIDER_AUTH_DB_PATH", "ZORAI_CODEX_CLI_AUTH_PATH"]);
    let root = tempdir().unwrap();
    set_test_auth_env(root.path(), &root.path().join("missing-codex-auth.json"));
    reset_openai_codex_auth_runtime_for_tests();
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.provider = zorai_shared::providers::PROVIDER_ID_OPENAI.to_string();
    config.auth_source = AuthSource::ChatgptSubscription;
    config.providers.insert(
        zorai_shared::providers::PROVIDER_ID_OPENAI.to_string(),
        ProviderConfig {
            base_url: "https://api.openai.com/v1".to_string(),
            model: "gpt-5.4".to_string(),
            api_key: String::new(),
            assistant_id: String::new(),
            auth_source: AuthSource::ChatgptSubscription,
            api_transport: ApiTransport::Responses,
            reasoning_effort: String::new(),
            context_window_tokens: 0,
            response_schema: None,
            stop_sequences: None,
            temperature: None,
            top_p: None,
            top_k: None,
            metadata: None,
            service_tier: None,
            container: None,
            inference_geo: None,
            cache_control: None,
            max_tokens: None,
            anthropic_tool_choice: None,
            output_effort: None,
            openrouter_provider_order: Vec::new(),
            openrouter_provider_ignore: Vec::new(),
            openrouter_allow_fallbacks: None,
            openrouter_response_cache_enabled: false,
            huggingface_provider: None,
        },
    );
    let engine: std::sync::Arc<AgentEngine> =
        AgentEngine::new_test(manager, config, root.path()).await;

    begin_openai_codex_auth_login().expect("login should start");
    let pending = engine
        .get_provider_auth_states()
        .await
        .into_iter()
        .find(|state| state.provider_id == zorai_shared::providers::PROVIDER_ID_OPENAI)
        .expect("openai state should exist");
    assert!(!pending.authenticated);

    complete_openai_codex_auth_with_code_for_tests(
        "good-code",
        &TestExchange {
            result: Ok(stored_auth_fixture()),
        },
    );
    let completed = engine
        .get_provider_auth_states()
        .await
        .into_iter()
        .find(|state| state.provider_id == zorai_shared::providers::PROVIDER_ID_OPENAI)
        .expect("openai state should exist");
    assert!(completed.authenticated);

    logout_openai_codex_auth().expect("logout should succeed");
    let logged_out = engine
        .get_provider_auth_states()
        .await
        .into_iter()
        .find(|state| state.provider_id == zorai_shared::providers::PROVIDER_ID_OPENAI)
        .expect("openai state should exist");
    assert!(!logged_out.authenticated);
}

#[tokio::test]
async fn provider_auth_states_use_codex_cli_auth_when_storage_is_empty() {
    let _lock = crate::agent::provider_auth_store::provider_auth_test_env_lock();
    let _guard = EnvGuard::new(&["ZORAI_PROVIDER_AUTH_DB_PATH", "ZORAI_CODEX_CLI_AUTH_PATH"]);
    let root = tempdir().unwrap();
    let codex_auth_path = prepare_openai_auth_test(root.path(), "codex-auth.json");
    write_codex_cli_auth_fixture(&codex_auth_path);

    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.provider = zorai_shared::providers::PROVIDER_ID_OPENAI.to_string();
    config.auth_source = AuthSource::ChatgptSubscription;
    config.providers.insert(
        zorai_shared::providers::PROVIDER_ID_OPENAI.to_string(),
        ProviderConfig {
            base_url: "https://api.openai.com/v1".to_string(),
            model: "gpt-5.4".to_string(),
            api_key: String::new(),
            assistant_id: String::new(),
            auth_source: AuthSource::ChatgptSubscription,
            api_transport: ApiTransport::Responses,
            reasoning_effort: String::new(),
            context_window_tokens: 0,
            response_schema: None,
            stop_sequences: None,
            temperature: None,
            top_p: None,
            top_k: None,
            metadata: None,
            service_tier: None,
            container: None,
            inference_geo: None,
            cache_control: None,
            max_tokens: None,
            anthropic_tool_choice: None,
            output_effort: None,
            openrouter_provider_order: Vec::new(),
            openrouter_provider_ignore: Vec::new(),
            openrouter_allow_fallbacks: None,
            openrouter_response_cache_enabled: false,
            huggingface_provider: None,
        },
    );
    let engine: std::sync::Arc<AgentEngine> =
        AgentEngine::new_test(manager, config, root.path()).await;

    assert!(read_stored_openai_codex_auth().is_none());
    let openai = engine
        .get_provider_auth_states()
        .await
        .into_iter()
        .find(|state| state.provider_id == zorai_shared::providers::PROVIDER_ID_OPENAI)
        .expect("openai state should exist");

    assert!(openai.authenticated);
    assert!(read_stored_openai_codex_auth().is_none());
}

#[tokio::test]
async fn provider_auth_states_are_listed_by_name_with_custom_last() {
    let _lock = crate::agent::provider_auth_store::provider_auth_test_env_lock();
    let _guard = EnvGuard::new(&["ZORAI_PROVIDER_AUTH_DB_PATH", "ZORAI_CODEX_CLI_AUTH_PATH"]);
    let root = tempdir().unwrap();
    set_test_auth_env(root.path(), &root.path().join("missing-codex-auth.json"));
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    let states = engine.get_provider_auth_states().await;

    assert_eq!(
        states.last().map(|state| state.provider_id.as_str()),
        Some(zorai_shared::providers::PROVIDER_ID_CUSTOM),
        "Custom is the catch-all entry, so it belongs after every named provider"
    );
    let names = states[..states.len() - 1]
        .iter()
        .map(|state| state.provider_name.to_lowercase())
        .collect::<Vec<_>>();
    let mut sorted = names.clone();
    sorted.sort();
    assert_eq!(names, sorted, "operators scan the auth list by provider name");
}
