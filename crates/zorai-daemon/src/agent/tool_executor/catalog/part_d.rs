use super::*;
pub(crate) fn add_available_tools_part_d(
    tools: &mut Vec<ToolDefinition>,
    config: &AgentConfig,
    agent_data_dir: &std::path::Path,
    _has_workspace_topology: bool,
) {
    if config.collaboration.enabled {
        tools.push(tool_def(tool_names::BROADCAST_CONTRIBUTION, "Publish a structured subagent contribution into the shared collaboration session for the current parent task.", serde_json::json!({
            "type": "object",
            "properties": {
                "parent_task_id": { "type": "string", "description": "Optional explicit parent task scope" },
                "topic": { "type": "string", "description": "Short topic under discussion" },
                "position": { "type": "string", "description": "Your stance or recommendation" },
                "evidence": { "type": "array", "items": { "type": "string" }, "description": "Supporting evidence bullets" },
                "confidence": { "type": "number", "description": "Confidence 0.0-1.0" }
            },
            "required": ["topic", "position"]
        })));
        tools.push(tool_def(tool_names::READ_PEER_MEMORY, "Read sibling subagent contributions, shared context, disagreements, and consensus for the current parent task.", serde_json::json!({
            "type": "object",
            "properties": {
                "parent_task_id": { "type": "string", "description": "Optional explicit parent task scope" }
            }
        })));
        tools.push(tool_def(tool_names::VOTE_ON_DISAGREEMENT, "Cast a weighted vote on a live subagent disagreement for the current collaboration session.", serde_json::json!({
            "type": "object",
            "properties": {
                "disagreement_id": { "type": "string", "description": "Disagreement ID from read_peer_memory or list_collaboration_sessions" },
                "position": { "type": "string", "description": "Position you vote for" },
                "confidence": { "type": "number", "description": "Optional explicit confidence override in the range 0.0-1.0" }
            },
            "required": ["disagreement_id", "position"]
        })));
        tools.push(tool_def(tool_names::DISPATCH_VIA_BID_PROTOCOL, "Dispatch a collaboration task through the bid protocol and return the resolved primary/reviewer assignment.", serde_json::json!({
            "type": "object",
            "properties": {
                "parent_task_id": { "type": "string", "description": "Parent collaboration task scope" },
                "bids": {
                    "type": "array",
                    "description": "Bid submissions to resolve",
                    "items": {
                        "type": "object",
                        "properties": {
                            "task_id": { "type": "string", "description": "Task submitting the bid" },
                            "confidence": { "type": "number", "description": "Bid confidence 0.0-1.0" },
                            "availability": { "type": "string", "enum": ["available", "busy", "unavailable"], "description": "Availability for ranking" }
                        },
                        "required": ["task_id", "confidence", "availability"]
                    }
                }
            },
            "required": ["parent_task_id", "bids"]
        })));
        tools.push(tool_def(tool_names::LIST_COLLABORATION_SESSIONS, "Inspect live collaboration sessions, contributions, disagreements, and consensus built from subagent work.", serde_json::json!({
            "type": "object",
            "properties": {
                "parent_task_id": { "type": "string", "description": "Optional parent task scope" }
            }
        })));
    }
    tools.push(tool_def(tool_names::LIST_THREADS, "List existing agent threads as lightweight summaries with optional deterministic filters.", serde_json::json!({
        "type": "object",
        "properties": {
            "created_after": { "type": "integer", "minimum": 0, "description": "Include threads created at or after this Unix timestamp in milliseconds" },
            "created_before": { "type": "integer", "minimum": 0, "description": "Include threads created at or before this Unix timestamp in milliseconds" },
            "updated_after": { "type": "integer", "minimum": 0, "description": "Include threads updated at or after this Unix timestamp in milliseconds" },
            "updated_before": { "type": "integer", "minimum": 0, "description": "Include threads updated at or before this Unix timestamp in milliseconds" },
            "agent_name": { "type": "string", "description": "Optional canonical or alias agent filter (case-insensitive)" },
            "title_query": { "type": "string", "description": "Optional case-insensitive substring match against the thread title" },
            "pinned": { "type": "boolean", "description": "Optional pinned-state filter" },
            "include_internal": { "type": "boolean", "description": "Include otherwise hidden WELES and handoff threads when true" },
            "limit": { "type": "integer", "minimum": 0, "description": "Optional maximum number of matching thread summaries to return" },
            "offset": { "type": "integer", "minimum": 0, "description": "Optional number of matching thread summaries to skip before returning results" }
        }
    })));
    tools.push(tool_def(tool_names::GET_THREAD, "Fetch one agent thread and a paged slice of its messages by thread ID, with optional internal-thread access.", serde_json::json!({
        "type": "object",
        "properties": {
            "thread_id": { "type": "string", "description": "Thread ID to fetch" },
            "limit": { "type": "integer", "minimum": 0, "description": "Optional maximum number of messages to return from the most recent end of the thread. Defaults to 5." },
            "offset": { "type": "integer", "minimum": 0, "description": "Optional number of newest messages to skip before applying the limit. Defaults to 0." },
            "include_internal": { "type": "boolean", "description": "Allow access to otherwise hidden WELES and handoff threads when true" }
        },
        "required": ["thread_id"]
    })));
    tools.push(tool_def(tool_names::READ_OFFLOADED_PAYLOAD, "Read an offloaded tool-result payload by payload ID. Thread-shaped JSON payloads default to a compact messages-only view with total/range metadata; set full=true to return the exact raw stored content.", serde_json::json!({
        "type": "object",
        "properties": {
            "payload_id": { "type": "string", "description": "Short handle such as `p1` (preferred) or the full payload id" },
            "message_start": { "type": "integer", "minimum": 0, "description": "Optional absolute start message index for compact thread payloads, inclusive" },
            "message_end": { "type": "integer", "minimum": 0, "description": "Optional absolute end message index for compact thread payloads, exclusive" },
            "start": { "type": "integer", "minimum": 0, "description": "Alias for message_start" },
            "end": { "type": "integer", "minimum": 0, "description": "Alias for message_end" },
            "limit": { "type": "integer", "minimum": 1, "description": "Maximum items or text lines to return for non-thread payloads (default: 20, max: 100)" },
            "offset": { "type": "integer", "minimum": 0, "description": "Zero-based offset for non-thread payload items or text lines (default: 0)" },
            "full": { "type": "boolean", "description": "Return the exact raw stored payload, including full metadata, instead of the default compact view. Defaults to false." }
        },
        "required": ["payload_id"]
    })));
    tools.push(tool_def(tool_names::ENQUEUE_TASK, "Create a daemon-managed background task. Use this for work that should run later, survive disconnects, wait on dependencies, or schedule follow-up actions like reminders and gateway messages.", serde_json::json!({
        "type": "object",
        "properties": {
            "title": { "type": "string", "description": "Short task title" },
            "description": { "type": "string", "description": "Detailed task instructions for the daemon agent" },
            "priority": { "type": "string", "enum": ["low", "normal", "high", "urgent"], "description": "Task priority" },
            "command": { "type": "string", "description": "Optional preferred command or entrypoint" },
            "session": { "type": "string", "description": "Optional preferred terminal session ID or substring" },
            "dependencies": { "type": "array", "items": { "type": "string" }, "description": "Task IDs that must complete first" },
            "scheduled_at": { "type": "integer", "description": "Optional Unix timestamp in milliseconds for when the task may start" },
            "schedule_at": { "type": "string", "description": "Optional RFC3339 timestamp for when the task may start" },
            "delay_seconds": { "type": "integer", "description": "Optional relative delay before the task may start" }
        },
        "required": ["description"]
    })));
    tools.push(tool_def(tool_names::LIST_TASKS, "List daemon-managed background tasks and their status, dependencies, schedule, and recent execution metadata.", serde_json::json!({
        "type": "object",
        "properties": {
            "status": { "type": "string", "enum": ["queued", "in_progress", "awaiting_approval", "blocked", "failed_analyzing", "budget_exceeded", "completed", "failed", "cancelled"] },
            "limit": { "type": "integer", "description": "Maximum number of tasks to return" }
        }
    })));
    tools.push(tool_def(tool_names::SCHEDULE_WAKEUP, "Schedule a wakeup that re-prompts you on this thread after a delay. For goal supervision, set kind=goal_supervision and goal_run_id; goal supervision must be finite and the triggered agent must reassess whether another follow-up is useful. Goal-linked wakeups are cleared automatically when the goal becomes terminal.", serde_json::json!({
        "type": "object",
        "properties": {
            "delay": { "type": "integer", "description": "How long until the wakeup fires (in `unit`s). Minimum 1." },
            "unit": { "type": "string", "enum": ["seconds", "minutes", "hours"], "description": "Time unit for delay (default: minutes)" },
            "repetitions": { "type": "integer", "description": "How many times to fire (default 1). Goal supervision requires exactly 1 so each triggered turn reassesses whether another follow-up is useful." },
            "message": { "type": "string", "description": "What you want to be reminded to do when the wakeup fires" },
            "kind": { "type": "string", "enum": ["generic", "goal_supervision"], "description": "Wakeup policy. Goal supervision is status-aware and finite." },
            "goal_run_id": { "type": "string", "description": "Goal handle such as `release-goal` (preferred) or the full goal id. Required when kind=goal_supervision." }
        },
        "required": ["delay"]
    })));
    tools.push(tool_def(tool_names::CANCEL_WAKEUP, "Cancel a previously scheduled wakeup by its wakeup_id.", serde_json::json!({
        "type": "object",
        "properties": {
            "wakeup_id": { "type": "string", "description": "The id returned by schedule_wakeup" }
        },
        "required": ["wakeup_id"]
    })));
    tools.push(tool_def(tool_names::START_GOAL_RUN, "Start a durable goal run for a long-running objective; it always executes on its own dedicated thread.", serde_json::json!({
        "type": "object",
        "properties": {
            "goal": { "type": "string", "description": "The durable objective to pursue" },
            "title": { "type": "string", "description": "Optional short title" },
            "thread_id": { "type": "string", "description": "Optional source thread for lineage; goal still gets a fresh thread" },
            "session_id": { "type": "string", "description": "Optional session override; defaults to current session" },
            "priority": { "type": "string", "enum": ["low", "normal", "high", "urgent"], "description": "Goal priority" },
            "autonomy_level": { "type": "string", "enum": ["supervised", "aware", "autonomous"], "description": "Optional autonomy override" },
            "requires_approval": { "type": "boolean", "default": false, "description": "Wait for operator approval gates (default: false, self-approved)" },
            "launch_assignments": {
                "type": "array",
                "description": "Optional goal-local assignment snapshot; include every role/persona the goal runner may choose.",
                "items": {
                    "type": "object",
                    "properties": {
                        "role_id": { "type": "string", "description": "Role or persona id, e.g. swarog, reviewer, researcher, mokosh" },
                        "enabled": { "type": "boolean", "description": "Whether this assignment is available" },
                        "provider": { "type": "string", "description": "Provider id for this role" },
                        "model": { "type": "string", "description": "Model id for this role" },
                        "reasoning_effort": { "type": "string", "description": "Optional reasoning effort" },
                        "inherit_from_main": { "type": "boolean", "description": "Inherits from the main assignment" }
                    },
                    "required": ["role_id", "provider", "model"]
                }
            }
        },
        "required": ["goal"]
    })));
    tools.push(tool_def(tool_names::LIST_GOAL_RUNS, "List durable goal runs with their current status, active step metadata, and recent execution state. Returns a paged result with total, limit, offset, returned, and next_offset.", serde_json::json!({
        "type": "object",
        "properties": {
            "limit": { "type": "integer", "minimum": 1, "description": "Maximum number of goal runs to return (default: 20, max: 100)" },
            "offset": { "type": "integer", "minimum": 0, "description": "Zero-based pagination offset over goal runs sorted by updated_at descending (default: 0)" }
        }
    })));
    tools.push(tool_def(tool_names::REQUEST_GOAL_REVIEW, "Worker-only: claim this goal is complete and block until the owner supervisor verdicts. The goal is not done until they accept.", serde_json::json!({
        "type": "object",
        "properties": {
            "report": { "type": "string", "description": "Concrete report of what was delivered" },
            "goal_run_id": { "type": "string", "description": "Optional guard. Goal handle such as `release-goal` (preferred) or the full goal id; must match the active goal worker" }
        },
        "required": ["report"]
    })));
    tools.push(tool_def(tool_names::SUBMIT_GOAL_REVIEW, "Owner-supervisor only: verdict a worker completeness report. accept completes the goal, soft_reject continues the same worker with your explanation, hard_reject stops the goal and unpins supervision.", serde_json::json!({
        "type": "object",
        "properties": {
            "verdict": { "type": "string", "enum": ["accept", "soft_reject", "hard_reject"] },
            "explanation": { "type": "string", "description": "Required for soft_reject and hard_reject" },
            "goal_run_id": { "type": "string", "description": "Goal handle such as `release-goal` (preferred) or the full goal id" }
        },
        "required": ["verdict", "goal_run_id"]
    })));
    tools.push(tool_def(tool_names::CREATE_ROUTINE, "Create a durable routine definition with a schedule and target payload; it does not execute immediately.", serde_json::json!({
        "type": "object",
        "properties": {
            "id": { "type": "string", "description": "Optional explicit routine id" },
            "title": { "type": "string", "description": "Routine title" },
            "description": { "type": "string", "description": "What the routine is for" },
            "enabled": { "type": "boolean", "description": "Starts enabled (default: true)" },
            "paused_at": { "type": ["integer", "null"], "description": "Optional paused timestamp, Unix ms" },
            "schedule_expression": { "type": "string", "description": "Cron-like schedule expression" },
            "target_kind": { "type": "string", "enum": ["task", "goal", "tool"], "description": "What the routine materializes when executed" },
            "target_payload": { "type": "object", "description": "JSON payload describing the target work" },
            "next_run_at": { "type": ["integer", "null"], "description": "Optional next run, Unix ms" },
            "last_run_at": { "type": ["integer", "null"], "description": "Optional last run, Unix ms" }
        },
        "required": ["title", "description", "schedule_expression", "target_kind", "target_payload"]
    })));
    tools.push(tool_def(tool_names::LIST_ROUTINES, "List durable routine definitions and their stored scheduling state. This surface lists routine objects only; it does not execute them.", serde_json::json!({
        "type": "object",
        "properties": {}
    })));
    tools.push(tool_def(tool_names::GET_ROUTINE, "Fetch one durable routine definition by id, including recent run history and summary state.", serde_json::json!({
        "type": "object",
        "properties": {
            "routine_id": { "type": "string", "description": "Routine definition id" }
        },
        "required": ["routine_id"]
    })));
    tools.push(tool_def(tool_names::PREVIEW_ROUTINE, "Preview a stored routine without mutation. Shows next fire times, materialized payload, delivery fan-out, and approval posture.", serde_json::json!({
        "type": "object",
        "properties": {
            "routine_id": { "type": "string", "description": "Routine definition id to preview" },
            "fire_count": { "type": "integer", "description": "How many upcoming fire times to project (default: 3)" }
        },
        "required": ["routine_id"]
    })));
    tools.push(tool_def(tool_names::UPDATE_ROUTINE, "Update a durable routine definition in place; validates and recomputes schedule state.", serde_json::json!({
        "type": "object",
        "properties": {
            "routine_id": { "type": "string", "description": "Routine definition id" },
            "title": { "type": "string", "description": "Updated title" },
            "description": { "type": "string", "description": "Updated description" },
            "enabled": { "type": "boolean", "description": "Whether the routine remains enabled" },
            "paused_at": { "type": ["integer", "null"], "description": "Paused timestamp Unix ms, or null to clear" },
            "schedule_expression": { "type": "string", "description": "Updated cron-like schedule expression" },
            "target_kind": { "type": "string", "enum": ["task", "goal", "tool"], "description": "Updated target kind" },
            "target_payload": { "type": "object", "description": "Updated target payload" },
            "next_run_at": { "type": ["integer", "null"], "description": "Next run Unix ms, or null to clear" },
            "last_run_at": { "type": ["integer", "null"], "description": "Last run Unix ms, or null to clear" }
        },
        "required": ["routine_id"]
    })));
    tools.push(tool_def(
        tool_names::RUN_ROUTINE_NOW,
        "Execute one stored routine immediately and record an explicit manual run history entry.",
        serde_json::json!({
            "type": "object",
            "properties": {
                "routine_id": { "type": "string", "description": "Routine definition id" }
            },
            "required": ["routine_id"]
        }),
    ));
    tools.push(tool_def(tool_names::LIST_ROUTINE_HISTORY, "List recent persisted run attempts for one routine, including success, failure, run-now, and rerun entries.", serde_json::json!({
        "type": "object",
        "properties": {
            "routine_id": { "type": "string", "description": "Routine definition id" },
            "limit": { "type": "integer", "description": "Maximum number of routine runs to return" }
        },
        "required": ["routine_id"]
    })));
    tools.push(tool_def(tool_names::RERUN_ROUTINE, "Rerun a prior routine attempt from its last materialized payload and record a linked rerun history entry.", serde_json::json!({
        "type": "object",
        "properties": {
            "run_id": { "type": "string", "description": "Routine run id to rerun from" }
        },
        "required": ["run_id"]
    })));
    tools.push(tool_def(tool_names::PAUSE_ROUTINE, "Pause one durable routine definition by id so due checks stop materializing it until resumed.", serde_json::json!({
        "type": "object",
        "properties": {
            "routine_id": { "type": "string", "description": "Routine definition id" }
        },
        "required": ["routine_id"]
    })));
    tools.push(tool_def(tool_names::RESUME_ROUTINE, "Resume one paused durable routine definition by id so due checks can materialize it again.", serde_json::json!({
        "type": "object",
        "properties": {
            "routine_id": { "type": "string", "description": "Routine definition id" }
        },
        "required": ["routine_id"]
    })));
    tools.push(tool_def(
        tool_names::DELETE_ROUTINE,
        "Delete one durable routine definition by id.",
        serde_json::json!({
            "type": "object",
            "properties": {
                "routine_id": { "type": "string", "description": "Routine definition id" }
            },
            "required": ["routine_id"]
        }),
    ));
    tools.push(tool_def(tool_names::RUN_WORKFLOW_PACK, "Execute one canonical workflow pack with prerequisite- and approval-aware behavior. Packs: daily-brief, pr-issue-triage, inbox-calendar-triage, watch-monitor, standup, approval-checkpoint-long-task.", serde_json::json!({
        "type": "object",
        "properties": {
            "pack_name": { "type": "string", "description": "Canonical pack name" },
            "mode": { "type": "string", "description": "Optional mode: standard, quiet, or executive" },
            "workspace_id": { "type": "string", "description": "Optional workspace id (default: main)" },
            "delivery_channel": { "type": "string", "description": "Optional channel: in-app, slack, discord, telegram, or whatsapp" },
            "deliver_now": { "type": "boolean", "description": "Deliver externally now instead of in-app only" },
            "repo_connector": { "type": "string", "description": "Optional repo connector: github or gitlab" },
            "tracker_connector": { "type": "string", "description": "Optional tracker: linear, jira, or none" },
            "task_kind": { "type": "string", "enum": ["task", "goal"], "description": "approval-checkpoint-long-task: materialize task or goal" },
            "watch_source": { "type": "string", "description": "watch-monitor source: event, repo, webpage, or connector resource" },
            "payload": { "type": "object", "description": "Optional event/source payload forwarded into execution" }
        },
        "required": ["pack_name"]
    })));
    tools.push(tool_def(tool_names::LIST_TRIGGERS, "List configured event triggers with status, cooldown, last-fired metadata, and whether each trigger comes from packaged defaults or a custom entry. On a fresh engine, packaged defaults are seeded automatically before listing.", serde_json::json!({
        "type": "object",
        "properties": {}
    })));
    tools.push(tool_def(tool_names::INGEST_WEBHOOK_EVENT, "Validate a webhook-style event payload and route it through the trigger engine. On a fresh engine, packaged defaults are seeded automatically before routing.", serde_json::json!({
        "type": "object",
        "properties": {
            "event_family": { "type": "string", "description": "Event family, e.g. filesystem or system" },
            "event_kind": { "type": "string", "description": "Event kind, e.g. file_changed or disk_pressure" },
            "state": { "type": "string", "description": "Optional state filter, e.g. detected or critical" },
            "thread_id": { "type": "string", "description": "Optional thread scope" },
            "payload": { "type": "object", "description": "Optional payload for template rendering and logging" }
        },
        "required": ["event_family", "event_kind"]
    })));
    tools.push(tool_def(tool_names::ADD_TRIGGER, "Create, validate, and persist a runtime event trigger; successful creations return metadata with source: custom. Pack 1 defaults already cover common health, filesystem, and system events.", serde_json::json!({
        "type": "object",
        "properties": {
            "id": { "type": "string", "description": "Optional explicit trigger id" },
            "event_family": { "type": "string", "description": "Event family, e.g. health, filesystem, or system" },
            "event_kind": { "type": "string", "description": "Event kind, e.g. weles_health, file_changed, or disk_pressure" },
            "agent_id": { "type": "string", "description": "Handling agent/subagent (default: weles)" },
            "target_state": { "type": "string", "description": "Optional state filter, e.g. degraded or critical" },
            "thread_id": { "type": "string", "description": "Optional thread scope filter" },
            "enabled": { "type": "boolean", "description": "Starts enabled (default: true)" },
            "cooldown_secs": { "type": "integer", "description": "Per-trigger cooldown in seconds" },
            "risk_label": { "type": "string", "enum": ["low", "medium", "high"], "description": "Risk label for routing/approval posture" },
            "notification_kind": { "type": "string", "description": "WorkflowNotice kind emitted on fire" },
            "prompt_template": { "type": "string", "description": "Optional task prompt template; when set, queues real work" },
            "tool_name": { "type": "string", "description": "Optional daemon tool to run on fire, e.g. run_workflow_pack" },
            "tool_payload": { "type": "object", "description": "Optional static JSON merged into tool execution" },
            "title_template": { "type": "string", "description": "Notice title template" },
            "body_template": { "type": "string", "description": "Notice body template" }
        },
        "required": ["event_family", "event_kind", "notification_kind", "title_template", "body_template"]
    })));
    tools.push(tool_def(tool_names::GET_COST_SUMMARY, "Get a cost and activity replay summary across a time window. Shows token usage, cost breakdown by provider/model, recent task/routine/trigger activity, and replay guidance for drilling into specific threads and tasks.", serde_json::json!({
        "type": "object",
        "properties": {
            "window": { "type": "string", "description": "Time window for cost aggregation: today, last7days (default), last30days, or all" }
        }
    })));
    tools.push(tool_def(tool_names::LIST_BROWSER_PROFILES, "List stored named browser profiles with health state, last-used metadata, and browser compatibility hints. Supports optional health-state and workspace filters.", serde_json::json!({
        "type": "object",
        "properties": {
            "health_state": { "type": "string", "enum": ["healthy", "stale", "expired", "corrupted", "repair_needed", "repair_in_progress", "retired"], "description": "Optional health-state filter" },
            "workspace_id": { "type": "string", "description": "Optional workspace scope filter" }
        }
    })));
    tools.push(tool_def(tool_names::CREATE_BROWSER_PROFILE, "Create or update a named browser profile for reuse across browsing tasks; returns persisted metadata.", serde_json::json!({
        "type": "object",
        "properties": {
            "profile_id": { "type": "string", "description": "Stable profile identifier, e.g. 'main-work'" },
            "label": { "type": "string", "description": "Human-readable label" },
            "profile_dir": { "type": "string", "description": "Path to the browser profile directory" },
            "browser_kind": { "type": "string", "description": "Optional: chrome or chromium" },
            "workspace_id": { "type": "string", "description": "Optional workspace scope" }
        },
        "required": ["profile_id", "label", "profile_dir"]
    })));
    tools.push(tool_def(tool_names::UPDATE_BROWSER_PROFILE_HEALTH, "Set a named browser profile's health state to signal freshness, expiry, corruption, or repair progress.", serde_json::json!({
        "type": "object",
        "properties": {
            "profile_id": { "type": "string", "description": "Stable profile identifier" },
            "health_state": { "type": "string", "enum": ["healthy", "stale", "expired", "corrupted", "repair_needed", "repair_in_progress", "retired"], "description": "New health state" },
            "last_auth_success_at": { "type": "integer", "description": "Last successful auth, Unix ms" },
            "last_auth_failure_at": { "type": "integer", "description": "Last auth failure, Unix ms" },
            "last_auth_failure_reason": { "type": "string", "description": "Optional failure reason" }
        },
        "required": ["profile_id", "health_state"]
    })));
    tools.push(tool_def(tool_names::LIST_TRIGGER_FIRE_HISTORY, "List recent trigger fire events with status, retry count, and error details. Supports filtering by trigger ID and/or status (fired, succeeded, failed, suppressed, dead_letter).", serde_json::json!({
        "type": "object",
        "properties": {
            "trigger_id": { "type": "string", "description": "Optional trigger ID filter" },
            "status": { "type": "string", "enum": ["fired", "succeeded", "failed", "suppressed", "dead_letter"], "description": "Optional fire status filter" },
            "limit": { "type": "integer", "description": "Maximum results to return (default: 20)" }
        }
    })));
    tools.push(tool_def(tool_names::SHOW_DREAMS, "Show recent dream-state cycles, counterfactual evaluations, and persisted [dream] strategy hints.", serde_json::json!({
        "type": "object",
        "properties": {
            "limit": { "type": "integer", "description": "Maximum number of recent dream hints/cycles to return" }
        }
    })));
    tools.push(tool_def(tool_names::SHOW_HARNESS_STATE, "Show the persisted state-transition harness projection (beliefs, tensions, commitments, effects, verifications, procedures) for a thread/goal/task scope.", serde_json::json!({
        "type": "object",
        "properties": {
            "thread_id": { "type": "string", "description": "Optional; defaults to the current thread" },
            "goal_run_id": { "type": "string", "description": "Optional goal-run scope" },
            "task_id": { "type": "string", "description": "Optional; defaults to the current task" },
            "limit": { "type": "integer", "description": "Max recent items per section" }
        }
    })));
    tools.push(tool_def(tool_names::IMPORT_EXTERNAL_RUNTIME, "Import Hermes/OpenClaw migration data into persisted import sessions and assets; supports dry-run and conflict policies.", serde_json::json!({
        "type": "object",
        "properties": {
            "runtime": { "type": "string", "description": "Runtime to import: hermes or openclaw" },
            "config_path": { "type": "string", "description": "Optional config path override" },
            "dry_run": { "type": "boolean", "description": "Project the import without mutating state" },
            "conflict_policy": { "type": "string", "enum": ["skip", "merge", "replace", "stage_for_review"], "description": "Conflict handling for imported assets" }
        },
        "required": ["runtime"]
    })));
    tools.push(tool_def(tool_names::SHOW_IMPORT_REPORT, "Show the persisted import report for Hermes/OpenClaw runtime-profile migration data, including imported config summaries and zorai MCP readiness.", serde_json::json!({
        "type": "object",
        "properties": {
            "runtime": { "type": "string", "description": "Optional runtime filter such as hermes or openclaw" },
            "limit": { "type": "integer", "description": "Maximum number of imported runtime profiles to include" }
        }
    })));
    tools.push(tool_def(tool_names::PREVIEW_SHADOW_RUN, "Preview an isolated shadow-run comparison for one imported Hermes/OpenClaw runtime profile against current zorai defaults. This is read-only and does not enqueue tasks, launch runners, or spawn sessions.", serde_json::json!({
        "type": "object",
        "properties": {
            "runtime": { "type": "string", "description": "Imported runtime to compare, such as hermes or openclaw" }
        },
        "required": ["runtime"]
    })));
    tools.push(tool_def(
        tool_names::CANCEL_TASK,
        "Cancel a queued, blocked, running, approval-pending, or retrying background task by ID. Also accepts an operation_id from a backgrounded command: kills the background process, removes the queued command, or interrupts the terminal session running it.",
        serde_json::json!({
            "type": "object",
            "properties": {
                "task_id": { "type": "string", "description": "Task or operation handle such as `researcher` or `op1` (preferred), or the full id" }
            },
            "required": ["task_id"]
        }),
    ));
    tools.push(tool_def(tool_names::TYPE_IN_TERMINAL, "Type text into an existing terminal session as raw keyboard input. Use this for: interactive TUI programs (codex, vim, htop), REPLs (python, node), typing commands in running shells, or any program that needs a real TTY. Text and Enter are sent with a small delay between them so TUIs process correctly. You can also send special keys like ctrl+c, escape, tab, arrow keys, etc.", serde_json::json!({
        "type": "object",
        "properties": {
            "text": { "type": "string", "description": "Text to type into the terminal" },
            "press_enter": { "type": "boolean", "description": "Whether to press Enter after typing (default: true)" },
            "key": { "type": "string", "description": "Send a special key instead of text. Options: enter, ctrl+c, ctrl+d, ctrl+z, ctrl+l, ctrl+a, ctrl+e, ctrl+u, ctrl+k, escape, tab, up, down, left, right, backspace, delete, home, end, page_up, page_down. When 'key' is set, 'text' is ignored." },
            "pane": { "type": "string", "description": "Pane ID or name (optional, defaults to first active session)" }
        },
        "required": []
    })));

    tools.push(tool_def(
        tool_names::LIST_WORKSPACES,
        "List workspaces, surfaces, and panes (with names and IDs).",
        serde_json::json!({"type":"object","properties":{}}),
    ));
    tools.push(tool_def(
        tool_names::CREATE_WORKSPACE,
        "Create a new workspace and make it active.",
        serde_json::json!({
            "type": "object",
            "properties": { "name": { "type": "string", "description": "Optional workspace name" } }
        }),
    ));
    tools.push(tool_def(tool_names::SET_ACTIVE_WORKSPACE, "Set the active workspace by ID or name.", serde_json::json!({
        "type": "object",
        "properties": { "workspace": { "type": "string", "description": "Workspace ID or name" } },
        "required": ["workspace"]
    })));
    tools.push(tool_def(
        tool_names::CREATE_SURFACE,
        "Create a new surface (tab) in a workspace.",
        serde_json::json!({
            "type": "object",
            "properties": {
                "workspace": { "type": "string", "description": "Optional workspace ID or name" },
                "name": { "type": "string", "description": "Optional surface name" }
            }
        }),
    ));
    tools.push(tool_def(
        tool_names::SET_ACTIVE_SURFACE,
        "Set active surface by ID or name.",
        serde_json::json!({
            "type": "object",
            "properties": {
                "surface": { "type": "string", "description": "Surface ID or name" },
                "workspace": { "type": "string", "description": "Optional workspace scope" }
            },
            "required": ["surface"]
        }),
    ));
    tools.push(tool_def(tool_names::SPLIT_PANE, "Split a pane horizontally or vertically. Works in BSP layout mode. In canvas mode, creates a new panel instead.", serde_json::json!({
        "type": "object",
        "properties": {
            "direction": { "type": "string", "enum": ["horizontal", "vertical"] },
            "pane": { "type": "string", "description": "Optional pane ID or name" },
            "new_pane_name": { "type": "string", "description": "Optional name for new pane" }
        },
        "required": ["direction"]
    })));
    tools.push(tool_def(
        tool_names::RENAME_PANE,
        "Rename a pane.",
        serde_json::json!({
            "type": "object",
            "properties": {
                "pane": { "type": "string", "description": "Optional pane ID or name" },
                "name": { "type": "string", "description": "New pane name" }
            },
            "required": ["name"]
        }),
    ));
    tools.push(tool_def(tool_names::SET_LAYOUT_PRESET, "Apply a layout preset to a surface.", serde_json::json!({
        "type": "object",
        "properties": {
            "preset": { "type": "string", "enum": ["single", "2-columns", "3-columns", "grid-2x2", "main-stack"] },
            "surface": { "type": "string", "description": "Optional surface ID or name" },
            "workspace": { "type": "string", "description": "Optional workspace scope" }
        },
        "required": ["preset"]
    })));
    tools.push(tool_def(
        tool_names::EQUALIZE_LAYOUT,
        "Equalize all split ratios in a surface.",
        serde_json::json!({
            "type": "object",
            "properties": {
                "surface": { "type": "string", "description": "Optional surface ID or name" },
                "workspace": { "type": "string", "description": "Optional workspace scope" }
            }
        }),
    ));
    tools.push(tool_def(
        tool_names::LIST_SNIPPETS,
        "List saved snippets with names and content previews.",
        serde_json::json!({
            "type": "object",
            "properties": { "owner": { "type": "string", "enum": ["user", "assistant", "both"] } }
        }),
    ));
    tools.push(tool_def(
        tool_names::CREATE_SNIPPET,
        "Create a new snippet.",
        serde_json::json!({
            "type": "object",
            "properties": {
                "name": { "type": "string" },
                "content": { "type": "string" },
                "category": { "type": "string" },
                "description": { "type": "string" },
                "tags": { "type": "array", "items": { "type": "string" } }
            },
            "required": ["name", "content"]
        }),
    ));
    tools.push(tool_def(tool_names::RUN_SNIPPET, "Execute a snippet by ID or name in a pane.", serde_json::json!({
        "type": "object",
        "properties": {
            "snippet": { "type": "string", "description": "Snippet ID or name" },
            "pane": { "type": "string", "description": "Optional pane ID or name" },
            "params": { "type": "object", "additionalProperties": { "type": "string" } },
            "execute": { "type": "boolean", "description": "Append Enter after inserting (default: true)" }
        },
        "required": ["snippet"]
    })));

    if config.tool_synthesis.enabled {
        tools.push(tool_def(tool_names::SYNTHESIZE_TOOL, "Generate a guarded runtime tool from a CLI --help surface or GET OpenAPI operation and register it locally.", serde_json::json!({
            "type": "object",
            "properties": {
                "kind": { "type": "string", "enum": ["cli", "openapi"], "description": "Source kind (default: cli)" },
                "target": { "type": "string", "description": "CLI invocation or OpenAPI spec URL" },
                "name": { "type": "string", "description": "Optional tool name override" },
                "operation_id": { "type": "string", "description": "Optional OpenAPI operationId" },
                "activate": { "type": "boolean", "description": "Activate immediately when policy allows" }
            },
            "required": ["target"]
        })));
        tools.push(tool_def(
            tool_names::LIST_GENERATED_TOOLS,
            "List generated runtime tools with status, effectiveness, and promotion metadata.",
            serde_json::json!({
                "type": "object",
                "properties": {}
            }),
        ));
        tools.push(tool_def(tool_names::PROMOTE_GENERATED_TOOL, "Promote a generated runtime tool into the generated skills library when it proves useful.", serde_json::json!({
            "type": "object",
            "properties": {
                "tool": { "type": "string", "description": "Generated tool ID" }
            },
            "required": ["tool"]
        })));
        tools.push(tool_def(tool_names::ACTIVATE_GENERATED_TOOL, "Activate a newly synthesized runtime tool after review so it can appear in the callable tool surface on the next turn.", serde_json::json!({
            "type": "object",
            "properties": {
                "tool": { "type": "string", "description": "Generated tool ID" }
            },
            "required": ["tool"]
        })));
        tools.push(tool_def(tool_names::RESTORE_GENERATED_TOOL, "Restore an archived generated runtime tool back to active status without promoting it.", serde_json::json!({
            "type": "object",
            "properties": {
                "tool": { "type": "string", "description": "Generated tool ID" }
            },
            "required": ["tool"]
        })));
        tools.extend(generated_tool_definitions(config, agent_data_dir));
    }

    tools.push(tool_def(
        tool_names::PLUGIN_API_CALL,
        "Call a plugin API endpoint. The daemon proxies the HTTP request, handles auth, rate limiting, and returns the response as text.",
        serde_json::json!({
            "type": "object",
            "properties": {
                "plugin_name": { "type": "string", "description": "Name of the installed plugin" },
                "endpoint_name": { "type": "string", "description": "Name of the API endpoint from the plugin manifest" },
                "params": { "type": "object", "description": "Parameters passed to the endpoint template (optional)" }
            },
            "required": ["plugin_name", "endpoint_name"]
        }),
    ));
    tools.push(tool_def(tool_names::ASK_PARENT, "Ask the parent task a blocking question. Only available inside child tasks that have a parent task or parent thread. Blocks the child task until the parent answers, the ask times out, or a default is applied. Batch related questions into one call; at most 5 open asks are kept per child.", serde_json::json!({
        "type": "object",
        "properties": {
            "question": { "type": "string", "description": "The blocking question for the parent" },
            "options": { "type": "array", "items": { "type": "string" }, "description": "Optional answer choices to help the parent answer quickly" },
            "timeout_minutes": { "type": "integer", "minimum": 1, "description": "Minutes to wait before the ask times out (default 240)" },
            "default": { "type": "string", "description": "Answer to auto-apply when the timeout elapses without a parent answer" }
        },
        "required": ["question"]
    })));
    tools.push(tool_def(tool_names::ANSWER_CHILD, "Answer one of a child task's open ask_parent questions. Validates that the caller is the child's parent (parent task or parent thread). Pass ask_id when the child has multiple open asks; the child stays blocked until every open ask is answered or times out. The answer is injected verbatim into the child's next-turn context.", serde_json::json!({
        "type": "object",
        "properties": {
            "child_task_id": { "type": "string", "description": "Child handle such as `researcher` (preferred) or the full task id" },
            "ask_id": { "type": "string", "description": "Ask handle such as `q1` (preferred) or the full ask id; required when more than one ask is open" },
            "answer": { "type": "string", "description": "Verbatim answer delivered to the child" }
        },
        "required": ["child_task_id", "answer"]
    })));
    tools.push(tool_def(tool_names::NOTE_TO_CHILD, "Append a non-blocking guidance note to a child task. Validates that the caller is the child's parent (parent task or parent thread). The note is injected into the child's next-turn context as a [parent note] block; the child's task status is not changed. At most 20 notes (2000 chars each) are kept per child.", serde_json::json!({
        "type": "object",
        "properties": {
            "child_task_id": { "type": "string", "description": "Child handle such as `researcher` (preferred) or the full task id" },
            "note": { "type": "string", "description": "Guidance note for the child (truncated to 2000 chars)" }
        },
        "required": ["child_task_id", "note"]
    })));
}
