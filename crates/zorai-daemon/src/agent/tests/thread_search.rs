use super::*;
use crate::session_manager::SessionManager;
use tempfile::tempdir;

async fn test_engine() -> (Arc<AgentEngine>, tempfile::TempDir) {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    (engine, root)
}

async fn seed_thread(engine: &AgentEngine, id: &str, title: &str, updated_at: i64) {
    engine
        .history
        .create_thread(&zorai_protocol::AgentDbThread {
            id: id.to_string(),
            workspace_id: None,
            surface_id: None,
            pane_id: None,
            agent_name: None,
            title: title.to_string(),
            created_at: updated_at,
            updated_at,
            message_count: 0,
            total_tokens: 0,
            last_preview: String::new(),
            metadata_json: None,
        })
        .await
        .expect("create thread");
}

async fn seed_message(engine: &AgentEngine, id: &str, thread_id: &str, role: &str, content: &str) {
    engine
        .history
        .add_message(&zorai_protocol::AgentDbMessage {
            id: id.to_string(),
            thread_id: thread_id.to_string(),
            created_at: 1,
            role: role.to_string(),
            content: content.to_string(),
            provider: None,
            model: None,
            input_tokens: None,
            output_tokens: None,
            total_tokens: None,
            cost_usd: None,
            reasoning: None,
            tool_calls_json: None,
            metadata_json: None,
        })
        .await
        .expect("add message");
}

fn ids(response: &ThreadSearchResponse) -> Vec<&str> {
    response
        .results
        .iter()
        .map(|result| result.thread_id.as_str())
        .collect()
}

#[tokio::test]
async fn finds_a_thread_by_what_was_said_in_it_not_only_by_its_title() {
    let (engine, _root) = test_engine().await;
    seed_thread(&engine, "thread_a", "Model-Specific SFT Data Filtering", 10).await;
    seed_message(&engine, "m1", "thread_a", "user", "Ok, proceed").await;
    seed_message(
        &engine,
        "m2",
        "thread_a",
        "assistant",
        "The Adam check finished; gradient alignment correlated at 0.82.",
    )
    .await;
    seed_thread(&engine, "thread_b", "Unrelated", 20).await;
    seed_message(&engine, "m3", "thread_b", "assistant", "Nothing to see.").await;

    let response = engine.search_threads("gradient align", 20).await.expect("search");

    assert_eq!(ids(&response), vec!["thread_a"]);
    let hit = &response.results[0];
    assert_eq!(hit.matched, vec!["content"]);
    assert!(
        hit.snippet.as_deref().is_some_and(|snippet| snippet.contains("gradient")),
        "{hit:?}"
    );
    assert!(!response.semantic, "embeddings are off by default, so keyword search must carry the result");
}

#[tokio::test]
async fn tool_output_and_deleted_or_rewritten_text_do_not_surface_stale_threads() {
    let (engine, _root) = test_engine().await;
    seed_thread(&engine, "thread_tool", "Tool noise", 10).await;
    seed_message(&engine, "t1", "thread_tool", "tool", "zebracorn appears in a log dump").await;
    seed_thread(&engine, "thread_deleted", "Deleted", 10).await;
    seed_message(&engine, "d1", "thread_deleted", "assistant", "zebracorn was mentioned").await;
    engine
        .history
        .delete_messages("thread_deleted", &["d1"])
        .await
        .expect("delete");
    seed_thread(&engine, "thread_rewritten", "Rewritten", 10).await;
    seed_message(&engine, "r1", "thread_rewritten", "assistant", "zebracorn draft").await;
    seed_message(&engine, "r1", "thread_rewritten", "assistant", "final answer").await;

    let response = engine.search_threads("zebracorn", 20).await.expect("search");

    assert!(response.results.is_empty(), "{:?}", response.results);
    let rewritten = engine.search_threads("final answer", 20).await.expect("search");
    assert_eq!(ids(&rewritten), vec!["thread_rewritten"]);
}

#[tokio::test]
async fn a_thread_matching_by_title_and_content_outranks_one_matching_only_content() {
    let (engine, _root) = test_engine().await;
    seed_thread(&engine, "thread_content_only", "Weekly sync", 30).await;
    seed_message(&engine, "c1", "thread_content_only", "assistant", "We discussed the lora sweep").await;
    seed_thread(&engine, "thread_both", "LoRA sweep plan", 10).await;
    seed_message(&engine, "b1", "thread_both", "user", "start the lora sweep").await;

    let response = engine.search_threads("lora sweep", 20).await.expect("search");

    assert_eq!(ids(&response), vec!["thread_both", "thread_content_only"]);
    assert_eq!(response.results[0].matched, vec!["title", "content"]);
}

#[tokio::test]
async fn internal_handoff_threads_and_fts_syntax_in_queries_do_not_leak_or_error() {
    let (engine, _root) = test_engine().await;
    seed_thread(&engine, "handoff:internal", "quokka handoff", 10).await;
    seed_message(&engine, "h1", "handoff:internal", "assistant", "quokka").await;

    let response = engine.search_threads("quokka", 20).await.expect("search");
    assert!(response.results.is_empty(), "{:?}", response.results);

    for query in ["\"unbalanced", "a AND OR (", "-*:", "   "] {
        engine
            .search_threads(query, 20)
            .await
            .unwrap_or_else(|error| panic!("query {query:?} must not fail: {error}"));
    }
}
