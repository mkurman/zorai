use clap::{Parser, Subcommand, ValueEnum};

fn parse_page(value: &str) -> Result<usize, String> {
    let page = value
        .parse::<usize>()
        .map_err(|_| "page must be a positive integer".to_string())?;
    if page == 0 {
        return Err("page must be at least 1".to_string());
    }
    Ok(page)
}

fn parse_list_limit(value: &str) -> Result<usize, String> {
    let limit = value
        .parse::<usize>()
        .map_err(|_| "limit must be a positive integer".to_string())?;
    if limit == 0 {
        return Err("limit must be at least 1".to_string());
    }
    if limit > 20 {
        return Err("limit must be 20 or less".to_string());
    }
    Ok(limit)
}

#[derive(Parser)]
#[command(
    name = "zorai",
    about = "Zorai Agent and Terminal Agentic Multiplexer CLI"
)]
pub(crate) struct Cli {
    #[command(subcommand)]
    pub(crate) command: Option<Commands>,
}

#[derive(Debug, Subcommand)]
pub(crate) enum Commands {
    /// Launch the terminal UI.
    Tui,

    /// Launch the desktop GUI (Electron).
    Gui,

    /// Show agent statistics (alias for status).
    Stats,

    /// List all running daemon-owned terminal sessions.
    #[command(alias = "ls")]
    List,

    /// Spawn a new terminal session.
    #[command(hide = true)]
    New {
        /// Shell to use (default: system shell).
        #[arg(short, long)]
        shell: Option<String>,
        /// Working directory.
        #[arg(short, long)]
        cwd: Option<String>,
        /// Workspace ID to associate with.
        #[arg(short, long)]
        workspace: Option<String>,
    },

    /// Attach to an existing session.
    #[command(hide = true)]
    Attach {
        /// Session ID.
        id: String,
    },

    /// Clone an existing session into a new independent session.
    Clone {
        /// Source session ID.
        #[arg(long)]
        source: String,
        /// Optional terminal columns override.
        #[arg(long)]
        cols: Option<u16>,
        /// Optional terminal rows override.
        #[arg(long)]
        rows: Option<u16>,
        /// Optional workspace override.
        #[arg(short, long)]
        workspace: Option<String>,
        /// Fallback CWD when the source session's CWD cannot be resolved.
        #[arg(long)]
        cwd: Option<String>,
    },

    /// Kill a daemon-owned terminal session.
    Kill {
        /// Session ID.
        id: String,
    },

    /// Get git status for a directory.
    Git {
        /// Directory path (default: current directory).
        #[arg(default_value = ".")]
        path: String,
    },

    /// Scrub sensitive data from stdin or a string.
    #[command(hide = true)]
    Scrub {
        /// Text to scrub (reads from stdin if not provided).
        text: Option<String>,
    },

    /// View the agent's action audit trail.
    #[command(alias = "log")]
    Audit {
        /// Filter by action type(s), comma-separated (heartbeat,tool,escalation,skill,subagent).
        #[arg(long, value_delimiter = ',')]
        r#type: Option<Vec<String>>,
        /// Show actions since this time ago (e.g., "1h", "24h", "7d").
        #[arg(long)]
        since: Option<String>,
        /// Show full detail for a specific action ID.
        #[arg(long)]
        detail: Option<String>,
        /// Maximum number of entries to show.
        #[arg(long, default_value = "50")]
        limit: usize,
    },

    /// Manage discovered skills.
    Skill {
        #[command(subcommand)]
        action: SkillAction,
    },

    /// Manage local guidelines.
    #[command(alias = "guidelines")]
    Guideline {
        #[command(subcommand)]
        action: GuidelineAction,
    },

    /// Manage semantic skill and guideline indexing.
    Semantic {
        #[command(subcommand)]
        action: SemanticAction,
    },

    /// Inspect the tools currently available to the daemon agent.
    Tool {
        #[command(subcommand)]
        action: ToolAction,
    },

    /// Show current agent status.
    Status,

    /// Show the assembled system prompt for an agent.
    Prompt {
        /// Inspect a specific agent id or name.
        #[arg(long)]
        agent: Option<String>,
        /// Inspect the WELES prompt.
        #[arg(long, conflicts_with_all = ["agent", "concierge", "rarog"])]
        weles: bool,
        /// Inspect the concierge agent prompt.
        #[arg(long, conflicts_with_all = ["agent", "weles", "rarog"])]
        concierge: bool,
        /// Inspect the Rarog agent prompt.
        #[arg(long, conflicts_with_all = ["agent", "weles", "concierge"])]
        rarog: bool,
        /// Emit the raw daemon payload as JSON.
        #[arg(long)]
        json: bool,
    },

    /// Query the status of an asynchronous daemon operation.
    Operation {
        /// Operation ID returned by the daemon.
        id: String,
        /// Emit the raw daemon payload as JSON.
        #[arg(long)]
        json: bool,
    },

    /// View and modify daemon configuration.
    Settings {
        #[command(subcommand)]
        action: SettingsAction,
    },

    /// Inspect and manage agent threads.
    Thread {
        #[command(subcommand)]
        action: ThreadAction,
    },

    /// Inspect and manage agent goals.
    Goal {
        #[command(subcommand)]
        action: GoalAction,
    },

    /// Inspect and manage workspace board tasks.
    Workspace {
        #[command(subcommand)]
        action: WorkspaceAction,
    },

    /// Migrate settings and reusable data from another local agent into zorai.
    Migrate {
        #[command(subcommand)]
        action: MigrateAction,
    },

    /// Send a direct message to svarog or Rarog from the CLI.
    Dm {
        /// Continue a specific thread.
        #[arg(long)]
        thread: Option<String>,
        /// Preferred terminal session hint.
        #[arg(long)]
        session: Option<String>,
        /// Route the message to svarog explicitly.
        #[arg(long = "svarog", alias = "swarog", conflicts_with_all = ["rarog", "main_target", "concierge"])]
        svarog: bool,
        /// Route the message to Rarog explicitly.
        #[arg(long, conflicts_with_all = ["svarog", "main_target", "concierge"])]
        rarog: bool,
        /// Route the message to svarog using the old alias.
        #[arg(long = "main", conflicts_with_all = ["svarog", "rarog", "concierge"])]
        main_target: bool,
        /// Route the message to Rarog instead of svarog.
        #[arg(long, conflicts_with_all = ["svarog", "rarog", "main_target"])]
        concierge: bool,
        /// Emit a JSON object with response and ids.
        #[arg(long)]
        json: bool,
        /// Message content.
        #[arg(required = true)]
        message: Vec<String>,
    },

    /// Run the first-time setup wizard.
    Setup,

    /// Ping the daemon (health check).
    Ping,

    /// Stop zorai background processes.
    Stop,

    /// Restart zorai by stopping background processes and starting the daemon again.
    Restart,

    /// Upgrade zorai to the npm registry's current @latest release.
    #[command(alias = "update")]
    Upgrade {
        /// Install without stopping the daemon and gateway; restart zorai afterwards to use the new version.
        #[arg(long)]
        keep_running: bool,
    },

    /// Start the daemon (if not already running).
    #[command(name = "daemon")]
    StartDaemon,

    /// Manage plugins (v2 manifest format).
    Plugin {
        #[command(subcommand)]
        action: PluginAction,
    },

    /// Install runtime extensions.
    Install {
        #[command(subcommand)]
        target: InstallTarget,
    },

    /// Internal agent bridge used by the Electron frontend.
    #[command(hide = true, name = "agent-bridge")]
    AgentBridge,

    /// Internal database bridge used by the Electron frontend.
    #[command(hide = true, name = "db-bridge")]
    DbBridge,

    /// Internal JSON bridge used by the Electron frontend.
    #[command(hide = true, name = "bridge")]
    Bridge {
        /// Attach to an existing session instead of spawning a new one.
        #[arg(long)]
        session: Option<String>,
        /// Shell to use when spawning a session.
        #[arg(long)]
        shell: Option<String>,
        /// Working directory when spawning a session.
        #[arg(long)]
        cwd: Option<String>,
        /// Workspace ID to associate with the spawned session.
        #[arg(long)]
        workspace: Option<String>,
        /// Initial terminal columns.
        #[arg(long, default_value_t = 80)]
        cols: u16,
        /// Initial terminal rows.
        #[arg(long, default_value_t = 24)]
        rows: u16,
        /// Fail instead of spawning a replacement when the session no longer exists.
        #[arg(long)]
        attach_only: bool,
    },
}

#[derive(Debug, Subcommand)]
pub(crate) enum InstallTarget {
    /// Install a zorai plugin from npm or a local package directory.
    Plugin {
        /// npm package spec or local package directory.
        package: String,
    },
    /// Install a markdown guideline into the zorai guidelines directory.
    Guideline {
        /// Source markdown guideline file.
        source: String,
        /// Destination filename under the guidelines directory.
        #[arg(long)]
        name: Option<String>,
        /// Overwrite an existing guideline with the same destination name.
        #[arg(long)]
        force: bool,
    },
}

#[derive(Debug, Subcommand)]
pub(crate) enum MigrateAction {
    /// Show detected Hermes/OpenClaw installations and import readiness.
    Status {
        /// Emit raw JSON instead of human-readable output.
        #[arg(long)]
        json: bool,
    },
    /// Preview what would be imported without changing zorai state.
    Preview {
        /// Migration source runtime: hermes or openclaw.
        runtime: String,
        /// Optional config path override.
        #[arg(long, alias = "config-path")]
        config: Option<String>,
        /// Emit raw JSON instead of human-readable output.
        #[arg(long)]
        json: bool,
    },
    /// Apply a migration into zorai.
    Apply {
        /// Migration source runtime: hermes or openclaw.
        runtime: String,
        /// Optional config path override.
        #[arg(long, alias = "config-path")]
        config: Option<String>,
        /// Conflict policy: skip, merge, replace, or stage_for_review.
        #[arg(long, default_value = "stage_for_review")]
        conflict_policy: String,
        /// Emit raw JSON instead of human-readable output.
        #[arg(long)]
        json: bool,
    },
    /// Show the persisted migration report.
    Report {
        /// Optional migration source filter: hermes or openclaw.
        runtime: Option<String>,
        /// Maximum profiles to include.
        #[arg(long, default_value_t = 20)]
        limit: usize,
        /// Emit raw JSON instead of human-readable output.
        #[arg(long)]
        json: bool,
    },
    /// Compare an imported source profile against current zorai state.
    ShadowRun {
        /// Migration source runtime: hermes or openclaw.
        runtime: String,
        /// Emit raw JSON instead of human-readable output.
        #[arg(long)]
        json: bool,
    },
}

#[derive(Debug, Subcommand)]
pub(crate) enum PluginAction {
    /// Install a plugin from npm, GitHub, or local path.
    Add {
        /// Plugin source: npm package name, GitHub URL, or local directory path.
        source: String,
    },
    /// Uninstall a plugin by name.
    Remove {
        /// Plugin name to uninstall.
        name: String,
    },
    /// List installed plugins. Per INST-05/D-08.
    #[command(alias = "list")]
    Ls,
    /// Enable a disabled plugin. Per INST-06.
    Enable {
        /// Plugin name to enable.
        name: String,
    },
    /// Disable a plugin without uninstalling. Per INST-06.
    Disable {
        /// Plugin name to disable.
        name: String,
    },
    /// List available plugin commands. Per PSKL-05.
    Commands,
}

#[derive(Debug, Subcommand)]
pub(crate) enum SkillAction {
    /// List all skills with their maturity status.
    #[command(alias = "ls")]
    List {
        /// Filter by status (draft, testing, active, proven, promoted_to_canonical).
        #[arg(long)]
        status: Option<String>,
        /// Maximum number of entries to show.
        #[arg(long, default_value = "50")]
        limit: usize,
        /// Continue from an earlier page cursor.
        #[arg(long)]
        cursor: Option<String>,
        /// Fetch every page by following cursors until exhaustion.
        #[arg(long)]
        all: bool,
    },
    /// Download the latest default skills from the zorai repository.
    Sync {
        /// Overwrite existing local skills with upstream versions.
        #[arg(long)]
        force: bool,
    },
    /// Rank installed skills for a task and suggest the next action.
    Discover {
        /// Task or problem description to match against installed skills.
        query: String,
        /// Optional terminal session UUID for workspace-aware ranking.
        #[arg(long)]
        session: Option<String>,
        /// Maximum number of ranked candidates to show.
        #[arg(long, default_value = "3")]
        limit: usize,
        /// Continue from an earlier discovery page cursor.
        #[arg(long)]
        cursor: Option<String>,
    },
    /// Show details of a specific skill.
    Inspect {
        /// Skill name or variant ID.
        name: String,
    },
    /// Reject and delete a draft skill.
    Reject {
        /// Skill name or variant ID.
        name: String,
    },
    /// Fast-promote a skill (skip to target status).
    Promote {
        /// Skill name or variant ID.
        name: String,
        /// Target status to promote to.
        #[arg(long, default_value = "active")]
        to: String,
    },
    /// Search the community skill registry.
    Search {
        /// Search query (matches skill name, description, tags).
        query: String,
    },
    /// Import a community skill by name or URL.
    Import {
        /// Skill name from registry or direct URL.
        source: String,
        /// Override security warnings.
        #[arg(long)]
        force: bool,
    },
    /// Export a local skill to a directory.
    Export {
        /// Skill name or variant ID.
        name: String,
        /// Output format: "zorai" (default) or "agentskills".
        #[arg(long, default_value = "zorai")]
        format: String,
        /// Output directory (default: current directory).
        #[arg(long, default_value = ".")]
        output: String,
    },
    /// Publish a proven skill to the community registry.
    Publish {
        /// Skill name or variant ID.
        name: String,
    },
}

#[derive(Debug, Subcommand)]
pub(crate) enum GuidelineAction {
    /// Rank installed guidelines for a task and suggest the next action.
    Discover {
        /// Task or problem description to match against installed guidelines.
        query: String,
        /// Optional terminal session UUID for workspace-aware ranking.
        #[arg(long)]
        session: Option<String>,
        /// Maximum number of ranked candidates to show.
        #[arg(long, default_value = "3")]
        limit: usize,
        /// Continue from an earlier discovery page cursor.
        #[arg(long)]
        cursor: Option<String>,
    },
    /// Show the contents of a specific guideline.
    #[command(alias = "read")]
    Inspect {
        /// Guideline name, file stem, or relative path.
        name: String,
    },
    /// Install a markdown guideline into the zorai guidelines directory.
    Install {
        /// Source markdown guideline file.
        source: String,
        /// Destination filename under the guidelines directory.
        #[arg(long)]
        name: Option<String>,
        /// Overwrite an existing guideline with the same destination name.
        #[arg(long)]
        force: bool,
    },
    /// Download the latest default guidelines from the zorai repository.
    Sync {
        /// Overwrite existing local guidelines with upstream versions.
        #[arg(long)]
        force: bool,
    },
    /// List installed guidelines from the zorai guidelines directory.
    #[command(alias = "ls")]
    List {
        /// Emit raw JSON instead of human-readable output.
        #[arg(long)]
        json: bool,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub(crate) enum SemanticRerankKind {
    Skill,
    Guideline,
}

#[derive(Debug, Subcommand)]
pub(crate) enum SemanticAction {
    /// Show semantic embedding queue and index status.
    Status {
        /// Emit raw JSON instead of human-readable output.
        #[arg(long)]
        json: bool,
    },
    /// Repair a corrupted local semantic vector index and requeue rebuild work.
    RepairIndex {
        /// Confirm destructive repair of the local LanceDB vector index.
        #[arg(long)]
        yes: bool,
        /// Emit raw JSON instead of human-readable output.
        #[arg(long)]
        json: bool,
    },
    /// Run the semantic skill/guideline discovery index sync job now.
    Sync {
        /// Emit raw JSON instead of human-readable output.
        #[arg(long)]
        json: bool,
    },
    /// Rerank installed skills or guidelines for a query using semantic vectors when available.
    Rerank {
        /// Which catalog to rerank.
        kind: SemanticRerankKind,
        /// Task or problem description to match against installed documents.
        query: String,
        /// Maximum number of ranked candidates to show.
        #[arg(long, default_value = "3")]
        limit: usize,
        /// Emit raw JSON instead of human-readable output.
        #[arg(long)]
        json: bool,
    },
}

#[derive(Debug, Subcommand)]
pub(crate) enum ToolAction {
    /// List the tools currently available to the daemon agent.
    #[command(alias = "ls")]
    List {
        /// Maximum number of tools to show.
        #[arg(long, default_value = "50")]
        limit: usize,
        /// Zero-based pagination offset.
        #[arg(long, default_value = "0")]
        offset: usize,
        /// Emit raw JSON instead of human-readable output.
        #[arg(long)]
        json: bool,
    },
    /// Search the currently available tool catalog.
    Search {
        /// What capability or action to look for.
        query: String,
        /// Maximum number of matches to show.
        #[arg(long, default_value = "20")]
        limit: usize,
        /// Zero-based pagination offset.
        #[arg(long, default_value = "0")]
        offset: usize,
        /// Emit raw JSON instead of human-readable output.
        #[arg(long)]
        json: bool,
    },
}

#[derive(Debug, Subcommand)]
pub(crate) enum SettingsAction {
    /// List all configuration settings.
    #[command(alias = "ls")]
    List,
    /// Get a specific configuration value.
    Get {
        /// Configuration key (dot-notation, e.g., "heartbeat.interval", "provider", "model").
        key: String,
    },
    /// Set a configuration value.
    Set {
        /// Configuration key (dot-notation, e.g., "heartbeat.interval", "provider", "model").
        key: String,
        /// Value to set (JSON or plain string).
        value: String,
    },
}

#[derive(Debug, Subcommand)]
pub(crate) enum ThreadAction {
    /// List recent agent threads.
    #[command(alias = "ls")]
    List {
        /// 1-based page number.
        #[arg(long, default_value_t = 1, value_parser = parse_page)]
        page: usize,
        /// Maximum items per page (capped at 20).
        #[arg(long, default_value_t = 20, value_parser = parse_list_limit)]
        limit: usize,
        /// Emit raw JSON instead of human-readable output.
        #[arg(long)]
        json: bool,
    },
    /// Show one agent thread and its messages.
    Get {
        /// Thread ID to fetch.
        thread_id: String,
        /// Emit raw JSON instead of human-readable output.
        #[arg(long)]
        json: bool,
    },
    /// Stop the active stream for one agent thread.
    Stop {
        /// Thread ID to stop.
        thread_id: String,
    },
    /// Resume one agent thread by retrying execution now.
    Resume {
        /// Thread ID to resume.
        thread_id: String,
    },
    /// Delete one agent thread.
    Delete {
        /// Thread ID to delete.
        thread_id: String,
        /// Skip the confirmation prompt.
        #[arg(long)]
        yes: bool,
    },
}

#[derive(Debug, Subcommand)]
pub(crate) enum GoalAction {
    /// List recent goal runs.
    #[command(alias = "ls")]
    List {
        /// 1-based page number.
        #[arg(long, default_value_t = 1, value_parser = parse_page)]
        page: usize,
        /// Maximum items per page (capped at 20).
        #[arg(long, default_value_t = 20, value_parser = parse_list_limit)]
        limit: usize,
        /// Emit raw JSON instead of human-readable output.
        #[arg(long)]
        json: bool,
    },
    /// Show one goal run and its current state.
    Get {
        /// Goal run ID to fetch.
        goal_run_id: String,
        /// Emit raw JSON instead of human-readable output.
        #[arg(long)]
        json: bool,
    },
    /// Show the structured dossier projection for one goal run.
    Dossier {
        /// Goal run ID to fetch.
        goal_run_id: String,
        /// Emit raw JSON instead of human-readable output.
        #[arg(long)]
        json: bool,
    },
    /// Show proof checks and evidence for one goal run.
    Proof {
        /// Goal run ID to fetch.
        goal_run_id: String,
        /// Emit raw JSON instead of human-readable output.
        #[arg(long)]
        json: bool,
    },
    /// Show implementation and verification reports for one goal run.
    Reports {
        /// Goal run ID to fetch.
        goal_run_id: String,
        /// Emit raw JSON instead of human-readable output.
        #[arg(long)]
        json: bool,
    },
    /// Stop one goal run.
    Stop {
        /// Goal run ID to stop.
        goal_run_id: String,
    },
    /// Resume one paused goal run.
    Resume {
        /// Goal run ID to resume.
        goal_run_id: String,
    },
    /// Accept a goal awaiting supervisor review.
    Accept {
        /// Goal run ID to accept.
        goal_run_id: String,
    },
    /// Soft-reject a completeness report and keep the same worker going.
    SoftReject {
        /// Goal run ID to continue.
        goal_run_id: String,
        /// Why the work is incomplete.
        explanation: String,
    },
    /// Hard-reject a goal and fail it.
    HardReject {
        /// Goal run ID to fail.
        goal_run_id: String,
        /// Why the goal is rejected.
        explanation: String,
    },
    /// Delete one goal run.
    Delete {
        /// Goal run ID to delete.
        goal_run_id: String,
        /// Skip the confirmation prompt.
        #[arg(long)]
        yes: bool,
    },
}

#[derive(Debug, Subcommand)]
pub(crate) enum WorkspaceAction {
    /// List workspace board tasks.
    #[command(alias = "ls")]
    List {
        /// Workspace ID.
        #[arg(long, default_value = "main")]
        workspace: String,
        /// Include soft-deleted tasks.
        #[arg(long)]
        include_deleted: bool,
        /// Filter by status: todo, in-progress, in-review, done.
        #[arg(long)]
        status: Option<String>,
        /// Filter by priority: low, normal, high, urgent.
        #[arg(long)]
        priority: Option<String>,
        /// Filter by assignee: user, svarog, agent:id, subagent:id, or none.
        #[arg(long)]
        assignee: Option<String>,
        /// Filter by reviewer: user, svarog, agent:id, subagent:id, or none.
        #[arg(long)]
        reviewer: Option<String>,
        /// Emit raw JSON instead of human-readable output.
        #[arg(long)]
        json: bool,
    },
    /// Show one workspace task.
    Get {
        /// Workspace task ID.
        task_id: String,
        /// Emit raw JSON instead of human-readable output.
        #[arg(long)]
        json: bool,
    },
    /// Create a workspace task.
    New {
        /// Task type: thread or goal.
        task_type: String,
        /// Task title.
        title: String,
        /// Task description.
        description: String,
        /// Workspace ID.
        #[arg(long, default_value = "main")]
        workspace: String,
        /// Optional definition of done.
        #[arg(long)]
        dod: Option<String>,
        /// Priority: low, normal, high, urgent.
        #[arg(long, default_value = "low")]
        priority: String,
        /// Assignee: svarog, agent:id, subagent:id, or none.
        #[arg(long)]
        assignee: Option<String>,
        /// Reviewer: user, svarog, agent:id, subagent:id, or none.
        #[arg(long, default_value = "user")]
        reviewer: String,
        /// Emit raw JSON instead of human-readable output.
        #[arg(long)]
        json: bool,
    },
    /// Update editable workspace task fields.
    Update {
        /// Workspace task ID.
        task_id: String,
        /// Replace task title.
        #[arg(long)]
        title: Option<String>,
        /// Replace task description.
        #[arg(long)]
        description: Option<String>,
        /// Replace definition of done.
        #[arg(long)]
        dod: Option<String>,
        /// Clear definition of done.
        #[arg(long, conflicts_with = "dod")]
        clear_dod: bool,
        /// Priority: low, normal, high, urgent.
        #[arg(long)]
        priority: Option<String>,
        /// Assignee: svarog, agent:id, subagent:id, or none.
        #[arg(long)]
        assignee: Option<String>,
        /// Reviewer: user, svarog, agent:id, subagent:id, or none.
        #[arg(long)]
        reviewer: Option<String>,
        /// Emit raw JSON instead of human-readable output.
        #[arg(long)]
        json: bool,
    },
    /// Show or set the workspace-level operator.
    Operator {
        /// Workspace ID.
        #[arg(long, default_value = "main")]
        workspace: String,
        /// Optional operator: user or svarog.
        operator: Option<String>,
        /// Emit raw JSON instead of human-readable output.
        #[arg(long)]
        json: bool,
    },
    /// Assign task to an agent/subagent, or clear with none.
    Assign {
        task_id: String,
        assignee: String,
        #[arg(long)]
        json: bool,
    },
    /// Set task reviewer, or clear with none.
    Reviewer {
        task_id: String,
        reviewer: String,
        #[arg(long)]
        json: bool,
    },
    /// Run one workspace task.
    Run {
        task_id: String,
        #[arg(long)]
        json: bool,
    },
    /// Pause one workspace task.
    Pause {
        task_id: String,
        #[arg(long)]
        json: bool,
    },
    /// Stop one workspace task.
    Stop {
        task_id: String,
        #[arg(long)]
        json: bool,
    },
    /// Move task to a board status.
    Move {
        task_id: String,
        status: String,
        /// Insert before tasks at this sort order in the target status.
        #[arg(long)]
        sort_order: Option<i64>,
        #[arg(long)]
        json: bool,
    },
    /// Submit a review verdict.
    Review {
        task_id: String,
        verdict: String,
        message: Vec<String>,
        #[arg(long)]
        json: bool,
    },
    /// Soft-delete one workspace task.
    Delete {
        task_id: String,
        #[arg(long)]
        yes: bool,
        #[arg(long)]
        json: bool,
    },
    /// List workspace task notices.
    Notices {
        /// Workspace ID.
        #[arg(long, default_value = "main")]
        workspace: String,
        /// Optional task ID filter.
        #[arg(long)]
        task: Option<String>,
        /// Emit raw JSON instead of human-readable output.
        #[arg(long)]
        json: bool,
    },
}

#[cfg(test)]
mod tests {
    use super::{
        Cli, Commands, GoalAction, GuidelineAction, MigrateAction, SemanticAction,
        SemanticRerankKind, SkillAction, ThreadAction, ToolAction, WorkspaceAction,
    };
    use clap::{CommandFactory, Parser};

    #[test]
    fn skill_discover_subcommand_parses_query() {
        let cli = Cli::try_parse_from(["zorai", "skill", "discover", "debug panic"])
            .expect("skill discover subcommand should parse");
        match cli.command {
            Some(Commands::Skill {
                action:
                    SkillAction::Discover {
                        query,
                        session,
                        limit,
                        ..
                    },
            }) => {
                assert_eq!(query, "debug panic");
                assert_eq!(session, None);
                assert_eq!(limit, 3);
            }
            other => panic!("parsed unexpected command: {other:?}"),
        }
    }

    #[test]
    fn semantic_sync_subcommand_parses_json_flag() {
        let cli = Cli::try_parse_from(["zorai", "semantic", "sync", "--json"])
            .expect("semantic sync subcommand should parse");
        match cli.command {
            Some(Commands::Semantic {
                action: SemanticAction::Sync { json },
            }) => assert!(json),
            other => panic!("parsed unexpected command: {other:?}"),
        }
    }

    #[test]
    fn semantic_status_subcommand_parses_json_flag() {
        let cli = Cli::try_parse_from(["zorai", "semantic", "status", "--json"])
            .expect("semantic status subcommand should parse");
        match cli.command {
            Some(Commands::Semantic {
                action: SemanticAction::Status { json },
            }) => assert!(json),
            other => panic!("parsed unexpected command: {other:?}"),
        }
    }

    #[test]
    fn semantic_repair_index_subcommand_parses_confirmation_and_json() {
        let cli = Cli::try_parse_from(["zorai", "semantic", "repair-index", "--yes", "--json"])
            .expect("semantic repair-index subcommand should parse");
        match cli.command {
            Some(Commands::Semantic {
                action: SemanticAction::RepairIndex { yes, json },
            }) => {
                assert!(yes);
                assert!(json);
            }
            other => panic!("parsed unexpected command: {other:?}"),
        }
    }

    #[test]
    fn semantic_rerank_subcommand_parses_kind_query_and_limit() {
        let cli = Cli::try_parse_from([
            "zorai",
            "semantic",
            "rerank",
            "guideline",
            "coding workflow",
            "--limit",
            "7",
            "--json",
        ])
        .expect("semantic rerank subcommand should parse");
        match cli.command {
            Some(Commands::Semantic {
                action:
                    SemanticAction::Rerank {
                        kind,
                        query,
                        limit,
                        json,
                    },
            }) => {
                assert_eq!(kind, SemanticRerankKind::Guideline);
                assert_eq!(query, "coding workflow");
                assert_eq!(limit, 7);
                assert!(json);
            }
            other => panic!("parsed unexpected command: {other:?}"),
        }
    }

    #[test]
    fn guideline_install_subcommand_parses_source_and_force() {
        let cli = Cli::try_parse_from([
            "zorai",
            "guideline",
            "install",
            "/tmp/coding-task.md",
            "--name",
            "team-coding.md",
            "--force",
        ])
        .expect("guideline install subcommand should parse");
        match cli.command {
            Some(Commands::Guideline {
                action:
                    GuidelineAction::Install {
                        source,
                        name,
                        force,
                    },
            }) => {
                assert_eq!(source, "/tmp/coding-task.md");
                assert_eq!(name.as_deref(), Some("team-coding.md"));
                assert!(force);
            }
            other => panic!("parsed unexpected command: {other:?}"),
        }
    }

    #[test]
    fn guideline_list_subcommand_parses_json_flag() {
        let cli = Cli::try_parse_from(["zorai", "guidelines", "list", "--json"])
            .expect("guidelines list alias should parse");
        match cli.command {
            Some(Commands::Guideline {
                action: GuidelineAction::List { json },
            }) => assert!(json),
            other => panic!("parsed unexpected command: {other:?}"),
        }
    }

    #[test]
    fn migrate_preview_subcommand_parses_runtime_path_and_json_flag() {
        let cli = Cli::try_parse_from([
            "zorai",
            "migrate",
            "preview",
            "hermes",
            "--config",
            "/tmp/hermes.yaml",
            "--json",
        ])
        .expect("migrate preview subcommand should parse");
        match cli.command {
            Some(Commands::Migrate {
                action:
                    MigrateAction::Preview {
                        runtime,
                        config,
                        json,
                    },
            }) => {
                assert_eq!(runtime, "hermes");
                assert_eq!(config.as_deref(), Some("/tmp/hermes.yaml"));
                assert!(json);
            }
            other => panic!("parsed unexpected command: {other:?}"),
        }
    }

    #[test]
    fn guideline_sync_subcommand_parses_force_flag() {
        let cli = Cli::try_parse_from(["zorai", "guidelines", "sync", "--force"])
            .expect("guidelines sync should parse");
        match cli.command {
            Some(Commands::Guideline {
                action: GuidelineAction::Sync { force },
            }) => assert!(force),
            other => panic!("parsed unexpected command: {other:?}"),
        }
    }

    #[test]
    fn skill_sync_subcommand_parses_force_flag() {
        let cli = Cli::try_parse_from(["zorai", "skill", "sync", "--force"])
            .expect("skill sync should parse");
        match cli.command {
            Some(Commands::Skill {
                action: SkillAction::Sync { force },
            }) => assert!(force),
            other => panic!("parsed unexpected command: {other:?}"),
        }
    }

    #[test]
    fn guideline_discover_subcommand_parses_like_skill_discover() {
        let cli = Cli::try_parse_from([
            "zorai",
            "guidelines",
            "discover",
            "--session",
            "550e8400-e29b-41d4-a716-446655440000",
            "--limit",
            "5",
            "research papers",
        ])
        .expect("guidelines discover alias should parse");
        match cli.command {
            Some(Commands::Guideline {
                action:
                    GuidelineAction::Discover {
                        query,
                        session,
                        limit,
                        cursor,
                    },
            }) => {
                assert_eq!(query, "research papers");
                assert_eq!(
                    session.as_deref(),
                    Some("550e8400-e29b-41d4-a716-446655440000")
                );
                assert_eq!(limit, 5);
                assert_eq!(cursor, None);
            }
            other => panic!("parsed unexpected command: {other:?}"),
        }
    }

    #[test]
    fn guideline_inspect_subcommand_parses_read_alias() {
        let cli = Cli::try_parse_from(["zorai", "guideline", "read", "research-task"])
            .expect("guideline read alias should parse");
        match cli.command {
            Some(Commands::Guideline {
                action: GuidelineAction::Inspect { name },
            }) => assert_eq!(name, "research-task"),
            other => panic!("parsed unexpected command: {other:?}"),
        }
    }

    #[test]
    fn install_guideline_target_parses_source_and_name() {
        let cli = Cli::try_parse_from([
            "zorai",
            "install",
            "guideline",
            "/tmp/coding-task.md",
            "--name",
            "team-coding.md",
        ])
        .expect("install guideline target should parse");
        match cli.command {
            Some(Commands::Install {
                target:
                    super::InstallTarget::Guideline {
                        source,
                        name,
                        force,
                    },
            }) => {
                assert_eq!(source, "/tmp/coding-task.md");
                assert_eq!(name.as_deref(), Some("team-coding.md"));
                assert!(!force);
            }
            other => panic!("parsed unexpected command: {other:?}"),
        }
    }

    #[test]
    fn thread_list_subcommand_parses() {
        let cli = Cli::try_parse_from([
            "zorai", "thread", "list", "--page", "2", "--limit", "15", "--json",
        ])
        .expect("thread list subcommand should parse");
        match cli.command {
            Some(Commands::Thread {
                action: ThreadAction::List { page, limit, json },
            }) => {
                assert_eq!(page, 2);
                assert_eq!(limit, 15);
                assert!(json);
            }
            other => panic!("expected thread list command, got {other:?}"),
        }
    }

    #[test]
    fn tool_list_subcommand_parses() {
        let cli = Cli::try_parse_from(["zorai", "tool", "list", "--limit", "25", "--offset", "5"])
            .expect("tool list subcommand should parse");
        match cli.command {
            Some(Commands::Tool {
                action:
                    ToolAction::List {
                        limit,
                        offset,
                        json,
                    },
            }) => {
                assert_eq!(limit, 25);
                assert_eq!(offset, 5);
                assert!(!json);
            }
            other => panic!("expected tool list command, got {other:?}"),
        }
    }

    #[test]
    fn tool_search_subcommand_parses() {
        let cli = Cli::try_parse_from(["zorai", "tool", "search", "discover skill", "--json"])
            .expect("tool search subcommand should parse");
        match cli.command {
            Some(Commands::Tool {
                action:
                    ToolAction::Search {
                        query,
                        limit,
                        offset,
                        json,
                    },
            }) => {
                assert_eq!(query, "discover skill");
                assert_eq!(limit, 20);
                assert_eq!(offset, 0);
                assert!(json);
            }
            other => panic!("expected tool search command, got {other:?}"),
        }
    }

    #[test]
    fn thread_get_subcommand_parses() {
        let cli = Cli::try_parse_from(["zorai", "thread", "get", "thread-123"])
            .expect("thread get subcommand should parse");
        match cli.command {
            Some(Commands::Thread {
                action: ThreadAction::Get { thread_id, json },
            }) => {
                assert_eq!(thread_id, "thread-123");
                assert!(!json);
            }
            other => panic!("expected thread get command, got {other:?}"),
        }
    }

    #[test]
    fn thread_delete_subcommand_parses() {
        let cli = Cli::try_parse_from(["zorai", "thread", "delete", "thread-123", "--yes"])
            .expect("thread delete subcommand should parse");
        match cli.command {
            Some(Commands::Thread {
                action: ThreadAction::Delete { thread_id, yes },
            }) => {
                assert_eq!(thread_id, "thread-123");
                assert!(yes);
            }
            other => panic!("expected thread delete command, got {other:?}"),
        }
    }

    #[test]
    fn thread_stop_subcommand_parses() {
        let cli = Cli::try_parse_from(["zorai", "thread", "stop", "thread-123"])
            .expect("thread stop subcommand should parse");
        match cli.command {
            Some(Commands::Thread {
                action: ThreadAction::Stop { thread_id },
            }) => {
                assert_eq!(thread_id, "thread-123");
            }
            other => panic!("expected thread stop command, got {other:?}"),
        }
    }

    #[test]
    fn thread_resume_subcommand_parses() {
        let cli = Cli::try_parse_from(["zorai", "thread", "resume", "thread-123"])
            .expect("thread resume subcommand should parse");
        match cli.command {
            Some(Commands::Thread {
                action: ThreadAction::Resume { thread_id },
            }) => {
                assert_eq!(thread_id, "thread-123");
            }
            other => panic!("expected thread resume command, got {other:?}"),
        }
    }

    #[test]
    fn goal_list_subcommand_parses() {
        let cli = Cli::try_parse_from([
            "zorai", "goal", "list", "--page", "3", "--limit", "10", "--json",
        ])
        .expect("goal list subcommand should parse");
        match cli.command {
            Some(Commands::Goal {
                action: GoalAction::List { page, limit, json },
            }) => {
                assert_eq!(page, 3);
                assert_eq!(limit, 10);
                assert!(json);
            }
            other => panic!("expected goal list command, got {other:?}"),
        }
    }

    #[test]
    fn list_subcommands_default_to_first_page_with_limit_20() {
        let thread_cli =
            Cli::try_parse_from(["zorai", "thread", "list"]).expect("thread list should parse");
        match thread_cli.command {
            Some(Commands::Thread {
                action: ThreadAction::List { page, limit, json },
            }) => {
                assert_eq!(page, 1);
                assert_eq!(limit, 20);
                assert!(!json);
            }
            other => panic!("expected thread list command, got {other:?}"),
        }

        let goal_cli =
            Cli::try_parse_from(["zorai", "goal", "list"]).expect("goal list should parse");
        match goal_cli.command {
            Some(Commands::Goal {
                action: GoalAction::List { page, limit, json },
            }) => {
                assert_eq!(page, 1);
                assert_eq!(limit, 20);
                assert!(!json);
            }
            other => panic!("expected goal list command, got {other:?}"),
        }
    }

    #[test]
    fn goal_get_subcommand_parses() {
        let cli = Cli::try_parse_from(["zorai", "goal", "get", "goal-123"])
            .expect("goal get subcommand should parse");
        match cli.command {
            Some(Commands::Goal {
                action: GoalAction::Get { goal_run_id, json },
            }) => {
                assert_eq!(goal_run_id, "goal-123");
                assert!(!json);
            }
            other => panic!("expected goal get command, got {other:?}"),
        }
    }

    #[test]
    fn goal_stop_subcommand_parses() {
        let cli = Cli::try_parse_from(["zorai", "goal", "stop", "goal-123"])
            .expect("goal stop subcommand should parse");
        match cli.command {
            Some(Commands::Goal {
                action: GoalAction::Stop { goal_run_id },
            }) => {
                assert_eq!(goal_run_id, "goal-123");
            }
            other => panic!("expected goal stop command, got {other:?}"),
        }
    }

    #[test]
    fn goal_dossier_subcommand_parses() {
        let cli = Cli::try_parse_from(["zorai", "goal", "dossier", "goal-123"])
            .expect("goal dossier subcommand should parse");
        match cli.command {
            Some(Commands::Goal {
                action: GoalAction::Dossier { goal_run_id, json },
            }) => {
                assert_eq!(goal_run_id, "goal-123");
                assert!(!json);
            }
            other => panic!("expected goal dossier command, got {other:?}"),
        }
    }

    #[test]
    fn goal_proof_subcommand_parses() {
        let cli = Cli::try_parse_from(["zorai", "goal", "proof", "goal-123"])
            .expect("goal proof subcommand should parse");
        match cli.command {
            Some(Commands::Goal {
                action: GoalAction::Proof { goal_run_id, json },
            }) => {
                assert_eq!(goal_run_id, "goal-123");
                assert!(!json);
            }
            other => panic!("expected goal proof command, got {other:?}"),
        }
    }

    #[test]
    fn goal_reports_subcommand_parses() {
        let cli = Cli::try_parse_from(["zorai", "goal", "reports", "goal-123"])
            .expect("goal reports subcommand should parse");
        match cli.command {
            Some(Commands::Goal {
                action: GoalAction::Reports { goal_run_id, json },
            }) => {
                assert_eq!(goal_run_id, "goal-123");
                assert!(!json);
            }
            other => panic!("expected goal reports command, got {other:?}"),
        }
    }

    #[test]
    fn goal_resume_subcommand_parses() {
        let cli = Cli::try_parse_from(["zorai", "goal", "resume", "goal-123"])
            .expect("goal resume subcommand should parse");
        match cli.command {
            Some(Commands::Goal {
                action: GoalAction::Resume { goal_run_id },
            }) => {
                assert_eq!(goal_run_id, "goal-123");
            }
            other => panic!("expected goal resume command, got {other:?}"),
        }
    }

    #[test]
    fn goal_accept_subcommand_parses() {
        let cli = Cli::try_parse_from(["zorai", "goal", "accept", "goal-123"])
            .expect("goal accept subcommand should parse");
        match cli.command {
            Some(Commands::Goal {
                action: GoalAction::Accept { goal_run_id },
            }) => {
                assert_eq!(goal_run_id, "goal-123");
            }
            other => panic!("expected goal accept command, got {other:?}"),
        }
    }

    #[test]
    fn goal_delete_subcommand_parses() {
        let cli = Cli::try_parse_from(["zorai", "goal", "delete", "goal-123", "--yes"])
            .expect("goal delete subcommand should parse");
        match cli.command {
            Some(Commands::Goal {
                action: GoalAction::Delete { goal_run_id, yes },
            }) => {
                assert_eq!(goal_run_id, "goal-123");
                assert!(yes);
            }
            other => panic!("expected goal delete command, got {other:?}"),
        }
    }

    #[test]
    fn workspace_new_subcommand_parses() {
        let cli = Cli::try_parse_from([
            "zorai",
            "workspace",
            "new",
            "goal",
            "Ship board",
            "Implement workspace board",
            "--workspace",
            "main",
            "--assignee",
            "svarog",
            "--reviewer",
            "user",
            "--priority",
            "high",
        ])
        .expect("workspace new subcommand should parse");
        match cli.command {
            Some(Commands::Workspace {
                action:
                    WorkspaceAction::New {
                        task_type,
                        title,
                        description,
                        workspace,
                        assignee,
                        reviewer,
                        priority,
                        ..
                    },
            }) => {
                assert_eq!(task_type, "goal");
                assert_eq!(title, "Ship board");
                assert_eq!(description, "Implement workspace board");
                assert_eq!(workspace, "main");
                assert_eq!(assignee.as_deref(), Some("svarog"));
                assert_eq!(reviewer, "user");
                assert_eq!(priority, "high");
            }
            other => panic!("expected workspace new command, got {other:?}"),
        }
    }

    #[test]
    fn workspace_list_subcommand_parses_filters() {
        let cli = Cli::try_parse_from([
            "zorai",
            "workspace",
            "list",
            "--workspace",
            "main",
            "--status",
            "in-review",
            "--priority",
            "urgent",
            "--assignee",
            "svarog",
            "--reviewer",
            "user",
            "--include-deleted",
        ])
        .expect("workspace list filters should parse");
        match cli.command {
            Some(Commands::Workspace {
                action:
                    WorkspaceAction::List {
                        workspace,
                        include_deleted,
                        status,
                        priority,
                        assignee,
                        reviewer,
                        ..
                    },
            }) => {
                assert_eq!(workspace, "main");
                assert!(include_deleted);
                assert_eq!(status.as_deref(), Some("in-review"));
                assert_eq!(priority.as_deref(), Some("urgent"));
                assert_eq!(assignee.as_deref(), Some("svarog"));
                assert_eq!(reviewer.as_deref(), Some("user"));
            }
            other => panic!("expected workspace list command, got {other:?}"),
        }
    }

    #[test]
    fn workspace_update_subcommand_parses_editable_fields() {
        let cli = Cli::try_parse_from([
            "zorai",
            "workspace",
            "update",
            "wtask_1",
            "--title",
            "New title",
            "--description",
            "New description",
            "--dod",
            "Tests pass",
            "--priority",
            "urgent",
            "--assignee",
            "svarog",
            "--reviewer",
            "none",
        ])
        .expect("workspace update subcommand should parse");
        match cli.command {
            Some(Commands::Workspace {
                action:
                    WorkspaceAction::Update {
                        task_id,
                        title,
                        description,
                        dod,
                        priority,
                        assignee,
                        reviewer,
                        ..
                    },
            }) => {
                assert_eq!(task_id, "wtask_1");
                assert_eq!(title.as_deref(), Some("New title"));
                assert_eq!(description.as_deref(), Some("New description"));
                assert_eq!(dod.as_deref(), Some("Tests pass"));
                assert_eq!(priority.as_deref(), Some("urgent"));
                assert_eq!(assignee.as_deref(), Some("svarog"));
                assert_eq!(reviewer.as_deref(), Some("none"));
            }
            other => panic!("expected workspace update command, got {other:?}"),
        }
    }

    #[test]
    fn workspace_move_subcommand_parses_sort_order() {
        let cli = Cli::try_parse_from([
            "zorai",
            "workspace",
            "move",
            "wtask_1",
            "in-review",
            "--sort-order",
            "4",
        ])
        .expect("workspace move subcommand should parse");
        match cli.command {
            Some(Commands::Workspace {
                action:
                    WorkspaceAction::Move {
                        task_id,
                        status,
                        sort_order,
                        ..
                    },
            }) => {
                assert_eq!(task_id, "wtask_1");
                assert_eq!(status, "in-review");
                assert_eq!(sort_order, Some(4));
            }
            other => panic!("expected workspace move command, got {other:?}"),
        }
    }

    #[test]
    fn stop_subcommand_parses() {
        let cli = Cli::try_parse_from(["zorai", "stop"]).expect("stop subcommand should parse");
        match cli.command {
            Some(Commands::Stop) => {}
            other => panic!("expected stop command, got {other:?}"),
        }
    }

    #[test]
    fn restart_subcommand_parses() {
        let cli =
            Cli::try_parse_from(["zorai", "restart"]).expect("restart subcommand should parse");
        match cli.command {
            Some(Commands::Restart) => {}
            other => panic!("expected restart command, got {other:?}"),
        }
    }

    #[test]
    fn skill_discover_subcommand_accepts_explicit_session() {
        let cli = Cli::try_parse_from([
            "zorai",
            "skill",
            "discover",
            "--session",
            "550e8400-e29b-41d4-a716-446655440000",
            "debug panic",
        ])
        .expect("skill discover subcommand should parse explicit session");

        match cli.command {
            Some(Commands::Skill {
                action:
                    SkillAction::Discover {
                        query,
                        session,
                        limit,
                        ..
                    },
            }) => {
                assert_eq!(query, "debug panic");
                assert_eq!(
                    session.as_deref(),
                    Some("550e8400-e29b-41d4-a716-446655440000")
                );
                assert_eq!(limit, 3);
            }
            other => panic!("parsed unexpected command: {other:?}"),
        }
    }

    #[test]
    fn prompt_help_text_describes_concierge_and_rarog_distinctly() {
        let command = Cli::command();
        let prompt = command
            .find_subcommand("prompt")
            .expect("prompt subcommand should exist");
        let concierge_help = prompt
            .get_arguments()
            .find(|arg| arg.get_id().as_str() == "concierge")
            .and_then(|arg| arg.get_help())
            .map(|help| help.to_string())
            .expect("concierge help should exist");
        let rarog_help = prompt
            .get_arguments()
            .find(|arg| arg.get_id().as_str() == "rarog")
            .and_then(|arg| arg.get_help())
            .map(|help| help.to_string())
            .expect("rarog help should exist");

        assert_eq!(concierge_help, "Inspect the concierge agent prompt");
        assert_eq!(rarog_help, "Inspect the Rarog agent prompt");
    }
}
