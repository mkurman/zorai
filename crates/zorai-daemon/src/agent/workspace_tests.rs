use super::*;
use zorai_protocol::{
    WorkspaceActor, WorkspacePriority, WorkspaceReviewSubmission, WorkspaceReviewVerdict,
    WorkspaceTaskCreate, WorkspaceTaskMove, WorkspaceTaskStatus, WorkspaceTaskType,
    WorkspaceTaskUpdate, AGENT_ID_SWAROG,
};

async fn test_engine(root: &std::path::Path) -> Result<Arc<AgentEngine>> {
    let manager = SessionManager::new_test(root).await;
    Ok(AgentEngine::new_test(manager, AgentConfig::default(), root).await)
}

fn workspace_request(task_type: WorkspaceTaskType) -> WorkspaceTaskCreate {
    WorkspaceTaskCreate {
        workspace_id: "main".to_string(),
        title: "Implement board".to_string(),
        task_type,
        description: "Create the workspace board surface".to_string(),
        definition_of_done: Some("Board state persists".to_string()),
        priority: None,
        assignee: None,
        reviewer: Some(WorkspaceActor::User),
    }
}

#[tokio::test]
async fn create_workspace_task_reserves_runtime_id_and_writes_mirror() -> Result<()> {
    let root = tempfile::tempdir()?;
    let engine = test_engine(root.path()).await?;

    let task = engine
        .create_workspace_task(
            workspace_request(WorkspaceTaskType::Thread),
            WorkspaceActor::User,
        )
        .await?;

    assert_eq!(task.status, WorkspaceTaskStatus::Todo);
    assert_eq!(task.priority, WorkspacePriority::Low);
    assert!(task
        .thread_id
        .as_deref()
        .is_some_and(|id| id.starts_with("workspace-thread:")));
    assert!(task.goal_run_id.is_none());

    let mirror_path = root.path().join("workspaces/workspace-main/workspace.json");
    let mirror = tokio::fs::read_to_string(mirror_path).await?;
    let mirror_json: serde_json::Value = serde_json::from_str(&mirror)?;
    assert_eq!(mirror_json["schema_version"].as_u64(), Some(1));
    assert!(mirror_json["generated_at"].as_u64().is_some());
    assert!(mirror.contains("\"workspace_id\": \"main\""));
    assert!(mirror.contains("\"title\": \"Implement board\""));
    assert!(mirror.contains("\"notice_type\": \"created\""));
    Ok(())
}

#[tokio::test]
async fn run_workspace_task_requires_assignee() -> Result<()> {
    let root = tempfile::tempdir()?;
    let engine = test_engine(root.path()).await?;
    let task = engine
        .create_workspace_task(
            workspace_request(WorkspaceTaskType::Thread),
            WorkspaceActor::User,
        )
        .await?;

    let error = engine
        .run_workspace_task(&task.id)
        .await
        .expect_err("task without assignee must not run");

    assert!(error.to_string().contains("without an assignee"));
    let loaded = engine
        .get_workspace_task(&task.id)
        .await?
        .expect("task remains stored");
    assert_eq!(loaded.status, WorkspaceTaskStatus::Todo);
    Ok(())
}

#[tokio::test]
async fn run_thread_workspace_task_stays_in_progress_until_thread_done() -> Result<()> {
    let root = tempfile::tempdir()?;
    let recorded = Arc::new(std::sync::Mutex::new(VecDeque::new()));
    let mut config = AgentConfig::default();
    config.provider = "openai".to_string();
    config.api_transport = ApiTransport::ChatCompletions;
    config.base_url = crate::agent::tests::spawn_goal_recording_server(
        recorded,
        "Thread work complete".to_string(),
    )
    .await;
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, config, root.path()).await;
    let mut request = workspace_request(WorkspaceTaskType::Thread);
    request.assignee = Some(WorkspaceActor::Agent(AGENT_ID_SWAROG.to_string()));
    request.reviewer = Some(WorkspaceActor::User);
    let task = engine
        .create_workspace_task(request, WorkspaceActor::User)
        .await?;

    let running = engine.run_workspace_task(&task.id).await?;

    assert_eq!(running.status, WorkspaceTaskStatus::InProgress);
    assert!(running.completed_at.is_none());
    assert!(running
        .runtime_history
        .iter()
        .any(|entry| entry.thread_id == running.thread_id
            && entry.source.as_deref() == Some("workspace_runtime")));
    Ok(())
}

#[tokio::test]
async fn rerunning_thread_workspace_task_uses_updated_assignee_model() -> Result<()> {
    let root = tempfile::tempdir()?;
    let recorded = Arc::new(std::sync::Mutex::new(VecDeque::new()));
    let mut config = AgentConfig::default();
    config.provider = "openai".to_string();
    config.model = "gpt-main".to_string();
    config.api_transport = ApiTransport::ChatCompletions;
    config.base_url = crate::agent::tests::spawn_goal_recording_server(
        recorded.clone(),
        "Thread work complete".to_string(),
    )
    .await;
    config.sub_agents.push(SubAgentDefinition {
        generation_settings: None,
        base_url: None,
        claude_permission_mode: None,
        id: "first".to_string(),
        name: "First".to_string(),
        provider: "openai".to_string(),
        model: "gpt-first".to_string(),
        role: Some("worker".to_string()),
        system_prompt: Some("First worker.".to_string()),
        tool_whitelist: None,
        tool_blacklist: None,
        context_budget_tokens: None,
        context_window_tokens: None,
        max_duration_secs: None,
        supervisor_config: None,
        enabled: true,
        builtin: false,
        immutable_identity: false,
        disable_allowed: true,
        delete_allowed: true,
        protected_reason: None,
        reasoning_effort: Some("medium".to_string()),
        api_transport: None,
        openrouter_provider_order: Vec::new(),
        openrouter_provider_ignore: Vec::new(),
        openrouter_allow_fallbacks: None,
        huggingface_provider: None,
        created_at: 1,
    });
    config.sub_agents.push(SubAgentDefinition {
        generation_settings: None,
        base_url: None,
        claude_permission_mode: None,
        id: "second".to_string(),
        name: "Second".to_string(),
        provider: "openai".to_string(),
        model: "gpt-second".to_string(),
        role: Some("worker".to_string()),
        system_prompt: Some("Second worker.".to_string()),
        tool_whitelist: None,
        tool_blacklist: None,
        context_budget_tokens: None,
        context_window_tokens: None,
        max_duration_secs: None,
        supervisor_config: None,
        enabled: true,
        builtin: false,
        immutable_identity: false,
        disable_allowed: true,
        delete_allowed: true,
        protected_reason: None,
        reasoning_effort: Some("medium".to_string()),
        api_transport: None,
        openrouter_provider_order: Vec::new(),
        openrouter_provider_ignore: Vec::new(),
        openrouter_allow_fallbacks: None,
        huggingface_provider: None,
        created_at: 2,
    });
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, config, root.path()).await;
    let mut request = workspace_request(WorkspaceTaskType::Thread);
    request.assignee = Some(WorkspaceActor::Subagent("first".to_string()));
    request.reviewer = Some(WorkspaceActor::User);
    let task = engine
        .create_workspace_task(request, WorkspaceActor::User)
        .await?;

    let first_run = engine.run_workspace_task(&task.id).await?;
    let first_thread_id = first_run.thread_id.as_deref().expect("thread id");
    let first_profile = engine
        .thread_execution_profiles
        .read()
        .await
        .get(first_thread_id)
        .cloned()
        .expect("first run should store thread execution profile");
    assert_eq!(first_profile.model.as_deref(), Some("gpt-first"));
    engine.stop_workspace_task(&task.id).await?;
    engine
        .update_workspace_task(
            &task.id,
            WorkspaceTaskUpdate {
                assignee: Some(Some(WorkspaceActor::Subagent("second".to_string()))),
                ..WorkspaceTaskUpdate::default()
            },
        )
        .await?;

    let second_run = engine.run_workspace_task(&task.id).await?;

    assert_eq!(first_run.thread_id, second_run.thread_id);
    let bodies = recorded.lock().expect("request log").clone();
    assert_eq!(bodies.len(), 2, "expected one request per workspace run");
    let second_thread_id = second_run.thread_id.as_deref().expect("thread id");
    let second_profile = engine
        .thread_execution_profiles
        .read()
        .await
        .get(second_thread_id)
        .cloned()
        .expect("second run should store thread execution profile");
    assert_eq!(second_profile.model.as_deref(), Some("gpt-second"));
    let active_agent_id = engine.active_agent_id_for_thread(second_thread_id).await;
    assert_eq!(active_agent_id.as_deref(), Some("second"));
    engine.stop_workspace_task(&task.id).await?;
    engine
        .update_workspace_task(
            &task.id,
            WorkspaceTaskUpdate {
                assignee: Some(Some(WorkspaceActor::Agent(AGENT_ID_SWAROG.to_string()))),
                ..WorkspaceTaskUpdate::default()
            },
        )
        .await?;

    let third_run = engine.run_workspace_task(&task.id).await?;

    assert_eq!(first_run.thread_id, third_run.thread_id);
    let bodies = recorded.lock().expect("request log").clone();
    assert_eq!(bodies.len(), 3, "expected one request per workspace run");
    let third_thread_id = third_run.thread_id.as_deref().expect("thread id");
    let third_profile = engine
        .thread_execution_profiles
        .read()
        .await
        .get(third_thread_id)
        .cloned()
        .expect("third run should store thread execution profile");
    assert_eq!(third_profile.model.as_deref(), Some("gpt-main"));
    let active_agent_id = engine.active_agent_id_for_thread(third_thread_id).await;
    assert_eq!(active_agent_id.as_deref(), Some(AGENT_ID_SWAROG));
    Ok(())
}

#[tokio::test]
async fn thread_done_completes_matching_workspace_thread_task() -> Result<()> {
    let root = tempfile::tempdir()?;
    let engine = test_engine(root.path()).await?;
    let mut request = workspace_request(WorkspaceTaskType::Thread);
    request.assignee = Some(WorkspaceActor::Agent(AGENT_ID_SWAROG.to_string()));
    request.reviewer = Some(WorkspaceActor::User);
    let mut task = engine
        .create_workspace_task(request, WorkspaceActor::User)
        .await?;
    task.status = WorkspaceTaskStatus::InProgress;
    task.thread_id = Some("workspace-thread:test".to_string());
    engine.history.upsert_workspace_task(&task).await?;

    engine
        .complete_workspace_thread_task_by_thread_id("workspace-thread:test")
        .await?;

    let completed = engine
        .get_workspace_task(&task.id)
        .await?
        .expect("task should exist");
    assert_eq!(completed.status, WorkspaceTaskStatus::InReview);
    assert!(completed.completed_at.is_none());
    Ok(())
}

#[tokio::test]
async fn get_workspace_task_backfills_missing_history_for_existing_in_review_thread() -> Result<()>
{
    let root = tempfile::tempdir()?;
    let engine = test_engine(root.path()).await?;
    let mut request = workspace_request(WorkspaceTaskType::Thread);
    request.assignee = Some(WorkspaceActor::Agent(AGENT_ID_SWAROG.to_string()));
    request.reviewer = Some(WorkspaceActor::User);
    let mut task = engine
        .create_workspace_task(request, WorkspaceActor::User)
        .await?;
    task.status = WorkspaceTaskStatus::InReview;
    task.thread_id = Some("workspace-thread:legacy-run".to_string());
    task.runtime_history.clear();
    engine.history.upsert_workspace_task(&task).await?;

    let loaded = engine
        .get_workspace_task(&task.id)
        .await?
        .expect("workspace task should exist");

    assert!(loaded.runtime_history.iter().any(|entry| {
        entry.thread_id.as_deref() == Some("workspace-thread:legacy-run")
            && entry.source.as_deref() == Some("workspace_runtime")
    }));
    let stored = engine
        .history
        .get_workspace_task(&task.id)
        .await?
        .expect("backfilled task should persist");
    assert!(stored.runtime_history.iter().any(|entry| {
        entry.thread_id.as_deref() == Some("workspace-thread:legacy-run")
            && entry.source.as_deref() == Some("workspace_runtime")
    }));
    Ok(())
}

#[tokio::test]
async fn failed_workspace_run_start_rolls_back_status_and_records_notice() -> Result<()> {
    let root = tempfile::tempdir()?;
    let engine = test_engine(root.path()).await?;
    let mut request = workspace_request(WorkspaceTaskType::Thread);
    request.assignee = Some(WorkspaceActor::Agent(AGENT_ID_SWAROG.to_string()));
    let mut task = engine
        .create_workspace_task(request, WorkspaceActor::User)
        .await?;
    let previous_status = task.status.clone();
    let previous_sort_order = task.sort_order;
    let previous_started_at = task.started_at;
    task.status = WorkspaceTaskStatus::InProgress;
    task.sort_order = 3;
    task.started_at = Some(now_millis());
    task.updated_at = now_millis();
    engine.history.upsert_workspace_task(&task).await?;

    let restored = engine
        .fail_workspace_task_run_start(
            task,
            previous_status,
            previous_sort_order,
            previous_started_at,
            "workspace launch failed",
        )
        .await
        .expect("failure state should be persisted");

    assert_eq!(restored.status, WorkspaceTaskStatus::Todo);
    assert_eq!(restored.sort_order, previous_sort_order);
    assert!(restored.started_at.is_none());
    let notices = engine
        .list_workspace_notices("main", Some(&restored.id))
        .await?;
    assert!(notices
        .iter()
        .any(|notice| notice.notice_type == "run_failed"));
    Ok(())
}

#[tokio::test]
async fn run_goal_workspace_task_uses_reserved_goal_id() -> Result<()> {
    let root = tempfile::tempdir()?;
    let engine = test_engine(root.path()).await?;
    let mut request = workspace_request(WorkspaceTaskType::Goal);
    request.assignee = Some(WorkspaceActor::Agent(AGENT_ID_SWAROG.to_string()));
    let task = engine
        .create_workspace_task(request, WorkspaceActor::User)
        .await?;
    let reserved_goal_id = task.goal_run_id.clone().expect("goal id reserved");

    let task = engine.run_workspace_task(&task.id).await?;

    assert_eq!(task.status, WorkspaceTaskStatus::InProgress);
    assert_eq!(task.goal_run_id.as_deref(), Some(reserved_goal_id.as_str()));
    let goal_run = engine
        .history
        .get_goal_run(&reserved_goal_id)
        .await?
        .expect("reserved goal run should materialize");
    assert_eq!(
        goal_run.client_request_id.as_deref(),
        Some(task.id.as_str())
    );
    assert_eq!(goal_run.status, GoalRunStatus::Queued);
    Ok(())
}

#[tokio::test]
async fn completed_thread_workspace_task_moves_to_review_when_reviewer_exists() -> Result<()> {
    let root = tempfile::tempdir()?;
    let engine = test_engine(root.path()).await?;
    let mut request = workspace_request(WorkspaceTaskType::Thread);
    request.assignee = Some(WorkspaceActor::Agent(AGENT_ID_SWAROG.to_string()));
    request.reviewer = Some(WorkspaceActor::User);
    let mut task = engine
        .create_workspace_task(request, WorkspaceActor::User)
        .await?;
    task.status = WorkspaceTaskStatus::InProgress;
    engine.history.upsert_workspace_task(&task).await?;

    let completed = engine
        .complete_workspace_task_runtime_success(task, "Workspace thread completed")
        .await?;

    assert_eq!(completed.status, WorkspaceTaskStatus::InReview);
    assert!(completed.completed_at.is_none());
    let notices = engine
        .list_workspace_notices("main", Some(&completed.id))
        .await?;
    assert!(notices
        .iter()
        .any(|notice| notice.notice_type == "runtime_completed"));
    Ok(())
}

#[tokio::test]
async fn stop_thread_workspace_task_returns_to_todo_with_notice() -> Result<()> {
    let root = tempfile::tempdir()?;
    let engine = test_engine(root.path()).await?;
    let mut request = workspace_request(WorkspaceTaskType::Thread);
    request.assignee = Some(WorkspaceActor::Agent(AGENT_ID_SWAROG.to_string()));
    let mut task = engine
        .create_workspace_task(request, WorkspaceActor::User)
        .await?;
    task.status = WorkspaceTaskStatus::InProgress;
    task.sort_order = 4;
    task.started_at = Some(now_millis());
    engine.history.upsert_workspace_task(&task).await?;

    let stopped = engine
        .stop_workspace_task(&task.id)
        .await?
        .expect("task should stop");

    assert_eq!(stopped.status, WorkspaceTaskStatus::Todo);
    assert_eq!(stopped.sort_order, 1);
    assert!(stopped.started_at.is_none());
    assert!(stopped.completed_at.is_none());
    let notices = engine
        .list_workspace_notices("main", Some(&task.id))
        .await?;
    assert!(notices.iter().any(|notice| notice.notice_type == "stop"));
    Ok(())
}

#[tokio::test]
async fn stop_goal_workspace_task_returns_to_todo_not_done() -> Result<()> {
    let root = tempfile::tempdir()?;
    let engine = test_engine(root.path()).await?;
    let mut request = workspace_request(WorkspaceTaskType::Goal);
    request.assignee = Some(WorkspaceActor::Agent(AGENT_ID_SWAROG.to_string()));
    let task = engine
        .create_workspace_task(request, WorkspaceActor::User)
        .await?;
    let task = engine.run_workspace_task(&task.id).await?;

    let stopped = engine
        .stop_workspace_task(&task.id)
        .await?
        .expect("task should stop");

    assert_eq!(stopped.status, WorkspaceTaskStatus::Todo);
    assert!(stopped.completed_at.is_none());
    assert!(stopped.started_at.is_none());
    assert_eq!(stopped.goal_run_id, task.goal_run_id);
    Ok(())
}

#[tokio::test]
async fn move_workspace_task_without_sort_order_appends_to_target_column() -> Result<()> {
    let root = tempfile::tempdir()?;
    let engine = test_engine(root.path()).await?;
    let mut first_request = workspace_request(WorkspaceTaskType::Thread);
    first_request.title = "First".to_string();
    let mut second_request = workspace_request(WorkspaceTaskType::Thread);
    second_request.title = "Second".to_string();
    let first = engine
        .create_workspace_task(first_request, WorkspaceActor::User)
        .await?;
    let second = engine
        .create_workspace_task(second_request, WorkspaceActor::User)
        .await?;

    let moved_first = engine
        .move_workspace_task(WorkspaceTaskMove {
            task_id: first.id.clone(),
            status: WorkspaceTaskStatus::InReview,
            sort_order: None,
        })
        .await?
        .expect("first task should move");
    let moved_second = engine
        .move_workspace_task(WorkspaceTaskMove {
            task_id: second.id.clone(),
            status: WorkspaceTaskStatus::InReview,
            sort_order: None,
        })
        .await?
        .expect("second task should move");

    assert_eq!(moved_first.sort_order, 1);
    assert_eq!(moved_second.sort_order, 2);
    Ok(())
}

#[tokio::test]
async fn move_workspace_task_with_sort_order_inserts_before_existing_task() -> Result<()> {
    let root = tempfile::tempdir()?;
    let engine = test_engine(root.path()).await?;
    let mut first_request = workspace_request(WorkspaceTaskType::Thread);
    first_request.title = "First".to_string();
    let mut second_request = workspace_request(WorkspaceTaskType::Thread);
    second_request.title = "Second".to_string();
    let mut third_request = workspace_request(WorkspaceTaskType::Thread);
    third_request.title = "Third".to_string();
    let first = engine
        .create_workspace_task(first_request, WorkspaceActor::User)
        .await?;
    let second = engine
        .create_workspace_task(second_request, WorkspaceActor::User)
        .await?;
    let third = engine
        .create_workspace_task(third_request, WorkspaceActor::User)
        .await?;

    engine
        .move_workspace_task(WorkspaceTaskMove {
            task_id: third.id.clone(),
            status: WorkspaceTaskStatus::Todo,
            sort_order: Some(second.sort_order),
        })
        .await?;

    let tasks = engine.list_workspace_tasks("main", false).await?;
    let titles = tasks
        .iter()
        .filter(|task| task.status == WorkspaceTaskStatus::Todo)
        .map(|task| task.title.as_str())
        .collect::<Vec<_>>();
    assert_eq!(titles, vec![first.title, third.title, second.title]);
    Ok(())
}

#[tokio::test]
async fn moving_to_review_with_agent_reviewer_records_review_request_notice() -> Result<()> {
    let root = tempfile::tempdir()?;
    let engine = test_engine(root.path()).await?;
    let mut request = workspace_request(WorkspaceTaskType::Thread);
    request.reviewer = Some(WorkspaceActor::Agent(AGENT_ID_SWAROG.to_string()));
    let task = engine
        .create_workspace_task(request, WorkspaceActor::User)
        .await?;

    engine
        .move_workspace_task(WorkspaceTaskMove {
            task_id: task.id.clone(),
            status: WorkspaceTaskStatus::InReview,
            sort_order: None,
        })
        .await?;

    let notices = engine
        .list_workspace_notices("main", Some(&task.id))
        .await?;
    assert!(notices
        .iter()
        .any(|notice| notice.notice_type == "review_requested"
            && notice.message.contains("agent:swarog")
            && notice.message.contains("queued review task")));
    let reviewed = engine
        .get_workspace_task(&task.id)
        .await?
        .expect("task should exist");
    assert!(reviewed.runtime_history.iter().any(|entry| {
        entry.agent_task_id.is_some() && entry.source.as_deref() == Some("workspace_review")
    }));
    Ok(())
}

#[tokio::test]
async fn moving_to_review_with_subagent_reviewer_queues_reviewer_persona_task() -> Result<()> {
    let root = tempfile::tempdir()?;
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.sub_agents.push(SubAgentDefinition {
        generation_settings: None,
        base_url: None,
        claude_permission_mode: None,
        id: "qa".to_string(),
        name: "QA".to_string(),
        provider: "openai".to_string(),
        model: "gpt-5.4-mini".to_string(),
        role: Some("reviewer".to_string()),
        system_prompt: Some("Review workspace delivery as QA.".to_string()),
        tool_whitelist: None,
        tool_blacklist: None,
        context_budget_tokens: None,
        context_window_tokens: None,
        max_duration_secs: None,
        supervisor_config: None,
        enabled: true,
        builtin: false,
        immutable_identity: false,
        disable_allowed: true,
        delete_allowed: true,
        protected_reason: None,
        reasoning_effort: Some("medium".to_string()),
        api_transport: None,
        openrouter_provider_order: Vec::new(),
        openrouter_provider_ignore: Vec::new(),
        openrouter_allow_fallbacks: None,
        huggingface_provider: None,
        created_at: 1,
    });
    let engine = AgentEngine::new_test(manager, config, root.path()).await;
    let mut request = workspace_request(WorkspaceTaskType::Thread);
    request.reviewer = Some(WorkspaceActor::Subagent("qa".to_string()));
    let task = engine
        .create_workspace_task(request, WorkspaceActor::User)
        .await?;

    engine
        .move_workspace_task(WorkspaceTaskMove {
            task_id: task.id.clone(),
            status: WorkspaceTaskStatus::InReview,
            sort_order: None,
        })
        .await?;

    let reviewed = engine
        .get_workspace_task(&task.id)
        .await?
        .expect("task should exist");
    let review_task_id = reviewed
        .runtime_history
        .iter()
        .find(|entry| entry.source.as_deref() == Some("workspace_review"))
        .and_then(|entry| entry.agent_task_id.clone())
        .expect("review task should be recorded");
    let review_task = engine
        .tasks
        .lock()
        .await
        .iter()
        .find(|task| task.id == review_task_id)
        .cloned()
        .expect("review task should remain queued");

    assert_eq!(review_task.sub_agent_def_id.as_deref(), Some("qa"));
    assert_eq!(review_task.override_provider.as_deref(), Some("openai"));
    assert_eq!(review_task.override_model.as_deref(), Some("gpt-5.4-mini"));
    let prompt = review_task
        .override_system_prompt
        .as_deref()
        .expect("review task should carry reviewer persona prompt");
    assert!(prompt.contains("Agent persona id: qa"), "{prompt}");
    assert!(
        prompt.contains("Review workspace delivery as QA."),
        "{prompt}"
    );
    Ok(())
}

#[tokio::test]
async fn failed_workspace_review_archives_runtime_and_starts_new_follow_up() -> Result<()> {
    let root = tempfile::tempdir()?;
    let recorded = Arc::new(std::sync::Mutex::new(VecDeque::new()));
    let mut config = AgentConfig::default();
    config.provider = "openai".to_string();
    config.api_transport = ApiTransport::ChatCompletions;
    config.base_url =
        crate::agent::tests::spawn_goal_recording_server(recorded, "Follow-up started".to_string())
            .await;
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, config, root.path()).await;
    let mut request = workspace_request(WorkspaceTaskType::Thread);
    request.assignee = Some(WorkspaceActor::Agent(AGENT_ID_SWAROG.to_string()));
    request.reviewer = Some(WorkspaceActor::User);
    let mut task = engine
        .create_workspace_task(request, WorkspaceActor::User)
        .await?;
    task.status = WorkspaceTaskStatus::InReview;
    task.thread_id = Some("workspace-thread:previous".to_string());
    engine.history.upsert_workspace_task(&task).await?;

    let restarted = engine
        .submit_workspace_review(WorkspaceReviewSubmission {
            task_id: task.id.clone(),
            verdict: WorkspaceReviewVerdict::Fail,
            message: Some("Add acceptance coverage for the drag path".to_string()),
        })
        .await?
        .expect("failed review should keep task");

    assert_eq!(restarted.status, WorkspaceTaskStatus::InProgress);
    assert_ne!(
        restarted.thread_id.as_deref(),
        Some("workspace-thread:previous")
    );
    assert!(restarted
        .thread_id
        .as_deref()
        .is_some_and(|id| id.starts_with(&format!("workspace-thread:{}:", task.id))));
    assert_eq!(restarted.runtime_history.len(), 2);
    assert_eq!(
        restarted.runtime_history[0].thread_id.as_deref(),
        restarted.thread_id.as_deref()
    );
    let archived = restarted
        .runtime_history
        .iter()
        .find(|entry| entry.thread_id.as_deref() == Some("workspace-thread:previous"))
        .expect("previous runtime should be archived");
    assert_eq!(
        archived.thread_id.as_deref(),
        Some("workspace-thread:previous")
    );
    let expected_review_path = format!("task-{}/failed-review.md", task.id);
    assert_eq!(
        archived.review_path.as_deref(),
        Some(expected_review_path.as_str())
    );
    assert!(archived
        .review_feedback
        .as_deref()
        .is_some_and(|feedback| feedback.contains("Add acceptance coverage")));
    let review_doc = tokio::fs::read_to_string(
        root.path()
            .join("workspaces")
            .join("workspace-main")
            .join(format!("task-{}/failed-review.md", task.id)),
    )
    .await?;
    assert!(review_doc.contains("workspace-thread:previous"));
    assert!(review_doc.contains("Add acceptance coverage"));

    let mirror = tokio::fs::read_to_string(
        root.path()
            .join("workspaces")
            .join("workspace-main")
            .join("workspace.json"),
    )
    .await?;
    assert!(mirror.contains("runtime_history"));
    assert!(mirror.contains("failed-review.md"));
    Ok(())
}

#[tokio::test]
async fn failed_workspace_review_returns_before_follow_up_thread_finishes() -> Result<()> {
    let root = tempfile::tempdir()?;
    let request_started = Arc::new(tokio::sync::Notify::new());
    let mut config = AgentConfig::default();
    config.provider = "openai".to_string();
    config.api_transport = ApiTransport::ChatCompletions;
    config.base_url = spawn_hanging_chat_server(request_started.clone()).await;
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, config, root.path()).await;
    let mut request = workspace_request(WorkspaceTaskType::Thread);
    request.assignee = Some(WorkspaceActor::Agent(AGENT_ID_SWAROG.to_string()));
    request.reviewer = Some(WorkspaceActor::User);
    let mut task = engine
        .create_workspace_task(request, WorkspaceActor::User)
        .await?;
    task.status = WorkspaceTaskStatus::InReview;
    task.thread_id = Some("workspace-thread:previous".to_string());
    engine.history.upsert_workspace_task(&task).await?;

    let restarted = tokio::time::timeout(
        std::time::Duration::from_millis(150),
        engine.submit_workspace_review(WorkspaceReviewSubmission {
            task_id: task.id.clone(),
            verdict: WorkspaceReviewVerdict::Fail,
            message: Some("Return this to implementation with acceptance tests".to_string()),
        }),
    )
    .await
    .expect("failed review should return before the follow-up run completes")?
    .expect("failed review should keep the task");

    assert_eq!(restarted.status, WorkspaceTaskStatus::InProgress);
    assert_ne!(
        restarted.thread_id.as_deref(),
        Some("workspace-thread:previous")
    );
    assert!(restarted.runtime_history.iter().any(|entry| {
        entry.thread_id.as_deref() == Some("workspace-thread:previous")
            && entry.review_feedback.as_deref()
                == Some("Return this to implementation with acceptance tests")
    }));
    let follow_up_thread_id = restarted
        .thread_id
        .as_deref()
        .expect("failed review should reserve a follow-up thread");
    let follow_up_thread = engine
        .threads
        .read()
        .await
        .get(follow_up_thread_id)
        .cloned()
        .expect("failed review should seed the follow-up thread");
    assert!(follow_up_thread.messages.iter().any(|message| message
        .content
        .contains("Review verdict: fail")
        && message
            .content
            .contains("Return this to implementation with acceptance tests")
        && message
            .content
            .contains("Create the workspace board surface")));
    Ok(())
}

#[tokio::test]
async fn failed_goal_workspace_review_archives_goal_and_pins_new_goal_run() -> Result<()> {
    let root = tempfile::tempdir()?;
    let engine = test_engine(root.path()).await?;
    let mut request = workspace_request(WorkspaceTaskType::Goal);
    request.assignee = Some(WorkspaceActor::Agent(AGENT_ID_SWAROG.to_string()));
    request.reviewer = Some(WorkspaceActor::User);
    let mut task = engine
        .create_workspace_task(request, WorkspaceActor::User)
        .await?;
    task.status = WorkspaceTaskStatus::InReview;
    task.goal_run_id = Some("workspace-goal:previous".to_string());
    engine.history.upsert_workspace_task(&task).await?;

    let restarted = engine
        .submit_workspace_review(WorkspaceReviewSubmission {
            task_id: task.id.clone(),
            verdict: WorkspaceReviewVerdict::Fail,
            message: Some("Document the missing release notes".to_string()),
        })
        .await?
        .expect("failed review should keep task");

    assert_eq!(restarted.status, WorkspaceTaskStatus::InProgress);
    assert_ne!(
        restarted.goal_run_id.as_deref(),
        Some("workspace-goal:previous")
    );
    assert!(restarted
        .goal_run_id
        .as_deref()
        .is_some_and(|id| id.starts_with(&format!("workspace-goal:{}:", task.id))));
    assert!(restarted.runtime_history.iter().any(|entry| {
        entry.goal_run_id.as_deref() == restarted.goal_run_id.as_deref()
            && entry.source.as_deref() == Some("workspace_runtime")
    }));
    assert!(restarted.runtime_history.iter().any(|entry| {
        entry.goal_run_id.as_deref() == Some("workspace-goal:previous")
            && entry.review_feedback.as_deref() == Some("Document the missing release notes")
    }));
    let new_goal_run_id = restarted.goal_run_id.as_deref().expect("new goal run id");
    assert!(engine
        .history
        .get_goal_run(new_goal_run_id)
        .await?
        .is_some());
    Ok(())
}

async fn spawn_hanging_chat_server(request_started: Arc<tokio::sync::Notify>) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind hanging chat server");
    let addr = listener.local_addr().expect("hanging chat server addr");
    tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = listener.accept().await else {
                break;
            };
            let request_started = request_started.clone();
            tokio::spawn(async move {
                let mut buffer = vec![0u8; 4096];
                let _ = tokio::io::AsyncReadExt::read(&mut socket, &mut buffer).await;
                request_started.notify_waiters();
                std::future::pending::<()>().await;
            });
        }
    });
    format!("http://{addr}/v1")
}

#[tokio::test]
async fn completed_goal_workspace_task_syncs_to_review_when_reviewer_exists() -> Result<()> {
    let root = tempfile::tempdir()?;
    let engine = test_engine(root.path()).await?;
    let mut request = workspace_request(WorkspaceTaskType::Goal);
    request.assignee = Some(WorkspaceActor::Agent(AGENT_ID_SWAROG.to_string()));
    request.reviewer = Some(WorkspaceActor::User);
    let task = engine
        .create_workspace_task(request, WorkspaceActor::User)
        .await?;
    let task = engine.run_workspace_task(&task.id).await?;
    let goal_run_id = task.goal_run_id.clone().expect("goal run id");
    let mut goal_run = engine
        .history
        .get_goal_run(&goal_run_id)
        .await?
        .expect("goal run exists");
    goal_run.status = GoalRunStatus::Completed;
    goal_run.completed_at = Some(now_millis());
    goal_run.updated_at = now_millis();
    engine.history.upsert_goal_run(&goal_run).await?;
    engine
        .history
        .conn
        .call({
            let goal_run_id = goal_run_id.clone();
            move |conn| {
                conn.execute(
                    "UPDATE goal_run_steps SET started_at = 'not-an-integer' WHERE goal_run_id = ?1",
                    rusqlite::params![goal_run_id],
                )?;
                Ok(())
            }
        })
        .await
        .map_err(|error| anyhow::anyhow!("{error}"))?;

    let synced = engine
        .get_workspace_task(&task.id)
        .await?
        .expect("workspace task exists");

    assert_eq!(synced.status, WorkspaceTaskStatus::InReview);
    let notices = engine
        .list_workspace_notices("main", Some(&task.id))
        .await?;
    assert!(notices
        .iter()
        .any(|notice| notice.notice_type == "runtime_completed"));
    let completion_notices = notices
        .iter()
        .filter(|notice| notice.notice_type == "task_completion")
        .collect::<Vec<_>>();
    assert_eq!(completion_notices.len(), 1);
    assert_eq!(
        completion_notices[0].actor,
        Some(WorkspaceActor::Agent(AGENT_ID_SWAROG.to_string()))
    );
    assert!(completion_notices[0].message.contains(&goal_run_id));

    let _ = engine
        .get_workspace_task(&task.id)
        .await?
        .expect("workspace task still exists");
    let notices_after_resync = engine
        .list_workspace_notices("main", Some(&task.id))
        .await?;
    assert_eq!(
        notices_after_resync
            .iter()
            .filter(|notice| notice.notice_type == "task_completion")
            .count(),
        1
    );
    Ok(())
}

#[tokio::test]
async fn failed_goal_workspace_task_records_runtime_notice() -> Result<()> {
    let root = tempfile::tempdir()?;
    let engine = test_engine(root.path()).await?;
    let mut request = workspace_request(WorkspaceTaskType::Goal);
    request.assignee = Some(WorkspaceActor::Agent(AGENT_ID_SWAROG.to_string()));
    let task = engine
        .create_workspace_task(request, WorkspaceActor::User)
        .await?;
    let task = engine.run_workspace_task(&task.id).await?;
    let goal_run_id = task.goal_run_id.clone().expect("goal run id");
    let mut goal_run = engine
        .history
        .get_goal_run(&goal_run_id)
        .await?
        .expect("goal run exists");
    goal_run.status = GoalRunStatus::Failed;
    goal_run.last_error = Some("goal runtime failed".to_string());
    goal_run.updated_at = now_millis();
    engine.history.upsert_goal_run(&goal_run).await?;

    let synced = engine
        .get_workspace_task(&task.id)
        .await?
        .expect("workspace task exists");

    assert_eq!(synced.status, WorkspaceTaskStatus::InProgress);
    let notices = engine
        .list_workspace_notices("main", Some(&task.id))
        .await?;
    assert!(notices.iter().any(|notice| {
        notice.notice_type == "runtime_failed" && notice.message == "goal runtime failed"
    }));
    Ok(())
}
