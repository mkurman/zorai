//! Helpers still needed by goal start/routing after the planner was removed.

use super::*;

fn goal_runtime_owner_profile(
    agent_label: String,
    provider: String,
    model: String,
    reasoning_effort: Option<String>,
) -> GoalRuntimeOwnerProfile {
    GoalRuntimeOwnerProfile {
        agent_label,
        provider,
        model,
        reasoning_effort,
    }
}

fn goal_agent_assignment(
    role_id: String,
    enabled: bool,
    provider: String,
    model: String,
    reasoning_effort: Option<String>,
    inherit_from_main: bool,
) -> GoalAgentAssignment {
    GoalAgentAssignment {
        role_id,
        enabled,
        provider,
        model,
        reasoning_effort,
        inherit_from_main,
    }
}

fn sub_agent_matches_identifier(def: &SubAgentDefinition, identifier: &str) -> bool {
    def.id.eq_ignore_ascii_case(identifier)
        || def.name.eq_ignore_ascii_case(identifier)
        || def
            .id
            .strip_suffix("_builtin")
            .is_some_and(|value| value.eq_ignore_ascii_case(identifier))
}

impl AgentEngine {
    pub(super) async fn normalized_goal_launch_assignment_snapshot(
        &self,
        assignments: Vec<GoalAgentAssignment>,
    ) -> Vec<GoalAgentAssignment> {
        assignments
    }

    pub(crate) async fn goal_launch_assignment_snapshot(&self) -> Vec<GoalAgentAssignment> {
        let config = self.config.read().await;
        let resolved = resolve_active_provider_config(&config).unwrap_or_else(|_| ProviderConfig {
            presence_penalty: None,
            base_url: config.base_url.clone(),
            model: config.model.clone(),
            api_key: config.api_key.clone(),
            assistant_id: config.assistant_id.clone(),
            auth_source: config.auth_source,
            api_transport: config.api_transport,
            reasoning_effort: config.reasoning_effort.clone(),
            context_window_tokens: config.context_window_tokens,
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
        });
        vec![goal_agent_assignment(
            crate::agent::agent_identity::MAIN_AGENT_ID.to_string(),
            true,
            config.provider.clone(),
            resolved.model,
            Some(resolved.reasoning_effort),
            false,
        )]
    }

    pub(crate) async fn current_step_owner_profile_for_task(
        &self,
        task: &AgentTask,
    ) -> GoalRuntimeOwnerProfile {
        if let Some(identifier) = task.sub_agent_def_id.as_deref() {
            let sub_agents = self.list_sub_agents().await;
            if let Some(def) = sub_agents
                .iter()
                .find(|definition| sub_agent_matches_identifier(definition, identifier))
            {
                return goal_runtime_owner_profile(
                    def.name.clone(),
                    def.provider.clone(),
                    def.model.clone(),
                    def.reasoning_effort.clone(),
                );
            }
        }

        let config = self.config.read().await;
        goal_runtime_owner_profile(
            crate::agent::agent_identity::MAIN_AGENT_NAME.to_string(),
            task.override_provider
                .clone()
                .unwrap_or_else(|| config.provider.clone()),
            task.override_model
                .clone()
                .unwrap_or_else(|| config.model.clone()),
            Some(config.reasoning_effort.clone()),
        )
    }

    pub(in crate::agent) async fn sync_goal_run_with_task(
        &self,
        goal_run_id: &str,
        task: &AgentTask,
    ) {
        if task.source != "goal_run" {
            return;
        }
        let current_step_owner_profile = self.current_step_owner_profile_for_task(task).await;
        let mut maybe_updated = None;
        {
            let mut goal_runs = self.goal_runs.lock().await;
            if let Some(goal_run) = goal_runs.iter_mut().find(|item| item.id == goal_run_id) {
                if matches!(
                    goal_run.status,
                    GoalRunStatus::AwaitingReview | GoalRunStatus::Paused | GoalRunStatus::Blocked
                ) || goal_run.status.is_terminal()
                {
                    return;
                }
                let next_status = if task.status == TaskStatus::AwaitingApproval {
                    GoalRunStatus::AwaitingApproval
                } else {
                    GoalRunStatus::Running
                };
                let mut changed = goal_run.status != next_status;
                let prior_thread_routing = (
                    goal_run.thread_id.clone(),
                    goal_run.root_thread_id.clone(),
                    goal_run.active_thread_id.clone(),
                    goal_run.execution_thread_ids.clone(),
                );
                goal_run.status = next_status;
                goal_run.updated_at = now_millis();
                goal_run.awaiting_approval_id = task.awaiting_approval_id.clone();
                goal_run.active_task_id = Some(task.id.clone());
                super::goal_run_apply_thread_routing(goal_run, task.thread_id.clone());
                if prior_thread_routing
                    != (
                        goal_run.thread_id.clone(),
                        goal_run.root_thread_id.clone(),
                        goal_run.active_thread_id.clone(),
                        goal_run.execution_thread_ids.clone(),
                    )
                {
                    changed = true;
                }
                let next_owner = Some(current_step_owner_profile.clone());
                if goal_run.current_step_owner_profile != next_owner {
                    changed = true;
                }
                goal_run.current_step_owner_profile = next_owner;
                if changed {
                    maybe_updated = Some(goal_run.clone());
                }
            }
        }
        if let Some(updated) = maybe_updated {
            self.persist_goal_runs().await;
            self.emit_goal_run_update(&updated, Some(goal_run_status_message(&updated).into()));
        }
    }
}
