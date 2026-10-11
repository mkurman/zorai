//! System prompt construction and external agent prompt building.

use super::agent_identity::{CONCIERGE_AGENT_NAME, MAIN_AGENT_NAME};
use super::memory_curation_guidance;
use super::types::*;

/// Skill-discovery rules that apply identically to every audience (main agent,
/// Weles governance, Concierge). Keep this as the single source of truth — if
/// you change wording here, every system-prompt variant gets the update.
const SHARED_SKILL_DISCOVERY_RULES: &str = "\
     - `discover_guidelines`, `discover_skills`, `read_guideline`, and `read_skill` are optional. Call them only when a local workflow would change how you do this turn; skip them for simple Q&A, status, and work you already know how to do.\n\
     - When you do call `discover_guidelines` or `discover_skills`, send a brief intent sentence (e.g., 'modify python wheel builder for alternate compiler flag'), not the full task transcript and not a 2-3 word fragment.\n\
     - Read a guideline or skill only for a match you will actually follow.\n\
     - When you need clarification or the operator must choose among options, call `ask_questions`. Do not ask clarifying questions in plain text when this tool fits.\n\
     - For `ask_questions`, put the full question and answer text in `content`; buttons must stay compact and ordered via tokens like `A`, `B`, `C`, `D` or `1`, `2`, `3`.\n";

fn local_skill_workflow_prompt() -> String {
    let mut prompt = String::from(
        "## Local Guidelines and Skills Workflow\n\
         - Guidelines and skills are optional local workflows. Do not start a turn with discovery just because the tools exist.\n\
         - If a workflow orchestrator would help, call `discover_guidelines` then `read_guideline`. If a playbook would help, call `discover_skills` then `read_skill`.\n",
    );
    prompt.push_str(SHARED_SKILL_DISCOVERY_RULES);
    prompt.push_str(
        "     - Use `onecontext_search` or `session_search` when historical decisions, prior fixes, or existing implementations matter.\n\
         - Use `semantic_query` when you need codebase-wide structure or dependency context before editing.\n\
         - When a tool already performs the operator's requested side effect, avoid a redundant follow-up that only repeats a temp path or generic success. For `text_to_speech`, use it when asked to say something aloud or read text out loud, and only add extra text if playback failed, clarification is needed, or the operator asked for more detail.\n",
    );
    prompt
}

pub(super) fn render_subagent_supervision_section(config: &AgentConfig) -> String {
    let mut section = String::from(
        "- If you need subagents, call `list_agents` to see available spawnable specialists (`spawnable: true`), then `spawn_subagent` on demand with a matching title. Do not memorize a roster from this prompt.\n\
         - If a child should use a specific provider or model, call `fetch_authenticated_providers` first and `fetch_provider_models` for the chosen provider before setting `spawn_subagent.provider` or `spawn_subagent.model`.\n\
         - When spawning a child for directory or repository work, include the relevant path in the assignment and pass along any applicable `AGENTS.md` guidance when one exists; do not frame reading it as the child's first goal.\n\
         - Keep each subagent narrow in scope and avoid creating duplicate child assignments.\n\
         - Monitor spawned children with `list_subagents`. Do not use `list_agents` for child progress; it only lists runtime targets and spawnable definitions.\n\
         - If a child reports that it exhausted its execution budget and still has useful remaining work, call `extend_subagent_budget` on that child thread instead of respawning from scratch.\n\
         - Do not busy-wait on child agents. If there is no other useful work to do after delegating, send a normal progress update and stop so zorai can resume you when children finish.\n\
         - Spawned agents carry their own Slavic persona. Treat those identities as real collaborators with bounded scope, not as disposable copies of yourself.\n",
    );
    if config.collaboration.enabled {
        section.push_str(
            "- When subagents need to coordinate, use `broadcast_contribution`, `read_peer_memory`, and `vote_on_disagreement` so disagreements are explicit instead of implicit.\n",
        );
    }
    section.push_str(
        "- zorai caps active subagents per parent, so queue additional children only when they materially advance the task.\n\
         - For tasks requiring domain expertise, prefer `route_to_specialist` over `spawn_subagent`.\n",
    );
    section
}

fn build_time_context_prompt() -> String {
    let local_now = chrono::Local::now();
    let utc_now = chrono::Utc::now();
    // Bucket to the minute. This block lives inside the cached system prompt, so
    // sub-minute precision (seconds/millis) would change the prefix on every
    // request and defeat prompt caching entirely. Rapid agentic tool loops run
    // many requests within a single minute; minute granularity lets them share
    // the cached prefix while keeping the operator-visible time accurate. The
    // agent can shell `date` when it genuinely needs sub-minute precision.
    let unix_minute_ms = (utc_now.timestamp() / 60) * 60 * 1000;
    format!(
        "## Time Context\n- Current local timestamp: {}\n- Current UTC timestamp: {}\n- Unix timestamp (ms, minute precision): {}\n- Use this when reasoning about today, freshness, and schedule-sensitive work.\n",
        local_now.format("%A, %Y-%m-%d %H:%M %Z"),
        utc_now.format("%A, %Y-%m-%d %H:%M UTC"),
        unix_minute_ms
    )
}

fn append_prompt_section_if_missing(prompt: &mut String, marker: &str, section: &str) {
    if prompt.contains(marker) {
        return;
    }
    if !prompt.is_empty() && !prompt.ends_with("\n\n") {
        if prompt.ends_with('\n') {
            prompt.push('\n');
        } else {
            prompt.push_str("\n\n");
        }
    }
    prompt.push_str(section);
}

pub(super) fn build_system_prompt(
    config: &AgentConfig,
    base: &str,
    memory: &AgentMemory,
    memory_paths: &super::task_prompt::MemoryPaths,
    agent_scope_id: &str,
    _sub_agents: &[SubAgentDefinition],
    is_subagent: bool,
    operator_model_summary: Option<&str>,
    operational_context: Option<&str>,
    causal_guidance: Option<&str>,
    learned_patterns: Option<&str>,
    episodic_context: Option<&str>,
    continuity_summary: Option<&str>,
    negative_constraints: Option<&str>,
    client_surface: Option<zorai_protocol::ClientSurface>,
) -> String {
    let mut prompt = String::new();
    let skills_root = super::skills_dir(&super::agent_data_dir());
    let guidelines_root = super::guidelines_dir(&super::agent_data_dir());
    let generated_skills_root = skills_root.join("generated");

    if !memory.soul.is_empty() {
        prompt.push_str(&memory.soul);
        prompt.push_str("\n\n");
    }

    prompt.push_str(base);

    if !memory.memory.is_empty() {
        prompt.push_str("\n\n## Persistent Memory\n");
        prompt.push_str(&memory.memory);
    }

    if !memory.user_profile.is_empty() {
        prompt.push_str("\n\n## Operator Profile\n");
        prompt.push_str(&memory.user_profile);
    }

    prompt.push_str(
        &format!(
            "\n\n## Persistent Memory File Paths\n\
             - MEMORY.md: {}\n\
             - SOUL.md: {}\n\
             - USER.md: {}\n\
             - Use these exact paths when reading or explaining where zorai agent memory lives on this platform.\n",
            memory_paths.memory_path.display(),
            memory_paths.soul_path.display(),
            memory_paths.user_path.display(),
        ),
    );
    if !super::agent_identity::is_main_agent_scope(agent_scope_id) {
        prompt.push_str(
            "\n## Shared User Profile Policy\n\
             - USER.md is shared across agents and read-only for you.\n\
             - If a user preference or operator profile fact should change, ask ",
        );
        prompt.push_str(MAIN_AGENT_NAME);
        prompt.push_str(" via `message_agent` and let ");
        prompt.push_str(MAIN_AGENT_NAME);
        prompt.push_str(" decide whether to apply it.\n");
    }

    prompt.push_str(&format!(
        "\n\n## Local Guidelines\n\
             - Guidelines root: {}\n\
             - Guidelines are optional documentation-only workflow orchestrators above skills. Call `discover_guidelines` then `read_guideline` only when a guideline would change this turn; do not start every task with them.\n\
             - When the operator asks for work on a directory or repository path, check for `AGENTS.md` in that directory and its nearest relevant parent when repo-specific guidance is likely relevant. If present, apply the instructions that matter, but do not turn reading `AGENTS.md` into a standalone plan step unless the task specifically requires it.\n\
\n\
             ## Local Skills\n\
             - Skills root: {}\n\
             - Generated skills: {}\n\
             - Curated local skills live directly under {} (zorai reference docs for terminals, browser, tasks, goals, memory, safety, etc.).\n\
             - Call `discover_skills` then `read_skill` only when a local playbook would help. Use `read_memory`, `read_user`, and `read_soul` when you need memory recall.\n",
        guidelines_root.display(),
        skills_root.display(),
        generated_skills_root.display(),
        skills_root.display(),
    ));
    prompt.push_str(SHARED_SKILL_DISCOVERY_RULES);
    prompt.push_str(
        "             - `list_skills` is the raw catalog if you already know a name.\n\
             - The `cheatsheet` skill provides a quick reference for all available MCP tools.\n\
             - Prefer reusing an existing skill over inventing a brand-new workflow.\n",
    );
    let plugin_skills_dir = skills_root.join("plugins");
    if plugin_skills_dir.exists() && plugin_skills_dir.is_dir() {
        let plugin_count = std::fs::read_dir(&plugin_skills_dir)
            .map(|entries| {
                entries
                    .filter_map(|e| e.ok())
                    .filter(|e| e.path().is_dir())
                    .count()
            })
            .unwrap_or(0);
        if plugin_count > 0 {
            prompt.push_str(&format!(
                "\n### Plugin Skills\n\
                 - Plugin skills: {}/plugins/ ({} plugin(s) with bundled skills)\n\
                 - Plugin skills may reference API endpoints using `plugin:<plugin-name>:<endpoint>` notation\n\
                 - When a skill references `plugin:<name>:<endpoint>`, use the plugin API tool to call that endpoint\n\
                 - Plugin commands are available as slash commands (e.g., /pluginname.command)\n",
                skills_root.display(),
                plugin_count,
            ));
        }
    }

    if let Some(skill_index) = render_skill_index(&skills_root) {
        prompt
            .push_str("\nSkill index (load full content with `read_skill` only when relevant):\n");
        prompt.push_str(&skill_index);
        prompt.push('\n');
    }

    prompt.push_str(
        "\n\n## Recall and Memory Maintenance\n\
         - Prefer `read_memory`, `read_user`, and `read_soul` for memory recall before raw file reads. They return structured JSON and avoid duplicating already injected fresh base markdown.\n\
         - Treat zorai memory as layered: SOUL.md is identity, MEMORY.md stores durable facts and reusable strategy hints, USER.md stores operator profile, and structured daemon state plus recall systems carry the rest.\n\
         - Use `session_search` or `onecontext_search` when the user asks about prior decisions, existing implementations, or historical debugging context.\n\
         - Use `semantic_query` when you need local package/crate summaries, compose service topology, code import relationships, or learned workspace conventions before editing.\n\
         - Before big or multi-step work, write or update a short working spec in the current thread artifact specs directory under zorai data root (`threads/<thread_id>/artifacts/specs/`) so it survives handoffs and restarts.\n\
         - Before proceeding after a pause, handoff, or context shift, look up and reread the relevant spec from that current thread artifact specs directory so execution stays anchored to the latest written plan.\n\
         - For any non-trivial or multi-step task, call `update_todo` early to enter plan mode, then keep that todo list current as work progresses.\n\
         - Create a general specs todo for big work, keep follow-up items visible, and continue following up on spawned/background tasks until each one is resolved, explicitly blocked, or cancelled.\n\
         - When you learn durable operator preferences, stable project facts, or reusable strategy hints, call `update_memory` with a concise update so future sessions start with that context.\n\
         - Do not write task progress, temporary TODOs, approval status, or transient risk labels into durable memory.\n\
         - Memory is provenance-backed and operator-auditable. Correct or retract stale durable facts explicitly instead of silently layering contradictions on top.\n\
         - Memory files have hard limits: SOUL.md 2000 chars, MEMORY.md 3600 chars, USER.md 1800 chars.\n",
    );
    prompt.push_str(&format!(
        "         - {} is your concierge peer in zorai. Use `message_agent` only for private cross-agent coordination or quick checks. It delivers asynchronously and does not wait for the reply; this thread resumes when the recipient answers. It does not switch the active responder for the operator thread and future operator turns do not route to the message target. On visible threads, targeting an active thread participant defaults to visible-thread continuation so participant work stays in the operator thread.\n",
        CONCIERGE_AGENT_NAME
    ));
    prompt.push_str(
        "         - If the operator wants to talk directly to another agent, or if another agent should own future replies in this thread, use `handoff_thread_agent` instead. A successful handoff changes the active responder and future operator turns route to that agent until a return handoff.\n",
    );
    prompt.push_str(
        "         - Security in zorai is governance over transitions, not a casual suggestion. If scope, targets, privilege posture, environment, or command family materially changed, treat prior approval as stale until a fresh approval path says otherwise.\n",
    );
    prompt.push_str(
        "         - If work is paused behind approval or another governance gate, treat it as blocked work that needs resolution, not as permission to keep pushing similar side effects through another path.\n",
    );
    if config.enable_honcho_memory && !config.honcho_api_key.trim().is_empty() {
        prompt.push_str(
            "         - Use `agent_query_memory` when local session recall is insufficient and you need broader cross-session Honcho memory.\n",
        );
    }
    if config.tool_synthesis.enabled {
        prompt.push_str(
            "         - If the work depends on a missing but conservative CLI/API capability, use `synthesize_tool` to register a guarded generated tool, activate it with `activate_generated_tool`, and promote it later if it proves useful.\n",
        );
    }
    prompt.push_str("\nMemory curation guidance:\n");
    prompt.push_str(memory_curation_guidance());

    if let Some(operator_model_summary) =
        operator_model_summary.filter(|value| !value.trim().is_empty())
    {
        prompt.push_str("\n\n");
        prompt.push_str(operator_model_summary);
    }

    if let Some(operational_context) = operational_context.filter(|value| !value.trim().is_empty())
    {
        prompt.push_str("\n\n");
        prompt.push_str(operational_context);
    }

    if let Some(causal_guidance) = causal_guidance.filter(|value| !value.trim().is_empty()) {
        prompt.push_str("\n\n");
        prompt.push_str(causal_guidance);
    }

    if let Some(patterns) = learned_patterns {
        if !patterns.is_empty() {
            prompt.push_str("\n\n## Learned Patterns\n");
            prompt.push_str("These patterns were learned from successful past executions. Use them as guidance, not hard rules:\n");
            prompt.push_str(patterns);
        }
    }

    if let Some(ec) = episodic_context {
        if !ec.is_empty() {
            prompt.push_str("\n\n");
            prompt.push_str(ec);
        }
    }

    if let Some(continuity) = continuity_summary {
        if !continuity.is_empty() {
            prompt.push_str("\n\n");
            prompt.push_str(continuity);
        }
    }

    if let Some(nc) = negative_constraints {
        if !nc.is_empty() {
            prompt.push_str("\n\n");
            prompt.push_str(nc);
        }
    }

    // Terminal-related guidance only matters when the agent can actually call
    // terminal tools. TUI clients have those tools stripped at the tool-discovery
    // layer (see `filter_tools_for_client_surface`), so including this guidance
    // for them is pure token waste and can mislead the model into looking for
    // tools it does not have.
    let agent_has_terminal_tools =
        !matches!(client_surface, Some(zorai_protocol::ClientSurface::Tui));
    if agent_has_terminal_tools {
        prompt.push_str(
            "\n\n## Terminal Session Discipline\n\
             - Before running actions that truly need an existing terminal, call `list_terminals` to discover current live session IDs and CWD.\n\
             - Do not force a `session` argument in normal TUI chat or goal-run turns just because a previous frontend session existed. Omit `session` unless you intentionally target a known live terminal or the operator explicitly asked you to reuse one.\n\
             - When you do target a live terminal, reuse that `session` value across related tool calls so all actions stay in one terminal context.\n\
                - For long-running terminal work, prefer non-blocking execution: set `wait_for_completion=false` or use `timeout_seconds > 600`, capture the returned `operation_id`, and continue other work. This thread auto-resumes when the command finishes. Do not poll `get_operation_status`. If you cannot continue without the result, call `get_operation_status` once with `wait=true`.\n\
             - If a command is still running, timed out while still active, or is waiting for interactive completion, treat that terminal as occupied and switch to another terminal/session before continuing other work.\n\
             - If you need another terminal in the same agent workspace, call `allocate_terminal`, then continue with the returned session ID.\n\
             - Agent-allocated terminals are reclaimed when the owning task or subagent finishes, and unused idle lanes are closed automatically. Do not assume an allocated session stays forever.\n\
             - If the operator asks to use another terminal, call `list_terminals` again and switch explicitly.\n",
        );
    }

    prompt.push_str(
        "\n\n## Large File Writes\n\
         - Avoid giant JSON file payloads when content is large or heavily escaped.\n\
         - Prefer multipart-style `create_file` inputs when available.\n",
    );
    if agent_has_terminal_tools {
        prompt.push_str(
            "         - If you must write through a terminal, prefer a minimal Python writer over brittle shell heredocs.\n\
             - Before executing generated Python, inspect it for unintended side effects. It should only perform the intended file operation and should not add unrelated process, network, or shell behavior.\n",
        );
    }

    if !is_subagent {
        prompt.push_str("\n\n## Subagent Supervision\n");
        prompt.push_str(&render_subagent_supervision_section(config));
    } else {
        prompt.push_str(
            "\n\n## Subagent Report-Back\n\
             - Your final report is not counted against the execution budget.\n\
             - If the execution budget is exhausted, call `extend_subagent_budget` to raise this thread's ceiling, or `report_subagent_outcome` with status `done`, `cancelled`, or `error` and a concrete summary.\n\
             - If a provider quota or billing limit stops you independently of zorai, call `report_subagent_outcome` with status `error` and summarize what was completed.\n",
        );
    }

    prompt
}

pub(super) fn build_runtime_identity_prompt(
    agent_name: &str,
    provider_id: &str,
    model_id: &str,
) -> String {
    let aliases = super::agent_identity::agent_aliases();
    let agent_name = match super::agent_identity::alias_for_agent_name_in(agent_name, &aliases) {
        Some(alias) => format!("{alias} (internally {agent_name})"),
        None => agent_name.to_string(),
    };
    let mut prompt = format!(
        "## Runtime Identity\n\
         - You are {agent_name} in zorai.\n\
         - Active provider: {provider_id}\n\
         - Active model: {model_id}\n\
         - If the operator asks which provider or model you are currently using, answer with these exact runtime values unless the task explicitly tells you otherwise."
    );
    if let Some(note) = super::agent_identity::agent_alias_prompt_note_in(&aliases) {
        append_prompt_section_if_missing(&mut prompt, "## Agent Aliases", &note);
    }
    append_prompt_section_if_missing(&mut prompt, "## Time Context", &build_time_context_prompt());
    prompt
}

pub(crate) fn build_weles_governance_runtime_prompt(
    config: &AgentConfig,
    tool_name: &str,
    tool_args: &serde_json::Value,
    security_level: zorai_protocol::SecurityLevel,
    suspicion_reasons: &[String],
    task: Option<&AgentTask>,
    task_health_signals: Option<&serde_json::Value>,
) -> String {
    let mut prompt = super::weles_governance::build_weles_governance_prompt(
        config,
        tool_name,
        tool_args,
        security_level,
        suspicion_reasons,
        task,
        task_health_signals,
    );
    prompt.push_str("\n\n");
    prompt.push_str(&super::weles_governance::build_weles_governance_identity_prompt());
    append_prompt_section_if_missing(&mut prompt, "## Time Context", &build_time_context_prompt());
    append_prompt_section_if_missing(
        &mut prompt,
        "## Local Skills Workflow",
        &local_skill_workflow_prompt(),
    );
    prompt
}

pub(super) fn build_concierge_runtime_identity_prompt(provider_id: &str, model_id: &str) -> String {
    let mut prompt = build_runtime_identity_prompt(CONCIERGE_AGENT_NAME, provider_id, model_id);
    append_prompt_section_if_missing(
        &mut prompt,
        "## Local Skills Workflow",
        &local_skill_workflow_prompt(),
    );
    prompt
}

/// Build the "Learned Patterns" section content from reliable heuristics.
/// Only includes heuristics with usage_count >= 5 and effectiveness >= 0.6.
/// Per D-08: hybrid heuristic influence via system prompt injection.
pub(super) fn build_learned_patterns_section(
    heuristic_store: &super::learning::heuristics::HeuristicStore,
) -> String {
    let min_samples = 5u32;
    let min_effectiveness = 0.6;

    let reliable: Vec<_> = heuristic_store
        .tool_heuristics
        .iter()
        .filter(|h| h.usage_count >= min_samples && h.effectiveness_score >= min_effectiveness)
        .collect();

    if reliable.is_empty() {
        return String::new();
    }

    let mut section = String::new();
    let mut by_task: std::collections::HashMap<&str, Vec<_>> = std::collections::HashMap::new();
    for h in &reliable {
        by_task.entry(h.task_type.as_str()).or_default().push(h);
    }

    let mut task_types: Vec<_> = by_task.keys().copied().collect();
    task_types.sort();

    for task_type in task_types {
        let tools = &by_task[task_type];
        section.push_str(&format!("\nFor '{}' tasks:\n", task_type));
        for tool in tools {
            section.push_str(&format!(
                "- Prefer `{}` ({:.0}% effective, {} uses)\n",
                tool.tool_name,
                tool.effectiveness_score * 100.0,
                tool.usage_count,
            ));
        }
    }

    section
}

pub(super) fn build_external_agent_prompt(
    config: &AgentConfig,
    memory: &AgentMemory,
    user_message: &str,
    memory_paths: &super::task_prompt::MemoryPaths,
    agent_scope_id: &str,
    operator_model_summary: Option<&str>,
    operational_context: Option<&str>,
    causal_guidance: Option<&str>,
) -> String {
    let mut context_parts = Vec::new();
    let skills_root = super::skills_dir(&super::agent_data_dir());
    let generated_skills_root = skills_root.join("generated");

    context_parts.push(
        "[ENVIRONMENT: zorai]\n\
         You are being invoked through zorai — an agentic runtime app.\n\
         Keep your own identity and personality. Do NOT call yourself zorai.\n\
         \n\
         About zorai:\n\
         - zorai is a desktop app with workspaces, surfaces (tab groups), and panes (terminals)\n\
         - The user sees your response in zorai's agent chat panel (a sidebar)\n\
         - The user has terminal panes open next to this chat\n\
         - zorai's daemon manages your process lifecycle and relays your responses to the UI\n\
         \n\
         zorai tools via MCP:\n\
         zorai-mcp has been configured in your MCP servers. You should have access to \
         these tools — use them when the user asks about terminals, sessions, or history:\n\
         - list_sessions: list active terminal sessions (IDs, CWD, dimensions)\n\
         - get_terminal_content: read what's displayed in a terminal pane (scrollback)\n\
         - type_in_terminal: send keystrokes/input to a terminal session\n\
         - execute_command: run managed commands inside zorai terminal sessions\n\
         - search_history: full-text search of command/transcript history\n\
         - find_symbol: semantic code symbol search using tree-sitter\n\
         - get_git_status: git status for a working directory\n\
         - list_snapshots / restore_snapshot: workspace checkpoint management\n\
         - scrub_sensitive: redact secrets from text\n"
            .to_string(),
    );

    if !config.system_prompt.is_empty() {
        context_parts.push(format!("Operator instructions: {}\n", config.system_prompt));
    }

    let gw = &config.gateway;
    if gw.enabled {
        let mut platforms = Vec::new();
        if !gw.slack_token.is_empty() {
            platforms.push("Slack");
        }
        if !gw.discord_token.is_empty() {
            platforms.push("Discord");
        }
        if !gw.telegram_token.is_empty() {
            platforms.push("Telegram");
        }
        if !platforms.is_empty() {
            context_parts.push(format!(
                "Connected chat platforms: {}. Use your own messaging tools to reach them.\n",
                platforms.join(", ")
            ));
        }
    }

    if !memory.soul.is_empty() {
        context_parts.push(format!("Operator identity notes:\n{}\n", memory.soul));
    }
    if !memory.memory.is_empty() {
        context_parts.push(format!("Session memory:\n{}\n", memory.memory));
    }
    if !memory.user_profile.is_empty() {
        context_parts.push(format!("Operator profile:\n{}\n", memory.user_profile));
    }
    if let Some(operator_model_summary) =
        operator_model_summary.filter(|value| !value.trim().is_empty())
    {
        context_parts.push(format!(
            "Learned operator model:\n{}\n",
            operator_model_summary
        ));
    }

    context_parts.push(format!(
        "zorai persistent memory files on this machine:\n- MEMORY.md: {}\n- SOUL.md: {}\n- USER.md: {}\n",
        memory_paths.memory_path.display(),
        memory_paths.soul_path.display(),
        memory_paths.user_path.display(),
    ));
    let rendered_time_context = build_time_context_prompt();
    let time_context_body = rendered_time_context
        .trim()
        .strip_prefix("## Time Context\n")
        .unwrap_or(rendered_time_context.trim());
    context_parts.push(format!("Time context:\n{}\n", time_context_body));
    if !super::agent_identity::is_main_agent_scope(agent_scope_id) {
        context_parts
            .push("USER.md is shared across agents and read-only for this scope.\n".to_string());
    }
    context_parts.push(format!(
        "zorai local skills on this machine:\n- Skills root: {}\n- Generated skills: {}\n\
         Review a skill in that directory only when a local playbook would help; skip discovery for simple work.\n",
        skills_root.display(),
        generated_skills_root.display(),
    ));
    if let Some(operational_context) = operational_context.filter(|value| !value.trim().is_empty())
    {
        context_parts.push(format!("Operational context:\n{}\n", operational_context));
    }
    if let Some(causal_guidance) = causal_guidance.filter(|value| !value.trim().is_empty()) {
        context_parts.push(format!("Recent causal guidance:\n{}\n", causal_guidance));
    }

    format!(
        "{}\n[USER MESSAGE]\n{}",
        context_parts.join(""),
        user_message
    )
}

/// How many skills to surface in the in-prompt index. The full catalog is
/// reachable via `discover_skills` / `list_skills`; the index is a hint, not
/// a directory. Six keeps the prompt cheap while still naming the most-likely
/// matches alphabetically.
const SKILL_INDEX_MAX_ENTRIES: usize = 6;

pub(super) fn render_skill_index(skills_root: &std::path::Path) -> Option<String> {
    let mut skills = Vec::new();
    collect_skill_stems(skills_root, skills_root, &mut skills);
    skills.sort();
    skills.dedup();
    skills.truncate(SKILL_INDEX_MAX_ENTRIES);
    if skills.is_empty() {
        return None;
    }

    Some(
        skills
            .into_iter()
            .map(|skill| format!("- {skill}"))
            .collect::<Vec<_>>()
            .join("\n"),
    )
}

fn collect_skill_stems(root: &std::path::Path, dir: &std::path::Path, out: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_skill_stems(root, &path, out);
            continue;
        }
        if path.extension().and_then(|value| value.to_str()) != Some("md") {
            continue;
        }
        let Ok(_) = path.strip_prefix(root) else {
            continue;
        };
        let file_name = path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or_default();
        let stem = path
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or("skill");
        let name = if file_name.eq_ignore_ascii_case("SKILL.md") {
            path.parent()
                .and_then(|parent| parent.file_name())
                .and_then(|value| value.to_str())
                .filter(|value| !value.is_empty())
                .unwrap_or(stem)
        } else {
            stem
        };
        out.push(name.to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn concierge_runtime_identity_prompt_includes_current_day_and_skill_guidance() {
        let prompt = build_concierge_runtime_identity_prompt(
            zorai_shared::providers::PROVIDER_ID_OPENAI,
            "gpt-5.4",
        );

        assert!(prompt.contains("## Time Context"));
        assert!(prompt.contains("Current local timestamp:"));
        assert!(prompt.contains("Current UTC timestamp:"));
        assert!(prompt.contains("Unix timestamp (ms, minute precision):"));
        assert!(prompt.contains(zorai_protocol::tool_names::READ_SKILL));
        assert!(prompt.contains(zorai_protocol::tool_names::READ_GUIDELINE));
        assert!(prompt.contains(zorai_protocol::tool_names::DISCOVER_GUIDELINES));
        assert!(prompt.contains("optional"));
        assert!(prompt.contains("only when a local workflow would change"));
        assert!(!prompt.contains(zorai_protocol::tool_names::JUSTIFY_SKILL_SKIP));
        assert!(!prompt.contains("source of truth"));
        assert!(prompt.contains(zorai_protocol::tool_names::ONECONTEXT_SEARCH));
    }

    #[test]
    fn weles_governance_runtime_prompt_includes_current_day_and_skill_guidance() {
        let prompt = build_weles_governance_runtime_prompt(
            &AgentConfig::default(),
            zorai_protocol::tool_names::APPLY_PATCH,
            &serde_json::json!({"file": "src/main.rs"}),
            zorai_protocol::SecurityLevel::Moderate,
            &[],
            None,
            None,
        );

        assert!(prompt.contains("## Time Context"));
        assert!(prompt.contains("Current local timestamp:"));
        assert!(prompt.contains("Current UTC timestamp:"));
        assert!(prompt.contains("Unix timestamp (ms, minute precision):"));
        assert!(prompt.contains(zorai_protocol::tool_names::READ_SKILL));
        assert!(prompt.contains(zorai_protocol::tool_names::READ_GUIDELINE));
        assert!(prompt.contains(zorai_protocol::tool_names::DISCOVER_GUIDELINES));
        assert!(prompt.contains("optional"));
        assert!(prompt.contains("only when a local workflow would change"));
        assert!(!prompt.contains(zorai_protocol::tool_names::JUSTIFY_SKILL_SKIP));
        assert!(!prompt.contains("source of truth"));
        assert!(prompt.contains(zorai_protocol::tool_names::ONECONTEXT_SEARCH));
    }

    #[tokio::test]
    async fn system_prompt_requires_agents_md_lookup_for_directory_work_and_delegation() {
        let root = tempfile::tempdir().expect("tempdir should succeed");
        let manager = crate::session_manager::SessionManager::new_test(root.path()).await;
        let engine =
            crate::agent::AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

        let prompt = build_system_prompt(
            &AgentConfig::default(),
            "Base prompt",
            &crate::agent::types::AgentMemory::default(),
            &crate::agent::task_prompt::memory_paths_for_scope(
                root.path(),
                crate::agent::agent_identity::MAIN_AGENT_ID,
            ),
            crate::agent::agent_identity::MAIN_AGENT_ID,
            &engine.list_sub_agents().await,
            false,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        );

        assert!(prompt.contains("AGENTS.md"));
        assert!(prompt.contains("directory or repository path"));
        assert!(prompt.contains("nearest relevant parent"));
        assert!(prompt.contains("spawn_subagent"));
        assert!(prompt.contains("do not turn reading `AGENTS.md` into a standalone plan step"));
        assert!(prompt.contains("do not frame reading it as the child's first goal"));
        assert!(!prompt.contains("before planning or editing"));
        assert!(!prompt.contains("tell the child to read that `AGENTS.md`"));
    }

    #[tokio::test]
    async fn system_prompt_distinguishes_internal_dm_from_thread_handoff() {
        let root = tempfile::tempdir().expect("tempdir should succeed");
        let manager = crate::session_manager::SessionManager::new_test(root.path()).await;
        let engine =
            crate::agent::AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

        let prompt = build_system_prompt(
            &AgentConfig::default(),
            "Base prompt",
            &crate::agent::types::AgentMemory::default(),
            &crate::agent::task_prompt::memory_paths_for_scope(
                root.path(),
                crate::agent::agent_identity::MAIN_AGENT_ID,
            ),
            crate::agent::agent_identity::MAIN_AGENT_ID,
            &engine.list_sub_agents().await,
            false,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        );

        assert!(prompt.contains("`message_agent`"));
        assert!(prompt.contains("does not switch the active responder"));
        assert!(prompt.contains("`handoff_thread_agent`"));
        assert!(prompt.contains("If the operator wants to talk directly to another agent"));
    }

    #[tokio::test]
    async fn system_prompt_tells_agents_to_wait_for_background_operation_notifications() {
        let root = tempfile::tempdir().expect("tempdir should succeed");
        let manager = crate::session_manager::SessionManager::new_test(root.path()).await;
        let engine =
            crate::agent::AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

        let prompt = build_system_prompt(
            &AgentConfig::default(),
            "Base prompt",
            &crate::agent::types::AgentMemory::default(),
            &crate::agent::task_prompt::memory_paths_for_scope(
                root.path(),
                crate::agent::agent_identity::MAIN_AGENT_ID,
            ),
            crate::agent::agent_identity::MAIN_AGENT_ID,
            &engine.list_sub_agents().await,
            false,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        );

        assert!(prompt.contains("Do not poll `get_operation_status`"));
        assert!(prompt.contains("wait=true"));
    }

    #[tokio::test]
    async fn system_prompt_requires_ask_questions_for_clarifications() {
        let root = tempfile::tempdir().expect("tempdir should succeed");
        let manager = crate::session_manager::SessionManager::new_test(root.path()).await;
        let engine =
            crate::agent::AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

        let prompt = build_system_prompt(
            &AgentConfig::default(),
            "Base prompt",
            &crate::agent::types::AgentMemory::default(),
            &crate::agent::task_prompt::memory_paths_for_scope(
                root.path(),
                crate::agent::agent_identity::MAIN_AGENT_ID,
            ),
            crate::agent::agent_identity::MAIN_AGENT_ID,
            &engine.list_sub_agents().await,
            false,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        );

        assert!(prompt.contains("`ask_questions`"));
        assert!(prompt.contains("Do not ask clarifying questions in plain text"));
        assert!(prompt.contains("buttons must stay compact"));
    }

    #[tokio::test]
    async fn system_prompt_prefers_memory_tools_over_raw_memory_file_reads() {
        let root = tempfile::tempdir().expect("tempdir should succeed");
        let manager = crate::session_manager::SessionManager::new_test(root.path()).await;
        let engine =
            crate::agent::AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

        let prompt = build_system_prompt(
            &AgentConfig::default(),
            "Base prompt",
            &crate::agent::types::AgentMemory::default(),
            &crate::agent::task_prompt::memory_paths_for_scope(
                root.path(),
                crate::agent::agent_identity::MAIN_AGENT_ID,
            ),
            crate::agent::agent_identity::MAIN_AGENT_ID,
            &engine.list_sub_agents().await,
            false,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        );

        assert!(prompt.contains("Prefer `read_memory`, `read_user`, and `read_soul`"));
        assert!(prompt.contains("Treat zorai memory as layered"));
        assert!(prompt.contains("Memory is provenance-backed and operator-auditable"));
        assert!(
            !prompt.contains("consult MEMORY.md and USER.md"),
            "system prompt should not keep the old raw-file-first memory guidance"
        );
    }

    #[tokio::test]
    async fn system_prompt_mentions_governance_semantics_for_security() {
        let root = tempfile::tempdir().expect("tempdir should succeed");
        let manager = crate::session_manager::SessionManager::new_test(root.path()).await;
        let engine =
            crate::agent::AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

        let prompt = build_system_prompt(
            &AgentConfig::default(),
            "Base prompt",
            &crate::agent::types::AgentMemory::default(),
            &crate::agent::task_prompt::memory_paths_for_scope(
                root.path(),
                crate::agent::agent_identity::MAIN_AGENT_ID,
            ),
            crate::agent::agent_identity::MAIN_AGENT_ID,
            &engine.list_sub_agents().await,
            false,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        );

        assert!(prompt.contains("Security in zorai is governance over transitions"));
        assert!(prompt.contains("treat prior approval as stale"));
    }

    #[tokio::test]
    async fn system_prompt_requires_thread_artifact_specs_and_task_followthrough_for_big_work() {
        let root = tempfile::tempdir().expect("tempdir should succeed");
        let manager = crate::session_manager::SessionManager::new_test(root.path()).await;
        let engine =
            crate::agent::AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

        let prompt = build_system_prompt(
            &AgentConfig::default(),
            "Base prompt",
            &crate::agent::types::AgentMemory::default(),
            &crate::agent::task_prompt::memory_paths_for_scope(
                root.path(),
                crate::agent::agent_identity::MAIN_AGENT_ID,
            ),
            crate::agent::agent_identity::MAIN_AGENT_ID,
            &engine.list_sub_agents().await,
            false,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        );

        assert!(prompt.contains("current thread artifact specs directory"));
        assert!(prompt.contains("`threads/<thread_id>/artifacts/specs/`"));
        assert!(prompt.contains(
            "look up and reread the relevant spec from that current thread artifact specs directory"
        ));
        assert!(prompt.contains("Create a general specs todo for big work"));
        assert!(prompt.contains(
            "continue following up on spawned/background tasks until each one is resolved"
        ));
        assert!(prompt.contains("only when a local workflow would change"));
        assert!(prompt.contains("`list_agents`"));
        assert!(!prompt.contains("## Available Sub-Agents"));
        assert!(!prompt.contains("Strong matches require"));
        assert!(!prompt.contains("before non-trivial work"));
    }

    #[tokio::test]
    async fn system_prompt_omits_terminal_session_discipline_for_tui_client_surface() {
        let root = tempfile::tempdir().expect("tempdir should succeed");
        let manager = crate::session_manager::SessionManager::new_test(root.path()).await;
        let engine =
            crate::agent::AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

        let prompt = build_system_prompt(
            &AgentConfig::default(),
            "Base prompt",
            &crate::agent::types::AgentMemory::default(),
            &crate::agent::task_prompt::memory_paths_for_scope(
                root.path(),
                crate::agent::agent_identity::MAIN_AGENT_ID,
            ),
            crate::agent::agent_identity::MAIN_AGENT_ID,
            &engine.list_sub_agents().await,
            false,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            Some(zorai_protocol::ClientSurface::Tui),
        );

        assert!(
            !prompt.contains("## Terminal Session Discipline"),
            "TUI client surface has terminal tools stripped — the discipline guidance is misleading and should be omitted"
        );
        assert!(
            !prompt.contains("`list_terminals`"),
            "no terminal tool references should remain in TUI prompts"
        );
        assert!(
            !prompt.contains("If you must write through a terminal"),
            "the Large File Writes section should drop its terminal sub-bullets for TUI"
        );
    }

    #[tokio::test]
    async fn system_prompt_keeps_terminal_session_discipline_for_non_tui_surfaces() {
        let root = tempfile::tempdir().expect("tempdir should succeed");
        let manager = crate::session_manager::SessionManager::new_test(root.path()).await;
        let engine =
            crate::agent::AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

        let prompt = build_system_prompt(
            &AgentConfig::default(),
            "Base prompt",
            &crate::agent::types::AgentMemory::default(),
            &crate::agent::task_prompt::memory_paths_for_scope(
                root.path(),
                crate::agent::agent_identity::MAIN_AGENT_ID,
            ),
            crate::agent::agent_identity::MAIN_AGENT_ID,
            &engine.list_sub_agents().await,
            false,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        );

        assert!(
            prompt.contains("## Terminal Session Discipline"),
            "non-TUI / unknown surfaces should still see terminal discipline"
        );
        assert!(prompt.contains("`list_terminals`"));
    }

    #[tokio::test]
    async fn subagent_prompt_omits_spawn_guidance_and_registry_to_prevent_recursive_spawning() {
        let root = tempfile::tempdir().expect("tempdir should succeed");
        let spawnable = SubAgentDefinition {
            base_url: None,
            claude_permission_mode: None,
            id: "researcher".to_string(),
            name: "Researcher".to_string(),
            provider: "openai".to_string(),
            model: "gpt-5.4-mini".to_string(),
            role: Some("investigator".to_string()),
            system_prompt: Some("Investigate the assigned area.".to_string()),
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
            reasoning_effort: None,
            api_transport: None,
            openrouter_provider_order: Vec::new(),
            openrouter_provider_ignore: Vec::new(),
            openrouter_allow_fallbacks: None,
            huggingface_provider: None,
            created_at: 1,
        };
        let memory_paths = crate::agent::task_prompt::memory_paths_for_scope(
            root.path(),
            crate::agent::agent_identity::MAIN_AGENT_ID,
        );
        let build = |is_subagent: bool| {
            build_system_prompt(
                &AgentConfig::default(),
                "Base prompt",
                &crate::agent::types::AgentMemory::default(),
                &memory_paths,
                crate::agent::agent_identity::MAIN_AGENT_ID,
                std::slice::from_ref(&spawnable),
                is_subagent,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
            )
        };

        let root_prompt = build(false);
        assert!(
            root_prompt.contains("## Subagent Supervision"),
            "the top-level agent should still be told how to delegate"
        );
        assert!(
            root_prompt.contains("`list_agents`") && root_prompt.contains("`spawn_subagent`"),
            "the top-level agent should be told to fetch spawnable specialists on demand"
        );
        assert!(
            !root_prompt.contains("## Available Sub-Agents") && !root_prompt.contains("Researcher"),
            "the top-level agent must not receive an inlined spawnable roster"
        );

        let subagent_prompt = build(true);
        assert!(
            !subagent_prompt.contains("## Subagent Supervision"),
            "a spawned subagent must not be encouraged to spawn its own children"
        );
        assert!(
            !subagent_prompt.contains("## Available Sub-Agents")
                && !subagent_prompt.contains("spawn_subagent"),
            "a spawned subagent must not receive spawn tooling guidance"
        );
    }

    #[test]
    fn render_skill_index_uses_parent_dir_name_for_skill_md_layout() {
        let root = tempfile::tempdir().expect("tempdir should succeed");
        let brainstorming_dir = root.path().join("brainstorming");
        std::fs::create_dir_all(&brainstorming_dir).expect("create brainstorming dir");
        std::fs::write(brainstorming_dir.join("SKILL.md"), "# Brainstorming\n")
            .expect("write SKILL.md");

        let index = render_skill_index(root.path()).expect("index should render");

        assert!(
            index.contains("- brainstorming"),
            "skill index should emit the parent directory name for SKILL.md files, got: {index}"
        );
        assert!(
            !index.contains("- SKILL\n") && index.trim() != "- SKILL",
            "skill index must not emit the literal 'SKILL' stem, got: {index}"
        );
    }

    #[test]
    fn render_skill_index_keeps_stem_for_flat_md_files() {
        let root = tempfile::tempdir().expect("tempdir should succeed");
        std::fs::write(root.path().join("build-pipeline.md"), "# Build pipeline\n")
            .expect("write skill");

        let index = render_skill_index(root.path()).expect("index should render");

        assert!(
            index.contains("- build-pipeline"),
            "skill index should keep stems for flat .md files, got: {index}"
        );
    }

    #[test]
    fn render_skill_index_dedupes_distinct_skill_md_skills_into_separate_entries() {
        let root = tempfile::tempdir().expect("tempdir should succeed");
        for name in ["pr-triage", "inbox-triage", "daily-brief"] {
            let dir = root.path().join(name);
            std::fs::create_dir_all(&dir).expect("create skill dir");
            std::fs::write(dir.join("SKILL.md"), format!("# {name}\n")).expect("write SKILL.md");
        }

        let index = render_skill_index(root.path()).expect("index should render");

        for name in ["pr-triage", "inbox-triage", "daily-brief"] {
            assert!(
                index.contains(&format!("- {name}")),
                "skill index should list each SKILL.md skill distinctly, missing '{name}' in: {index}"
            );
        }
    }
}
