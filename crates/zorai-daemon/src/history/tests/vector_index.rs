use super::*;

#[tokio::test]
async fn history_store_does_not_open_lancedb_vector_index_by_default() -> Result<()> {
    let (store, root) = make_test_store().await?;

    assert!(
        !root.join("vector-index").join("lancedb").exists(),
        "HistoryStore startup must not create or open the LanceDB sidecar"
    );
    let (_summary, hits) = store.search("missing vector sidecar", 5).await?;
    assert!(hits.is_empty());

    Ok(())
}

#[tokio::test]
async fn lancedb_vector_index_upserts_replaces_and_searches_by_embedding() -> Result<()> {
    let root = tempfile::tempdir()?;
    let index = crate::history::vector_index::VectorIndex::open(root.path());

    index
        .upsert(crate::history::vector_index::VectorDocument {
            source_kind: crate::history::vector_index::VectorSourceKind::AgentMessage,
            source_id: "msg-near".to_string(),
            chunk_id: "0".to_string(),
            title: "near message".to_string(),
            body: "first body".to_string(),
            workspace_id: Some("workspace-a".to_string()),
            thread_id: Some("thread-a".to_string()),
            agent_id: Some("agent-a".to_string()),
            timestamp: 10,
            embedding_model: "test-embed".to_string(),
            embedding: vec![1.0, 0.0, 0.0],
            metadata_json: Some("{\"v\":1}".to_string()),
        })
        .await?;
    index
        .upsert(crate::history::vector_index::VectorDocument {
            source_kind: crate::history::vector_index::VectorSourceKind::AgentMessage,
            source_id: "msg-far".to_string(),
            chunk_id: "0".to_string(),
            title: "far message".to_string(),
            body: "other body".to_string(),
            workspace_id: Some("workspace-a".to_string()),
            thread_id: Some("thread-a".to_string()),
            agent_id: Some("agent-a".to_string()),
            timestamp: 20,
            embedding_model: "test-embed".to_string(),
            embedding: vec![0.0, 1.0, 0.0],
            metadata_json: None,
        })
        .await?;
    index
        .upsert(crate::history::vector_index::VectorDocument {
            source_kind: crate::history::vector_index::VectorSourceKind::AgentMessage,
            source_id: "msg-near".to_string(),
            chunk_id: "0".to_string(),
            title: "near message updated".to_string(),
            body: "updated body".to_string(),
            workspace_id: Some("workspace-a".to_string()),
            thread_id: Some("thread-a".to_string()),
            agent_id: Some("agent-a".to_string()),
            timestamp: 30,
            embedding_model: "test-embed".to_string(),
            embedding: vec![1.0, 0.0, 0.0],
            metadata_json: Some("{\"v\":2}".to_string()),
        })
        .await?;

    let hits = index
        .search(crate::history::vector_index::VectorSearchRequest {
            embedding: vec![1.0, 0.0, 0.0],
            embedding_model: "test-embed".to_string(),
            limit: 5,
            source_kinds: vec![crate::history::vector_index::VectorSourceKind::AgentMessage],
            workspace_id: Some("workspace-a".to_string()),
            thread_id: Some("thread-a".to_string()),
            agent_id: Some("agent-a".to_string()),
        })
        .await?;

    assert_eq!(hits.len(), 2);
    assert_eq!(hits[0].source_id, "msg-near");
    assert_eq!(hits[0].title, "near message updated");
    assert_eq!(hits[0].snippet.as_deref(), Some("updated body"));
    assert_eq!(hits[0].timestamp, Some(30));
    assert_eq!(
        hits.iter()
            .filter(|hit| hit.source_id == "msg-near")
            .count(),
        1,
        "upsert must replace the previous source chunk instead of appending duplicates"
    );

    Ok(())
}

#[tokio::test]
async fn lancedb_vector_index_deletes_source_chunks() -> Result<()> {
    let root = tempfile::tempdir()?;
    let index = crate::history::vector_index::VectorIndex::open(root.path());

    index
        .upsert(crate::history::vector_index::VectorDocument {
            source_kind: crate::history::vector_index::VectorSourceKind::HistoryEntry,
            source_id: "exec-1".to_string(),
            chunk_id: "0".to_string(),
            title: "delete me".to_string(),
            body: "body".to_string(),
            workspace_id: None,
            thread_id: None,
            agent_id: None,
            timestamp: 1,
            embedding_model: "test-embed".to_string(),
            embedding: vec![1.0, 0.0],
            metadata_json: None,
        })
        .await?;
    index
        .delete_source(
            crate::history::vector_index::VectorSourceKind::HistoryEntry,
            "exec-1",
        )
        .await?;

    let hits = index
        .search(crate::history::vector_index::VectorSearchRequest {
            embedding: vec![1.0, 0.0],
            embedding_model: "test-embed".to_string(),
            limit: 5,
            source_kinds: vec![crate::history::vector_index::VectorSourceKind::HistoryEntry],
            workspace_id: None,
            thread_id: None,
            agent_id: None,
        })
        .await?;
    assert!(hits.is_empty());

    Ok(())
}

#[tokio::test]
async fn lancedb_vector_search_is_scoped_to_embedding_model() -> Result<()> {
    let root = tempfile::tempdir()?;
    let index = crate::history::vector_index::VectorIndex::open(root.path());

    index
        .upsert(crate::history::vector_index::VectorDocument {
            source_kind: crate::history::vector_index::VectorSourceKind::AgentMessage,
            source_id: "msg-old-model".to_string(),
            chunk_id: "0".to_string(),
            title: "old model".to_string(),
            body: "body".to_string(),
            workspace_id: None,
            thread_id: None,
            agent_id: None,
            timestamp: 1,
            embedding_model: "text-embedding-3-small".to_string(),
            embedding: vec![1.0, 0.0],
            metadata_json: None,
        })
        .await?;

    let hits = index
        .search(crate::history::vector_index::VectorSearchRequest {
            embedding: vec![1.0, 0.0],
            embedding_model: "text-embedding-3-large".to_string(),
            limit: 5,
            source_kinds: vec![crate::history::vector_index::VectorSourceKind::AgentMessage],
            workspace_id: None,
            thread_id: None,
            agent_id: None,
        })
        .await?;

    assert!(
        hits.is_empty(),
        "switching embedding models must not reuse vectors generated by another model"
    );

    Ok(())
}

#[tokio::test]
async fn lancedb_vector_index_optimize_compacts_upsert_fragments_and_keeps_search_results(
) -> Result<()> {
    let root = tempfile::tempdir()?;
    let index = crate::history::vector_index::VectorIndex::open(root.path());

    assert!(
        index.optimize().await?.is_none(),
        "optimizing before any vectors exist must not create the LanceDB table"
    );

    for revision in 0..6 {
        index
            .upsert(crate::history::vector_index::VectorDocument {
                source_kind: crate::history::vector_index::VectorSourceKind::AgentMessage,
                source_id: format!("msg-{}", revision % 2),
                chunk_id: "0".to_string(),
                title: format!("message revision {revision}"),
                body: format!("body revision {revision}"),
                workspace_id: Some("workspace-a".to_string()),
                thread_id: Some("thread-a".to_string()),
                agent_id: Some("agent-a".to_string()),
                timestamp: revision,
                embedding_model: "test-embed".to_string(),
                embedding: if revision % 2 == 0 {
                    vec![1.0, 0.0, 0.0]
                } else {
                    vec![0.0, 1.0, 0.0]
                },
                metadata_json: None,
            })
            .await?;
    }

    let stats = index
        .optimize()
        .await?
        .expect("optimize must run once the table exists");
    let compaction = stats
        .compaction
        .expect("optimize must compact the fragments left by upserts");
    assert!(
        compaction.fragments_removed > compaction.fragments_added,
        "every upsert writes a new fragment, so compaction must merge them or the index grows without bound"
    );

    let hits = index
        .search(crate::history::vector_index::VectorSearchRequest {
            embedding: vec![1.0, 0.0, 0.0],
            embedding_model: "test-embed".to_string(),
            limit: 5,
            source_kinds: vec![crate::history::vector_index::VectorSourceKind::AgentMessage],
            workspace_id: Some("workspace-a".to_string()),
            thread_id: Some("thread-a".to_string()),
            agent_id: Some("agent-a".to_string()),
        })
        .await?;
    assert_eq!(hits.len(), 2, "compaction must not drop or duplicate live vectors");
    assert_eq!(hits[0].source_id, "msg-0");
    assert_eq!(hits[0].title, "message revision 4");

    Ok(())
}
