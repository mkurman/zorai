use super::*;

#[derive(Debug, Clone, serde::Serialize)]
struct PromptInspectionSection {
    id: String,
    title: String,
    content: String,
}

#[derive(Debug, Clone, serde::Serialize)]
struct PromptTimestampBlock {
    local_timestamp: String,
    utc_timestamp: String,
    unix_timestamp_ms: u64,
}

#[derive(Debug, Clone, serde::Serialize)]
struct TrackedThreadMemoryInjectionStateView {
    thread_id: String,
    #[serde(flatten)]
    state: crate::agent::memory_context::PromptMemoryInjectionState,
}

#[derive(Debug, Clone, serde::Serialize)]
struct PromptToolInspection {
    name: String,
    serialized: String,
    serialized_chars: usize,
}

#[derive(Debug, Clone, serde::Serialize)]
struct PromptInspectionPayload {
    agent_id: String,
    agent_name: String,
    provider_id: String,
    model: String,
    timestamp_block: PromptTimestampBlock,
    tracked_thread_memory_injection_state_count: usize,
    tracked_thread_memory_injection_states: Vec<TrackedThreadMemoryInjectionStateView>,
    sections: Vec<PromptInspectionSection>,
    final_prompt: String,
    tool_count: usize,
    tools_serialized_chars: usize,
    tools: Vec<PromptToolInspection>,
}

const MAX_INSPECTED_INJECTION_STATES: usize = 16;

#[derive(Debug, Clone)]
struct PromptInspectionTarget {
    agent_id: String,
    agent_name: String,
    memory_scope_id: String,
    base_prompt: String,
    provider_id: String,
    model: String,
}

fn push_section(
    sections: &mut Vec<PromptInspectionSection>,
    id: &str,
    title: &str,
    content: impl Into<String>,
) {
    let content = content.into();
    if content.trim().is_empty() {
        return;
    }
    sections.push(PromptInspectionSection {
        id: id.to_string(),
        title: title.to_string(),
        content,
    });
}

fn build_timestamp_block() -> PromptTimestampBlock {
    let local_now = chrono::Local::now();
    let utc_now = chrono::Utc::now();
    PromptTimestampBlock {
        local_timestamp: local_now.format("%A, %Y-%m-%d %H:%M:%S %Z").to_string(),
        utc_timestamp: utc_now.format("%A, %Y-%m-%d %H:%M:%S UTC").to_string(),
        unix_timestamp_ms: utc_now.timestamp_millis().max(0) as u64,
    }
}

fn render_memory_paths_section(memory_paths: &super::task_prompt::MemoryPaths) -> String {
    format!(
        "- MEMORY.md: {}\n- SOUL.md: {}\n- USER.md: {}\n- Use these exact paths when reading or explaining where zorai agent memory lives on this platform.",
        memory_paths.memory_path.display(),
        memory_paths.soul_path.display(),
        memory_paths.user_path.display(),
    )
}

fn render_shared_user_profile_policy() -> String {
    format!(
        "- USER.md is shared across agents and read-only for you.\n- If a user preference or operator profile fact should change, ask {} via `message_agent` and let {} decide whether to apply it.",
        MAIN_AGENT_NAME, MAIN_AGENT_NAME,
    )
}

fn render_local_skills_section(
    skills_root: &std::path::Path,
    generated_skills_root: &std::path::Path,
) -> String {
    format!(
        "- Skills root: {}\n- Generated skills: {}\n- Curated local skills live directly under {} (zorai reference docs for terminals, browser, tasks, goals, memory, safety, etc.).\n- Call `discover_skills` then `read_skill` only when a local playbook would help. Use `read_memory`, `read_user`, and `read_soul` when you need memory recall.\n- `discover_guidelines`, `discover_skills`, `read_guideline`, and `read_skill` are optional. Call them only when a local workflow would change how you do this turn; skip them for simple Q&A, status, and work you already know how to do.\n- When you do call `discover_guidelines` or `discover_skills`, send a brief intent sentence (e.g., 'modify python wheel builder for alternate compiler flag'), not the full task transcript and not a 2-3 word fragment.\n- Read a guideline or skill only for a match you will actually follow.\n- When you need clarification or the operator must choose among options, call `ask_questions`. Do not ask clarifying questions in plain text when this tool fits.\n- For `ask_questions`, keep buttons compact with ordered tokens like `A`, `B`, `C`, `D` or `1`, `2`, `3`, and place the full answer text in `content`.\n- `list_skills` is the raw catalog if you already know a name.\n- The `cheatsheet` skill provides a quick reference for all available MCP tools.\n- Prefer reusing an existing skill over inventing a brand-new workflow.",
        skills_root.display(),
        generated_skills_root.display(),
        skills_root.display(),
    )
}

fn render_local_guidelines_section(guidelines_root: &std::path::Path) -> String {
    format!(
        "- Guidelines root: {}\n- Guidelines are optional documentation-only workflow orchestrators above skills.\n- Call `discover_guidelines` then `read_guideline` only when a guideline would change this turn; do not start every task with them.",
        guidelines_root.display(),
    )
}

fn render_plugin_skills_section(skills_root: &std::path::Path) -> Option<String> {
    let plugin_skills_dir = skills_root.join("plugins");
    if !plugin_skills_dir.exists() || !plugin_skills_dir.is_dir() {
        return None;
    }
    let plugin_count = std::fs::read_dir(&plugin_skills_dir)
        .map(|entries| {
            entries
                .filter_map(|entry| entry.ok())
                .filter(|entry| entry.path().is_dir())
                .count()
        })
        .unwrap_or(0);
    if plugin_count == 0 {
        return None;
    }

    Some(format!(
        "- Plugin skills: {}/plugins/ ({} plugin(s) with bundled skills)\n- Plugin skills may reference API endpoints using `plugin:<plugin-name>:<endpoint>` notation\n- When a skill references `plugin:<name>:<endpoint>`, use the plugin API tool to call that endpoint\n- Plugin commands are available as slash commands (e.g., /pluginname.command)",
        skills_root.display(),
        plugin_count,
    ))
}

fn render_recall_and_memory_section(config: &AgentConfig) -> String {
    let mut section = String::from(
        "- Prefer `read_memory`, `read_user`, and `read_soul` for memory recall before raw file reads.\n- Treat zorai memory as layered: SOUL.md is identity, MEMORY.md stores durable facts and reusable strategy hints, USER.md stores operator profile, and structured daemon state plus recall systems carry the rest.\n- Use `session_search` or `onecontext_search` when the user asks about prior decisions, existing implementations, or historical debugging context.\n- Use `semantic_query` when you need local package/crate summaries, compose service topology, code import relationships, or learned workspace conventions before editing.\n- Before big or multi-step work, write or update a short working spec in the current thread artifact specs directory under zorai data root (`threads/<thread_id>/artifacts/specs/`) so it survives handoffs and restarts.\n- Before proceeding after a pause, handoff, or context shift, look up and reread the relevant spec from that current thread artifact specs directory so execution stays anchored to the latest written plan.\n- For any non-trivial or multi-step task, call `update_todo` early to enter plan mode, then keep that todo list current as work progresses.\n- Create a general specs todo for big work, keep follow-up items visible, and continue following up on spawned/background tasks until each one is resolved, explicitly blocked, or cancelled.\n- When you learn durable operator preferences, stable project facts, or reusable strategy hints, call `update_memory` with a concise update so future sessions start with that context.\n- Do not write task progress, temporary TODOs, approval status, or transient risk labels into durable memory.\n- Memory is provenance-backed and operator-auditable. Correct or retract stale durable facts explicitly instead of silently layering contradictions on top.\n",
    );
    section.push_str(&memory::memory_limits_prompt_line());
    section.push_str(&format!(
        "- {} is your concierge peer in zorai. Use `message_agent` only for private cross-agent coordination or quick checks. It delivers asynchronously and does not wait for the reply; this thread resumes when the recipient answers. It does not switch the active responder for the operator thread and future operator turns do not route to the message target. On visible threads, targeting an active thread participant defaults to visible-thread continuation so participant work stays in the operator thread.\n",
        CONCIERGE_AGENT_NAME
    ));
    section.push_str(
        "- If the operator wants to talk directly to another agent, or if another agent should own future replies in this thread, use `handoff_thread_agent` instead. A successful handoff changes the active responder and future operator turns route to that agent until a return handoff.\n",
    );
    section.push_str(
        "- Security in zorai is governance over transitions, not a casual suggestion. If scope, targets, privilege posture, environment, or command family materially changed, treat prior approval as stale until a fresh approval path says otherwise.\n- If work is paused behind approval or another governance gate, treat it as blocked work that needs resolution, not as permission to keep pushing similar side effects through another path.\n",
    );
    if config.enable_honcho_memory && !config.honcho_api_key.trim().is_empty() {
        section.push_str(
            "- Use `agent_query_memory` when local session recall is insufficient and you need broader cross-session Honcho memory.\n",
        );
    }
    if config.tool_synthesis.enabled {
        section.push_str(
            "- If the work depends on a missing but conservative CLI/API capability, use `synthesize_tool` to register a guarded generated tool, activate it with `activate_generated_tool`, and promote it later if it proves useful.\n",
        );
    }
    section.push_str("\nMemory curation guidance:\n");
    section.push_str(memory_curation_guidance());
    section
}

fn render_terminal_session_discipline_section() -> &'static str {
    "- Before running actions that truly need an existing terminal, call `list_terminals` to discover current live session IDs and CWD.\n- Do not force a `session` argument in normal TUI chat or goal-run turns just because a previous frontend session existed. Omit `session` unless you intentionally target a known live terminal or the operator explicitly asked you to reuse one.\n- When you do target a live terminal, reuse that `session` value across related tool calls so all actions stay in one terminal context.\n- For long-running terminal work, prefer non-blocking execution: set `wait_for_completion=false` or use `timeout_seconds > 600`, capture the returned `operation_id`, and continue other work. This thread auto-resumes when the command finishes. Do not poll `get_operation_status`. If you cannot continue without the result, call `get_operation_status` once with `wait=true`.\n- If a command is still running, timed out while still active, or is waiting for interactive completion, treat that terminal as occupied and switch to another terminal/session before continuing other work.\n- If you need another terminal in the same agent workspace, call `allocate_terminal`, then continue with the returned session ID.\n- Agent-allocated terminals are reclaimed when the owning task or subagent finishes, and unused idle lanes are closed automatically. Do not assume an allocated session stays forever.\n- If the operator asks to use another terminal, call `list_terminals` again and switch explicitly."
}

fn render_large_file_writes_section() -> &'static str {
    "- Avoid giant JSON file payloads when content is large or heavily escaped.\n- Prefer multipart-style `create_file` inputs when available.\n- If you must write through a terminal, prefer a minimal Python writer over brittle shell heredocs.\n- Before executing generated Python, inspect it for unintended side effects. It should only perform the intended file operation and should not add unrelated process, network, or shell behavior."
}

fn render_prompt_timestamp_block_section(block: &PromptTimestampBlock) -> String {
    format!(
        "- Local timestamp: {}\n- UTC timestamp: {}\n- Unix timestamp (ms): {}",
        block.local_timestamp, block.utc_timestamp, block.unix_timestamp_ms
    )
}

fn render_prompt_memory_injection_state_section(
    total_count: usize,
    states: &[TrackedThreadMemoryInjectionStateView],
) -> String {
    if states.is_empty() {
        return "- No tracked thread memory injection states.\n- This inspection payload is thread-agnostic, so `final_prompt` excludes bootstrap-only structured memory.".to_string();
    }

    let mut lines = vec![
        format!(
            "- Tracked thread injection states: {} (showing {} most recent)",
            total_count,
            states.len()
        ),
        "- This inspection payload is thread-agnostic, so `final_prompt` excludes bootstrap-only structured memory.".to_string(),
    ];
    for entry in states {
        lines.push(format!(
            "- Thread `{}`: base_injected={}, base_hash={}, base_updated_at_ms={}, structured_summary_hash={}, injected_after_compaction={}",
            entry.thread_id,
            if entry.state.is_base_layer_injected() { "yes" } else { "no" },
            entry.state
                .base_markdown_hash
                .as_deref()
                .unwrap_or("unknown"),
            entry.state
                .base_markdown_updated_at_ms
                .map(|value| value.to_string())
                .unwrap_or_else(|| "unknown".to_string()),
            entry.state
                .structured_summary_hash
                .as_deref()
                .unwrap_or("unknown"),
            if entry.state.injected_after_compaction {
                "yes"
            } else {
                "no"
            }
        ));
    }
    lines.join("\n")
}

fn render_morphogenesis_affinity_updates_section(
    updates: &[crate::agent::morphogenesis::types::AffinityUpdate],
) -> String {
    if updates.is_empty() {
        return String::new();
    }

    let mut lines = vec![format!("- Logged affinity updates: {}", updates.len())];
    for update in updates {
        lines.push(format!(
            "- {}: {:.3} -> {:.3} via {}{} at {}",
            update.domain,
            update.old_affinity,
            update.new_affinity,
            update.trigger_type,
            update
                .task_id
                .as_deref()
                .map(|task_id| format!(" (task `{task_id}`)"))
                .unwrap_or_default(),
            update.updated_at_ms
        ));
    }
    lines.join("\n")
}

fn render_morphogenesis_soul_adaptations_section(
    adaptations: &[crate::agent::morphogenesis::types::SoulAdaptation],
) -> String {
    if adaptations.is_empty() {
        return String::new();
    }

    let mut lines = vec![format!("- Logged soul adaptations: {}", adaptations.len())];
    for adaptation in adaptations {
        lines.push(format!(
            "- {} {:?} at {}: {}",
            adaptation.domain,
            adaptation.adaptation_type,
            adaptation.created_at_ms,
            adaptation.soul_snippet.trim()
        ));
    }
    lines.join("\n")
}

fn render_active_skill_gate_section(
    state: &crate::agent::types::LatestSkillDiscoveryState,
) -> String {
    let mut lines = vec![format!("- Query: {}", state.query)];
    if let Some(skill) = state.recommended_skill.as_deref() {
        lines.push(format!("- Skill: {}", skill));
    }
    lines.push(format!("- Confidence: {}", state.confidence_tier));
    lines.push(format!("- Next action: {}", state.recommended_action));
    lines.push(format!(
        "- Approval required: {}",
        if state.mesh_requires_approval {
            "yes"
        } else {
            "no"
        }
    ));
    lines.push(format!(
        "- Skill already read: {}",
        if state.skill_read_completed {
            "yes"
        } else {
            "no"
        }
    ));
    if let Some(rationale) = state.skip_rationale.as_deref() {
        lines.push(format!("- Skip rationale: {}", rationale));
    }
    lines.join("\n")
}

fn exact_sub_agent_match<'a>(
    sub_agents: &'a [SubAgentDefinition],
    requested: &str,
) -> Option<&'a SubAgentDefinition> {
    sub_agents.iter().find(|entry| {
        entry.id.eq_ignore_ascii_case(requested) || entry.name.eq_ignore_ascii_case(requested)
    })
}

fn build_exact_subagent_target(
    engine: &AgentEngine,
    config: &AgentConfig,
    definition: &SubAgentDefinition,
) -> Result<PromptInspectionTarget> {
    let mut provider_config =
        engine.resolve_sub_agent_provider_config(config, &definition.provider)?;
    provider_config.model = definition.model.clone();
    let base_prompt = definition
        .system_prompt
        .clone()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| config.system_prompt.clone());
    Ok(PromptInspectionTarget {
        agent_id: definition.id.clone(),
        agent_name: definition.name.clone(),
        memory_scope_id: definition.id.clone(),
        base_prompt,
        provider_id: definition.provider.clone(),
        model: provider_config.model,
    })
}

fn build_direct_target(
    engine: &AgentEngine,
    config: &AgentConfig,
    requested_agent_id: &str,
    sub_agents: &[SubAgentDefinition],
) -> Result<PromptInspectionTarget> {
    if is_main_agent_scope(requested_agent_id) {
        let provider_config = engine.resolve_provider_config(config)?;
        return Ok(PromptInspectionTarget {
            agent_id: MAIN_AGENT_ID.to_string(),
            agent_name: MAIN_AGENT_NAME.to_string(),
            memory_scope_id: MAIN_AGENT_ID.to_string(),
            base_prompt: config.system_prompt.clone(),
            provider_id: config.provider.clone(),
            model: provider_config.model,
        });
    }

    if is_concierge_target(requested_agent_id) {
        let provider_id = config
            .concierge
            .provider
            .as_deref()
            .unwrap_or(&config.provider)
            .to_string();
        let provider_config = crate::agent::concierge::resolve_concierge_provider(config)?;
        return Ok(PromptInspectionTarget {
            agent_id: CONCIERGE_AGENT_ID.to_string(),
            agent_name: CONCIERGE_AGENT_NAME.to_string(),
            memory_scope_id: CONCIERGE_AGENT_ID.to_string(),
            base_prompt: crate::agent::concierge::concierge_system_prompt(),
            provider_id,
            model: provider_config.model,
        });
    }

    let normalized = canonical_agent_id(requested_agent_id);
    if normalized == WELES_AGENT_ID {
        let weles = sub_agents
            .iter()
            .find(|entry| entry.id == crate::agent::agent_identity::WELES_BUILTIN_SUBAGENT_ID)
            .context("missing builtin Weles definition")?;
        let mut provider_config =
            engine.resolve_sub_agent_provider_config(config, &weles.provider)?;
        provider_config.model = weles.model.clone();
        let system_prompt = weles
            .system_prompt
            .clone()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| config.system_prompt.clone());
        return Ok(PromptInspectionTarget {
            agent_id: WELES_AGENT_ID.to_string(),
            agent_name: WELES_AGENT_NAME.to_string(),
            memory_scope_id: WELES_AGENT_ID.to_string(),
            base_prompt: format!(
                "{}\n\n{}",
                crate::agent::agent_identity::build_weles_persona_prompt(
                    crate::agent::agent_identity::WELES_GOVERNANCE_SCOPE,
                ),
                system_prompt,
            ),
            provider_id: weles.provider.clone(),
            model: provider_config.model,
        });
    }

    if normalized != MAIN_AGENT_ID && normalized != CONCIERGE_AGENT_ID {
        if is_explicit_builtin_persona_scope(normalized)
            && builtin_persona_requires_setup(config, normalized)
        {
            return Err(builtin_persona_setup_error(normalized));
        }
        let provider_id = builtin_persona_overrides(config, normalized)
            .and_then(|overrides| overrides.provider.clone())
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| config.provider.clone());
        let mut provider_config = engine.resolve_sub_agent_provider_config(config, &provider_id)?;
        if let Some(model) = builtin_persona_overrides(config, normalized)
            .and_then(|overrides| overrides.model.clone())
            .filter(|value| !value.trim().is_empty())
        {
            provider_config.model = model;
        }
        return Ok(PromptInspectionTarget {
            agent_id: normalized.to_string(),
            agent_name: canonical_agent_name(normalized).to_string(),
            memory_scope_id: normalized.to_string(),
            base_prompt: format!(
                "{}\n\n{}",
                crate::agent::agent_identity::build_spawned_persona_prompt(normalized),
                config.system_prompt,
            ),
            provider_id,
            model: provider_config.model,
        });
    }

    anyhow::bail!("unknown prompt target: {requested_agent_id}")
}

fn build_sections(
    config: &AgentConfig,
    target: &PromptInspectionTarget,
    memory: &AgentMemory,
    memory_paths: &super::task_prompt::MemoryPaths,
    operator_model_summary: Option<&str>,
    operational_context: Option<&str>,
    causal_guidance: Option<&str>,
    learned_patterns: Option<&str>,
    active_skill_gate: Option<&crate::agent::types::LatestSkillDiscoveryState>,
) -> Vec<PromptInspectionSection> {
    let skills_root = super::skills_dir(&super::agent_data_dir());
    let guidelines_root = super::guidelines_dir(&super::agent_data_dir());
    let generated_skills_root = skills_root.join("generated");
    let mut sections = Vec::new();

    push_section(
        &mut sections,
        "base_prompt",
        "Base Prompt",
        target.base_prompt.clone(),
    );
    push_section(
        &mut sections,
        "identity_notes",
        "Identity Notes",
        memory.soul.clone(),
    );
    push_section(
        &mut sections,
        "persistent_memory",
        "Persistent Memory",
        memory.memory.clone(),
    );
    push_section(
        &mut sections,
        "operator_profile",
        "Operator Profile",
        memory.user_profile.clone(),
    );
    push_section(
        &mut sections,
        "memory_paths",
        "Persistent Memory File Paths",
        render_memory_paths_section(memory_paths),
    );
    if !is_main_agent_scope(&target.agent_id) {
        push_section(
            &mut sections,
            "shared_user_profile_policy",
            "Shared User Profile Policy",
            render_shared_user_profile_policy(),
        );
    }
    push_section(
        &mut sections,
        "local_guidelines",
        "Local Guidelines",
        render_local_guidelines_section(&guidelines_root),
    );
    push_section(
        &mut sections,
        "local_skills",
        "Local Skills",
        render_local_skills_section(&skills_root, &generated_skills_root),
    );
    if let Some(state) = active_skill_gate {
        push_section(
            &mut sections,
            "active_skill_gate",
            "Active Skill Gate",
            render_active_skill_gate_section(state),
        );
    }
    if let Some(plugin_skills) = render_plugin_skills_section(&skills_root) {
        push_section(
            &mut sections,
            "plugin_skills",
            "Plugin Skills",
            plugin_skills,
        );
    }
    if let Some(skill_index) = render_skill_index(&skills_root) {
        push_section(&mut sections, "skill_index", "Skill Index", skill_index);
    }
    push_section(
        &mut sections,
        "recall_and_memory",
        "Recall and Memory Maintenance",
        render_recall_and_memory_section(config),
    );
    push_section(
        &mut sections,
        "operator_model_summary",
        "Learned Operator Model",
        operator_model_summary.unwrap_or_default().to_string(),
    );
    push_section(
        &mut sections,
        "operational_context",
        "Operational Context",
        operational_context.unwrap_or_default().to_string(),
    );
    push_section(
        &mut sections,
        "causal_guidance",
        "Recent Causal Guidance",
        causal_guidance.unwrap_or_default().to_string(),
    );
    push_section(
        &mut sections,
        "learned_patterns",
        "Learned Patterns",
        learned_patterns.unwrap_or_default().to_string(),
    );
    push_section(
        &mut sections,
        "terminal_session_discipline",
        "Terminal Session Discipline",
        render_terminal_session_discipline_section(),
    );
    push_section(
        &mut sections,
        "large_file_writes",
        "Large File Writes",
        render_large_file_writes_section(),
    );
    push_section(
        &mut sections,
        "subagent_supervision",
        "Subagent Supervision",
        render_subagent_supervision_section(config),
    );
    push_section(
        &mut sections,
        "runtime_identity",
        "Runtime Identity",
        build_runtime_identity_prompt(&target.agent_name, &target.provider_id, &target.model),
    );

    sections
}

impl AgentEngine {
    pub(crate) async fn inspect_prompt_json(
        &self,
        requested_agent_id: Option<&str>,
        client_surface: Option<zorai_protocol::ClientSurface>,
    ) -> Result<String> {
        let config = self.config.read().await.clone();
        let sub_agents = self.list_sub_agents().await;
        let requested_agent_id = requested_agent_id
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(MAIN_AGENT_ID);

        let target =
            if let Some(definition) = exact_sub_agent_match(&sub_agents, requested_agent_id) {
                if definition.id == crate::agent::agent_identity::WELES_BUILTIN_SUBAGENT_ID {
                    build_direct_target(self, &config, WELES_AGENT_ID, &sub_agents)?
                } else {
                    build_exact_subagent_target(self, &config, definition)?
                }
            } else {
                build_direct_target(self, &config, requested_agent_id, &sub_agents)?
            };

        let memory = self
            .memory_snapshot_for_scope(&target.memory_scope_id)
            .await;
        let active_skill_gate = self
            .thread_skill_discovery_states
            .read()
            .await
            .values()
            .filter(|state| !state.compliant)
            .max_by_key(|state| state.updated_at)
            .cloned();
        let memory_paths = memory_paths_for_scope(&self.data_dir, &target.memory_scope_id);
        let operator_model_summary = self.build_operator_model_prompt_summary().await;
        let operational_context = self.build_operational_context_summary().await;
        let causal_guidance = self.build_causal_guidance_summary().await;
        let (tracked_thread_memory_injection_state_count, tracked_thread_memory_injection_states) = {
            let states = self.thread_memory_injection_states.read().await;
            let total = states.len();
            let mut entries = states
                .iter()
                .map(|(thread_id, state)| TrackedThreadMemoryInjectionStateView {
                    thread_id: thread_id.clone(),
                    state: state.clone(),
                })
                .collect::<Vec<_>>();
            entries.sort_by(|left, right| {
                right
                    .state
                    .base_markdown_injected_at_ms
                    .cmp(&left.state.base_markdown_injected_at_ms)
                    .then_with(|| left.thread_id.cmp(&right.thread_id))
            });
            entries.truncate(MAX_INSPECTED_INJECTION_STATES);
            (total, entries)
        };
        let learned_patterns = {
            let store = self.heuristic_store.read().await;
            let patterns = build_learned_patterns_section(&store);
            if patterns.is_empty() {
                None
            } else {
                Some(patterns)
            }
        };

        let mut final_prompt = build_system_prompt(
            &config,
            &target.base_prompt,
            &memory,
            &memory_paths,
            &target.memory_scope_id,
            &sub_agents,
            false,
            operator_model_summary.as_deref(),
            operational_context.as_deref(),
            causal_guidance.as_deref(),
            learned_patterns.as_deref(),
            None,
            None,
            None,
            None,
        );
        final_prompt.push_str("\n\n");
        final_prompt.push_str(&build_runtime_identity_prompt(
            &target.agent_name,
            &target.provider_id,
            &target.model,
        ));
        let structured_memory_summary =
            crate::agent::memory_context::build_structured_memory_summary(
                &memory,
                &memory_paths,
                None,
                None,
            );
        let timestamp_block = build_timestamp_block();
        let mut sections = build_sections(
            &config,
            &target,
            &memory,
            &memory_paths,
            operator_model_summary.as_deref(),
            operational_context.as_deref(),
            causal_guidance.as_deref(),
            learned_patterns.as_deref(),
            active_skill_gate.as_ref(),
        );
        push_section(
            &mut sections,
            "current_timestamp_block",
            "Current Timestamp Block",
            render_prompt_timestamp_block_section(&timestamp_block),
        );
        push_section(
            &mut sections,
            "structured_memory_summary",
            "Structured Memory Summary",
            structured_memory_summary.rendered_markdown.clone(),
        );
        push_section(
            &mut sections,
            "prompt_memory_injection_state",
            "Prompt Memory Injection State",
            render_prompt_memory_injection_state_section(
                tracked_thread_memory_injection_state_count,
                &tracked_thread_memory_injection_states,
            ),
        );
        let affinity_updates = self
            .load_morphogenesis_affinity_updates(&target.agent_id, None, 8)
            .await
            .unwrap_or_default();
        push_section(
            &mut sections,
            "morphogenesis_affinity_updates",
            "Morphogenesis Affinity Updates",
            render_morphogenesis_affinity_updates_section(&affinity_updates),
        );
        let soul_adaptations = self
            .load_soul_adaptations(&target.agent_id, 8)
            .await
            .unwrap_or_default();
        push_section(
            &mut sections,
            "morphogenesis_soul_adaptations",
            "Morphogenesis Soul Adaptations",
            render_morphogenesis_soul_adaptations_section(&soul_adaptations),
        );

        let tools_inspection = {
            let mut tools = crate::agent::tool_executor::get_available_tools(
                &config,
                &super::agent_data_dir(),
                true,
            );
            crate::agent::tool_executor::filter_tools_for_client_surface(
                &mut tools,
                client_surface,
            );
            if config.tools.deferred_tool_loading {
                let _ = crate::agent::tool_executor::partition_deferred_tools(&mut tools);
            }
            let mut entries = tools
                .iter()
                .map(|tool| {
                    let serialized = serde_json::to_string(tool).unwrap_or_default();
                    PromptToolInspection {
                        name: tool.function.name.clone(),
                        serialized_chars: serialized.chars().count(),
                        serialized,
                    }
                })
                .collect::<Vec<_>>();
            entries.sort_by(|left, right| right.serialized_chars.cmp(&left.serialized_chars));
            entries
        };
        let tool_count = tools_inspection.len();
        let tools_serialized_chars = tools_inspection
            .iter()
            .map(|tool| tool.serialized_chars)
            .sum();

        let payload = PromptInspectionPayload {
            agent_id: target.agent_id,
            agent_name: target.agent_name,
            provider_id: target.provider_id,
            model: target.model,
            timestamp_block,
            tracked_thread_memory_injection_state_count,
            tracked_thread_memory_injection_states,
            sections,
            final_prompt,
            tool_count,
            tools_serialized_chars,
            tools: tools_inspection,
        };

        serde_json::to_string(&payload).map_err(Into::into)
    }
}

#[cfg(test)]
mod tests {
    use super::render_active_skill_gate_section;
    use crate::agent::{AgentConfig, AgentEngine, SessionManager};
    use tempfile::tempdir;

    #[test]
    fn active_skill_gate_section_includes_key_gate_fields() {
        let rendered =
            render_active_skill_gate_section(&crate::agent::types::LatestSkillDiscoveryState {
                query: "debug panic".to_string(),
                confidence_tier: "strong".to_string(),
                recommended_skill: Some("systematic-debugging".to_string()),
                recommended_action: "request_approval systematic-debugging".to_string(),
                mesh_next_step: Some(crate::agent::skill_mesh::types::SkillMeshNextStep::ReadSkill),
                mesh_requires_approval: true,
                mesh_approval_id: Some("approval-1".to_string()),
                read_skill_identifier: Some("systematic-debugging".to_string()),
                skip_rationale: None,
                discovery_pending: false,
                skill_read_completed: true,
                compliant: false,
                updated_at: 1,
            });

        assert!(rendered.contains("Query: debug panic"));
        assert!(rendered.contains("Skill: systematic-debugging"));
        assert!(rendered.contains("Next action: request_approval systematic-debugging"));
        assert!(rendered.contains("Approval required: yes"));
        assert!(rendered.contains("Skill already read: yes"));
    }

    #[tokio::test]
    async fn inspect_prompt_for_tui_surface_excludes_gui_only_terminal_tools() {
        let root = tempdir().expect("tempdir");
        let manager = SessionManager::new_test(root.path()).await;
        let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

        let tool_names_for = |payload: &serde_json::Value| -> Vec<String> {
            payload
                .get("tools")
                .and_then(|value| value.as_array())
                .expect("payload should include tools")
                .iter()
                .filter_map(|tool| tool.get("name").and_then(|value| value.as_str()))
                .map(ToOwned::to_owned)
                .collect()
        };
        let gui_only = [
            zorai_protocol::tool_names::RUN_TERMINAL_COMMAND,
            zorai_protocol::tool_names::EXECUTE_MANAGED_COMMAND,
            zorai_protocol::tool_names::TYPE_IN_TERMINAL,
            zorai_protocol::tool_names::ALLOCATE_TERMINAL,
            zorai_protocol::tool_names::READ_ACTIVE_TERMINAL_CONTENT,
        ];

        let tui_payload: serde_json::Value = serde_json::from_str(
            &engine
                .inspect_prompt_json(None, Some(zorai_protocol::ClientSurface::Tui))
                .await
                .expect("inspect prompt should succeed"),
        )
        .expect("inspect prompt should return json");
        let tui_tools = tool_names_for(&tui_payload);
        for name in gui_only {
            assert!(
                !tui_tools.iter().any(|tool| tool == name),
                "TUI prompt inspection must not advertise GUI-only terminal tool `{name}`; the TUI cannot host managed terminal panes"
            );
        }
        assert!(
            tui_tools
                .iter()
                .any(|tool| tool == zorai_protocol::tool_names::BASH_COMMAND),
            "headless bash must remain available to TUI clients"
        );

        let unfiltered_payload: serde_json::Value = serde_json::from_str(
            &engine
                .inspect_prompt_json(None, None)
                .await
                .expect("inspect prompt should succeed"),
        )
        .expect("inspect prompt should return json");
        let unfiltered_tools = tool_names_for(&unfiltered_payload);
        assert!(
            unfiltered_tools
                .iter()
                .any(|tool| tool == zorai_protocol::tool_names::RUN_TERMINAL_COMMAND),
            "surface-agnostic inspection should still report terminal tools for GUI clients"
        );
    }

    #[tokio::test]
    async fn inspect_prompt_reports_structured_summary_without_claiming_it_is_always_injected() {
        let root = tempdir().expect("tempdir");
        let manager = SessionManager::new_test(root.path()).await;
        let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

        let payload: serde_json::Value = serde_json::from_str(
            &engine
                .inspect_prompt_json(None, None)
                .await
                .expect("inspect prompt should succeed"),
        )
        .expect("inspect prompt should return json");

        let final_prompt = payload
            .get("final_prompt")
            .and_then(|value| value.as_str())
            .expect("payload should include final_prompt");
        assert!(
            !final_prompt.contains("## Structured Memory Summary"),
            "thread-agnostic prompt inspection should not claim the structured summary is always injected into the final prompt"
        );

        let sections = payload
            .get("sections")
            .and_then(|value| value.as_array())
            .expect("payload should include sections");
        assert!(
            sections.iter().any(|section| {
                section.get("id").and_then(|value| value.as_str())
                    == Some("structured_memory_summary")
                    && section
                        .get("content")
                        .and_then(|value| value.as_str())
                        .is_some_and(|content| content.contains("## Structured Memory Summary"))
            }),
            "inspection output should expose the structured memory summary as a separate section"
        );
        assert!(
            sections.iter().any(|section| {
                section.get("id").and_then(|value| value.as_str())
                    == Some("prompt_memory_injection_state")
            }),
            "inspection output should expose prompt memory injection state diagnostics"
        );
        assert!(
            sections.iter().any(|section| {
                section.get("id").and_then(|value| value.as_str())
                    == Some("current_timestamp_block")
            }),
            "inspection output should expose the current timestamp block"
        );

        let timestamp_block = payload
            .get("timestamp_block")
            .and_then(|value| value.as_object())
            .expect("payload should expose timestamp_block");
        assert!(timestamp_block.contains_key("local_timestamp"));
        assert!(timestamp_block.contains_key("utc_timestamp"));
        assert!(timestamp_block.contains_key("unix_timestamp_ms"));

        let tracked_states = payload
            .get("tracked_thread_memory_injection_states")
            .and_then(|value| value.as_array())
            .expect("payload should expose tracked_thread_memory_injection_states");
        assert!(
            tracked_states.is_empty(),
            "thread-agnostic inspection should not synthesize a single merged memory state"
        );
    }

    #[tokio::test]
    async fn inspect_prompt_exposes_explicit_tracked_thread_memory_injection_states() {
        let root = tempdir().expect("tempdir");
        let manager = SessionManager::new_test(root.path()).await;
        let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
        engine
            .set_thread_memory_injection_state(
                "thread-explicit-memory-state",
                crate::agent::memory_context::PromptMemoryInjectionState {
                    base_markdown_hash: Some("sha256:base".to_string()),
                    base_markdown_updated_at_ms: Some(111),
                    soul_markdown_hash: Some("sha256:soul".to_string()),
                    soul_markdown_updated_at_ms: Some(112),
                    memory_markdown_hash: Some("sha256:memory".to_string()),
                    memory_markdown_updated_at_ms: Some(113),
                    user_markdown_hash: Some("sha256:user".to_string()),
                    user_markdown_updated_at_ms: Some(114),
                    structured_summary_hash: Some("sha256:summary".to_string()),
                    base_markdown_injected_at_ms: Some(115),
                    injected_after_compaction: true,
                },
            )
            .await;

        let payload: serde_json::Value = serde_json::from_str(
            &engine
                .inspect_prompt_json(None, None)
                .await
                .expect("inspect prompt should succeed"),
        )
        .expect("inspect prompt should return json");

        let tracked_states = payload
            .get("tracked_thread_memory_injection_states")
            .and_then(|value| value.as_array())
            .expect("payload should expose tracked thread memory injection states");
        assert_eq!(tracked_states.len(), 1);
        let state = tracked_states[0]
            .as_object()
            .expect("tracked state should be an object");
        assert_eq!(
            state.get("thread_id").and_then(|value| value.as_str()),
            Some("thread-explicit-memory-state")
        );
        assert_eq!(
            state
                .get("base_markdown_hash")
                .and_then(|value| value.as_str()),
            Some("sha256:base")
        );
        assert_eq!(
            state
                .get("structured_summary_hash")
                .and_then(|value| value.as_str()),
            Some("sha256:summary")
        );
        assert_eq!(
            state
                .get("injected_after_compaction")
                .and_then(|value| value.as_bool()),
            Some(true)
        );
    }

    #[tokio::test]
    async fn inspect_prompt_surfaces_morphogenesis_audit_sections() {
        let root = tempdir().expect("tempdir");
        let manager = SessionManager::new_test(root.path()).await;
        let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

        for _ in 0..11 {
            engine
                .record_morphogenesis_outcome(
                    crate::agent::agent_identity::MAIN_AGENT_ID,
                    &[String::from("debugging")],
                    crate::agent::morphogenesis::types::MorphogenesisOutcome::Success,
                )
                .await
                .expect("record morphogenesis outcome");
        }

        let payload: serde_json::Value = serde_json::from_str(
            &engine
                .inspect_prompt_json(None, None)
                .await
                .expect("inspect prompt should succeed"),
        )
        .expect("inspect prompt should return json");

        let sections = payload
            .get("sections")
            .and_then(|value| value.as_array())
            .expect("payload should include sections");
        assert!(
            sections.iter().any(|section| {
                section.get("id").and_then(|value| value.as_str())
                    == Some("morphogenesis_affinity_updates")
                    && section
                        .get("content")
                        .and_then(|value| value.as_str())
                        .is_some_and(|content| content.contains("debugging"))
            }),
            "inspection output should expose morphogenesis affinity update history"
        );
        assert!(
            sections.iter().any(|section| {
                section.get("id").and_then(|value| value.as_str())
                    == Some("morphogenesis_soul_adaptations")
                    && section
                        .get("content")
                        .and_then(|value| value.as_str())
                        .is_some_and(|content| content.contains("debugging"))
            }),
            "inspection output should expose morphogenesis soul adaptations"
        );
    }
}
