use super::*;

#[derive(Debug, Clone)]
pub(crate) struct ResolvedGoalLocalAgent {
    pub role_id: String,
    pub provider: String,
    pub model: String,
}

#[derive(Debug, Clone)]
pub(crate) enum GoalResolvedAgentTarget {
    BuiltinMain,
    GoalLocal(ResolvedGoalLocalAgent),
    GlobalSubagent(SubAgentDefinition),
}

impl GoalResolvedAgentTarget {
    #[cfg(test)]
    pub(crate) fn role_id(&self) -> &str {
        match self {
            Self::BuiltinMain => crate::agent::agent_identity::MAIN_AGENT_ID,
            Self::GoalLocal(agent) => agent.role_id.as_str(),
            Self::GlobalSubagent(definition) => definition.id.as_str(),
        }
    }

    #[cfg(test)]
    pub(crate) fn is_goal_local(&self) -> bool {
        matches!(self, Self::GoalLocal(_))
    }

    #[cfg(test)]
    pub(crate) fn is_global_subagent(&self) -> bool {
        matches!(self, Self::GlobalSubagent(_))
    }
}

fn apply_goal_resolved_target_to_task_state(
    task: &mut AgentTask,
    target: Option<&GoalResolvedAgentTarget>,
) -> Option<String> {
    match target {
        Some(GoalResolvedAgentTarget::GoalLocal(agent)) => {
            task.override_provider = Some(agent.provider.clone());
            task.override_model = Some(agent.model.clone());
            task.sub_agent_def_id = None;
        }
        Some(GoalResolvedAgentTarget::GlobalSubagent(definition)) => {
            task.override_provider = Some(definition.provider.clone());
            task.override_model = Some(definition.model.clone());
            task.override_api_transport = definition.api_transport;
            task.override_system_prompt = definition.system_prompt.clone();
            task.tool_whitelist = definition.tool_whitelist.clone();
            task.tool_blacklist = definition.tool_blacklist.clone();
            task.context_budget_tokens = definition.context_budget_tokens;
            task.max_duration_secs = definition.max_duration_secs;
            task.supervisor_config = definition.supervisor_config.clone();
            task.sub_agent_def_id = Some(definition.id.clone());
            crate::agent::task_crud::enforce_goal_task_autonomy_tool_blacklist(task);
            if definition.id == crate::agent::agent_identity::WELES_BUILTIN_SUBAGENT_ID {
                return Some(task.id.clone());
            }
        }
        Some(GoalResolvedAgentTarget::BuiltinMain) | None => {}
    }
    None
}

fn is_main_identifier(identifier: &str) -> bool {
    let normalized = identifier.trim().to_ascii_lowercase();
    matches!(
        normalized.as_str(),
        crate::agent::agent_identity::MAIN_AGENT_ID
            | crate::agent::agent_identity::MAIN_AGENT_PUBLIC_ALIAS
            | crate::agent::agent_identity::MAIN_AGENT_ALIAS
            | crate::agent::agent_identity::MAIN_AGENT_LEGACY_ALIAS
            | crate::agent::agent_identity::MAIN_AGENT_FALLBACK_ALIAS
    )
}

fn normalize_role_token(raw: &str) -> String {
    raw.trim()
        .to_ascii_lowercase()
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn heuristic_role_aliases(normalized: &str) -> &'static [&'static str] {
    match normalized {
        "plan" | "planner" | "planning" | "strategist" | "strategy" => {
            &["planning", "planner", "plan", "strategy", "strategist"]
        }
        "research" | "researcher" | "investigator" => &["research", "researcher", "investigator"],
        "medical" | "medicalresearch" | "medical researcher" | "clinical" => &[
            "medical_research",
            "medical research",
            "medical-research",
            "clinical",
        ],
        "finance" | "financial" | "financialanalyst" | "financial analyst" => &[
            "financial_analysis",
            "financial analysis",
            "financial-analyst",
            "finance",
        ],
        "legal" | "legalresearch" | "legal researcher" | "law" => {
            &["legal_research", "legal research", "legal-research", "law"]
        }
        "art" | "artdirection" | "art direction" | "creative" => {
            &["art_direction", "art direction", "art-director", "creative"]
        }
        "scientific" | "scientificreview" | "scientific review" | "science" => &[
            "scientific_review",
            "scientific review",
            "scientific-reviewer",
            "science",
        ],
        "marketing" | "productmarketing" | "product marketing" | "positioning" => &[
            "product_marketing",
            "product marketing",
            "product-marketing-strategist",
            "positioning",
        ],
        "verify" | "verifier" | "review" | "reviewer" | "qa" => {
            &["verifier", "verify", "review", "reviewer", "qa"]
        }
        _ => &[],
    }
}

fn matches_global_subagent(definition: &SubAgentDefinition, identifier: &str) -> bool {
    definition.id.eq_ignore_ascii_case(identifier)
        || definition.name.eq_ignore_ascii_case(identifier)
        || definition
            .role
            .as_deref()
            .is_some_and(|role| role.eq_ignore_ascii_case(identifier))
        || definition
            .id
            .strip_suffix("_builtin")
            .is_some_and(|value| value.eq_ignore_ascii_case(identifier))
}

fn exact_goal_local_match(
    identifier: &str,
    assignments: &[GoalAgentAssignment],
) -> Option<GoalResolvedAgentTarget> {
    assignments
        .iter()
        .find(|assignment| {
            assignment.enabled && assignment.role_id.eq_ignore_ascii_case(identifier)
        })
        .map(|assignment| {
            GoalResolvedAgentTarget::GoalLocal(ResolvedGoalLocalAgent {
                role_id: assignment.role_id.clone(),
                provider: assignment.provider.clone(),
                model: assignment.model.clone(),
            })
        })
}

fn heuristic_goal_local_match(
    identifier: &str,
    assignments: &[GoalAgentAssignment],
) -> Option<GoalResolvedAgentTarget> {
    let normalized = normalize_role_token(identifier);
    let aliases = heuristic_role_aliases(&normalized);
    if aliases.is_empty() {
        return None;
    }
    assignments
        .iter()
        .find(|assignment| {
            assignment.enabled
                && aliases.iter().any(|alias| {
                    let candidate = normalize_role_token(&assignment.role_id);
                    candidate == *alias
                })
        })
        .map(|assignment| {
            GoalResolvedAgentTarget::GoalLocal(ResolvedGoalLocalAgent {
                role_id: assignment.role_id.clone(),
                provider: assignment.provider.clone(),
                model: assignment.model.clone(),
            })
        })
}

#[cfg(test)]
pub(crate) fn resolve_goal_binding_candidate(
    identifier: &str,
    _step_title: &str,
    _step_instructions: &str,
    assignments: &[GoalAgentAssignment],
    global_subagents: &[SubAgentDefinition],
) -> Option<GoalResolvedAgentTarget> {
    if is_main_identifier(identifier) {
        return Some(GoalResolvedAgentTarget::BuiltinMain);
    }
    exact_goal_local_match(identifier, assignments)
        .or_else(|| heuristic_goal_local_match(identifier, assignments))
        .or_else(|| {
            global_subagents
                .iter()
                .find(|definition| {
                    definition.enabled && matches_global_subagent(definition, identifier)
                })
                .cloned()
                .map(GoalResolvedAgentTarget::GlobalSubagent)
        })
}

pub(crate) fn goal_local_agent_prompt_block(assignments: &[GoalAgentAssignment]) -> String {
    assignments
        .iter()
        .filter(|assignment| assignment.enabled)
        .filter(|assignment| !is_main_identifier(&assignment.role_id))
        .map(|assignment| {
            let mut line = format!(
                "- {}: provider={}, model={}",
                assignment.role_id, assignment.provider, assignment.model
            );
            if let Some(reasoning_effort) = assignment
                .reasoning_effort
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
            {
                line.push_str(", reasoning=");
                line.push_str(reasoning_effort);
            }
            line
        })
        .collect::<Vec<_>>()
        .join("\n")
}

impl AgentEngine {
    pub(crate) async fn apply_goal_resolved_target_to_task(
        &self,
        task_id: &str,
        target: Option<&GoalResolvedAgentTarget>,
    ) -> Option<AgentTask> {
        let (updated, mark_trusted_weles) = {
            let mut tasks = self.tasks.lock().await;
            if let Some(task) = tasks.iter_mut().find(|task| task.id == task_id) {
                let mark_trusted_weles = apply_goal_resolved_target_to_task_state(task, target);
                (Some(task.clone()), mark_trusted_weles)
            } else {
                (None, None)
            }
        };
        let (updated, mark_trusted_weles) = if let Some(updated) = updated {
            self.persist_tasks().await;
            (updated, mark_trusted_weles)
        } else {
            let mut task = self
                .list_tasks_filtered(&crate::history::AgentTaskListQuery {
                    id: Some(task_id.to_string()),
                    status: None,
                    statuses: Vec::new(),
                    source: None,
                    thread_id: None,
                    thread_ids: Vec::new(),
                    goal_run_id: None,
                    parent_task_id: None,
                    awaiting_approval_id: None,
                    supervisor_config_present: false,
                    exclude_terminal_statuses: false,
                    order_by_recent_activity_desc: false,
                    limit: Some(1),
                    ids: Vec::new(),
                    parent_task_ids: Vec::new(),
                })
                .await
                .into_iter()
                .next()?;
            let mark_trusted_weles = apply_goal_resolved_target_to_task_state(&mut task, target);
            if let Err(error) = self.history.upsert_agent_task(&task).await {
                tracing::warn!(
                    task_id = %task.id,
                    %error,
                    "failed to persist goal-resolved task target"
                );
                return None;
            }
            (task, mark_trusted_weles)
        };
        if let Some(task_id) = mark_trusted_weles {
            self.trusted_weles_tasks.write().await.insert(task_id);
        }
        Some(updated)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_assignment(role_id: &str, provider: &str, model: &str) -> GoalAgentAssignment {
        GoalAgentAssignment {
            role_id: role_id.to_string(),
            enabled: true,
            provider: provider.to_string(),
            model: model.to_string(),
            reasoning_effort: Some("medium".to_string()),
            inherit_from_main: false,
        }
    }

    fn sample_subagent(id: &str, name: &str) -> SubAgentDefinition {
        SubAgentDefinition {
            generation_settings: None,
            base_url: None,
            id: id.to_string(),
            name: name.to_string(),
            provider: "openai".to_string(),
            model: "gpt-5.4-mini".to_string(),
            role: None,
            system_prompt: None,
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
            claude_permission_mode: None,
            openrouter_provider_order: Vec::new(),
            openrouter_provider_ignore: Vec::new(),
            openrouter_allow_fallbacks: None,
            huggingface_provider: None,
            created_at: 0,
        }
    }

    #[test]
    fn goal_local_resolver_prefers_exact_role_id_match() {
        let assignments = vec![sample_assignment("planning", "openai", "gpt-5.4-mini")];
        let resolved = resolve_goal_binding_candidate(
            "planning",
            "Plan the rollout",
            "Break work into steps",
            &assignments,
            &[],
        )
        .expect("resolver should match");
        assert_eq!(resolved.role_id(), "planning");
        assert!(resolved.is_goal_local());
    }

    #[test]
    fn goal_local_resolver_uses_heuristic_alias_match() {
        let assignments = vec![sample_assignment("research", "openai", "gpt-5.4-mini")];
        let resolved = resolve_goal_binding_candidate(
            "researcher",
            "Collect facts",
            "Search docs and summarize",
            &assignments,
            &[],
        )
        .expect("resolver should match");
        assert_eq!(resolved.role_id(), "research");
    }

    #[test]
    fn goal_local_resolver_matches_new_domain_role_aliases() {
        let assignments = vec![sample_assignment(
            "medical_research",
            "openai",
            "gpt-5.4-mini",
        )];
        let resolved = resolve_goal_binding_candidate(
            "medical researcher",
            "Review evidence",
            "Compare guidelines and summarize uncertainty",
            &assignments,
            &[],
        )
        .expect("resolver should match new alias");
        assert_eq!(resolved.role_id(), "medical_research");
    }

    #[test]
    fn goal_local_resolver_falls_back_to_global_subagent_when_local_missing() {
        let globals = vec![sample_subagent("android-verifier", "Android Verifier")];
        let resolved = resolve_goal_binding_candidate(
            "android-verifier",
            "Verify build",
            "Run checks",
            &[],
            &globals,
        )
        .expect("resolver should match");
        assert!(resolved.is_global_subagent());
    }

    #[test]
    fn goal_local_agent_prompt_block_omits_main_and_lists_enabled_local_roles() {
        let block = goal_local_agent_prompt_block(&[
            sample_assignment(
                crate::agent::agent_identity::MAIN_AGENT_ID,
                "openai",
                "gpt-5.4",
            ),
            sample_assignment("planning", "openai", "gpt-5.4-mini"),
            GoalAgentAssignment {
                enabled: false,
                ..sample_assignment("research", "openai", "gpt-5.4")
            },
        ]);

        assert!(block.contains("planning"));
        assert!(!block.contains(crate::agent::agent_identity::MAIN_AGENT_ID));
        assert!(!block.contains("research"));
    }

    #[tokio::test]
    async fn apply_goal_resolved_target_updates_persisted_task_after_live_queue_clear() {
        let root = tempfile::tempdir().expect("tempdir");
        let manager = crate::session_manager::SessionManager::new_test(root.path()).await;
        let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
        let task = engine
            .enqueue_task(
                "Persisted goal target".to_string(),
                "Apply goal-local routing to a persisted task row".to_string(),
                "normal",
                None,
                None,
                Vec::new(),
                None,
                "goal_run",
                Some("goal-persisted-target".to_string()),
                None,
                Some("thread-persisted-target".to_string()),
                Some("daemon".to_string()),
            )
            .await;
        engine.persist_tasks().await;
        engine.tasks.lock().await.clear();
        let target = GoalResolvedAgentTarget::GoalLocal(ResolvedGoalLocalAgent {
            role_id: "planning".to_string(),
            provider: "openai".to_string(),
            model: "gpt-5.4-mini".to_string(),
        });

        let updated = engine
            .apply_goal_resolved_target_to_task(&task.id, Some(&target))
            .await
            .expect("persisted task should update");

        assert_eq!(updated.override_provider.as_deref(), Some("openai"));
        assert_eq!(updated.override_model.as_deref(), Some("gpt-5.4-mini"));
        let persisted = engine
            .list_tasks_filtered(&crate::history::AgentTaskListQuery {
                id: Some(task.id.clone()),
                status: None,
                statuses: Vec::new(),
                source: None,
                thread_id: None,
                thread_ids: Vec::new(),
                goal_run_id: None,
                parent_task_id: None,
                awaiting_approval_id: None,
                supervisor_config_present: false,
                exclude_terminal_statuses: false,
                order_by_recent_activity_desc: false,
                limit: Some(1),
                ids: Vec::new(),
                parent_task_ids: Vec::new(),
            })
            .await
            .pop()
            .expect("persisted task should remain queryable");
        assert_eq!(persisted.override_provider.as_deref(), Some("openai"));
        assert_eq!(persisted.override_model.as_deref(), Some("gpt-5.4-mini"));
        assert!(persisted.sub_agent_def_id.is_none());
    }
}
