use super::*;
pub(crate) async fn execute_list_skills(
    args: &serde_json::Value,
    agent_data_dir: &std::path::Path,
    history: &HistoryStore,
) -> Result<String> {
    let skills_root = super::super::task_prompt::skills_dir(agent_data_dir);
    let query = args
        .get("query")
        .and_then(|value| value.as_str())
        .map(|value| value.trim().to_lowercase())
        .filter(|value| !value.is_empty());
    let limit = args
        .get("limit")
        .and_then(|value| value.as_u64())
        .unwrap_or(20)
        .clamp(1, 100) as usize;

    super::super::skill_recommendation::schedule_background_skill_catalog_sync(
        history.clone(),
        skills_root.to_path_buf(),
    );

    let mut entries = match query.as_deref() {
        Some(_) => history.list_skill_variants(query.as_deref(), limit).await?,
        None => history.list_discoverable_skill_variants(limit).await?,
    };
    if entries.is_empty() {
        entries = super::super::skill_recommendation::list_filesystem_skill_variants(
            &skills_root,
            query.as_deref(),
            limit,
        )?;
    }

    if entries.is_empty() {
        return Ok(format!(
            "No local skills matched under {}.",
            skills_root.display()
        ));
    }

    let mut body = format!("Local skills under {}:\n", skills_root.display());
    for entry in entries {
        let tags = if entry.context_tags.is_empty() {
            "none".to_string()
        } else {
            entry.context_tags.join(", ")
        };
        body.push_str(&format!(
            "- {} [{} | status={}] ({}) tags={} uses={} success={:.0}%\n",
            entry.skill_name,
            entry.variant_name,
            entry.status,
            entry.relative_path,
            tags,
            entry.use_count,
            entry.success_rate() * 100.0,
        ));
    }
    Ok(body)
}

pub(crate) async fn execute_discover_skills(
    args: &serde_json::Value,
    agent: &AgentEngine,
    current_session_id: Option<SessionId>,
) -> Result<String> {
    let query = args
        .get("query")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow::anyhow!("missing 'query' argument"))?;

    let limit = args
        .get("limit")
        .and_then(|value| value.as_u64())
        .unwrap_or(3)
        .clamp(1, 20) as usize;
    let cursor = args
        .get("cursor")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty());

    let session_id = args
        .get("session")
        .or_else(|| args.get("session_id"))
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| {
            SessionId::parse_str(value)
                .map_err(|error| anyhow::anyhow!("invalid session `{value}`: {error}"))
        })
        .transpose()?
        .or(current_session_id);

    let result = agent
        .discover_skill_recommendations_public(query, session_id, limit, cursor)
        .await?;
    serde_json::to_string(&result)
        .map_err(|error| anyhow::anyhow!("failed to serialize skill discovery result: {error}"))
}

pub(crate) async fn execute_list_guidelines(
    args: &serde_json::Value,
    agent_data_dir: &std::path::Path,
) -> Result<String> {
    let guidelines_root = super::super::task_prompt::guidelines_dir(agent_data_dir);
    let query = args
        .get("query")
        .and_then(|value| value.as_str())
        .map(|value| value.trim().to_lowercase())
        .filter(|value| !value.is_empty());
    let limit = args
        .get("limit")
        .and_then(|value| value.as_u64())
        .unwrap_or(20)
        .clamp(1, 100) as usize;

    let mut files = Vec::new();
    collect_skill_documents(&guidelines_root, &mut files)?;
    files.sort();
    let mut entries = files
        .into_iter()
        .filter_map(|path| {
            let relative = path
                .strip_prefix(&guidelines_root)
                .ok()?
                .to_string_lossy()
                .replace('\\', "/");
            let name = path.file_stem()?.to_str()?.to_string();
            Some((name, relative))
        })
        .filter(|(name, relative)| match query.as_ref() {
            Some(needle) => {
                name.to_ascii_lowercase().contains(needle)
                    || relative.to_ascii_lowercase().contains(needle)
            }
            None => true,
        })
        .collect::<Vec<_>>();
    entries.truncate(limit);

    if entries.is_empty() {
        return Ok(format!(
            "No local guidelines matched under {}.",
            guidelines_root.display()
        ));
    }

    let mut body = format!("Local guidelines under {}:\n", guidelines_root.display());
    for (name, relative) in entries {
        body.push_str(&format!("- {name} ({relative})\n"));
    }
    Ok(body)
}

pub(crate) async fn execute_discover_guidelines(
    args: &serde_json::Value,
    agent: &AgentEngine,
    current_session_id: Option<SessionId>,
) -> Result<String> {
    let query = args
        .get("query")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow::anyhow!("missing 'query' argument"))?;

    let limit = args
        .get("limit")
        .and_then(|value| value.as_u64())
        .unwrap_or(3)
        .clamp(1, 20) as usize;
    let cursor = args
        .get("cursor")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty());

    let session_id = args
        .get("session")
        .or_else(|| args.get("session_id"))
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| {
            SessionId::parse_str(value)
                .map_err(|error| anyhow::anyhow!("invalid session `{value}`: {error}"))
        })
        .transpose()?
        .or(current_session_id);

    let result = agent
        .discover_guideline_recommendations_public(query, session_id, limit, cursor)
        .await?;
    serde_json::to_string(&result)
        .map_err(|error| anyhow::anyhow!("failed to serialize guideline discovery result: {error}"))
}

pub(crate) async fn execute_read_guideline(
    args: &serde_json::Value,
    agent_data_dir: &std::path::Path,
) -> Result<String> {
    let guideline = args
        .get("guideline")
        .and_then(|value| value.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing 'guideline' argument"))?
        .trim();
    if guideline.is_empty() {
        return Err(anyhow::anyhow!("'guideline' must not be empty"));
    }

    let max_lines = args
        .get("max_lines")
        .and_then(|value| value.as_u64())
        .unwrap_or(200)
        .clamp(20, 1000) as usize;
    let guidelines_root = super::super::task_prompt::guidelines_dir(agent_data_dir);
    let guideline_path = resolve_guideline_document_path(&guidelines_root, guideline)?;
    let content = tokio::fs::read_to_string(&guideline_path).await?;
    let total_lines = content.lines().count();
    let lines = content.lines().take(max_lines).collect::<Vec<_>>();
    let relative = guideline_path
        .strip_prefix(&guidelines_root)
        .unwrap_or(guideline_path.as_path())
        .display()
        .to_string();

    let mut body = format!("Guideline {}:\n\n{}", relative, lines.join("\n"));
    if total_lines > max_lines {
        body.push_str(&format!(
            "\n\n... (truncated, showing {max_lines} of {total_lines} lines)"
        ));
    }
    Ok(body)
}

fn resolve_guideline_document_path(
    guidelines_root: &std::path::Path,
    guideline: &str,
) -> Result<std::path::PathBuf> {
    validate_read_path(guideline)?;
    let root_canonical =
        std::fs::canonicalize(guidelines_root).unwrap_or(guidelines_root.to_path_buf());
    let direct_candidate = std::path::Path::new(guideline);
    if direct_candidate.components().count() > 1 || direct_candidate.is_absolute() {
        let candidate = if direct_candidate.is_absolute() {
            direct_candidate.to_path_buf()
        } else {
            guidelines_root.join(direct_candidate)
        };
        let canonical = std::fs::canonicalize(&candidate)
            .with_context(|| format!("guideline '{}' was not found", guideline))?;
        if !canonical.starts_with(&root_canonical) {
            anyhow::bail!(
                "guideline path must stay inside {}",
                guidelines_root.display()
            );
        }
        return Ok(canonical);
    }

    let mut files = Vec::new();
    collect_skill_documents(guidelines_root, &mut files)?;
    let normalized = guideline.to_lowercase();

    files.sort();
    for path in &files {
        let relative = path
            .strip_prefix(&root_canonical)
            .or_else(|_| path.strip_prefix(guidelines_root))
            .unwrap_or(path.as_path())
            .to_string_lossy()
            .replace('\\', "/");
        let stem = path
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or("")
            .to_lowercase();
        if stem == normalized || relative.to_lowercase() == normalized {
            return Ok(path.clone());
        }
    }

    for path in &files {
        let relative = path
            .strip_prefix(&root_canonical)
            .or_else(|_| path.strip_prefix(guidelines_root))
            .unwrap_or(path.as_path())
            .to_string_lossy()
            .replace('\\', "/");
        let stem = path
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or("")
            .to_lowercase();
        if stem.contains(&normalized) || relative.to_lowercase().contains(&normalized) {
            return Ok(path.clone());
        }
    }

    anyhow::bail!(
        "guideline '{}' was not found under {}",
        guideline,
        guidelines_root.display()
    )
}

fn parse_clamped_non_negative_usize_arg(
    args: &serde_json::Value,
    key: &str,
    default: usize,
    max: usize,
) -> Result<usize> {
    match args.get(key) {
        None => Ok(default),
        Some(value) => {
            if let Some(raw) = value.as_u64() {
                return Ok((raw as usize).clamp(0, max));
            }
            if let Some(raw) = value.as_i64() {
                if raw < 0 {
                    anyhow::bail!("'{key}' must be a non-negative integer");
                }
                return Ok((raw as usize).clamp(0, max));
            }
            anyhow::bail!("'{key}' must be a non-negative integer");
        }
    }
}

pub(crate) async fn execute_list_tools(
    args: &serde_json::Value,
    agent: &AgentEngine,
    session_manager: &Arc<SessionManager>,
    agent_data_dir: &std::path::Path,
    thread_id: &str,
    task_id: Option<&str>,
) -> Result<String> {
    let limit = parse_clamped_non_negative_usize_arg(args, "limit", 20, 200)?;
    let offset = parse_clamped_non_negative_usize_arg(args, "offset", 0, usize::MAX)?;
    let has_workspace_topology = session_manager.read_workspace_topology().is_some();
    let config = agent.config.read().await;
    let client_surface = resolve_shell_tool_client_surface(agent, thread_id, task_id).await;
    let result = list_available_tools_public(
        &config,
        agent_data_dir,
        has_workspace_topology,
        client_surface,
        limit,
        offset,
    );
    serde_json::to_string(&result)
        .map_err(|error| anyhow::anyhow!("failed to serialize tool list result: {error}"))
}

pub(crate) async fn execute_tool_search(
    args: &serde_json::Value,
    agent: &AgentEngine,
    session_manager: &Arc<SessionManager>,
    agent_data_dir: &std::path::Path,
    thread_id: &str,
    task_id: Option<&str>,
) -> Result<String> {
    let query = args
        .get("query")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow::anyhow!("missing 'query' argument"))?;
    let limit = parse_clamped_non_negative_usize_arg(args, "limit", 10, 200)?;
    let offset = parse_clamped_non_negative_usize_arg(args, "offset", 0, usize::MAX)?;
    let has_workspace_topology = session_manager.read_workspace_topology().is_some();
    let config = agent.config.read().await;
    let client_surface = resolve_shell_tool_client_surface(agent, thread_id, task_id).await;
    let result = search_available_tools_public(
        &config,
        agent_data_dir,
        has_workspace_topology,
        client_surface,
        query,
        limit,
        offset,
    );
    serde_json::to_string(&result)
        .map_err(|error| anyhow::anyhow!("failed to serialize tool search result: {error}"))
}

pub(crate) async fn execute_read_skill(
    args: &serde_json::Value,
    agent: &AgentEngine,
    agent_data_dir: &std::path::Path,
    history: &HistoryStore,
    session_manager: &Arc<SessionManager>,
    session_id: Option<SessionId>,
    thread_id: &str,
    task_id: Option<&str>,
) -> Result<String> {
    let skills = parse_read_skill_targets(args)?;
    let max_lines = args
        .get("max_lines")
        .and_then(|value| value.as_u64())
        .unwrap_or(200)
        .clamp(20, 1000) as usize;
    let skills_root = super::super::task_prompt::skills_dir(agent_data_dir);
    super::super::skill_recommendation::schedule_background_skill_catalog_sync(
        history.clone(),
        skills_root.to_path_buf(),
    );
    let context_tags =
        resolve_skill_context_tags(agent.workspace_root.as_ref(), session_manager, session_id)
            .await;

    let mut bodies = Vec::with_capacity(skills.len());
    for skill in &skills {
        bodies.push(
            read_single_skill(
                skill,
                max_lines,
                agent,
                history,
                thread_id,
                task_id,
                &skills_root,
                &context_tags,
            )
            .await?,
        );
    }

    Ok(bodies.join("\n\n---\n\n"))
}

fn parse_read_skill_targets(args: &serde_json::Value) -> Result<Vec<String>> {
    let mut skills = Vec::new();

    if let Some(skill) = args.get("skill") {
        let skill = skill
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("'skill' must be a string"))?
            .trim();
        if skill.is_empty() {
            return Err(anyhow::anyhow!("'skill' must not be empty"));
        }
        skills.push(skill.to_string());
    }

    if let Some(values) = args.get("skills") {
        let values = values
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("'skills' must be an array of strings"))?;
        for (index, value) in values.iter().enumerate() {
            let skill = value
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("'skills[{index}]' must be a string"))?
                .trim();
            if skill.is_empty() {
                return Err(anyhow::anyhow!("'skills[{index}]' must not be empty"));
            }
            skills.push(skill.to_string());
        }
    }

    if skills.is_empty() {
        anyhow::bail!("missing 'skill' or 'skills' argument");
    }

    Ok(skills)
}

#[cfg(test)]
mod read_skill_target_tests {
    use super::*;

    #[test]
    fn read_skill_targets_accept_empty_skills_when_skill_is_present() {
        let targets = parse_read_skill_targets(&serde_json::json!({
            "skill": "executing-plans",
            "skills": []
        }))
        .expect("empty optional skills array should not reject singular skill");

        assert_eq!(targets, vec!["executing-plans"]);
    }

    #[test]
    fn read_skill_targets_reject_empty_skills_without_skill() {
        let err = parse_read_skill_targets(&serde_json::json!({ "skills": [] }))
            .expect_err("empty skills array without skill should still be invalid");

        assert!(
            err.to_string().contains("missing"),
            "unexpected error: {err:#}"
        );
    }
}

async fn read_single_skill(
    skill: &str,
    max_lines: usize,
    agent: &AgentEngine,
    history: &HistoryStore,
    thread_id: &str,
    task_id: Option<&str>,
    skills_root: &std::path::Path,
    context_tags: &[String],
) -> Result<String> {
    let variant = history.resolve_skill_variant(skill, &context_tags).await?;
    let candidate_variants = match variant.as_ref() {
        Some(selected) => history
            .list_skill_variants(Some(&selected.skill_name), 8)
            .await
            .unwrap_or_default(),
        None => Vec::new(),
    };
    let inspection = match variant.as_ref() {
        Some(selected) => history
            .inspect_skill_variants(&selected.skill_name, &context_tags)
            .await
            .unwrap_or_default()
            .into_iter()
            .find(|item| item.record.variant_id == selected.variant_id),
        None => None,
    };
    let skill_path = resolve_skill_path(&skills_root, skill, variant.as_ref())?;
    let content = tokio::fs::read_to_string(&skill_path).await?;
    if let Some(variant) = variant.as_ref() {
        let (goal_run_id, _, _) = agent.goal_context_for_task(task_id).await;
        agent
            .persist_skill_selection_causal_trace(
                thread_id,
                goal_run_id.as_deref(),
                task_id,
                variant,
                &candidate_variants,
                &context_tags,
            )
            .await;
        agent
            .record_skill_consultation(thread_id, task_id, variant, &context_tags)
            .await;
    }
    let total_lines = content.lines().count();
    let lines = content.lines().take(max_lines).collect::<Vec<_>>();
    let relative = skill_path
        .strip_prefix(&skills_root)
        .unwrap_or(skill_path.as_path())
        .display()
        .to_string();

    let mut body = if let Some(ref variant) = variant {
        let tags = if variant.context_tags.is_empty() {
            "none".to_string()
        } else {
            variant.context_tags.join(", ")
        };
        format!(
            "Skill {} [{} | {} | uses={} | success={:.0}% | tags={}]:\n\n{}",
            relative,
            variant.skill_name,
            variant.variant_name,
            variant.use_count.saturating_add(1),
            variant.success_rate() * 100.0,
            tags,
            lines.join("\n")
        )
    } else {
        format!("Skill {}:\n\n{}", relative, lines.join("\n"))
    };
    if total_lines > max_lines {
        body.push_str(&format!(
            "\n\n... (truncated, showing {max_lines} of {total_lines} lines)"
        ));
    }
    if let Some(inspection) = inspection.as_ref() {
        let recent_history = inspection
            .fitness_history
            .iter()
            .rev()
            .take(3)
            .map(|row| format!("{}:{:.2}", row.outcome, row.fitness_score))
            .collect::<Vec<_>>();
        let history_summary = if recent_history.is_empty() {
            "none".to_string()
        } else {
            recent_history.join(", ")
        };
        let fitness_block = format!(
            "\n\nFitness snapshot:\n- fitness={:.2} success_rate={:.0}% uses={} recorded_at={}\n- lifecycle={}\n- selection={}\n- Recent fitness history: {}",
            inspection.fitness_snapshot.fitness_score,
            inspection.fitness_snapshot.success_rate * 100.0,
            inspection.fitness_snapshot.use_count,
            inspection.fitness_snapshot.recorded_at,
            inspection.lifecycle_summary,
            inspection.selection_summary,
            history_summary,
        );
        body.push_str(&fitness_block);
    }
    if let Some(variant) = variant.as_ref() {
        let first_state = agent
            .record_thread_skill_read_compliance(thread_id, &variant.variant_id)
            .await;
        let allow_legacy_name_fallback = first_state.as_ref().is_some_and(|state| {
            !state.compliant
                && state
                    .recommended_skill
                    .as_deref()
                    .is_some_and(|recommended| {
                        recommended.eq_ignore_ascii_case(&variant.skill_name)
                    })
        });
        if allow_legacy_name_fallback {
            let _ = agent
                .record_thread_skill_read_compliance(thread_id, &variant.skill_name)
                .await;
        }
    } else {
        let _ = agent
            .record_thread_skill_read_compliance(thread_id, skill)
            .await;
    }
    Ok(body)
}

pub(crate) async fn execute_justify_skill_skip(
    args: &serde_json::Value,
    agent: &AgentEngine,
    thread_id: &str,
) -> Result<String> {
    let rationale = args
        .get("rationale")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow::anyhow!("missing 'rationale' argument"))?;
    let state = agent
        .record_thread_skill_skip_rationale(thread_id, rationale)
        .await?;
    Ok(format!(
        "Recorded skill skip rationale. Confidence={} compliant={} next_action={}.",
        state.confidence_tier, state.compliant, state.recommended_action
    ))
}

pub(crate) async fn execute_update_todo(
    args: &serde_json::Value,
    agent: &AgentEngine,
    thread_id: &str,
    task_id: Option<&str>,
) -> Result<String> {
    let raw_items = args
        .get("todos")
        .or_else(|| args.get("items"))
        .and_then(|value| value.as_array())
        .ok_or_else(|| anyhow::anyhow!("missing 'todos' argument"))?;

    let now = super::now_millis();
    let mut items = Vec::new();
    for (index, raw_item) in raw_items.iter().enumerate() {
        let content = raw_item
            .get("content")
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| anyhow::anyhow!("todo item {index} is missing non-empty 'content'"))?;
        let status = match raw_item
            .get("status")
            .and_then(|value| value.as_str())
            .unwrap_or("pending")
        {
            "pending" => TodoStatus::Pending,
            "in_progress" => TodoStatus::InProgress,
            "completed" => TodoStatus::Completed,
            "blocked" => TodoStatus::Blocked,
            other => {
                return Err(anyhow::anyhow!(
                    "todo item {index} has invalid status '{other}'"
                ));
            }
        };

        items.push(TodoItem {
            id: format!("todo_{}", uuid::Uuid::new_v4()),
            content: content.to_string(),
            status,
            position: index,
            step_index: raw_item
                .get("step_index")
                .and_then(|value| value.as_u64())
                .map(|value| value as usize),
            created_at: now,
            updated_at: now,
        });
    }

    agent
        .replace_thread_todos(thread_id, items.clone(), task_id)
        .await;

    Ok(format!("Updated todo list with {} item(s).", items.len()))
}

pub(crate) async fn execute_web_search(
    args: &serde_json::Value,
    http_client: &reqwest::Client,
    search_provider: &str,
    exa_api_key: &str,
    tavily_api_key: &str,
    firecrawl_api_key: &str,
    duckduckgo_region: &str,
    duckduckgo_safe_search: &str,
) -> Result<String> {
    execute_web_search_with_runner(
        args,
        search_provider,
        exa_api_key,
        tavily_api_key,
        firecrawl_api_key,
        |request, provider| async move {
            match provider {
                "firecrawl" => {
                    execute_firecrawl_search(
                        http_client,
                        &request.query,
                        request.max_results,
                        firecrawl_api_key,
                    )
                    .await
                }
                "exa" => {
                    execute_exa_search(
                        http_client,
                        &request.query,
                        request.max_results,
                        exa_api_key,
                    )
                    .await
                }
                "tavily" => {
                    execute_tavily_search(
                        http_client,
                        &request.query,
                        request.max_results,
                        tavily_api_key,
                    )
                    .await
                }
                "duckduckgo" => {
                    execute_ddg_search(
                        http_client,
                        &request.query,
                        request.max_results,
                        duckduckgo_region,
                        duckduckgo_safe_search,
                    )
                    .await
                }
                _ => {
                    execute_ddg_search(
                        http_client,
                        &request.query,
                        request.max_results,
                        duckduckgo_region,
                        duckduckgo_safe_search,
                    )
                    .await
                }
            }
        },
    )
    .await
}

pub(crate) async fn execute_web_search_with_runner<F, Fut>(
    args: &serde_json::Value,
    search_provider: &str,
    exa_api_key: &str,
    tavily_api_key: &str,
    firecrawl_api_key: &str,
    runner: F,
) -> Result<String>
where
    F: FnOnce(WebSearchRequest, &'static str) -> Fut,
    Fut: Future<Output = Result<String>>,
{
    let request = web_search_request(args)?;
    let timeout_seconds = request.timeout_seconds;
    let provider = match search_provider {
        "exa" if !exa_api_key.is_empty() => "exa",
        "tavily" if !tavily_api_key.is_empty() => "tavily",
        "firecrawl" if !firecrawl_api_key.is_empty() => "firecrawl",
        "duckduckgo" | "ddg" => "duckduckgo",
        _ => "ddg",
    };

    tokio::time::timeout(
        std::time::Duration::from_secs(timeout_seconds),
        runner(request, provider),
    )
    .await
    .map_err(|_| anyhow::anyhow!("web search timed out after {timeout_seconds} seconds"))?
}

pub(crate) fn safe_snippet_preview(text: &str, max_chars: usize) -> String {
    truncate_on_char_boundary(text, max_chars, "...")
}

fn safe_text_excerpt(text: &str, max_chars: usize) -> String {
    truncate_on_char_boundary(text, max_chars, "")
}

fn truncate_on_char_boundary(text: &str, max_chars: usize, suffix: &str) -> String {
    if let Some((idx, _)) = text.char_indices().nth(max_chars) {
        let mut truncated = text[..idx].to_string();
        truncated.push_str(suffix);
        truncated
    } else {
        text.to_string()
    }
}

async fn execute_exa_search(
    http_client: &reqwest::Client,
    query: &str,
    max_results: u64,
    api_key: &str,
) -> Result<String> {
    let body = serde_json::json!({
        "query": query,
        "numResults": max_results,
        "type": "auto",
        "contents": {
            "text": { "maxCharacters": 1000 },
            "highlights": { "numSentences": 2 },
        },
    });

    let resp = http_client
        .post("https://api.exa.ai/search")
        .header("x-api-key", api_key)
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        anyhow::bail!(
            "Exa API returned {status}: {}",
            safe_text_excerpt(&text, 200)
        );
    }

    let json: serde_json::Value = resp.json().await?;
    let results = json["results"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .map(|r| {
                    let title = r["title"].as_str().unwrap_or("(no title)");
                    let url = r["url"].as_str().unwrap_or("");
                    let text = r["text"].as_str().unwrap_or("");
                    let published_at = r["publishedDate"]
                        .as_str()
                        .or_else(|| r["published_date"].as_str())
                        .or_else(|| r["publishedAt"].as_str());
                    let snippet = safe_snippet_preview(text, 300);
                    format_result_with_metadata(title, url, &snippet, published_at)
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    if results.is_empty() {
        Ok(format!("No web results found for: {query}"))
    } else {
        Ok(format!(
            "Web results for \"{query}\":\n\n{}",
            results.join("\n\n")
        ))
    }
}

async fn execute_tavily_search(
    http_client: &reqwest::Client,
    query: &str,
    max_results: u64,
    api_key: &str,
) -> Result<String> {
    let body = serde_json::json!({
        "query": query,
        "max_results": max_results,
        "search_depth": "basic",
    });

    let resp = http_client
        .post("https://api.tavily.com/search")
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        anyhow::bail!(
            "Tavily API returned {status}: {}",
            safe_text_excerpt(&text, 200)
        );
    }

    let json: serde_json::Value = resp.json().await?;
    let results = json["results"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .map(|r| {
                    let title = r["title"].as_str().unwrap_or("(no title)");
                    let url = r["url"].as_str().unwrap_or("");
                    let content = r["content"].as_str().unwrap_or("");
                    let published_at = r["published_date"]
                        .as_str()
                        .or_else(|| r["publishedDate"].as_str())
                        .or_else(|| r["publishedAt"].as_str());
                    let snippet = safe_snippet_preview(content, 300);
                    format_result_with_metadata(title, url, &snippet, published_at)
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    if results.is_empty() {
        Ok(format!("No web results found for: {query}"))
    } else {
        Ok(format!(
            "Web results for \"{query}\":\n\n{}",
            results.join("\n\n")
        ))
    }
}

async fn execute_firecrawl_search(
    http_client: &reqwest::Client,
    query: &str,
    max_results: u64,
    api_key: &str,
) -> Result<String> {
    let body = serde_json::json!({
        "query": query,
        "limit": max_results.clamp(1, 100),
    });

    let resp = http_client
        .post("https://api.firecrawl.dev/v2/search")
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        anyhow::bail!(
            "Firecrawl API returned {status}: {}",
            safe_text_excerpt(&text, 200)
        );
    }

    let json: serde_json::Value = resp.json().await?;
    Ok(format_firecrawl_search_results(query, &json))
}

pub(crate) fn format_firecrawl_search_results(query: &str, json: &serde_json::Value) -> String {
    let results = json["data"]["web"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .map(|r| {
                    let title = r["title"].as_str().unwrap_or("(no title)");
                    let url = r["url"].as_str().unwrap_or("");
                    let description = r["description"].as_str().unwrap_or("");
                    let snippet = safe_snippet_preview(description, 300);
                    format_result_with_metadata(title, url, &snippet, None)
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    if results.is_empty() {
        format!("No web results found for: {query}")
    } else {
        format!("Web results for \"{query}\":\n\n{}", results.join("\n\n"))
    }
}

async fn execute_ddg_search(
    http_client: &reqwest::Client,
    query: &str,
    max_results: u64,
    region: &str,
    safe_search: &str,
) -> Result<String> {
    let url = build_duckduckgo_search_url(query, region, safe_search);

    let resp = http_client
        .get(&url)
        .header("User-Agent", "zorai-agent/0.1")
        .send()
        .await?;

    let text = resp.text().await?;

    let mut results = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("<a rel=\"nofollow\"") {
            if let (Some(href_start), Some(href_end)) =
                (trimmed.find("href=\""), trimmed.find("\">"))
            {
                let url = &trimmed[href_start + 6..href_end];
                let text_start = href_end + 2;
                if let Some(text_end) = trimmed[text_start..].find("</a>") {
                    let title = &trimmed[text_start..text_start + text_end];
                    results.push(format_result_with_metadata(
                        title,
                        url,
                        "No snippet available.",
                        None,
                    ));
                }
            }
        }
        if results.len() >= max_results as usize {
            break;
        }
    }

    if results.is_empty() {
        Ok(format!("No web results found for: {query}"))
    } else {
        Ok(format!(
            "Web results for \"{query}\":\n\n{}",
            results.join("\n\n")
        ))
    }
}

pub(crate) fn build_duckduckgo_search_url(query: &str, region: &str, safe_search: &str) -> String {
    let region = normalize_duckduckgo_region(region);
    let safe_search = duckduckgo_safe_search_param(safe_search);
    format!(
        "https://lite.duckduckgo.com/lite/?q={}&kl={}&kp={}",
        super::gateway_workspace::urlencoding::encode(query),
        super::gateway_workspace::urlencoding::encode(&region),
        safe_search
    )
}

fn normalize_duckduckgo_region(region: &str) -> String {
    let trimmed = region.trim();
    if trimmed.is_empty() {
        "us-en".to_string()
    } else {
        trimmed.to_ascii_lowercase()
    }
}

fn duckduckgo_safe_search_param(safe_search: &str) -> &'static str {
    match safe_search.trim().to_ascii_lowercase().as_str() {
        "strict" | "on" => "1",
        "off" => "-2",
        _ => "-1",
    }
}
