use super::*;
fn configured_image_generation_setting<'a>(
    config: &'a AgentConfig,
    field: &str,
) -> Option<&'a str> {
    config
        .extra
        .get("image")
        .and_then(|value| value.get("generation"))
        .and_then(|value| value.get(field))
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .or_else(|| {
            let legacy_key = format!("image_generation_{field}");
            config
                .extra
                .get(&legacy_key)
                .and_then(|value| value.as_str())
                .map(str::trim)
                .filter(|value| !value.is_empty())
        })
}

fn configured_image_generation_provider(config: &AgentConfig) -> Option<&str> {
    configured_image_generation_setting(config, "provider")
}

fn configured_image_generation_model(config: &AgentConfig) -> Option<&str> {
    configured_image_generation_setting(config, "model")
}

pub(crate) fn add_available_tools_part_c(
    tools: &mut Vec<ToolDefinition>,
    config: &AgentConfig,
    _agent_data_dir: &std::path::Path,
    _has_workspace_topology: bool,
) {
    if config.tools.web_search {
        tools.push(tool_def(
            tool_names::WEB_SEARCH,
            "Search the web and return results. Requires a search API key in config.",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "Search query" },
                    "max_results": { "type": "integer", "description": "Max results (default: 5)" },
                    "timeout_seconds": { "type": "integer", "minimum": 0, "maximum": 600, "description": "Max time to wait for completion (default: 300, max: 600)" }
                },
                "required": ["query"]
            }),
        ));
    }

    if config.tools.web_browse {
        tools.push(tool_def(
            tool_names::FETCH_URL,
            "Download a URL into the thread inventory directory and return the saved file path plus a short preview. HTML is auto-converted to Markdown to cut tokens; JSON/text saved as-is; binaries saved raw. Read the saved file with read_file for full content.",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "url": { "type": "string", "description": "URL to fetch" },
                    "max_length": { "type": "integer", "description": "Max inline preview characters (default: 800, max: 4000); full content is in the saved file" },
                    "timeout_seconds": { "type": "integer", "minimum": 0, "maximum": 600, "description": "Max time to wait for completion (default: 300, max: 600)" },
                    "profile_id": { "type": "string", "description": "Optional named browser profile for authenticated browsing reuse" }
                },
                "required": ["url"]
            }),
        ));
    }

    tools.push(tool_def(
        tool_names::SETUP_WEB_BROWSING,
        "Detect, install, or configure a headless browser for web browsing. Run detect first; install adds Lightpanda via npm (Chrome must be installed manually); configure sets browse_provider (chrome requires Chrome on PATH).",
        serde_json::json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["detect", "install", "configure"],
                    "description": "detect: check browsers. install: Lightpanda via npm. configure: set browse_provider."
                },
                "provider": {
                    "type": "string",
                    "enum": ["auto", "lightpanda", "chrome", "none"],
                    "description": "browse_provider for configure (default: auto)"
                }
            },
            "required": ["action"]
        }),
    ));

    if config.tools.gateway_messaging {
        for (name, desc, params) in [
            (tool_names::SEND_SLACK_MESSAGE, "Send a message to a Slack channel. If channel is omitted, sends to the default channel from gateway settings (slack_channel_filter).", serde_json::json!({
                "type": "object",
                "properties": {
                    "channel": { "type": "string", "description": "Slack channel name or ID. Optional — uses default from config if omitted." },
                    "message": { "type": "string", "description": "Message text" }
                },
                "required": ["message"]
            })),
            (tool_names::SEND_DISCORD_MESSAGE, "Send a message to a Discord channel or user. If channel_id and user_id are both omitted, sends to the default channel (discord_channel_filter) or default user DM (discord_allowed_users) from gateway settings.", serde_json::json!({
                "type": "object",
                "properties": {
                    "channel_id": { "type": "string", "description": "Discord channel ID. Optional — uses default from config if omitted." },
                    "user_id": { "type": "string", "description": "Discord user ID for DM. Optional." },
                    "message": { "type": "string", "description": "Message text" }
                },
                "required": ["message"]
            })),
            (tool_names::SEND_TELEGRAM_MESSAGE, "Send a message to a Telegram chat. If chat_id is omitted, sends to the default chat from gateway settings (telegram_allowed_chats).", serde_json::json!({
                "type": "object",
                "properties": {
                    "chat_id": { "type": "string", "description": "Telegram chat ID. Optional — uses default from config if omitted." },
                    "message": { "type": "string", "description": "Message text" }
                },
                "required": ["message"]
            })),
            (tool_names::SEND_WHATSAPP_MESSAGE, "Send a message to a WhatsApp contact. If phone is omitted, sends to the default contact from gateway settings (whatsapp_allowed_contacts).", serde_json::json!({
                "type": "object",
                "properties": {
                    "phone": { "type": "string", "description": "Phone in E.164 format or WhatsApp JID. Optional — uses default from config if omitted." },
                    "message": { "type": "string", "description": "Message text" }
                },
                "required": ["message"]
            })),
            (tool_names::WHATSAPP_LINK_START, "Start the WhatsApp link runtime and begin pairing or reconnect flow.", serde_json::json!({
                "type": "object",
                "properties": {}
            })),
            (tool_names::WHATSAPP_LINK_STOP, "Stop the WhatsApp link runtime and disconnect the current session.", serde_json::json!({
                "type": "object",
                "properties": {}
            })),
            (tool_names::WHATSAPP_LINK_RESET, "Reset the WhatsApp link runtime and clear persisted session state.", serde_json::json!({
                "type": "object",
                "properties": {}
            })),
            (tool_names::WHATSAPP_LINK_STATUS, "Return the current WhatsApp link runtime status, including linked phone and last error when present.", serde_json::json!({
                "type": "object",
                "properties": {}
            })),
        ] {
            tools.push(tool_def(name, desc, params));
        }
    }

    tools.push(tool_def(
        tool_names::PYTHON_EXECUTE,
        "Execute Python code directly in a daemon-native subprocess without going through a shell. Use this instead of shell-launched Python when you need Python execution.",
        serde_json::json!({
            "type": "object",
            "properties": {
                "code": { "type": "string", "description": "Python source code to execute" },
                "cwd": { "type": "string", "description": "Optional working directory" },
                "wait_for_completion": { "type": "boolean", "description": "Wait for completion and return exit status/output summary (default: true). On the TUI surface, long-running executions detach after a short foreground grace and continue as a background operation." },
                "timeout_seconds": { "type": "integer", "minimum": 0, "maximum": 600, "description": "Wait timeout when wait_for_completion=true (default: 30, max: 600). Above 600 the execution auto-backgrounds, returns an `operation_id`, and will auto-notify the thread on completion." },
                "notify_on_completion": { "type": "boolean", "description": "When backgrounded, automatically resume this thread on completion (default: true). Set false only for fire-and-forget work you will not act on. Do not poll get_operation_status; if you need the result before continuing, call get_operation_status with wait=true." }
            },
            "required": ["code"]
        }),
    ));

    let active_model_features = zorai_shared::providers::derive_model_feature_capabilities(
        &config.provider,
        &config.model,
        None,
        false,
    );
    let image_generation_enabled = configured_image_generation_model(config).is_some()
        || active_model_features.image_generation;
    let active_model_supports_image = crate::agent::types::model_supports(
        &config.provider,
        &config.model,
        crate::agent::types::Modality::Image,
    );
    let image_analysis_enabled =
        config.tools.vision || active_model_features.vision || active_model_supports_image;

    if image_analysis_enabled {
        tools.push(tool_def(
            tool_names::ANALYZE_IMAGE,
            "Analyze an image with the active or specified multimodal model. Accepts exactly one of `path`, `url`, `base64`, or `data_url`, then returns a textual analysis.",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Local image path to analyze" },
                    "url": { "type": "string", "description": "Remote image URL to analyze" },
                    "base64": { "type": "string", "description": "Base64-encoded image bytes. Requires `mime_type`." },
                    "data_url": { "type": "string", "description": "Base64 data URL for the image" },
                    "mime_type": { "type": "string", "description": "Override MIME type when using `path` or `base64`" },
                    "prompt": { "type": "string", "description": "Optional analysis instruction. Defaults to a general detailed analysis." },
                    "provider": { "type": "string", "description": "Optional provider override, only when the operator explicitly specifies it" },
                    "model": { "type": "string", "description": "Optional model override, only when the operator explicitly specifies it" },
                    "timeout_seconds": { "type": "integer", "minimum": 0, "maximum": 600, "description": "Max time to wait for completion (default: 600, max: 600)" },
                    "include_reasoning": { "type": "boolean", "description": "Include model reasoning summary when available" },
                    "include_provider_result": { "type": "boolean", "description": "Append the raw structured provider final result when available" }
                }
            }),
        ));
    }

    if image_generation_enabled {
        tools.push(tool_def(
            tool_names::GENERATE_IMAGE,
            "Generate an image through an OpenAI-compatible image generation endpoint and return JSON with the saved artifact path or upstream URL.",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "prompt": { "type": "string", "description": "Image generation prompt" },
                    "provider": { "type": "string", "description": "Optional provider override, only when the operator explicitly specifies it" },
                    "model": { "type": "string", "description": "Optional model override, only when the operator explicitly specifies it" },
                    "size": { "type": "string", "description": "Optional output size such as 1024x1024" },
                    "quality": { "type": "string", "description": "Optional quality hint supported by the provider" },
                    "style": { "type": "string", "description": "Optional style hint supported by the provider" },
                    "background": { "type": "string", "description": "Optional background hint supported by the provider" },
                    "output_format": { "type": "string", "description": "Desired image format for saved bytes, e.g. png, jpg, webp" },
                    "timeout_seconds": { "type": "integer", "minimum": 0, "maximum": 600, "description": "Max time to wait for completion (default: 600, max: 600)" }
                },
                "required": ["prompt"]
            }),
        ));
    }

    tools.push(tool_def(
        tool_names::SPEECH_TO_TEXT,
        "Transcribe an audio file through an OpenAI-compatible transcription endpoint and return the recognized text or provider JSON.",
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "Local audio file path to transcribe" },
                "mime_type": { "type": "string", "description": "Optional MIME type override for the uploaded audio file" },
                "provider": { "type": "string", "description": "Optional provider override, only when the operator explicitly specifies it" },
                "model": { "type": "string", "description": "Optional model override, only when the operator explicitly specifies it" },
                "language": { "type": "string", "description": "Optional language hint for the transcription model" },
                "prompt": { "type": "string", "description": "Optional transcription prompt or vocabulary hint" },
                "response_format": { "type": "string", "description": "Optional transcription response format such as json, verbose_json, srt, or text" },
                "timeout_seconds": { "type": "integer", "minimum": 0, "maximum": 600, "description": "Max time to wait for completion (default: 600, max: 600)" }
            },
            "required": ["path"]
        }),
    ));

    tools.push(tool_def(
        tool_names::TEXT_TO_SPEECH,
        "Synthesize speech through an OpenAI-compatible speech endpoint and return JSON with the saved audio artifact path. Use this when the operator asks you to say something aloud or read text out loud. After success, do not send a follow-up message that only repeats the temporary file path.",
        serde_json::json!({
            "type": "object",
            "properties": {
                "input": { "type": "string", "description": "Text to synthesize into speech" },
                "provider": { "type": "string", "description": "Optional provider override, only when the operator explicitly specifies it" },
                "model": { "type": "string", "description": "Optional model override, only when the operator explicitly specifies it" },
                "voice": { "type": "string", "description": "Voice identifier supported by the provider" },
                "response_format": { "type": "string", "description": "Desired output format such as mp3, wav, ogg, flac, or m4a" },
                "timeout_seconds": { "type": "integer", "minimum": 0, "maximum": 600, "description": "Max time to wait for completion (default: 600, max: 600)" }
            },
            "required": ["input"]
        }),
    ));

    tools.push(tool_def(
        tool_names::LIST_TERMINALS,
        "List all open terminal panes with their IDs and names.",
        serde_json::json!({"type":"object","properties":{}}),
    ));
    tools.push(tool_def(tool_names::READ_ACTIVE_TERMINAL_CONTENT, "Read the current terminal buffer content from a pane, or browser panel info. For browser panels, returns URL and title; use include_dom to get page text content.", serde_json::json!({
        "type": "object",
        "properties": {
            "pane": { "type": "string", "description": "Pane ID or name (optional, defaults to active)" },
            "include_dom": { "type": "boolean", "description": "For browser panels: include page DOM text content. Ignored for terminal panes." }
        }
    })));
    tools.push(tool_def(tool_names::RUN_TERMINAL_COMMAND, "Execute a shell command through a zorai-managed terminal session. This runs in the app's terminal context (not a daemon-native subprocess). For long-running work, prefer non-blocking execution; this thread auto-resumes when background operations complete. Do not poll `get_operation_status`; call it once with `wait=true` only if you need the result before doing more work. Use this for shell-native networking such as `curl -I`, range requests, large or binary downloads, or streaming transfers; for browser-readable text pages, prefer `web_search` or `fetch_url`.", serde_json::json!({
        "type": "object",
        "properties": {
            "command": { "type": "string", "description": "Shell command to execute in a managed terminal session" },
            "rationale": { "type": "string", "description": "Why this command should run" },
            "session": { "type": "string", "description": "Optional terminal session ID or unique substring" },
            "cwd": { "type": "string", "description": "Optional working directory" },
            "allow_network": { "type": "boolean", "description": "Whether network access is expected" },
            "sandbox_enabled": { "type": "boolean", "description": "Whether sandboxing should be requested" },
            "security_level": { "type": "string", "enum": ["highest", "moderate", "lowest", "yolo"], "description": "Approval strictness level" },
            "language_hint": { "type": "string", "description": "Optional language hint for validation" },
            "wait_for_completion": { "type": "boolean", "description": "Wait for completion and return exit status/output summary (default: true)" },
            "timeout_seconds": { "type": "integer", "description": "Wait timeout when wait_for_completion=true (default: 30, max: 600). Above 600 the command auto-backgrounds, returns an `operation_id`, and will auto-notify the thread on completion." },
            "notify_on_completion": { "type": "boolean", "description": "When backgrounded, automatically resume this thread on completion (default: true). Set false only for fire-and-forget work you will not act on. Do not poll get_operation_status; if you need the result before continuing, call get_operation_status with wait=true." }
        },
        "required": ["command"]
    })));
    tools.push(tool_def(tool_names::EXECUTE_MANAGED_COMMAND, "Queue a command in a daemon-managed terminal lane. By default this tool waits for completion and returns final status/output tail. If session is omitted, uses the first active terminal session. For non-blocking execution, this thread auto-resumes when background operations complete. Do not poll `get_operation_status`; call it once with `wait=true` only if you need the result before doing more work. Use this for shell-native networking such as `curl -I`, range requests, large or binary downloads, or streaming transfers; for browser-readable text pages, prefer `web_search` or `fetch_url`.", serde_json::json!({
        "type": "object",
        "properties": {
            "command": { "type": "string", "description": "Shell command to run in the managed terminal session" },
            "rationale": { "type": "string", "description": "Why this command should run" },
            "session": { "type": "string", "description": "Optional session ID or unique substring" },
            "cwd": { "type": "string", "description": "Optional working directory" },
            "allow_network": { "type": "boolean", "description": "Whether network access is expected" },
            "sandbox_enabled": { "type": "boolean", "description": "Whether sandboxing should be requested" },
            "security_level": { "type": "string", "enum": ["highest", "moderate", "lowest", "yolo"], "description": "Approval strictness level" },
            "language_hint": { "type": "string", "description": "Optional language hint for validation" },
            "wait_for_completion": { "type": "boolean", "description": "Wait for completion and return exit status/output summary (default: true)" },
            "timeout_seconds": { "type": "integer", "description": "Wait timeout when wait_for_completion=true (default: 30, max: 600). Above 600 the command auto-backgrounds, returns an `operation_id`, and will auto-notify the thread on completion." },
            "notify_on_completion": { "type": "boolean", "description": "When backgrounded, automatically resume this thread on completion (default: true). Set false only for fire-and-forget work you will not act on. Do not poll get_operation_status; if you need the result before continuing, call get_operation_status with wait=true." }
        },
        "required": ["command", "rationale"]
    })));
    tools.push(tool_def(tool_names::GET_OPERATION_STATUS, "Look up an asynchronous operation by operation_id. Do not poll this tool. If you need the result before doing more work, set wait=true so the daemon blocks until completion (or timeout) and returns the final payload in one call. Otherwise continue other work; this thread auto-resumes when the operation finishes. Snapshot-only calls (wait=false) are for a single check, not a loop. For background terminal commands, pass the returned operation_id; `background_task_id` is the same value for compatibility. When a background headless shell command completes or fails, this response includes `terminal_result` with the captured payload and exit code.", serde_json::json!({
        "type": "object",
        "properties": {
            "operation_id": { "type": "string", "description": "Short handle such as `op1` (preferred) or the full operation id" },
            "wait": { "type": "boolean", "description": "When true, block in the daemon until the operation reaches completed/failed/cancelled or timeout_seconds elapses (default: false). Use this instead of polling." },
            "timeout_seconds": { "type": "integer", "minimum": 0, "maximum": 3600, "description": "Max seconds to wait when wait=true (default: 600, max: 3600). Ignored when wait=false." }
        },
        "required": ["operation_id"]
    })));
    tools.push(tool_def(tool_names::ALLOCATE_TERMINAL, "Allocate another daemon-managed terminal lane in the same workspace as the current session. Use this when your chosen session is occupied by a blocking or long-running command and you need another terminal to continue working.", serde_json::json!({
        "type": "object",
        "properties": {
            "session": { "type": "string", "description": "Optional source session ID or unique substring. Defaults to the preferred/current session." },
            "pane_name": { "type": "string", "description": "Optional name for the new terminal pane" },
            "cwd": { "type": "string", "description": "Optional working directory hint to show in the workspace metadata" }
        }
    })));
    tools.push(tool_def(tool_names::LIST_PROVIDERS, "List configured providers with authentication state, auth source, configured/default model, and base URL. Use this before selecting a provider for subagents or model switching.", serde_json::json!({
        "type": "object",
        "properties": {}
    })));
    tools.push(tool_def(tool_names::LIST_MODELS, "List the remotely available models for one authenticated provider using its configured credentials and base URL. Use this before setting a model override or calling `switch_model`.", serde_json::json!({
        "type": "object",
        "properties": {
            "provider": { "type": "string", "description": "Authenticated provider ID to inspect, such as `openai` or `github_copilot`." }
        },
        "required": ["provider"]
    })));
    tools.push(tool_def(tool_names::LIST_AGENTS, "List runtime agent targets and spawnable specialist definitions. Call this when you need to pick a specialist before `spawn_subagent`. Filter on `spawnable: true` for spawnable specialists. This does not report spawned child-task progress; use `list_subagents` for that.", serde_json::json!({
        "type": "object",
        "properties": {}
    })));
    tools.push(tool_def(tool_names::LIST_PARTICIPANTS, "List the current thread participants, including active/inactive status and instructions. On participant-managed threads, use this instead of `list_agents` when choosing who can own the thread.", serde_json::json!({
        "type": "object",
        "properties": {}
    })));
    if current_agent_scope_id() == MAIN_AGENT_ID {
        tools.push(tool_def(tool_names::SWITCH_MODEL, "Update which provider and model a target agent uses as its LLM access point. This writes the same persisted settings the Settings UI edits. Only svarog can call this tool.", serde_json::json!({
            "type": "object",
            "properties": {
                "agent": { "type": "string", "description": "Target agent id or name, such as `svarog`, `rarog`, `weles`, or a user subagent id from `list_agents`." },
                "provider": { "type": "string", "description": "Provider id to assign to the target agent." },
                "model": { "type": "string", "description": "Model id to assign to the target agent." }
            },
            "required": ["agent", "provider", "model"]
        })));
    }
    tools.push(tool_def(tool_names::SPAWN_SUBAGENT, "Spawn a bounded child task under the current task or thread. If you need a specialist, call `list_agents` first and use a matching title.", serde_json::json!({
        "type": "object",
        "properties": {
            "title": { "type": "string", "description": "Short subagent title" },
            "description": { "type": "string", "description": "Detailed instructions for the child task" },
            "runtime": { "type": "string", "enum": ["daemon", "hermes", "openclaw"], "description": "Child runtime (default: daemon)" },
            "provider": { "type": "string", "description": "Optional authenticated provider override" },
            "model": { "type": "string", "description": "Optional model override; requires `provider`" },
            "reasoning_effort": { "type": "string", "description": "Optional override: off, low, medium, or high" },
            "priority": { "type": "string", "enum": ["low", "normal", "high", "urgent"], "description": "Child task priority" },
            "command": { "type": "string", "description": "Optional preferred entrypoint or command" },
            "session": { "type": "string", "description": "Optional session ID or substring; defaults to a fresh lane" },
            "cwd": { "type": "string", "description": "Optional working directory hint" },
            "dependencies": { "type": "array", "items": { "type": "string" }, "description": "Optional additional task dependencies" },
            "max_depth": { "type": "integer", "description": "Max recursive delegation depth (default: 1, hard cap: 3)" },
            "budget": {
                "type": "object",
                "properties": {
                    "max_tokens": { "type": "integer", "description": "Visible output-token budget; input/reasoning tokens not counted" },
                    "max_wall_time_secs": { "type": "integer", "description": "Wall-clock budget in seconds" }
                },
                "description": "Optional child budget; when omitted, derived from delegation depth."
            }
        },
        "required": ["title", "description"]
    })));
    tools.push(tool_def(tool_names::LIST_SUBAGENTS, "List child tasks spawned under the current parent task or thread, with status, delegation depth, and budget info.", serde_json::json!({
        "type": "object",
        "properties": {
            "status": { "type": "string", "enum": ["queued", "in_progress", "awaiting_approval", "blocked", "failed_analyzing", "budget_exceeded", "completed", "failed", "cancelled"], "description": "Optional status filter" },
            "parent_task_id": { "type": "string", "description": "Override parent task scope" },
            "parent_thread_id": { "type": "string", "description": "Override parent thread scope" },
            "limit": { "type": "integer", "description": "Maximum subagents to return (default: 20)" }
        }
    })));
    tools.push(tool_def(tool_names::REPORT_SUBAGENT_OUTCOME, "Spawned child only. Report final job status and a usable summary. This report is not counted against the execution budget.", serde_json::json!({
        "type": "object",
        "properties": {
            "status": { "type": "string", "enum": ["done", "cancelled", "error"], "description": "done if complete, cancelled if stopped, error if blocked including provider quota" },
            "summary": { "type": "string", "description": "Concrete summary of completed work, remaining gaps, and artifacts" }
        },
        "required": ["status", "summary"]
    })));
    tools.push(tool_def(tool_names::EXTEND_SUBAGENT_BUDGET, "Raise a spawned child's visible-output token budget on that child thread. Child may self-call; parent may pass child_task_id or child_thread_id.", serde_json::json!({
        "type": "object",
        "properties": {
            "child_task_id": { "type": "string", "description": "Child handle such as `researcher` (preferred) or the full task id; omit when calling from the child itself" },
            "child_thread_id": { "type": "string", "description": "Target child thread; omit when calling from the child itself" },
            "additional_tokens": { "type": "integer", "description": "Extra visible output tokens to add to the child ceiling" },
            "additional_wall_time_secs": { "type": "integer", "description": "Optional extra wall-clock seconds" },
            "reason": { "type": "string", "description": "Why more budget is required" }
        },
        "required": ["additional_tokens", "reason"]
    })));
    tools.push(tool_def(tool_names::MESSAGE_AGENT, "Send a private internal DM to another zorai agent. Returns immediately; the reply arrives later as a continuation on this thread. It does not switch the visible thread responder. DMs between spawned subagents are routed to the sibling's dedicated task thread (active siblings in the same parent scope only — never the global persona registry). Use `handoff_thread_agent` when the operator should talk to another agent directly.", serde_json::json!({
        "type": "object",
        "properties": {
            "target": { "type": "string", "description": "Receiving agent id or persona name, e.g. `svarog`, `rarog`, `weles`, a sibling subagent persona/task id, or the spawn title of an active sibling" },
            "message": { "type": "string", "description": "Message to send" },
            "request_visible_thread_continuation": { "type": "boolean", "description": "Ask target to continue the visible thread afterward; defaults true when targeting an active participant" }
        },
        "required": ["target", "message"]
    })));
    tools.push(tool_def(tool_names::HANDOFF_THREAD_AGENT, "Switch the active responder for the current thread. On participant-managed threads push_handoff may target only active participants, and agent-initiated push handoffs require approval outside yolo mode.", serde_json::json!({
        "type": "object",
        "properties": {
            "action": { "type": "string", "enum": ["push_handoff", "return_handoff"], "description": "push_handoff moves the thread; return_handoff pops back to the previous responder." },
            "target_agent_id": { "type": "string", "description": "Required for push_handoff: agent id or persona to take over" },
            "reason": { "type": "string", "description": "Why the handoff is happening." },
            "summary": { "type": "string", "description": "Compact summary for the receiving agent." },
            "requested_by": { "type": "string", "enum": ["user", "agent"], "description": "Whether the operator or the agent requested this." }
        },
        "required": ["action", "reason", "summary", "requested_by"]
    })));
    tools.push(tool_def(tool_names::ROUTE_TO_SPECIALIST, "Route a task to a specialist subagent matched by capability tags, with a structured context handoff.", serde_json::json!({
        "type": "object",
        "properties": {
            "task_description": { "type": "string", "description": "Work to hand off to a specialist" },
            "capability_tags": { "type": "array", "items": { "type": "string" }, "description": "Capability tags for matching, e.g. [\"rust\", \"backend\"]" },
            "acceptance_criteria": { "type": "string", "description": "Output checks, e.g. \"min_length:100\" (default: \"non_empty\")" }
        },
        "required": ["task_description", "capability_tags"]
    })));
    tools.push(tool_def(tool_names::RUN_DIVERGENT, "Spawn 2-3 parallel framings of one problem, detect disagreements, and surface tensions as the output. Returns a session ID and framing labels.", serde_json::json!({
        "type": "object",
        "properties": {
            "problem_statement": { "type": "string", "description": "Problem to analyze from multiple perspectives" },
            "mode": { "type": "string", "enum": ["divergent", "debate"], "description": "divergent (default) maps tensions; debate runs the debate protocol" },
            "custom_framings": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "label": { "type": "string", "description": "Short name for this perspective" },
                        "system_prompt_override": { "type": "string", "description": "System prompt for this framing" }
                    },
                    "required": ["label", "system_prompt_override"]
                },
                "description": "Optional 2-3 custom framings; defaults to analytical + pragmatic lenses."
            }
        },
        "required": ["problem_statement"]
    })));
    tools.push(tool_def(tool_names::GET_DIVERGENT_SESSION, "Fetch divergent session status and output payload (framing progress, tensions markdown, mediator prompt, optional mediation result). Use this after run_divergent to retrieve completion artifacts.", serde_json::json!({
        "type": "object",
        "properties": {
            "session_id": { "type": "string", "description": "Divergent session ID returned by run_divergent" }
        },
        "required": ["session_id"]
    })));
    tools.push(tool_def(tool_names::RUN_DEBATE, "Start a structured debate session with 2-3 framings and rotating roles; returns a session ID.", serde_json::json!({
        "type": "object",
        "properties": {
            "topic": { "type": "string", "description": "The topic to debate" },
            "custom_framings": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "label": { "type": "string", "description": "Short name for this perspective" },
                        "system_prompt_override": { "type": "string", "description": "System prompt for this framing" }
                    },
                    "required": ["label", "system_prompt_override"]
                },
                "description": "Optional 2-3 custom framings; defaults to analytical + pragmatic lenses."
            }
        },
        "required": ["topic"]
    })));
    tools.push(tool_def(tool_names::GET_DEBATE_SESSION, "Fetch debate session status and payload, including roles, arguments, and verdict when available.", serde_json::json!({
        "type": "object",
        "properties": {
            "session_id": { "type": "string", "description": "Debate session ID returned by run_debate" }
        },
        "required": ["session_id"]
    })));
    tools.push(tool_def(tool_names::APPEND_DEBATE_ARGUMENT, "Append one structured argument to a debate session for the current round.", serde_json::json!({
        "type": "object",
        "properties": {
            "session_id": { "type": "string", "description": "Debate session ID" },
            "role": { "type": "string", "enum": ["proponent", "skeptic", "synthesizer"], "description": "Debate role for this argument" },
            "agent_id": { "type": "string", "description": "Framing/agent label submitting the argument" },
            "content": { "type": "string", "description": "Argument content" },
            "evidence_refs": { "type": "array", "items": { "type": "string" }, "description": "Supporting evidence references" },
            "responds_to": { "type": "string", "description": "Optional prior argument ID being answered" }
        },
        "required": ["session_id", "role", "agent_id", "content"]
    })));
    tools.push(tool_def(
        tool_names::ADVANCE_DEBATE_ROUND,
        "Advance a debate session to the next round and rotate roles when configured.",
        serde_json::json!({
            "type": "object",
            "properties": {
                "session_id": { "type": "string", "description": "Debate session ID" }
            },
            "required": ["session_id"]
        }),
    ));
    tools.push(tool_def(
        tool_names::COMPLETE_DEBATE_SESSION,
        "Finalize a debate session and synthesize a verdict from the accumulated arguments.",
        serde_json::json!({
            "type": "object",
            "properties": {
                "session_id": { "type": "string", "description": "Debate session ID" }
            },
            "required": ["session_id"]
        }),
    ));
    tools.push(tool_def(tool_names::GET_CRITIQUE_SESSION, "Fetch an auto-generated critique preflight session, including advocate/critic arguments and the arbiter resolution.", serde_json::json!({
        "type": "object",
        "properties": {
            "session_id": { "type": "string", "description": "Critique session ID returned in critique preflight notices or blocking messages" }
        },
        "required": ["session_id"]
    })));
    tools.push(tool_def(tool_names::LOOKUP_EMERGENT_PROTOCOL, "Look up an accepted emergent protocol for the current thread by token; optionally record a usage or fallback outcome.", serde_json::json!({
        "type": "object",
        "properties": {
            "token": { "type": "string", "description": "Protocol token, e.g. @proto_deadbeef" },
            "record_usage": { "type": "boolean", "description": "Record usage/fallback against the entry" },
            "success": { "type": "boolean", "description": "Usage outcome when record_usage=true" },
            "fallback_reason": { "type": "string", "description": "Why the protocol could not be applied" },
            "execution_time_ms": { "type": "integer", "description": "Optional execution time in ms" }
        },
        "required": ["token"]
    })));
    tools.push(tool_def(tool_names::LIST_EMERGENT_PROTOCOL_PROPOSALS, "List pending emergent protocol proposals for the current thread that require explicit acceptance or rejection before activation.", serde_json::json!({
        "type": "object",
        "properties": {}
    })));
    tools.push(tool_def(tool_names::RESPOND_EMERGENT_PROTOCOL_PROPOSAL, "Accept or reject a pending emergent protocol proposal in the current thread. Accepted proposals activate into the registry; rejected proposals remain suppressed.", serde_json::json!({
        "type": "object",
        "properties": {
            "candidate_id": { "type": "string", "description": "Pending protocol candidate ID to respond to" },
            "accept": { "type": "boolean", "description": "True to accept and activate the proposal; false to reject it" }
        },
        "required": ["candidate_id", "accept"]
    })));
    tools.push(tool_def(tool_names::RELOAD_EMERGENT_PROTOCOL_REGISTRY, "Reload and return accepted emergent protocol registry entries for the current thread from durable storage.", serde_json::json!({
        "type": "object",
        "properties": {}
    })));
    tools.push(tool_def(tool_names::DECODE_EMERGENT_PROTOCOL, "Decode an accepted emergent protocol token for the current thread, validating the stored context signature. Does not execute the decoded steps.", serde_json::json!({
        "type": "object",
        "properties": {
            "token": { "type": "string", "description": "Protocol token, e.g. @proto_deadbeef" },
            "current_role": { "type": "string", "description": "Sender role for validation, e.g. user" },
            "target_role": { "type": "string", "description": "Receiver role for validation, e.g. assistant" },
            "normalized_pattern": { "type": "string", "description": "Observed normalized pattern to validate against the signature" }
        },
        "required": ["token"]
    })));
    tools.push(tool_def(
        tool_names::GET_EMERGENT_PROTOCOL_USAGE_LOG,
        "Fetch recorded usage/fallback entries for an accepted emergent protocol.",
        serde_json::json!({
            "type": "object",
            "properties": {
                "protocol_id": { "type": "string", "description": "Accepted emergent protocol ID" }
            },
            "required": ["protocol_id"]
        }),
    ));
}
