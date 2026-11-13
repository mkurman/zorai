use anyhow::{bail, Result};
use clap::Parser;

use crate::cli::{
    AnalyzeAction, AudioAction, Cli, Commands, GenerateAction, GoalAction, InstallTarget,
    MediaRoute, MigrateAction, SettingsAction, ThreadAction,
};
use crate::commands::common::{
    handle_post_setup_action, launch_gui, launch_tui, resolve_dm_target, LaunchTarget,
};
use crate::output::audit::{
    format_timestamp, parse_duration_ago, print_audit_detail, print_audit_row,
};
use crate::output::settings::{flatten_json, is_sensitive_key, resolve_dot_path};
use crate::{client, plugins, setup_wizard, update};

pub(crate) fn audio_speak_args(text: String, voice: Option<String>) -> serde_json::Value {
    let mut args = serde_json::json!({ "input": text });
    if let Some(voice) = voice.filter(|voice| !voice.trim().is_empty()) {
        args["voice"] = serde_json::Value::String(voice);
    }
    args
}

pub(crate) fn audio_transcribe_args(path: String, language: Option<String>) -> serde_json::Value {
    let mut args = serde_json::json!({ "path": path });
    if let Some(language) = language.filter(|language| !language.trim().is_empty()) {
        args["language"] = serde_json::Value::String(language);
    }
    args
}

pub(crate) fn generate_args(
    prompt: String,
    options: &[(&str, Option<serde_json::Value>)],
) -> serde_json::Value {
    let mut args = serde_json::json!({ "prompt": prompt });
    for (key, value) in options {
        match value {
            Some(serde_json::Value::String(text)) if text.trim().is_empty() => {}
            Some(value) => args[*key] = value.clone(),
            None => {}
        }
    }
    args
}

pub(crate) fn with_route(mut args: serde_json::Value, route: MediaRoute) -> serde_json::Value {
    for (key, value) in [("provider", route.provider), ("model", route.model)] {
        if let Some(value) = value.filter(|value| !value.trim().is_empty()) {
            args[key] = serde_json::Value::String(value.trim().to_string());
        }
    }
    args
}

pub(crate) fn should_check_for_updates(command: &Commands) -> bool {
    matches!(
        command,
        Commands::List
            | Commands::Clone { .. }
            | Commands::Kill { .. }
            | Commands::Git { .. }
            | Commands::Audit { .. }
            | Commands::Status
            | Commands::Prompt { .. }
            | Commands::Operation { .. }
            | Commands::Stats
            | Commands::Settings { .. }
            | Commands::Audio { .. }
            | Commands::Generate { .. }
            | Commands::Analyze { .. }
            | Commands::Migrate { .. }
            | Commands::Thread { .. }
            | Commands::Goal { .. }
            | Commands::Workspace { .. }
            | Commands::Dm { .. }
            | Commands::Setup
            | Commands::Ping
            | Commands::StartDaemon
            | Commands::Plugin { .. }
            | Commands::Tool { .. }
            | Commands::Install { .. }
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DefaultStartupAction {
    ShowHelp,
    RunSetup,
    StartDaemonAndRetry,
}

fn default_startup_action(probe: setup_wizard::SetupProbe) -> DefaultStartupAction {
    match probe {
        setup_wizard::SetupProbe::Ready => DefaultStartupAction::ShowHelp,
        setup_wizard::SetupProbe::NeedsSetup => DefaultStartupAction::RunSetup,
        setup_wizard::SetupProbe::DaemonUnavailable => DefaultStartupAction::StartDaemonAndRetry,
    }
}

fn command_startup_action(
    command: &Commands,
    probe: setup_wizard::SetupProbe,
) -> DefaultStartupAction {
    if matches!(
        command,
        Commands::Stop | Commands::Restart | Commands::Upgrade { .. }
    ) {
        return DefaultStartupAction::ShowHelp;
    }

    match probe {
        setup_wizard::SetupProbe::DaemonUnavailable => DefaultStartupAction::StartDaemonAndRetry,
        setup_wizard::SetupProbe::NeedsSetup
            if !matches!(
                command,
                Commands::Setup
                    | Commands::AgentBridge
                    | Commands::DbBridge
                    | Commands::Bridge { .. }
            ) =>
        {
            DefaultStartupAction::RunSetup
        }
        setup_wizard::SetupProbe::NeedsSetup | setup_wizard::SetupProbe::Ready => {
            DefaultStartupAction::ShowHelp
        }
    }
}

pub(super) async fn run_startup_preflight(command: &Commands) -> Result<()> {
    let mut restarted_daemon = false;
    loop {
        match command_startup_action(command, setup_wizard::probe_setup_via_ipc().await) {
            DefaultStartupAction::ShowHelp => break,
            DefaultStartupAction::RunSetup => {
                println!("Zorai setup is required before running this command.\n");
                let action = setup_wizard::run_setup_wizard().await?;
                match handle_post_setup_action(action) {
                    Some(LaunchTarget::Tui) => {
                        println!("\nLaunching TUI...");
                        launch_tui();
                    }
                    Some(LaunchTarget::Gui) => {
                        println!("\nLaunching desktop app...");
                        launch_gui()?;
                    }
                    None => {
                        println!("\nSetup complete. Continuing with the requested command.");
                    }
                }
                break;
            }
            DefaultStartupAction::StartDaemonAndRetry => {
                if restarted_daemon {
                    bail!("daemon is still unreachable after startup retry");
                }
                setup_wizard::ensure_daemon_running().await?;
                restarted_daemon = true;
            }
        }
    }

    Ok(())
}

fn format_direct_message_output(
    response: &client::DirectMessageResponse,
    json: bool,
) -> Result<String> {
    let provider_final_result = response
        .provider_final_result_json
        .as_deref()
        .map(serde_json::from_str::<serde_json::Value>)
        .transpose()?;

    if json {
        return serde_json::to_string_pretty(&serde_json::json!({
            "target": response.target,
            "thread_id": response.thread_id,
            "session_id": response.session_id,
            "response": response.response,
            "provider_final_result": provider_final_result,
        }))
        .map_err(Into::into);
    }

    let mut rendered = response.response.clone();
    rendered.push_str("\n\n");
    rendered.push_str(&format!("thread_id:{}", response.thread_id));
    if let Some(session_id) = response.session_id.as_deref() {
        rendered.push_str(&format!("\nsession_id:{session_id}"));
    }
    if let Some(value) = provider_final_result {
        rendered.push_str("\nprovider_final_result:\n");
        rendered.push_str(&serde_json::to_string_pretty(&value)?);
    }

    Ok(rendered)
}

fn format_migration_output(payload: &serde_json::Value, json: bool) -> Result<String> {
    if json {
        return serde_json::to_string_pretty(payload).map_err(Into::into);
    }

    if payload.get("daemon_only").and_then(|value| value.as_bool()) == Some(true) {
        let mut lines = vec![
            "Zorai migration status".to_string(),
            "runtime: daemon".to_string(),
        ];
        if let Some(sources) = payload.get("sources").and_then(|value| value.as_array()) {
            for source in sources {
                let runtime = source
                    .get("runtime")
                    .and_then(|value| value.as_str())
                    .unwrap_or("unknown");
                let installed = source
                    .get("installed")
                    .and_then(|value| value.as_bool())
                    .unwrap_or(false);
                let config_exists = source
                    .get("config_exists")
                    .and_then(|value| value.as_bool())
                    .unwrap_or(false);
                let path = source
                    .get("default_config_path")
                    .and_then(|value| value.as_str())
                    .unwrap_or("-");
                lines.push(format!(
                    "{runtime}: installed={}, config={}, path={path}",
                    installed, config_exists
                ));
            }
        }
        return Ok(lines.join("\n"));
    }

    if let Some(summary) = payload.get("summary") {
        return Ok(serde_json::to_string_pretty(summary)?);
    }

    let runtime = payload
        .get("runtime")
        .and_then(|value| value.as_str())
        .unwrap_or("migration");
    let dry_run = payload
        .get("dry_run")
        .and_then(|value| value.as_bool())
        .unwrap_or(false);
    let persisted = payload
        .get("persisted")
        .and_then(|value| value.as_bool())
        .unwrap_or(false);
    let asset_count = payload
        .pointer("/asset_summary/count")
        .and_then(|value| value.as_u64())
        .unwrap_or(0);
    Ok(format!(
        "{runtime} migration: dry_run={dry_run}, persisted={persisted}, assets={asset_count}"
    ))
}

fn format_thread_list_output(threads: &[client::AgentThreadRecord], json: bool) -> Result<String> {
    if json {
        return serde_json::to_string_pretty(threads).map_err(Into::into);
    }

    if threads.is_empty() {
        return Ok("No threads found.".to_string());
    }

    let mut rendered = String::new();
    for thread in threads {
        let updated = format_timestamp(thread.updated_at as i64);
        let agent_name = thread.agent_name.as_deref().unwrap_or("unknown");
        rendered.push_str(&format!(
            "{} [{}] {} ({})\n",
            thread.id, updated, thread.title, agent_name
        ));
    }
    Ok(rendered.trim_end().to_string())
}

fn format_thread_detail_output(
    thread: Option<&client::AgentThreadRecord>,
    json: bool,
) -> Result<String> {
    if json {
        return serde_json::to_string_pretty(&thread).map_err(Into::into);
    }

    let Some(thread) = thread else {
        return Ok("Thread not found.".to_string());
    };

    let mut rendered = String::new();
    rendered.push_str(&format!("ID:      {}\n", thread.id));
    rendered.push_str(&format!("Title:   {}\n", thread.title));
    rendered.push_str(&format!(
        "Agent:   {}\n",
        thread.agent_name.as_deref().unwrap_or("unknown")
    ));
    rendered.push_str(&format!(
        "Updated: {}\n",
        format_timestamp(thread.updated_at as i64)
    ));
    rendered.push_str("Messages:\n");
    if thread.messages.is_empty() {
        rendered.push_str("  (none)");
    } else {
        for message in &thread.messages {
            rendered.push_str(&format!(
                "  - {}: {}\n",
                message.role,
                message.content.replace('\n', " ")
            ));
        }
        rendered.truncate(rendered.trim_end().len());
    }
    Ok(rendered)
}

fn format_thread_delete_output(thread_id: &str, deleted: bool) -> String {
    if deleted {
        format!("Deleted thread {}.", thread_id)
    } else {
        format!("Thread {} was not found.", thread_id)
    }
}

fn format_thread_control_output(thread_id: &str, action: &str, ok: bool) -> String {
    match action {
        "stop" => {
            if ok {
                format!("Stopped active stream for thread {}.", thread_id)
            } else {
                format!("No active stream for thread {}.", thread_id)
            }
        }
        "resume" => {
            if ok {
                format!("Requested resume for thread {}.", thread_id)
            } else {
                format!("Thread {} was not resumable.", thread_id)
            }
        }
        _ => {
            if ok {
                format!("Updated thread {}.", thread_id)
            } else {
                format!("Thread {} update failed.", thread_id)
            }
        }
    }
}

fn format_goal_list_output(goals: &[client::AgentGoalRunRecord], json: bool) -> Result<String> {
    if json {
        return serde_json::to_string_pretty(goals).map_err(Into::into);
    }

    if goals.is_empty() {
        return Ok("No goals found.".to_string());
    }

    let mut rendered = String::new();
    for goal in goals {
        let updated = format_timestamp(goal.updated_at as i64);
        rendered.push_str(&format!(
            "{} [{}] {} ({})\n",
            goal.id, updated, goal.title, goal.status
        ));
    }

    Ok(rendered.trim_end().to_string())
}

fn format_goal_detail_output(
    goal: Option<&client::AgentGoalRunRecord>,
    json: bool,
) -> Result<String> {
    if json {
        return serde_json::to_string_pretty(&goal).map_err(Into::into);
    }

    let Some(goal) = goal else {
        return Ok("Goal not found.".to_string());
    };

    let mut rendered = String::new();
    let priority = if goal.priority.is_empty() {
        "unknown"
    } else {
        goal.priority.as_str()
    };
    rendered.push_str(&format!("ID:      {}\n", goal.id));
    rendered.push_str(&format!("Title:   {}\n", goal.title));
    rendered.push_str(&format!("Goal:    {}\n", goal.goal));
    rendered.push_str(&format!("Status:  {}\n", goal.status));
    rendered.push_str(&format!("Priority: {}\n", priority));
    if let Some(stopped_reason) = goal.stopped_reason.as_deref() {
        rendered.push_str(&format!("Stopped: {}\n", stopped_reason));
    }
    if let Some(thread_id) = goal.thread_id.as_deref() {
        rendered.push_str(&format!("Thread:  {}\n", thread_id));
    }
    if let Some(session_id) = goal.session_id.as_deref() {
        rendered.push_str(&format!("Session: {}\n", session_id));
    }
    rendered.push_str(&format!(
        "Updated: {}\n",
        format_timestamp(goal.updated_at as i64)
    ));
    if let Some(duration_ms) = goal.duration_ms {
        rendered.push_str(&format!("Duration: {} ms\n", duration_ms));
    }
    if goal.total_prompt_tokens > 0 || goal.total_completion_tokens > 0 {
        rendered.push_str(&format!(
            "Tokens:  {} prompt / {} completion\n",
            goal.total_prompt_tokens, goal.total_completion_tokens
        ));
        if let Some(cost) = goal.estimated_cost_usd {
            rendered.push_str(&format!("Cost:    ${:.6}\n", cost));
        }
    }
    if !goal.model_usage.is_empty() {
        rendered.push_str("Model Usage:\n");
        for usage in &goal.model_usage {
            let duration = usage
                .duration_ms
                .map(|value| format!(", {} ms", value))
                .unwrap_or_default();
            let cost = usage
                .estimated_cost_usd
                .map(|value| format!(", ${value:.6}"))
                .unwrap_or_default();
            rendered.push_str(&format!(
                "  - {}/{}: {} request(s), {} prompt / {} completion tokens{}{}\n",
                usage.provider,
                usage.model,
                usage.request_count,
                usage.prompt_tokens,
                usage.completion_tokens,
                duration,
                cost
            ));
        }
    }
    if let Some(step_title) = goal.current_step_title.as_deref() {
        rendered.push_str(&format!("Current: {}\n", step_title));
    }
    if let Some(dossier) = goal.dossier.as_ref() {
        rendered.push_str(&format!("Projection: {}\n", dossier.projection_state));
        if let Some(summary) = dossier.summary.as_deref() {
            rendered.push_str(&format!("Dossier: {}\n", summary));
        }
    }

    rendered.push_str("Steps:\n");
    if goal.steps.is_empty() {
        rendered.push_str("  (none)\n");
    } else {
        for step in &goal.steps {
            rendered.push_str(&format!(
                "  - #{} {} [{}]\n",
                step.position, step.title, step.status
            ));
        }
    }

    rendered.push_str("Events:\n");
    if goal.events.is_empty() {
        rendered.push_str("  (none)");
    } else {
        for event in &goal.events {
            rendered.push_str(&format!(
                "  - {} [{}] {}\n",
                format_timestamp(event.timestamp as i64),
                event.phase,
                event.message.replace('\n', " ")
            ));
        }
        rendered.truncate(rendered.trim_end().len());
    }

    Ok(rendered)
}

fn format_goal_dossier_output(
    goal: Option<&client::AgentGoalRunRecord>,
    json: bool,
) -> Result<String> {
    if json {
        return serde_json::to_string_pretty(&goal.and_then(|goal| goal.dossier.as_ref()))
            .map_err(Into::into);
    }

    let Some(goal) = goal else {
        return Ok("Goal not found.".to_string());
    };
    let Some(dossier) = goal.dossier.as_ref() else {
        return Ok("Goal dossier unavailable.".to_string());
    };

    let mut rendered = String::from("Goal Dossier\n============\n");
    rendered.push_str(&format!("ID:         {}\n", goal.id));
    rendered.push_str(&format!("Projection: {}\n", dossier.projection_state));
    if let Some(summary) = dossier.summary.as_deref() {
        rendered.push_str(&format!("Summary:    {}\n", summary));
    }
    if let Some(error) = dossier.projection_error.as_deref() {
        rendered.push_str(&format!("Error:      {}\n", error));
    }
    if let Some(decision) = dossier.latest_resume_decision.as_ref() {
        rendered.push_str(&format!(
            "Latest Decision: {} [{}] {}\n",
            decision.action, decision.projection_state, decision.reason_code
        ));
    }

    rendered.push_str("\nUnits:\n");
    if dossier.units.is_empty() {
        rendered.push_str("  (none)");
        return Ok(rendered);
    }

    for unit in &dossier.units {
        rendered.push_str(&format!("  - {} [{}]\n", unit.title, unit.status));
        rendered.push_str(&format!("    execution: {}\n", unit.execution_binding));
        rendered.push_str(&format!(
            "    verification: {}\n",
            unit.verification_binding
        ));
        if let Some(summary) = unit.summary.as_deref() {
            rendered.push_str(&format!("    summary: {}\n", summary));
        }
        if !unit.proof_checks.is_empty() {
            rendered.push_str(&format!("    proof checks: {}\n", unit.proof_checks.len()));
        }
    }

    Ok(rendered.trim_end().to_string())
}

fn format_goal_proof_output(
    goal: Option<&client::AgentGoalRunRecord>,
    json: bool,
) -> Result<String> {
    if json {
        let proof = goal.and_then(|goal| goal.dossier.as_ref()).map(|dossier| {
            dossier
                .units
                .iter()
                .map(|unit| {
                    serde_json::json!({
                        "unit_id": unit.id,
                        "unit_title": unit.title,
                        "proof_checks": unit.proof_checks,
                        "evidence": unit.evidence,
                    })
                })
                .collect::<Vec<_>>()
        });
        return serde_json::to_string_pretty(&proof).map_err(Into::into);
    }

    let Some(goal) = goal else {
        return Ok("Goal not found.".to_string());
    };
    let Some(dossier) = goal.dossier.as_ref() else {
        return Ok("Goal proof unavailable.".to_string());
    };

    let mut rendered = String::from("Goal Proof\n==========\n");
    rendered.push_str(&format!("ID: {}\n", goal.id));

    let mut has_any = false;
    for unit in &dossier.units {
        if unit.proof_checks.is_empty() && unit.evidence.is_empty() {
            continue;
        }
        has_any = true;
        rendered.push_str(&format!("\n{} ({})\n", unit.title, unit.id));
        for check in &unit.proof_checks {
            rendered.push_str(&format!(
                "  - {} [{}] {}\n",
                check.id, check.state, check.title
            ));
        }
        for evidence in &unit.evidence {
            rendered.push_str(&format!("    evidence: {}", evidence.title));
            if let Some(summary) = evidence.summary.as_deref() {
                rendered.push_str(&format!(" - {}", summary));
            }
            rendered.push('\n');
        }
    }

    if !has_any {
        rendered.push_str("\n(no proof checks or evidence)");
    }

    Ok(rendered.trim_end().to_string())
}

fn format_goal_reports_output(
    goal: Option<&client::AgentGoalRunRecord>,
    json: bool,
) -> Result<String> {
    if json {
        let reports = goal.and_then(|goal| goal.dossier.as_ref()).map(|dossier| {
            serde_json::json!({
                "goal_report": dossier.report,
                "unit_reports": dossier.units.iter().filter_map(|unit| {
                    unit.report.as_ref().map(|report| serde_json::json!({
                        "unit_id": unit.id,
                        "unit_title": unit.title,
                        "report": report,
                    }))
                }).collect::<Vec<_>>(),
            })
        });
        return serde_json::to_string_pretty(&reports).map_err(Into::into);
    }

    let Some(goal) = goal else {
        return Ok("Goal not found.".to_string());
    };
    let Some(dossier) = goal.dossier.as_ref() else {
        return Ok("Goal reports unavailable.".to_string());
    };

    let mut rendered = String::from("Goal Reports\n============\n");
    rendered.push_str(&format!("ID: {}\n", goal.id));
    if let Some(report) = dossier.report.as_ref() {
        rendered.push_str(&format!(
            "Goal Report [{}]: {}\n",
            report.state, report.summary
        ));
    }

    let mut has_unit_reports = false;
    for unit in &dossier.units {
        let Some(report) = unit.report.as_ref() else {
            continue;
        };
        if !has_unit_reports {
            rendered.push_str("\nUnit Reports:\n");
            has_unit_reports = true;
        }
        rendered.push_str(&format!(
            "  - {} [{}]: {}\n",
            unit.title, report.state, report.summary
        ));
    }

    if dossier.report.is_none() && !has_unit_reports {
        rendered.push_str("No reports recorded.");
    }

    Ok(rendered.trim_end().to_string())
}

fn format_goal_control_output(goal_run_id: &str, action: &str, ok: bool) -> String {
    match action {
        "stop" => {
            if ok {
                format!("Stopped goal {}.", goal_run_id)
            } else {
                format!("Goal {} was not stoppable.", goal_run_id)
            }
        }
        "resume" => {
            if ok {
                format!("Resumed goal {}.", goal_run_id)
            } else {
                format!("Goal {} was not resumable.", goal_run_id)
            }
        }
        "accept" => {
            if ok {
                format!("Accepted goal {}.", goal_run_id)
            } else {
                format!("Goal {} was not accepted.", goal_run_id)
            }
        }
        "soft_reject" => {
            if ok {
                format!("Soft-rejected goal {}; worker continues.", goal_run_id)
            } else {
                format!("Goal {} was not soft-rejected.", goal_run_id)
            }
        }
        "hard_reject" => {
            if ok {
                format!("Hard-rejected goal {}.", goal_run_id)
            } else {
                format!("Goal {} was not hard-rejected.", goal_run_id)
            }
        }
        _ => {
            if ok {
                format!("Updated goal {}.", goal_run_id)
            } else {
                format!("Goal {} update failed.", goal_run_id)
            }
        }
    }
}

fn format_goal_delete_output(goal_run_id: &str, deleted: bool) -> String {
    if deleted {
        format!("Deleted goal {}.", goal_run_id)
    } else {
        format!("Goal {} was not found.", goal_run_id)
    }
}

fn pagination_offset(page: usize, limit: usize) -> usize {
    page.saturating_sub(1).saturating_mul(limit)
}

fn format_status_output(
    status: &client::AgentStatusSnapshot,
    current_version: &str,
) -> Result<String> {
    let mut rendered = String::from("Agent Status\n============\n");
    rendered.push_str(&format!("Version:  {current_version}\n"));
    rendered.push_str(&format!("Tier:     {}\n", status.tier.replace('_', " ")));
    rendered.push_str(&format!(
        "Activity: {}\n",
        status.activity.replace('_', " ")
    ));

    if let Some(title) = &status.active_goal_run_title {
        rendered.push_str(&format!("Goal:     {title}\n"));
    }
    if let Some(thread) = &status.active_thread_id {
        rendered.push_str(&format!("Thread:   {thread}\n"));
    }

    if let Ok(providers) = serde_json::from_str::<serde_json::Value>(&status.provider_health_json) {
        if let Some(obj) = providers.as_object() {
            if !obj.is_empty() {
                rendered.push_str("\nProviders:\n");
                for (name, info) in obj {
                    let can_exec = info
                        .get("can_execute")
                        .and_then(|value| value.as_bool())
                        .unwrap_or(true);
                    let trips = info
                        .get("trip_count")
                        .and_then(|value| value.as_u64())
                        .unwrap_or(0);
                    let health = if can_exec { "healthy" } else { "tripped" };
                    if trips > 0 {
                        rendered.push_str(&format!("  {name} - {health} (trips: {trips})\n"));
                    } else {
                        rendered.push_str(&format!("  {name} - {health}\n"));
                    }
                }
            }
        }
    }

    if let Ok(gateways) = serde_json::from_str::<serde_json::Value>(&status.gateway_statuses_json) {
        if let Some(obj) = gateways.as_object() {
            if !obj.is_empty() {
                rendered.push_str("\nGateways:\n");
                for (platform, info) in obj {
                    let gateway_status = info
                        .get("status")
                        .and_then(|value| value.as_str())
                        .unwrap_or("unknown");
                    rendered.push_str(&format!("  {platform} - {gateway_status}\n"));
                }
            }
        }
    }

    if let Ok(actions) = serde_json::from_str::<Vec<serde_json::Value>>(&status.recent_actions_json)
    {
        if !actions.is_empty() {
            rendered.push_str("\nRecent Actions:\n");
            for action in actions.iter().take(5) {
                let action_type = action
                    .get("action_type")
                    .and_then(|value| value.as_str())
                    .unwrap_or("");
                let summary = action
                    .get("summary")
                    .and_then(|value| value.as_str())
                    .unwrap_or("");
                let timestamp = action
                    .get("timestamp")
                    .and_then(|value| value.as_i64())
                    .unwrap_or(0);
                rendered.push_str(&format!(
                    "  {} [{}] {}\n",
                    format_timestamp(timestamp),
                    action_type,
                    summary
                ));
            }
        }
    }

    while rendered.ends_with('\n') {
        rendered.pop();
    }

    Ok(rendered)
}

fn resolve_prompt_target(
    agent: Option<String>,
    weles: bool,
    concierge: bool,
    rarog: bool,
) -> Option<String> {
    if let Some(agent) = agent.map(|value| value.trim().to_string()) {
        if !agent.is_empty() {
            return Some(agent);
        }
    }
    if weles {
        return Some("weles".to_string());
    }
    if concierge || rarog {
        return Some("rarog".to_string());
    }
    None
}

fn format_prompt_output(prompt: &client::AgentPromptInspection, json: bool) -> Result<String> {
    if json {
        return serde_json::to_string_pretty(prompt).map_err(Into::into);
    }

    let final_prompt = prompt.final_prompt.trim();
    let prompt_tokens = final_prompt.chars().count().div_ceil(4);

    let mut rendered = String::from(final_prompt);

    if !prompt.tools.is_empty() {
        let tool_tokens: usize = prompt
            .tools
            .iter()
            .map(|tool| tool.serialized_chars.div_ceil(4))
            .sum();
        rendered.push_str(&format!(
            "\n\n=== Tool Schemas ({} tools, ~{} tokens, exact request payload, largest first) ===\n",
            prompt.tools.len(),
            tool_tokens,
        ));
        for tool in &prompt.tools {
            rendered.push_str(&format!(
                "\n[~{} tokens] {}\n{}\n",
                tool.serialized_chars.div_ceil(4),
                tool.name,
                tool.serialized,
            ));
        }
        rendered.push_str(&format!(
            "\nEstimated tokens: ~{} (system prompt ~{} + {} tool schemas ~{}; chars/4 heuristic)",
            prompt_tokens + tool_tokens,
            prompt_tokens,
            prompt.tools.len(),
            tool_tokens,
        ));
    } else {
        rendered.push_str(&format!(
            "\n\nEstimated tokens: ~{prompt_tokens} (system prompt only; chars/4 heuristic)"
        ));
    }

    Ok(rendered)
}

fn format_operation_status_output(
    snapshot: &zorai_protocol::OperationStatusSnapshot,
    json: bool,
) -> Result<String> {
    if json {
        return serde_json::to_string_pretty(snapshot).map_err(Into::into);
    }

    let mut rendered = String::from("Operation Status\n================\n");
    rendered.push_str(&format!("ID:       {}\n", snapshot.operation_id));
    rendered.push_str(&format!("Kind:     {}\n", snapshot.kind));
    rendered.push_str(&format!(
        "State:    {}\n",
        serde_json::to_string(&snapshot.state)?.trim_matches('"')
    ));
    rendered.push_str(&format!("Revision: {}\n", snapshot.revision));
    if let Some(dedup) = snapshot.dedup.as_deref() {
        rendered.push_str(&format!("Dedup:    {dedup}\n"));
    }

    while rendered.ends_with('\n') {
        rendered.pop();
    }

    Ok(rendered)
}

pub(crate) async fn run_default() -> Result<()> {
    let mut restarted_daemon = false;
    loop {
        match default_startup_action(setup_wizard::probe_setup_via_ipc().await) {
            DefaultStartupAction::ShowHelp => {
                update::print_upgrade_notice_if_available(env!("CARGO_PKG_VERSION")).await;
                Cli::parse_from(["zorai", "--help"]);
                break;
            }
            DefaultStartupAction::RunSetup => {
                println!("Welcome to zorai! Running first-time setup...\n");
                let action = setup_wizard::run_setup_wizard().await?;
                match handle_post_setup_action(action) {
                    Some(LaunchTarget::Tui) => {
                        println!("\nLaunching TUI...");
                        launch_tui();
                    }
                    Some(LaunchTarget::Gui) => {
                        println!("\nLaunching desktop app...");
                        launch_gui()?;
                    }
                    None => {
                        println!("\nSetup complete. Start later with `zorai tui` or `zorai gui`.");
                    }
                }
                break;
            }
            DefaultStartupAction::StartDaemonAndRetry => {
                if restarted_daemon {
                    bail!("daemon is still unreachable after startup retry");
                }
                setup_wizard::ensure_daemon_running().await?;
                restarted_daemon = true;
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        audio_speak_args, audio_transcribe_args, command_startup_action, default_startup_action,
        format_direct_message_output, format_goal_control_output, format_goal_delete_output,
        format_goal_detail_output, format_goal_dossier_output, format_goal_list_output,
        format_goal_proof_output, format_goal_reports_output, format_operation_status_output,
        format_prompt_output, format_status_output, format_thread_control_output,
        format_thread_delete_output, format_thread_detail_output, format_thread_list_output,
        generate_args, with_route, DefaultStartupAction,
    };
    use crate::cli::Commands;
    use crate::client::{
        AgentGoalDeliveryUnitRecord, AgentGoalEvidenceRecord, AgentGoalProofCheckRecord,
        AgentGoalResumeDecisionRecord, AgentGoalRunDossierRecord, AgentGoalRunEventRecord,
        AgentGoalRunModelUsageRecord, AgentGoalRunRecord, AgentGoalRunReportRecord,
        AgentGoalRunStepRecord, AgentPromptInspection, AgentPromptInspectionSection,
        AgentPromptToolInspection, AgentStatusSnapshot, AgentThreadMessageRecord,
        AgentThreadRecord, DirectMessageResponse,
    };
    use crate::setup_wizard::SetupProbe;

    fn sample_status() -> AgentStatusSnapshot {
        AgentStatusSnapshot {
            tier: "mission_control".to_string(),
            activity: "waiting_for_operator".to_string(),
            active_thread_id: Some("thread-1".to_string()),
            active_goal_run_id: None,
            active_goal_run_title: Some("Close release gap".to_string()),
            provider_health_json: r#"{"openai":{"can_execute":true,"trip_count":0}}"#.to_string(),
            gateway_statuses_json: r#"{"slack":{"status":"connected"}}"#.to_string(),
            recent_actions_json:
                r#"[{"action_type":"tool_call","summary":"Ran status","timestamp":1712345678}]"#
                    .to_string(),
        }
    }

    fn sample_prompt_inspection() -> AgentPromptInspection {
        AgentPromptInspection {
            agent_id: "swarog".to_string(),
            agent_name: "Svarog".to_string(),
            provider_id: "openai".to_string(),
            model: "gpt-5.4-mini".to_string(),
            sections: vec![
                AgentPromptInspectionSection {
                    id: "base_prompt".to_string(),
                    title: "Base Prompt".to_string(),
                    content: "Custom operator prompt".to_string(),
                },
                AgentPromptInspectionSection {
                    id: "runtime_identity".to_string(),
                    title: "Runtime Identity".to_string(),
                    content: "## Runtime Identity\n- You are Svarog in zorai.".to_string(),
                },
            ],
            final_prompt:
                "Custom operator prompt\n\n## Runtime Identity\n- You are Svarog in zorai."
                    .to_string(),
            tool_count: Some(2),
            tools_serialized_chars: Some(180),
            tools: vec![
                AgentPromptToolInspection {
                    name: "bash_command".to_string(),
                    serialized:
                        "{\"type\":\"function\",\"function\":{\"name\":\"bash_command\",\"description\":\"Run a shell command in the workspace and return stdout\"}}"
                            .to_string(),
                    serialized_chars: 120,
                },
                AgentPromptToolInspection {
                    name: "read_memory".to_string(),
                    serialized:
                        "{\"type\":\"function\",\"function\":{\"name\":\"read_memory\"}}"
                            .to_string(),
                    serialized_chars: 60,
                },
            ],
        }
    }

    #[test]
    fn default_startup_restarts_daemon_before_considering_setup() {
        assert_eq!(
            default_startup_action(SetupProbe::DaemonUnavailable),
            DefaultStartupAction::StartDaemonAndRetry
        );
    }

    #[test]
    fn default_startup_runs_setup_only_when_config_requires_it() {
        assert_eq!(
            default_startup_action(SetupProbe::NeedsSetup),
            DefaultStartupAction::RunSetup
        );
        assert_eq!(
            default_startup_action(SetupProbe::Ready),
            DefaultStartupAction::ShowHelp
        );
    }

    #[test]
    fn command_startup_runs_setup_before_requested_command_when_config_requires_it() {
        assert_eq!(
            command_startup_action(&Commands::Status, SetupProbe::NeedsSetup),
            DefaultStartupAction::RunSetup
        );
        assert_eq!(
            command_startup_action(&Commands::Status, SetupProbe::DaemonUnavailable),
            DefaultStartupAction::StartDaemonAndRetry
        );
        assert_eq!(
            command_startup_action(&Commands::Setup, SetupProbe::NeedsSetup),
            DefaultStartupAction::ShowHelp
        );
    }

    #[test]
    fn desktop_bridges_skip_the_interactive_setup_so_the_gui_can_run_its_own() {
        for command in [Commands::AgentBridge, Commands::DbBridge] {
            assert_eq!(
                command_startup_action(&command, SetupProbe::NeedsSetup),
                DefaultStartupAction::ShowHelp
            );
            assert_eq!(
                command_startup_action(&command, SetupProbe::DaemonUnavailable),
                DefaultStartupAction::StartDaemonAndRetry
            );
        }
    }

    #[test]
    fn command_startup_does_not_start_daemon_for_upgrade() {
        assert_eq!(
            command_startup_action(
                &Commands::Upgrade {
                    keep_running: false
                },
                SetupProbe::DaemonUnavailable
            ),
            DefaultStartupAction::ShowHelp
        );
    }

    #[test]
    fn direct_message_json_output_embeds_provider_final_result() {
        let rendered = format_direct_message_output(
            &DirectMessageResponse {
                target: "main".to_string(),
                thread_id: "thread-1".to_string(),
                response: "protocol reply".to_string(),
                session_id: Some("session-1".to_string()),
                provider_final_result_json: Some(
                    r#"{"provider":"open_ai_responses","id":"resp_1"}"#.to_string(),
                ),
            },
            true,
        )
        .expect("render json output");

        let value: serde_json::Value =
            serde_json::from_str(&rendered).expect("parse rendered json");
        assert_eq!(value.get("target").and_then(|v| v.as_str()), Some("main"));
        assert_eq!(
            value.get("thread_id").and_then(|v| v.as_str()),
            Some("thread-1")
        );
        assert_eq!(
            value.get("session_id").and_then(|v| v.as_str()),
            Some("session-1")
        );
        assert_eq!(
            value.get("response").and_then(|v| v.as_str()),
            Some("protocol reply")
        );
        assert_eq!(
            value
                .pointer("/provider_final_result/provider")
                .and_then(|v| v.as_str()),
            Some("open_ai_responses")
        );
        assert_eq!(
            value
                .pointer("/provider_final_result/id")
                .and_then(|v| v.as_str()),
            Some("resp_1")
        );
    }

    #[test]
    fn direct_message_plain_output_prints_provider_final_result_block() {
        let rendered = format_direct_message_output(
            &DirectMessageResponse {
                target: "main".to_string(),
                thread_id: "thread-1".to_string(),
                response: "protocol reply".to_string(),
                session_id: None,
                provider_final_result_json: Some(
                    r#"{"provider":"anthropic_message","id":"msg_1"}"#.to_string(),
                ),
            },
            false,
        )
        .expect("render plain output");

        assert!(rendered.contains("protocol reply"));
        assert!(rendered.contains("thread_id:thread-1"));
        assert!(rendered.contains("provider_final_result:"));
        assert!(rendered.contains("\"provider\": \"anthropic_message\""));
        assert!(rendered.contains("\"id\": \"msg_1\""));
    }

    #[test]
    fn thread_list_plain_output_shows_recent_threads() {
        let rendered = format_thread_list_output(
            &[
                AgentThreadRecord {
                    id: "thread-1".to_string(),
                    agent_name: Some("swarog".to_string()),
                    title: "First thread".to_string(),
                    messages: Vec::new(),
                    pinned: false,
                    created_at: 0,
                    updated_at: 1_712_345_678_000,
                    total_input_tokens: 0,
                    total_output_tokens: 0,
                },
                AgentThreadRecord {
                    id: "thread-2".to_string(),
                    agent_name: None,
                    title: "Second thread".to_string(),
                    messages: Vec::new(),
                    pinned: false,
                    created_at: 0,
                    updated_at: 1_712_345_679_000,
                    total_input_tokens: 0,
                    total_output_tokens: 0,
                },
            ],
            false,
        )
        .expect("render thread list");

        assert!(rendered.contains("thread-1"));
        assert!(rendered.contains("First thread"));
        assert!(rendered.contains("swarog"));
        assert!(rendered.contains("thread-2"));
    }

    #[test]
    fn thread_detail_plain_output_shows_messages() {
        let rendered = format_thread_detail_output(
            Some(&AgentThreadRecord {
                id: "thread-1".to_string(),
                agent_name: Some("swarog".to_string()),
                title: "Thread title".to_string(),
                messages: vec![
                    AgentThreadMessageRecord {
                        id: "msg-1".to_string(),
                        role: "user".to_string(),
                        content: "hello".to_string(),
                        timestamp: 1,
                    },
                    AgentThreadMessageRecord {
                        id: "msg-2".to_string(),
                        role: "assistant".to_string(),
                        content: "world".to_string(),
                        timestamp: 2,
                    },
                ],
                pinned: false,
                created_at: 0,
                updated_at: 1_712_345_678_000,
                total_input_tokens: 0,
                total_output_tokens: 0,
            }),
            false,
        )
        .expect("render thread detail");

        assert!(rendered.contains("ID:      thread-1"));
        assert!(rendered.contains("Title:   Thread title"));
        assert!(rendered.contains("Messages:"));
        assert!(rendered.contains("- user: hello"));
        assert!(rendered.contains("- assistant: world"));
    }

    #[test]
    fn thread_delete_output_reports_deleted_state() {
        assert_eq!(
            format_thread_delete_output("thread-1", true),
            "Deleted thread thread-1."
        );
        assert_eq!(
            format_thread_delete_output("thread-1", false),
            "Thread thread-1 was not found."
        );
    }

    #[test]
    fn thread_control_output_reports_stop_and_resume_state() {
        assert_eq!(
            format_thread_control_output("thread-1", "stop", true),
            "Stopped active stream for thread thread-1."
        );
        assert_eq!(
            format_thread_control_output("thread-1", "resume", false),
            "Thread thread-1 was not resumable."
        );
    }

    #[test]
    fn goal_list_plain_output_shows_recent_goals() {
        let rendered = format_goal_list_output(
            &[
                AgentGoalRunRecord {
                    id: "goal-1".to_string(),
                    title: "Deploy release".to_string(),
                    goal: "Ship v1.2".to_string(),
                    status: "running".to_string(),
                    priority: "urgent".to_string(),
                    updated_at: 1_712_345_678_000,
                    current_step_index: 0,
                    max_replans: 3,
                    autonomy_level: "autonomous".to_string(),
                    ..Default::default()
                },
                AgentGoalRunRecord {
                    id: "goal-2".to_string(),
                    title: "Audit logs".to_string(),
                    goal: "Review changes".to_string(),
                    status: "paused".to_string(),
                    priority: "normal".to_string(),
                    updated_at: 1_712_345_679_000,
                    current_step_index: 0,
                    max_replans: 3,
                    autonomy_level: "supervised".to_string(),
                    ..Default::default()
                },
            ],
            false,
        )
        .expect("render goal list");

        assert!(rendered.contains("goal-1"));
        assert!(rendered.contains("Deploy release"));
        assert!(rendered.contains("running"));
        assert!(rendered.contains("goal-2"));
    }

    #[test]
    fn goal_detail_plain_output_shows_steps_and_events() {
        let rendered = format_goal_detail_output(
            Some(&AgentGoalRunRecord {
                id: "goal-1".to_string(),
                title: "Deploy release".to_string(),
                goal: "Ship v1.2".to_string(),
                status: "running".to_string(),
                priority: "urgent".to_string(),
                updated_at: 1_712_345_678_000,
                thread_id: Some("thread-1".to_string()),
                session_id: Some("session-1".to_string()),
                current_step_index: 0,
                current_step_title: Some("Deploy".to_string()),
                replan_count: 1,
                max_replans: 3,
                duration_ms: Some(2500),
                total_prompt_tokens: 100,
                total_completion_tokens: 25,
                estimated_cost_usd: Some(0.00125),
                model_usage: vec![AgentGoalRunModelUsageRecord {
                    provider: "openai".to_string(),
                    model: "gpt-4o".to_string(),
                    request_count: 2,
                    prompt_tokens: 100,
                    completion_tokens: 25,
                    estimated_cost_usd: Some(0.00125),
                    duration_ms: Some(2500),
                }],
                steps: vec![AgentGoalRunStepRecord {
                    id: "step-1".to_string(),
                    position: 0,
                    title: "Deploy".to_string(),
                    instructions: "Run deploy".to_string(),
                    kind: "command".to_string(),
                    success_criteria: "Deployment succeeds".to_string(),
                    session_id: Some("session-1".to_string()),
                    status: "in_progress".to_string(),
                    ..Default::default()
                }],
                events: vec![AgentGoalRunEventRecord {
                    id: "event-1".to_string(),
                    timestamp: 1_712_345_678_000,
                    phase: "control".to_string(),
                    message: "goal run resumed".to_string(),
                    step_index: Some(0),
                    ..Default::default()
                }],
                autonomy_level: "autonomous".to_string(),
                ..Default::default()
            }),
            false,
        )
        .expect("render goal detail");

        assert!(rendered.contains("ID:      goal-1"));
        assert!(rendered.contains("Title:   Deploy release"));
        assert!(rendered.contains("Goal:    Ship v1.2"));
        assert!(rendered.contains("Duration: 2500 ms"));
        assert!(rendered.contains("Tokens:  100 prompt / 25 completion"));
        assert!(rendered.contains("Model Usage:"));
        assert!(rendered.contains("openai/gpt-4o: 2 request(s)"));
        assert!(rendered.contains("Steps:"));
        assert!(rendered.contains("Deploy"));
        assert!(rendered.contains("Events:"));
        assert!(rendered.contains("goal run resumed"));
    }

    #[test]
    fn goal_control_output_reports_stop_resume_and_review() {
        assert_eq!(
            format_goal_control_output("goal-1", "stop", true),
            "Stopped goal goal-1."
        );
        assert_eq!(
            format_goal_control_output("goal-1", "resume", false),
            "Goal goal-1 was not resumable."
        );
        assert_eq!(
            format_goal_control_output("goal-1", "accept", true),
            "Accepted goal goal-1."
        );
    }

    #[test]
    fn goal_dossier_output_prints_units_and_resume_decision() {
        let rendered = format_goal_dossier_output(
            Some(&AgentGoalRunRecord {
                id: "goal-1".to_string(),
                title: "Deploy release".to_string(),
                dossier: Some(AgentGoalRunDossierRecord {
                    projection_state: "in_progress".to_string(),
                    latest_resume_decision: Some(AgentGoalResumeDecisionRecord {
                        action: "stop".to_string(),
                        reason_code: "operator_stop".to_string(),
                        projection_state: "failed".to_string(),
                        ..Default::default()
                    }),
                    summary: Some("Two delivery units active".to_string()),
                    units: vec![AgentGoalDeliveryUnitRecord {
                        id: "unit-1".to_string(),
                        title: "MQTT gateway".to_string(),
                        status: "in_progress".to_string(),
                        execution_binding: "builtin:main".to_string(),
                        verification_binding: "subagent:qa-reviewer".to_string(),
                        proof_checks: vec![AgentGoalProofCheckRecord {
                            id: "VAL-MQTT-001".to_string(),
                            title: "Publishes heartbeats".to_string(),
                            state: "pending".to_string(),
                            ..Default::default()
                        }],
                        ..Default::default()
                    }],
                    ..Default::default()
                }),
                ..Default::default()
            }),
            false,
        )
        .expect("render dossier output");

        assert!(rendered.contains("Goal Dossier"));
        assert!(rendered.contains("Projection: in_progress"));
        assert!(rendered.contains("Latest Decision: stop"));
        assert!(rendered.contains("MQTT gateway"));
        assert!(rendered.contains("builtin:main"));
        assert!(rendered.contains("subagent:qa-reviewer"));
    }

    #[test]
    fn goal_proof_output_prints_checks_and_evidence() {
        let rendered = format_goal_proof_output(
            Some(&AgentGoalRunRecord {
                id: "goal-1".to_string(),
                dossier: Some(AgentGoalRunDossierRecord {
                    units: vec![AgentGoalDeliveryUnitRecord {
                        id: "unit-1".to_string(),
                        title: "MQTT gateway".to_string(),
                        proof_checks: vec![AgentGoalProofCheckRecord {
                            id: "VAL-MQTT-001".to_string(),
                            title: "Publishes heartbeats".to_string(),
                            state: "completed".to_string(),
                            evidence_ids: vec!["ev-1".to_string()],
                            ..Default::default()
                        }],
                        evidence: vec![AgentGoalEvidenceRecord {
                            id: "ev-1".to_string(),
                            title: "Broker log".to_string(),
                            summary: Some("Observed heartbeat publish".to_string()),
                            ..Default::default()
                        }],
                        ..Default::default()
                    }],
                    ..Default::default()
                }),
                ..Default::default()
            }),
            false,
        )
        .expect("render proof output");

        assert!(rendered.contains("Goal Proof"));
        assert!(rendered.contains("VAL-MQTT-001"));
        assert!(rendered.contains("completed"));
        assert!(rendered.contains("Broker log"));
    }

    #[test]
    fn goal_reports_output_prints_goal_and_unit_reports() {
        let rendered = format_goal_reports_output(
            Some(&AgentGoalRunRecord {
                id: "goal-1".to_string(),
                dossier: Some(AgentGoalRunDossierRecord {
                    report: Some(AgentGoalRunReportRecord {
                        summary: "Overall milestone healthy".to_string(),
                        state: "in_progress".to_string(),
                        ..Default::default()
                    }),
                    units: vec![AgentGoalDeliveryUnitRecord {
                        id: "unit-1".to_string(),
                        title: "MQTT gateway".to_string(),
                        report: Some(AgentGoalRunReportRecord {
                            summary: "Worker implemented reconnect logic".to_string(),
                            state: "completed".to_string(),
                            ..Default::default()
                        }),
                        ..Default::default()
                    }],
                    ..Default::default()
                }),
                ..Default::default()
            }),
            false,
        )
        .expect("render reports output");

        assert!(rendered.contains("Goal Reports"));
        assert!(rendered.contains("Overall milestone healthy"));
        assert!(rendered.contains("MQTT gateway"));
        assert!(rendered.contains("Worker implemented reconnect logic"));
    }

    #[test]
    fn goal_delete_output_reports_deleted_state() {
        assert_eq!(
            format_goal_delete_output("goal-1", true),
            "Deleted goal goal-1."
        );
        assert_eq!(
            format_goal_delete_output("goal-1", false),
            "Goal goal-1 was not found."
        );
    }

    #[test]
    fn status_output_prints_current_zorai_version() {
        let rendered = format_status_output(&sample_status(), "1.2.3").expect("render status");

        assert!(rendered.contains("Version:  1.2.3"));
    }

    #[test]
    fn prompt_output_prints_pure_final_prompt_with_exact_tool_schemas() {
        let inspection = sample_prompt_inspection();
        let prompt_tokens = inspection.final_prompt.trim().chars().count().div_ceil(4);

        let rendered = format_prompt_output(&inspection, false).expect("render prompt output");

        assert!(rendered.starts_with("Custom operator prompt"));
        assert!(rendered.contains("- You are Svarog in zorai."));
        assert!(
            !rendered.contains("[Base Prompt]") && !rendered.contains("Final Prompt"),
            "non-JSON output must be the pure system prompt, not the section breakdown"
        );
        assert!(
            rendered.contains("=== Tool Schemas (2 tools, ~45 tokens"),
            "tool schema section must show count and token total: {rendered}"
        );
        assert!(
            rendered.contains("[~30 tokens] bash_command"),
            "each tool must show its own token cost: {rendered}"
        );
        assert!(
            rendered.contains(
                "\"description\":\"Run a shell command in the workspace and return stdout\""
            ),
            "the exact serialized schema must be printed verbatim: {rendered}"
        );
        assert!(
            rendered.contains(&format!(
                "Estimated tokens: ~{} (system prompt ~{} + 2 tool schemas ~45",
                prompt_tokens + 45,
                prompt_tokens,
            )),
            "token estimate must cover prompt and tool schemas: {rendered}"
        );
    }

    #[test]
    fn prompt_output_token_estimate_degrades_without_tool_stats() {
        let mut inspection = sample_prompt_inspection();
        inspection.tool_count = None;
        inspection.tools_serialized_chars = None;
        inspection.tools = Vec::new();
        let prompt_tokens = inspection.final_prompt.trim().chars().count().div_ceil(4);

        let rendered = format_prompt_output(&inspection, false).expect("render prompt output");

        assert!(rendered.contains(&format!(
            "Estimated tokens: ~{prompt_tokens} (system prompt only"
        )));
    }

    #[test]
    fn operation_status_output_prints_fields() {
        let rendered = format_operation_status_output(
            &zorai_protocol::OperationStatusSnapshot {
                operation_id: "op-cli-1".to_string(),
                kind: "managed_command".to_string(),
                dedup: Some("managed:exec-1".to_string()),
                state: zorai_protocol::OperationLifecycleState::Completed,
                revision: 3,
            },
            false,
        )
        .expect("render operation status");

        assert!(rendered.contains("Operation Status"));
        assert!(rendered.contains("ID:       op-cli-1"));
        assert!(rendered.contains("Kind:     managed_command"));
        assert!(rendered.contains("State:    completed"));
        assert!(rendered.contains("Revision: 3"));
        assert!(rendered.contains("Dedup:    managed:exec-1"));
    }

    #[test]
    fn audio_commands_send_only_the_options_given_so_audio_settings_fill_the_rest() {
        assert_eq!(
            audio_speak_args("Hi".to_string(), None),
            serde_json::json!({ "input": "Hi" })
        );
        assert_eq!(
            audio_speak_args("Hi".to_string(), Some("eve".to_string())),
            serde_json::json!({ "input": "Hi", "voice": "eve" })
        );
        assert_eq!(
            audio_transcribe_args("/tmp/a.wav".to_string(), Some(" ".to_string())),
            serde_json::json!({ "path": "/tmp/a.wav" })
        );
    }

    #[test]
    fn a_chosen_provider_and_model_override_the_media_settings_for_one_request_only() {
        let route = |provider: Option<&str>, model: Option<&str>| crate::cli::MediaRoute {
            provider: provider.map(str::to_string),
            model: model.map(str::to_string),
        };
        assert_eq!(
            with_route(
                serde_json::json!({ "prompt": "a fox" }),
                route(Some(" openrouter "), Some("google/gemini-3-pro-image"))
            ),
            serde_json::json!({ "prompt": "a fox", "provider": "openrouter", "model": "google/gemini-3-pro-image" })
        );
        assert_eq!(
            with_route(serde_json::json!({ "input": "Hi" }), route(None, Some(" "))),
            serde_json::json!({ "input": "Hi" })
        );
    }

    #[test]
    fn generate_commands_send_only_the_options_given_so_media_settings_fill_the_rest() {
        assert_eq!(
            generate_args(
                "a fox".to_string(),
                &[
                    ("duration_seconds", Some(5.into())),
                    ("aspect_ratio", Some(" ".into())),
                    ("thread_id", None),
                ],
            ),
            serde_json::json!({ "prompt": "a fox", "duration_seconds": 5 })
        );
    }
}

pub(crate) async fn run(command: Commands) -> Result<()> {
    if should_check_for_updates(&command) {
        update::print_upgrade_notice_if_available(env!("CARGO_PKG_VERSION")).await;
    }

    match command {
        Commands::Tui => {
            launch_tui();
        }
        Commands::Gui => {
            launch_gui()?;
        }
        Commands::List => {
            let sessions = client::list_sessions().await?;
            if sessions.is_empty() {
                println!("No active sessions.");
            } else {
                println!(
                    "{:<38} {:>5} {:>5}  {:>5}  {}",
                    "ID", "COLS", "ROWS", "ALIVE", "CWD"
                );
                for session in sessions {
                    println!(
                        "{:<38} {:>5} {:>5}  {:>5}  {}",
                        session.id,
                        session.cols,
                        session.rows,
                        if session.is_alive { "yes" } else { "no" },
                        session.cwd.unwrap_or_default()
                    );
                }
            }
        }
        Commands::New {
            shell,
            cwd,
            workspace,
        } => {
            let id = client::spawn_session(shell, cwd, workspace).await?;
            println!("Session created: {id}");
        }
        Commands::Attach { id } => {
            println!("Attaching to session {id}...");
            client::attach_session(&id).await?;
        }
        Commands::Clone {
            source,
            cols,
            rows,
            workspace,
            cwd,
        } => {
            let (id, active_command) =
                client::clone_session(&source, workspace, cols, rows, cwd).await?;
            println!("{id}");
            if let Some(command) = active_command {
                println!("active_command:{command}");
            }
        }
        Commands::Kill { id } => {
            client::kill_session(&id).await?;
            println!("Session killed: {id}");
        }
        Commands::Git { path } => {
            let abs_path =
                std::fs::canonicalize(&path).unwrap_or_else(|_| std::path::PathBuf::from(&path));
            let info = client::get_git_status(abs_path.to_string_lossy().to_string()).await?;
            println!("Branch: {}", info.branch.as_deref().unwrap_or("(none)"));
            println!("Dirty:  {}", info.is_dirty);
            println!("Ahead:  {} Behind: {}", info.ahead, info.behind);
            println!(
                "Staged: {} Modified: {} Untracked: {}",
                info.staged, info.modified, info.untracked
            );
        }
        Commands::Audit {
            r#type,
            since,
            detail,
            limit,
        } => {
            let since_ts = since.as_deref().and_then(parse_duration_ago);
            let entries = client::send_audit_query(r#type, since_ts, Some(limit)).await?;

            if let Some(detail_id) = &detail {
                if let Some(entry) = entries.iter().find(|entry| entry.id == *detail_id) {
                    print_audit_detail(entry);
                } else {
                    eprintln!("No audit entry found with ID: {detail_id}");
                }
                return Ok(());
            }

            if entries.is_empty() {
                println!("No audit entries found.");
                return Ok(());
            }

            for entry in &entries {
                print_audit_row(entry);
            }
            println!(
                "\n{} entries shown. Use --detail <id> for full trace.",
                entries.len()
            );
        }
        Commands::Status | Commands::Stats => {
            let status = client::send_status_query().await?;
            println!(
                "{}",
                format_status_output(&status, env!("CARGO_PKG_VERSION"))?
            );
        }
        Commands::Prompt {
            agent,
            weles,
            concierge,
            rarog,
            json,
        } => {
            let inspection =
                client::send_prompt_query(resolve_prompt_target(agent, weles, concierge, rarog))
                    .await?;
            println!("{}", format_prompt_output(&inspection, json)?);
        }
        Commands::Operation { id, json } => {
            let snapshot = client::send_operation_status_query(id).await?;
            println!("{}", format_operation_status_output(&snapshot, json)?);
        }
        Commands::Audio { action } => {
            let content = match action {
                AudioAction::Speak { text, voice, route } => {
                    client::send_text_to_speech(with_route(audio_speak_args(text, voice), route))
                        .await?
                }
                AudioAction::Transcribe {
                    path,
                    language,
                    route,
                } => {
                    client::send_speech_to_text(with_route(
                        audio_transcribe_args(path, language),
                        route,
                    ))
                    .await?
                }
            };
            println!("{content}");
        }
        Commands::Generate { action } => {
            let text = |value: Option<String>| value.map(serde_json::Value::String);
            let content = match action {
                GenerateAction::Image {
                    prompt,
                    size,
                    thread,
                    route,
                } => {
                    client::send_generate_image(with_route(
                        generate_args(prompt, &[("size", text(size)), ("thread_id", text(thread))]),
                        route,
                    ))
                    .await?
                }
                GenerateAction::Video {
                    prompt,
                    duration,
                    aspect_ratio,
                    resolution,
                    thread,
                    route,
                } => {
                    client::send_generate_video(with_route(
                        generate_args(
                            prompt,
                            &[
                                ("duration_seconds", duration.map(Into::into)),
                                ("aspect_ratio", text(aspect_ratio)),
                                ("resolution", text(resolution)),
                                ("thread_id", text(thread)),
                            ],
                        ),
                        route,
                    ))
                    .await?
                }
            };
            println!("{content}");
        }
        Commands::Analyze { action } => {
            let AnalyzeAction::Image {
                path,
                prompt,
                route,
            } = action;
            let mut args = serde_json::json!({ "path": path });
            if let Some(prompt) = prompt.filter(|prompt| !prompt.trim().is_empty()) {
                args["prompt"] = serde_json::Value::String(prompt);
            }
            println!(
                "{}",
                client::send_analyze_image(with_route(args, route)).await?
            );
        }
        Commands::Settings { action } => match action {
            SettingsAction::List => {
                let config = client::send_config_get().await?;
                let mut pairs = Vec::new();
                flatten_json("", &config, &mut pairs);
                if pairs.is_empty() {
                    println!("No configuration found.");
                } else {
                    for (key, value) in &pairs {
                        println!("{key} = {value}");
                    }
                }
            }
            SettingsAction::Get { key } => {
                let config = client::send_config_get().await?;
                match resolve_dot_path(&config, &key) {
                    Some(value) => {
                        if is_sensitive_key(&key) {
                            println!("***");
                        } else if let Some(string_value) = value.as_str() {
                            println!("{string_value}");
                        } else {
                            println!(
                                "{}",
                                serde_json::to_string_pretty(value).unwrap_or_default()
                            );
                        }
                    }
                    None => {
                        eprintln!("Key not found: {key}");
                        std::process::exit(1);
                    }
                }
            }
            SettingsAction::Set { key, value } => {
                let json_pointer = format!("/{}", key.replace('.', "/"));
                let value_json = if serde_json::from_str::<serde_json::Value>(&value).is_ok() {
                    value.clone()
                } else {
                    serde_json::to_string(&value).unwrap_or_else(|_| format!("\"{}\"", value))
                };
                client::send_config_set(json_pointer, value_json).await?;
                println!("{key} = {value}");
            }
        },
        Commands::Migrate { action } => match action {
            MigrateAction::Status { json } => {
                let payload = client::send_external_runtime_migration_status().await?;
                println!("{}", format_migration_output(&payload, json)?);
            }
            MigrateAction::Preview {
                runtime,
                config,
                json,
            } => {
                let payload =
                    client::send_external_runtime_migration_preview(runtime, config).await?;
                println!("{}", format_migration_output(&payload, json)?);
            }
            MigrateAction::Apply {
                runtime,
                config,
                conflict_policy,
                json,
            } => {
                let payload =
                    client::send_external_runtime_migration_apply(runtime, config, conflict_policy)
                        .await?;
                println!("{}", format_migration_output(&payload, json)?);
            }
            MigrateAction::Report {
                runtime,
                limit,
                json,
            } => {
                let payload =
                    client::send_external_runtime_migration_report(runtime, Some(limit)).await?;
                println!("{}", format_migration_output(&payload, json)?);
            }
            MigrateAction::ShadowRun { runtime, json } => {
                let payload = client::send_external_runtime_migration_shadow_run(runtime).await?;
                println!("{}", format_migration_output(&payload, json)?);
            }
        },
        Commands::Thread { action } => match action {
            ThreadAction::List { page, limit, json } => {
                let threads =
                    client::send_thread_list_query(limit, pagination_offset(page, limit)).await?;
                println!("{}", format_thread_list_output(&threads, json)?);
            }
            ThreadAction::Get { thread_id, json } => {
                let thread = client::send_thread_get_query(thread_id).await?;
                println!("{}", format_thread_detail_output(thread.as_ref(), json)?);
            }
            ThreadAction::Stop { thread_id } => {
                let result = client::send_thread_control(thread_id, "stop").await?;
                println!(
                    "{}",
                    format_thread_control_output(&result.thread_id, &result.action, result.ok)
                );
            }
            ThreadAction::Resume { thread_id } => {
                let result = client::send_thread_control(thread_id, "resume").await?;
                println!(
                    "{}",
                    format_thread_control_output(&result.thread_id, &result.action, result.ok)
                );
            }
            ThreadAction::Delete { thread_id, yes } => {
                if !yes {
                    use std::io::{self, Write};
                    print!("Delete thread {thread_id}? [y/N] ");
                    io::stdout().flush()?;
                    let mut answer = String::new();
                    io::stdin().read_line(&mut answer)?;
                    let normalized = answer.trim().to_ascii_lowercase();
                    if normalized != "y" && normalized != "yes" {
                        println!("Aborted.");
                        return Ok(());
                    }
                }
                let result = client::send_thread_delete(thread_id).await?;
                println!(
                    "{}",
                    format_thread_delete_output(&result.thread_id, result.deleted)
                );
            }
        },
        Commands::Goal { action } => match action {
            GoalAction::List { page, limit, json } => {
                let goals =
                    client::send_goal_list_query(limit, pagination_offset(page, limit)).await?;
                println!("{}", format_goal_list_output(&goals, json)?);
            }
            GoalAction::Get { goal_run_id, json } => {
                let goal = client::send_goal_get_query(goal_run_id).await?;
                println!("{}", format_goal_detail_output(goal.as_ref(), json)?);
            }
            GoalAction::Dossier { goal_run_id, json } => {
                let goal = client::send_goal_get_query(goal_run_id).await?;
                println!("{}", format_goal_dossier_output(goal.as_ref(), json)?);
            }
            GoalAction::Proof { goal_run_id, json } => {
                let goal = client::send_goal_get_query(goal_run_id).await?;
                println!("{}", format_goal_proof_output(goal.as_ref(), json)?);
            }
            GoalAction::Reports { goal_run_id, json } => {
                let goal = client::send_goal_get_query(goal_run_id).await?;
                println!("{}", format_goal_reports_output(goal.as_ref(), json)?);
            }
            GoalAction::Stop { goal_run_id } => {
                let result = client::send_goal_control(goal_run_id, "stop").await?;
                println!(
                    "{}",
                    format_goal_control_output(&result.goal_run_id, &result.action, result.ok)
                );
            }
            GoalAction::Resume { goal_run_id } => {
                let result = client::send_goal_control(goal_run_id, "resume").await?;
                println!(
                    "{}",
                    format_goal_control_output(&result.goal_run_id, &result.action, result.ok)
                );
            }
            GoalAction::Accept { goal_run_id } => {
                let result = client::send_goal_control(goal_run_id, "accept").await?;
                println!(
                    "{}",
                    format_goal_control_output(&result.goal_run_id, &result.action, result.ok)
                );
            }
            GoalAction::SoftReject {
                goal_run_id,
                explanation,
            } => {
                let payload = serde_json::json!({ "explanation": explanation }).to_string();
                let result = client::send_goal_control_with_step(
                    goal_run_id,
                    "soft_reject",
                    None,
                    Some(payload),
                )
                .await?;
                println!(
                    "{}",
                    format_goal_control_output(&result.goal_run_id, &result.action, result.ok)
                );
            }
            GoalAction::HardReject {
                goal_run_id,
                explanation,
            } => {
                let payload = serde_json::json!({ "explanation": explanation }).to_string();
                let result = client::send_goal_control_with_step(
                    goal_run_id,
                    "hard_reject",
                    None,
                    Some(payload),
                )
                .await?;
                println!(
                    "{}",
                    format_goal_control_output(&result.goal_run_id, &result.action, result.ok)
                );
            }
            GoalAction::Delete { goal_run_id, yes } => {
                if !yes {
                    use std::io::{self, Write};
                    print!("Delete goal {goal_run_id}? [y/N] ");
                    io::stdout().flush()?;
                    let mut answer = String::new();
                    io::stdin().read_line(&mut answer)?;
                    let normalized = answer.trim().to_ascii_lowercase();
                    if normalized != "y" && normalized != "yes" {
                        println!("Aborted.");
                        return Ok(());
                    }
                }
                let result = client::send_goal_delete(goal_run_id).await?;
                println!(
                    "{}",
                    format_goal_delete_output(&result.goal_run_id, result.deleted)
                );
            }
        },
        Commands::Workspace { .. } => unreachable!("workspace commands are handled before core"),
        Commands::Dm {
            thread,
            session,
            svarog,
            rarog,
            main_target,
            concierge,
            json,
            message,
        } => {
            let content = message.join(" ").trim().to_string();
            if content.is_empty() {
                bail!("message cannot be empty");
            }
            let target = resolve_dm_target(svarog, rarog, main_target, concierge);
            let response = client::send_direct_message(target, thread, content, session).await?;
            println!("{}", format_direct_message_output(&response, json)?);
        }
        Commands::Scrub { text } => {
            let input = if let Some(value) = text {
                value
            } else {
                use std::io::Read;
                let mut buffer = String::new();
                std::io::stdin().read_to_string(&mut buffer)?;
                buffer
            };
            let result = client::scrub_text(input).await?;
            print!("{result}");
        }
        Commands::Setup => {
            let action = setup_wizard::run_setup_wizard().await?;
            match handle_post_setup_action(action) {
                Some(LaunchTarget::Tui) => {
                    println!("\nLaunching TUI...");
                    launch_tui();
                }
                Some(LaunchTarget::Gui) => {
                    println!("\nLaunching desktop app...");
                    launch_gui()?;
                }
                None => {
                    println!("\nSetup complete. Start later with `zorai tui` or `zorai gui`.");
                }
            }
        }
        Commands::Ping => {
            client::ping().await?;
            println!("Daemon is alive (pong).");
        }
        Commands::Stop => {
            update::run_stop()?;
        }
        Commands::Restart => {
            update::run_restart()?;
        }
        Commands::Upgrade { keep_running } => {
            if keep_running {
                update::run_upgrade_keep_running()?;
            } else {
                update::run_upgrade()?;
            }
        }
        Commands::StartDaemon => {
            setup_wizard::ensure_daemon_running().await?;
            println!("Daemon is running.");
        }
        Commands::Install { target } => match target {
            InstallTarget::Plugin { package } => {
                let installed = plugins::install_plugin(&package)?;
                println!(
                    "Installed plugin {}@{}\nentry: {}\nformat: {}",
                    installed.package_name,
                    installed.package_version,
                    installed.entry_path,
                    installed.format
                );
            }
            InstallTarget::Guideline {
                source,
                name,
                force,
            } => {
                let installed =
                    super::guidelines::install_guideline_command(&source, name.as_deref(), force)?;
                println!("Installed guideline: {}", installed.display());
                println!(
                    "Guidelines root: {}",
                    zorai_protocol::zorai_guidelines_dir().display()
                );
            }
        },
        Commands::AgentBridge => {
            client::run_agent_bridge().await?;
        }
        Commands::DbBridge => {
            client::run_db_bridge().await?;
        }
        Commands::Bridge {
            session,
            shell,
            cwd,
            workspace,
            cols,
            rows,
            attach_only,
        } => {
            client::run_bridge(session, shell, cwd, workspace, cols, rows, attach_only).await?;
        }
        Commands::Guideline { .. }
        | Commands::Skill { .. }
        | Commands::Semantic { .. }
        | Commands::Plugin { .. }
        | Commands::Tool { .. } => {
            unreachable!()
        }
    }

    Ok(())
}
