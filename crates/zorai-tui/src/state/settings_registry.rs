use super::config::ConfigState;
use super::settings::SettingsTab;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsFieldKind {
    Toggle,
    Choice,
    Picker,
    Text,
    Secret,
    Number,
    Textarea,
    Action,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SettingsFieldValue {
    Bool(bool),
    Text(String),
}

impl From<bool> for SettingsFieldValue {
    fn from(value: bool) -> Self {
        Self::Bool(value)
    }
}

impl From<String> for SettingsFieldValue {
    fn from(value: String) -> Self {
        Self::Text(value)
    }
}

impl From<&str> for SettingsFieldValue {
    fn from(value: &str) -> Self {
        Self::Text(value.to_string())
    }
}

pub struct SettingsFieldSpec {
    pub name: &'static str,
    pub label: &'static str,
    pub kind: SettingsFieldKind,
    pub value: fn(&ConfigState) -> SettingsFieldValue,
    pub visible: fn(&ConfigState) -> bool,
}

pub struct SettingsSectionSpec {
    pub title: &'static str,
    pub note: Option<fn(&ConfigState) -> Option<String>>,
    pub fields: &'static [SettingsFieldSpec],
}

pub struct SettingsPageSpec {
    pub title: &'static str,
    pub description: &'static str,
    pub sections: &'static [SettingsSectionSpec],
}

#[derive(Clone)]
pub enum SettingsPageRow {
    Blank,
    Title(&'static str),
    Description(&'static str),
    Section(&'static str),
    Note(String),
    Field {
        index: usize,
        spec: &'static SettingsFieldSpec,
    },
    TextareaLine(usize),
}

pub const SETTINGS_PAGE_LABEL_WIDTH: usize = 24;
pub const SETTINGS_PAGE_VALUE_COL: usize = 2 + SETTINGS_PAGE_LABEL_WIDTH + 1;
pub const SETTINGS_PAGE_TEXTAREA_COL: usize = 4;

macro_rules! field {
    ($kind:ident, $name:literal, $label:literal, $c:ident => $value:expr) => {
        field!($kind, $name, $label, $c => $value, if _config => true)
    };
    ($kind:ident, $name:literal, $label:literal, $c:ident => $value:expr, if $v:ident => $visible:expr) => {
        SettingsFieldSpec {
            name: $name,
            label: $label,
            kind: SettingsFieldKind::$kind,
            value: |$c: &ConfigState| SettingsFieldValue::from($value),
            visible: |$v: &ConfigState| $visible,
        }
    };
}

macro_rules! section {
    ($title:literal, [$($field:expr),* $(,)?]) => {
        SettingsSectionSpec { title: $title, note: None, fields: &[$($field),*] }
    };
    ($title:literal, note $c:ident => $note:expr, [$($field:expr),* $(,)?]) => {
        SettingsSectionSpec {
            title: $title,
            note: Some(|$c: &ConfigState| $note),
            fields: &[$($field),*],
        }
    };
}

fn raw<'a>(config: &'a ConfigState, path: &[&str]) -> Option<&'a serde_json::Value> {
    let mut node = config.agent_config_raw.as_ref()?;
    for key in path {
        node = node.get(*key)?;
    }
    Some(node)
}

fn raw_bool(config: &ConfigState, path: &[&str], default: bool) -> bool {
    raw(config, path)
        .and_then(|value| value.as_bool())
        .unwrap_or(default)
}

fn raw_number(config: &ConfigState, path: &[&str], default: &str) -> String {
    raw(config, path)
        .and_then(|value| value.as_f64())
        .map(|value| format!("{value:.0}"))
        .unwrap_or_else(|| default.to_string())
}

fn raw_str<'a>(config: &'a ConfigState, path: &[&str], default: &'a str) -> &'a str {
    raw(config, path)
        .and_then(|value| value.as_str())
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(default)
}

fn quiet_hour(config: &ConfigState, key: &str) -> String {
    raw(config, &[key])
        .and_then(|value| value.as_u64())
        .map(|hour| format!("{hour:02}:00"))
        .unwrap_or_else(|| "off".to_string())
}

fn or_default(value: String, default: &str) -> String {
    if value.trim().is_empty() {
        default.to_string()
    } else {
        value
    }
}

fn directory_list(raw: &str) -> String {
    let entries = raw
        .lines()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>();
    if entries.is_empty() {
        "(none)".to_string()
    } else {
        entries.join(", ")
    }
}

fn snapshot_summary(config: &ConfigState) -> String {
    let bytes = config.snapshot_total_size_bytes as f64;
    let size = if config.snapshot_total_size_bytes >= 1024 * 1024 * 1024 {
        format!("{:.1} GB", bytes / (1024.0 * 1024.0 * 1024.0))
    } else {
        format!("{:.1} MB", bytes / (1024.0 * 1024.0))
    };
    format!("{} snapshots stored, {size}", config.snapshot_count)
}

const HEARTBEAT_PAGE: SettingsPageSpec = SettingsPageSpec {
    title: "Heartbeat",
    description: "Background checks, a tool-free summary, and notifications.",
    sections: &[
        section!(
            "Schedule",
            [
                field!(Toggle, "feat_heartbeat_enabled", "Heartbeat", c => raw_bool(c, &["heartbeat_enabled"], false)),
                field!(Number, "feat_heartbeat_interval", "Interval", c => format!("{} min", raw_number(c, &["heartbeat_interval_mins"], "30"))),
                field!(Text, "feat_heartbeat_cron", "Cron schedule", c => raw_str(c, &["heartbeat_cron"], "(uses interval)")),
                field!(Number, "feat_heartbeat_quiet_start", "Quiet hours start", c => quiet_hour(c, "quiet_hours_start")),
                field!(Number, "feat_heartbeat_quiet_end", "Quiet hours end", c => quiet_hour(c, "quiet_hours_end")),
            ]
        ),
        section!(
            "Checks",
            [
                field!(Toggle, "feat_check_stale_todos", "Stale todos", c => raw_bool(c, &["heartbeat_checks", "stale_todos_enabled"], true)),
                field!(Toggle, "feat_check_stuck_goals", "Stuck goals", c => raw_bool(c, &["heartbeat_checks", "stuck_goals_enabled"], true)),
                field!(Toggle, "feat_check_unreplied_messages", "Unreplied messages", c => raw_bool(c, &["heartbeat_checks", "unreplied_messages_enabled"], true)),
                field!(Toggle, "feat_check_repo_changes", "Repo changes", c => raw_bool(c, &["heartbeat_checks", "repo_changes_enabled"], true)),
                field!(Toggle, "feat_check_plugin_auth", "Plugin auth", c => raw_bool(c, &["heartbeat_checks", "plugin_auth_enabled"], true)),
            ]
        ),
        section!(
            "Notifications",
            [
                field!(Toggle, "desktop_notifications", "Desktop notifications", c => raw_bool(c, &["desktop_notifications"], true)),
            ]
        ),
    ],
};

const WEB_PAGE: SettingsPageSpec = SettingsPageSpec {
    title: "Web",
    description: "Web search provider and browsing.",
    sections: &[
        section!(
            "Web search",
            [
                field!(Toggle, "web_search_enabled", "Web search", c => c.tool_web_search),
                field!(Choice, "search_provider", "Provider", c => if c.search_provider.is_empty() { "none" } else { c.search_provider.as_str() }),
                field!(Text, "duckduckgo_region", "DuckDuckGo region", c => c.duckduckgo_region.as_str(), if c => c.search_provider == "duckduckgo"),
                field!(Choice, "duckduckgo_safe_search", "DuckDuckGo safe search", c => c.duckduckgo_safe_search.as_str(), if c => c.search_provider == "duckduckgo"),
                field!(Secret, "firecrawl_api_key", "Firecrawl key", c => c.firecrawl_api_key.as_str(), if c => c.search_provider == "firecrawl"),
                field!(Secret, "exa_api_key", "Exa key", c => c.exa_api_key.as_str(), if c => c.search_provider == "exa"),
                field!(Secret, "tavily_api_key", "Tavily key", c => c.tavily_api_key.as_str(), if c => c.search_provider == "tavily"),
                field!(Number, "search_max_results", "Max results", c => c.search_max_results.to_string()),
                field!(Number, "search_timeout", "Timeout (s)", c => c.search_timeout_secs.to_string()),
            ]
        ),
        section!(
            "Browsing",
            [
                field!(Toggle, "tool_web_browse", "Browser tool", c => c.tool_web_browse),
                field!(Choice, "browse_provider", "Browser", c => c.browse_provider.as_str()),
            ]
        ),
    ],
};

const TOOLS_PAGE: SettingsPageSpec = SettingsPageSpec {
    title: "Tools",
    description: "Which tools agents may call, and generated tools.",
    sections: &[
        section!(
            "Agent capabilities",
            [
                field!(Toggle, "tool_bash", "Terminal", c => c.tool_bash),
                field!(Toggle, "tool_file_ops", "File operations", c => c.tool_file_ops),
                field!(Toggle, "tool_vision", "Vision", c => c.tool_vision),
                field!(Toggle, "tool_web_browse", "Browser", c => c.tool_web_browse),
                field!(Toggle, "tool_web_search", "Web search", c => c.tool_web_search),
                field!(Toggle, "tool_system_info", "System info", c => c.tool_system_info),
                field!(Toggle, "tool_gateway", "Gateway messaging", c => c.tool_gateway),
            ]
        ),
        section!(
            "Generated tools",
            [
                field!(Toggle, "tool_synthesis_enabled", "Tool synthesis", c => c.tool_synthesis_enabled),
                field!(Toggle, "tool_synthesis_require_activation", "Require activation", c => c.tool_synthesis_require_activation),
                field!(Number, "tool_synthesis_max_generated_tools", "Tool limit", c => c.tool_synthesis_max_generated_tools.to_string()),
                field!(Action, "generated_tools_inspect", "Inspect generated tools", _c => ""),
            ]
        ),
    ],
};

const SKILLS_PAGE: SettingsPageSpec = SettingsPageSpec {
    title: "Skills",
    description: "Skill recommendation and community skill scouting.",
    sections: &[section!(
        "Skill recommendation",
        [
            field!(Toggle, "feat_skill_recommendation_enabled", "Local skill gate", c => raw_bool(c, &["skill_recommendation", "enabled"], true)),
            field!(Toggle, "feat_skill_background_community_search", "Community scout", c => raw_bool(c, &["skill_recommendation", "background_community_search"], true)),
            field!(Number, "feat_skill_community_preapprove_timeout_secs", "Scout timeout (s)", c => raw_number(c, &["skill_recommendation", "community_preapprove_timeout_secs"], "30")),
            field!(Number, "feat_skill_suggest_global_enable_after_approvals", "Suggest after approvals", c => raw_number(c, &["skill_recommendation", "suggest_global_enable_after_approvals"], "3")),
        ]
    )],
};

const MEDIA_PAGE: SettingsPageSpec = SettingsPageSpec {
    title: "Audio & Images",
    description: "Speech-to-text, text-to-speech, and image generation.",
    sections: &[
        section!("Speech-to-text", note _c => Some("Hotkeys: Ctrl+L record | Ctrl+P speak / pause-resume | Ctrl+S stop playback".to_string()), [
            field!(Toggle, "feat_audio_stt_enabled", "Speech-to-text", c => c.audio_stt_enabled()),
            field!(Picker, "feat_audio_stt_provider", "Provider", c => or_default(c.audio_stt_provider(), "openai")),
            field!(Picker, "feat_audio_stt_model", "Model", c => or_default(c.audio_stt_model(), "whisper-1")),
            field!(Text, "feat_audio_stt_language", "Language", c => raw_str(c, &["audio", "stt", "language"], "auto")),
        ]),
        section!(
            "Text-to-speech",
            [
                field!(Toggle, "feat_audio_tts_enabled", "Text-to-speech", c => c.audio_tts_enabled()),
                field!(Picker, "feat_audio_tts_provider", "Provider", c => or_default(c.audio_tts_provider(), "openai")),
                field!(Picker, "feat_audio_tts_model", "Model", c => or_default(c.audio_tts_model(), "gpt-4o-mini-tts")),
                field!(Text, "feat_audio_tts_voice", "Voice", c => or_default(c.audio_tts_voice(), "alloy")),
                field!(Toggle, "feat_audio_tts_auto_speak", "Auto speak replies", c => raw_bool(c, &["audio", "tts", "auto_speak"], false)),
            ]
        ),
        section!(
            "Image generation",
            [
                field!(Picker, "feat_image_generation_provider", "Provider", c => or_default(c.image_generation_provider(), "openai")),
                field!(Picker, "feat_image_generation_model", "Model", c => or_default(c.image_generation_model(), "gpt-image-1")),
            ]
        ),
    ],
};

const MEMORY_PAGE: SettingsPageSpec = SettingsPageSpec {
    title: "Memory",
    description: "Honcho memory, consolidation, and semantic embeddings.",
    sections: &[
        section!("Honcho memory", note _c => Some("Enter on Honcho memory opens its connection settings.".to_string()), [
            field!(Toggle, "enable_honcho_memory", "Honcho memory", c => c.enable_honcho_memory),
        ]),
        section!(
            "Consolidation",
            [
                field!(Toggle, "feat_consolidation_enabled", "Consolidation", c => raw_bool(c, &["consolidation", "enabled"], true)),
                field!(Number, "feat_decay_half_life_hours", "Decay half-life (h)", c => raw_number(c, &["consolidation", "memory_decay_half_life_hours"], "69")),
                field!(Number, "feat_heuristic_promotion_threshold", "Promotion threshold", c => raw_number(c, &["consolidation", "heuristic_promotion_threshold"], "3")),
            ]
        ),
        section!(
            "Semantic embeddings",
            [
                field!(Toggle, "feat_embedding_enabled", "Embeddings", c => c.semantic_embedding_enabled()),
                field!(Picker, "feat_embedding_provider", "Provider", c => or_default(c.semantic_embedding_provider(), "openai")),
                field!(Picker, "feat_embedding_model", "Model", c => or_default(c.semantic_embedding_model(), "text-embedding-3-small")),
                field!(Number, "feat_embedding_dimensions", "Dimensions", c => c.semantic_embedding_dimensions().to_string()),
            ]
        ),
    ],
};

const PROACTIVITY_PAGE: SettingsPageSpec = SettingsPageSpec {
    title: "Proactivity",
    description: "Anticipatory support, operator model, and feature tier.",
    sections: &[
        section!(
            "Anticipatory support",
            [
                field!(Toggle, "anticipatory_enabled", "Anticipatory support", c => c.anticipatory_enabled),
                field!(Toggle, "anticipatory_morning_brief", "Morning brief", c => c.anticipatory_morning_brief),
                field!(Toggle, "anticipatory_predictive_hydration", "Predictive hydration", c => c.anticipatory_predictive_hydration),
                field!(Toggle, "anticipatory_stuck_detection", "Stuck detection", c => c.anticipatory_stuck_detection),
            ]
        ),
        section!(
            "Operator model",
            [
                field!(Toggle, "operator_model_enabled", "Operator model", c => c.operator_model_enabled),
                field!(Toggle, "operator_model_allow_message_statistics", "Message statistics", c => c.operator_model_allow_message_statistics),
                field!(Toggle, "operator_model_allow_approval_learning", "Approval learning", c => c.operator_model_allow_approval_learning),
                field!(Toggle, "operator_model_allow_attention_tracking", "Attention tracking", c => c.operator_model_allow_attention_tracking),
                field!(Toggle, "operator_model_allow_implicit_feedback", "Implicit feedback", c => c.operator_model_allow_implicit_feedback),
                field!(Action, "operator_model_inspect", "Inspect operator model", _c => ""),
                field!(Action, "operator_model_reset", "Reset operator model", _c => ""),
            ]
        ),
        section!(
            "Feature tier",
            [
                field!(Choice, "feat_tier_override", "Tier override", c => raw_str(c, &["tier", "user_override"], "automatic")),
            ]
        ),
    ],
};

const WORKFLOWS_PAGE: SettingsPageSpec = SettingsPageSpec {
    title: "Workflows",
    description: "Collaboration, thread titles, and code review.",
    sections: &[
        section!(
            "Collaboration",
            [
                field!(Toggle, "collaboration_enabled", "Collaboration", c => c.collaboration_enabled),
                field!(Action, "collaboration_sessions_inspect", "Inspect collaboration", _c => ""),
            ]
        ),
        section!(
            "Threads",
            [
                field!(Choice, "feat_auto_thread_title", "Auto title", c => raw_str(c, &["auto_thread_title"], "off")),
            ]
        ),
        section!("Code review", note _c => Some("Used by Find Issues in the GUI Code tab.".to_string()), [
            field!(Choice, "code_review_agent", "Review agent", c => match raw_str(c, &["code_review_agent"], "thread") {
                "thread" => "this thread's agent".to_string(),
                agent => agent.to_string(),
            }),
            field!(Number, "code_review_commit_window", "Commits per review", c => raw_number(c, &["code_review_commit_window"], "5")),
        ]),
    ],
};

const CHAT_PAGE: SettingsPageSpec = SettingsPageSpec {
    title: "Chat",
    description: "Streaming, visible history, and session restore.",
    sections: &[
        section!(
            "Responses",
            [field!(Toggle, "enable_streaming", "Streaming", c => c.enable_streaming),]
        ),
        section!(
            "History and restore",
            [
                field!(Number, "tui_chat_history_page_size", "Visible messages", c => c.tui_chat_history_page_size.to_string()),
                field!(Number, "participant_observer_restore_window_hours", "Restore hours", c => c.participant_observer_restore_window_hours.to_string()),
                field!(Number, "auto_refresh_interval_secs", "Auto refresh (s)", c => c.auto_refresh_interval_secs.to_string()),
            ]
        ),
    ],
};

const COMPACTION_PAGE: SettingsPageSpec = SettingsPageSpec {
    title: "Compaction",
    description: "When and how long conversations are compacted.",
    sections: &[
        section!(
            "Context compaction",
            [
                field!(Toggle, "auto_compact_context", "Auto compact", c => c.auto_compact_context),
                field!(Choice, "compaction_strategy", "Strategy", c => c.compaction_strategy.replace('_', " ")),
                field!(Number, "compact_threshold_pct", "Threshold %", c => c.compact_threshold_pct.to_string()),
                field!(Number, "keep_recent_on_compact", "Keep recent", c => c.keep_recent_on_compact.to_string()),
                field!(Number, "max_context_messages", "Heuristic max messages", c => c.max_context_messages.to_string()),
            ]
        ),
        section!("Strategy settings", note c => (c.compaction_strategy != "weles" && c.compaction_strategy != "custom_model")
            .then(|| "Heuristic compaction uses the built-in rule based summary path.".to_string()), [
            field!(Picker, "compaction_weles_provider", "WELES provider", c => c.compaction_weles_provider.as_str(), if c => c.compaction_strategy == "weles"),
            field!(Picker, "compaction_weles_model", "WELES model", c => c.compaction_weles_model.as_str(), if c => c.compaction_strategy == "weles"),
            field!(Picker, "compaction_weles_reasoning_effort", "WELES reasoning", c => c.compaction_weles_reasoning_effort.as_str(), if c => c.compaction_strategy == "weles"),
            field!(Choice, "compaction_weles_api_transport", "WELES transport", c => or_default(c.compaction_weles_api_transport.clone(), "(inherit)"), if c => c.compaction_strategy == "weles"),
            field!(Picker, "compaction_custom_provider", "Custom provider", c => c.compaction_custom_provider.as_str(), if c => c.compaction_strategy == "custom_model"),
            field!(Text, "compaction_custom_base_url", "Custom base URL", c => c.compaction_custom_base_url.as_str(), if c => c.compaction_strategy == "custom_model"),
            field!(Choice, "compaction_custom_auth_source", "Custom auth", c => c.compaction_custom_auth_source.as_str(), if c => c.compaction_strategy == "custom_model"),
            field!(Picker, "compaction_custom_model", "Custom model", c => c.compaction_custom_model.as_str(), if c => c.compaction_strategy == "custom_model"),
            field!(Choice, "compaction_custom_api_transport", "Custom transport", c => c.compaction_custom_api_transport.as_str(), if c => c.compaction_strategy == "custom_model"),
            field!(Secret, "compaction_custom_api_key", "Custom API key", c => c.compaction_custom_api_key.as_str(), if c => c.compaction_strategy == "custom_model"),
            field!(Text, "compaction_custom_assistant_id", "Assistant ID", c => or_default(c.compaction_custom_assistant_id.clone(), "(empty)"), if c => c.compaction_strategy == "custom_model"),
            field!(Picker, "compaction_custom_reasoning_effort", "Custom reasoning", c => c.compaction_custom_reasoning_effort.as_str(), if c => c.compaction_strategy == "custom_model"),
            field!(Number, "compaction_custom_context_window_tokens", "Custom context tokens", c => c.compaction_custom_context_window_tokens.to_string(), if c => c.compaction_strategy == "custom_model"),
        ]),
    ],
};

const EXECUTION_PAGE: SettingsPageSpec = SettingsPageSpec {
    title: "Execution",
    description: "Tool loops, retries, delays, and timeouts.",
    sections: &[
        section!(
            "Tool loop",
            [
                field!(Number, "max_tool_loops", "Max tool loops", c => c.max_tool_loops.to_string()),
                field!(Number, "max_subagent_budget_extensions", "Subagent extensions", c => if c.max_subagent_budget_extensions == 0 { "unlimited".to_string() } else { c.max_subagent_budget_extensions.to_string() }),
                field!(Number, "tool_call_delay_ms", "Tool call gap (ms)", c => c.tool_call_delay_ms.to_string()),
                field!(Number, "message_loop_delay_ms", "Message loop (ms)", c => c.message_loop_delay_ms.to_string()),
            ]
        ),
        section!(
            "Retries and timeouts",
            [
                field!(Toggle, "auto_retry", "Auto retry", c => c.auto_retry),
                field!(Number, "max_retries", "Max retries", c => c.max_retries.to_string()),
                field!(Number, "retry_delay_ms", "Retry delay (ms)", c => c.retry_delay_ms.to_string()),
                field!(Number, "llm_stream_chunk_timeout_secs", "Stream timeout (s)", c => c.llm_stream_chunk_timeout_secs.to_string()),
            ]
        ),
        section!(
            "Reviews",
            [
                field!(Number, "weles_max_concurrent_reviews", "WELES reviews", c => c.weles_max_concurrent_reviews.to_string()),
            ]
        ),
    ],
};

const SAFETY_PAGE: SettingsPageSpec = SettingsPageSpec {
    title: "Safety",
    description: "Managed command sandbox, approvals, and compliance.",
    sections: &[
        section!(
            "Managed commands",
            [
                field!(Toggle, "managed_sandbox_enabled", "Sandbox managed commands", c => c.managed_sandbox_enabled),
                field!(Choice, "managed_security_level", "Security level", c => c.managed_security_level.as_str()),
            ]
        ),
        section!(
            "Compliance",
            [
                field!(Choice, "compliance_mode", "Compliance mode", c => c.compliance_mode.as_str()),
                field!(Number, "compliance_retention_days", "Retention days", c => c.compliance_retention_days.to_string()),
                field!(Toggle, "compliance_sign_all_events", "Sign all events", c => c.compliance_sign_all_events),
            ]
        ),
    ],
};

const STORAGE_PAGE: SettingsPageSpec = SettingsPageSpec {
    title: "Storage",
    description: "Snapshot retention and workspace git monitoring.",
    sections: &[
        section!("Snapshots", note c => Some(snapshot_summary(c)), [
            field!(Toggle, "snapshot_auto_cleanup", "Auto cleanup", c => c.snapshot_auto_cleanup),
            field!(Number, "snapshot_max_count", "Max snapshots", c => c.snapshot_max_count.to_string()),
            field!(Number, "snapshot_max_size_mb", "Max size (MB)", c => c.snapshot_max_size_mb.to_string()),
        ]),
        section!("Workspace repo monitor", note _c => Some("Only included directories are monitored; excludes subtract from that set.".to_string()), [
            field!(Toggle, "workspace_repo_monitor_enabled", "Repo monitor", c => c.workspace_repo_monitor_enabled),
            field!(Textarea, "workspace_repo_monitor_include_dirs", "Include dirs", c => directory_list(&c.workspace_repo_monitor_include_dirs)),
            field!(Textarea, "workspace_repo_monitor_exclude_dirs", "Exclude dirs", c => directory_list(&c.workspace_repo_monitor_exclude_dirs)),
        ]),
    ],
};

pub fn settings_page_spec(tab: SettingsTab) -> Option<&'static SettingsPageSpec> {
    match tab {
        SettingsTab::Tools => Some(&TOOLS_PAGE),
        SettingsTab::Skills => Some(&SKILLS_PAGE),
        SettingsTab::WebSearch => Some(&WEB_PAGE),
        SettingsTab::Media => Some(&MEDIA_PAGE),
        SettingsTab::Memory => Some(&MEMORY_PAGE),
        SettingsTab::Heartbeat => Some(&HEARTBEAT_PAGE),
        SettingsTab::Proactivity => Some(&PROACTIVITY_PAGE),
        SettingsTab::Workflows => Some(&WORKFLOWS_PAGE),
        SettingsTab::Chat => Some(&CHAT_PAGE),
        SettingsTab::Compaction => Some(&COMPACTION_PAGE),
        SettingsTab::Execution => Some(&EXECUTION_PAGE),
        SettingsTab::Safety => Some(&SAFETY_PAGE),
        SettingsTab::Storage => Some(&STORAGE_PAGE),
        _ => None,
    }
}

pub fn settings_page_tab_for_field(field: &str) -> Option<SettingsTab> {
    SettingsTab::all().iter().copied().find(|tab| {
        settings_page_spec(*tab)
            .is_some_and(|page| page.all_fields().any(|spec| spec.name == field))
    })
}

impl SettingsPageSpec {
    pub fn rows(
        &'static self,
        config: &ConfigState,
        editing_textarea: Option<(&str, &str)>,
    ) -> Vec<SettingsPageRow> {
        let mut rows = vec![
            SettingsPageRow::Blank,
            SettingsPageRow::Title(self.title),
            SettingsPageRow::Description(self.description),
        ];
        let mut index = 0;
        for section in self.sections {
            let visible = section
                .fields
                .iter()
                .filter(|spec| (spec.visible)(config))
                .collect::<Vec<_>>();
            let note = section.note.and_then(|note| note(config));
            if visible.is_empty() && note.is_none() {
                continue;
            }
            rows.push(SettingsPageRow::Blank);
            rows.push(SettingsPageRow::Section(section.title));
            if let Some(note) = note {
                rows.push(SettingsPageRow::Note(note));
            }
            for spec in visible {
                rows.push(SettingsPageRow::Field { index, spec });
                index += 1;
                if let Some((field, buffer)) = editing_textarea {
                    if field == spec.name {
                        let line_count = buffer.split('\n').count().max(1);
                        rows.extend((0..line_count).map(SettingsPageRow::TextareaLine));
                    }
                }
            }
        }
        rows
    }

    pub fn visible_fields(&'static self, config: &ConfigState) -> Vec<&'static SettingsFieldSpec> {
        self.sections
            .iter()
            .flat_map(|section| section.fields.iter())
            .filter(|spec| (spec.visible)(config))
            .collect()
    }

    pub fn all_fields(&'static self) -> impl Iterator<Item = &'static SettingsFieldSpec> {
        self.sections
            .iter()
            .flat_map(|section| section.fields.iter())
    }

    pub fn field_row(
        &'static self,
        config: &ConfigState,
        editing_textarea: Option<(&str, &str)>,
        field: &str,
    ) -> Option<usize> {
        self.rows(config, editing_textarea).iter().position(
            |row| matches!(row, SettingsPageRow::Field { spec, .. } if spec.name == field),
        )
    }

    pub fn field_at_row(
        &'static self,
        config: &ConfigState,
        editing_textarea: Option<(&str, &str)>,
        row: usize,
    ) -> Option<usize> {
        let rows = self.rows(config, editing_textarea);
        match rows.get(row)? {
            SettingsPageRow::Field { index, .. } => Some(*index),
            SettingsPageRow::TextareaLine(_) => {
                rows[..row].iter().rev().find_map(|row| match row {
                    SettingsPageRow::Field { index, .. } => Some(*index),
                    _ => None,
                })
            }
            _ => None,
        }
    }
}
