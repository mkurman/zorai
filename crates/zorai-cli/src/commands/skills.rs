use anyhow::{Context, Result};
use std::path::Path;
use zorai_protocol::{SessionId, SessionInfo};

use crate::cli::SkillAction;
use crate::client;
use crate::output::truncate_for_display;

use super::skill_sync::{fetch_remote_skill_documents, locked_skill_keys, sync_skill_documents};

pub(crate) async fn run(action: SkillAction) -> Result<()> {
    match action {
        SkillAction::List {
            status,
            limit,
            cursor,
            all,
        } => {
            let mut next_cursor = cursor;
            let mut variants = Vec::new();

            loop {
                let (page_variants, page_cursor) =
                    client::send_skill_list(status.clone(), limit, next_cursor.clone()).await?;
                let done = page_cursor.is_none() || !all;
                variants.extend(page_variants);
                next_cursor = page_cursor;
                if done {
                    break;
                }
            }

            if variants.is_empty() {
                println!("No skills found.");
            } else {
                println!(
                    "{:<12} {:<24} {:>5}  {:>9}  {}",
                    "STATUS", "SKILL NAME", "USES", "SUCCESS", "TAGS"
                );
                for variant in &variants {
                    let success = format!("{}/{}", variant.success_count, variant.use_count);
                    let tags = variant.context_tags.join(", ");
                    println!(
                        "{:<12} {:<24} {:>5}  {:>9}  {}",
                        variant.status, variant.skill_name, variant.use_count, success, tags
                    );
                }
                println!("\n{} skill(s) shown.", variants.len());
                if let Some(next_cursor) = next_cursor {
                    println!("Next cursor: {next_cursor}");
                }
            }
        }
        SkillAction::Sync { force } => {
            let root = zorai_protocol::zorai_skills_dir();
            let locked = match client::send_config_get().await {
                Ok(config) => locked_skill_keys(&config),
                Err(error) if force => anyhow::bail!(
                    "cannot read locked skills from the daemon ({error}); start the daemon before a forced skill sync"
                ),
                Err(_) => Vec::new(),
            };
            let documents = fetch_remote_skill_documents().await?;
            let summary = sync_skill_documents(&root, &documents, force, &locked)?;
            println!("Synced skills from https://github.com/mkurman/zorai/tree/main/skills");
            println!("Skills root: {}", root.display());
            println!(
                "Installed: {} | Overwritten: {} | Skipped existing: {} | Skipped locked: {}",
                summary.installed,
                summary.overwritten,
                summary.skipped_existing,
                summary.skipped_locked
            );
        }
        SkillAction::Discover {
            query,
            session,
            limit,
            cursor,
        } => {
            let session_id = resolve_skill_discovery_session(session.as_deref()).await?;
            let result = client::send_skill_discover(&query, session_id, limit, cursor).await?;
            println!("{}", render_skill_discovery(&result));
        }
        SkillAction::Inspect { name } => {
            let (variant, content) = client::send_skill_inspect(&name).await?;
            if let Some(variant) = variant {
                println!("Skill:       {}", variant.skill_name);
                println!(
                    "Variant:     {} ({})",
                    variant.variant_name, variant.variant_id
                );
                println!("Status:      {}", variant.status);
                println!("Path:        {}", variant.relative_path);
                println!(
                    "Usage:       {} uses ({} success, {} failure)",
                    variant.use_count, variant.success_count, variant.failure_count
                );
                if !variant.context_tags.is_empty() {
                    println!("Tags:        {}", variant.context_tags.join(", "));
                }
                if let Some(content) = content {
                    println!("\n--- SKILL.md ---\n{}", content);
                }
            } else {
                eprintln!("Skill not found: {}", name);
            }
        }
        SkillAction::Reject { name } => {
            let (success, message) = client::send_skill_reject(&name).await?;
            if success {
                println!("{}", message);
            } else {
                eprintln!("{}", message);
            }
        }
        SkillAction::Promote { name, to } => {
            let (success, message) = client::send_skill_promote(&name, &to).await?;
            if success {
                println!("{}", message);
            } else {
                eprintln!("{}", message);
            }
        }
        SkillAction::Search { query } => {
            let entries = client::send_skill_search(&query).await?;
            if entries.is_empty() {
                println!("No community skills found for '{}'.", query);
            } else {
                println!(
                    "{:<10} {:<24} {:>6} {:>8} {:<10} {}",
                    "VERIFIED", "NAME", "USES", "SUCCESS", "PUBLISHER", "DESCRIPTION"
                );
                for entry in &entries {
                    let verified = if entry.publisher_verified { "✓" } else { "-" };
                    let success = format!("{:.0}%", entry.success_rate * 100.0);
                    let publisher = truncate_for_display(&entry.publisher_id, 8);
                    let description = truncate_for_display(&entry.description, 40);
                    println!(
                        "{:<10} {:<24} {:>6} {:>8} {:<10} {}",
                        verified,
                        truncate_for_display(&entry.name, 24),
                        entry.use_count,
                        success,
                        publisher,
                        description
                    );
                }
                println!("\n{} skill(s) found.", entries.len());
            }
        }
        SkillAction::Import { source, force } => {
            let (success, message, variant_id, scan_verdict, findings_count) =
                client::send_skill_import(&source, force).await?;
            if success {
                println!(
                    "Imported skill as Draft (variant: {}).",
                    variant_id.unwrap_or_default()
                );
                if scan_verdict.as_deref() == Some("warn") {
                    println!(
                        "Note: {} security warning(s) overridden with --force.",
                        findings_count
                    );
                }
            } else {
                match scan_verdict.as_deref() {
                    Some("block") => eprintln!("Import blocked: {}", message),
                    Some("warn") => eprintln!("Import requires --force: {}", message),
                    _ => eprintln!("{}", message),
                }
                std::process::exit(1);
            }
        }
        SkillAction::Export {
            name,
            format,
            output,
        } => {
            let (success, message, output_path) =
                client::send_skill_export(&name, &format, &output).await?;
            if success {
                println!("Exported to: {}", output_path.unwrap_or_default());
            } else {
                eprintln!("Export failed: {}", message);
                std::process::exit(1);
            }
        }
        SkillAction::Publish { name } => {
            let (success, message) = client::send_skill_publish(&name).await?;
            if success {
                println!("{}", message);
            } else {
                eprintln!("Publish failed: {}", message);
                std::process::exit(1);
            }
        }
    }

    Ok(())
}

fn render_skill_discovery(result: &zorai_protocol::SkillDiscoveryResultPublic) -> String {
    let mut lines = vec![
        format!("Confidence: {}", display_or_none(&result.confidence_tier)),
        format!(
            "Normalized intent: {}",
            display_or_none(&result.normalized_intent)
        ),
        format!(
            "Next action: {}",
            display_or_none(&result.recommended_action)
        ),
        format!("Mesh state: {}", display_or_none(&result.mesh_state)),
    ];

    if !result.rationale.is_empty() {
        lines.push(format!("Rationale: {}", result.rationale.join(", ")));
    }
    if !result.capability_family.is_empty() {
        lines.push(format!(
            "Capability family: {}",
            result.capability_family.join(" / ")
        ));
    }

    if result.candidates.is_empty() {
        lines.push("No matching skills found.".to_string());
        return lines.join("\n");
    }

    for (index, candidate) in result.candidates.iter().enumerate() {
        lines.push(format!(
            "{}. {} [{}] score={}",
            index + 1,
            candidate.skill_name,
            candidate.status,
            (candidate.score * 100.0).round() as u32
        ));
        let reasons = if candidate.reasons.is_empty() {
            "none".to_string()
        } else {
            candidate.reasons.join(", ")
        };
        lines.push(format!("   reasons: {reasons}"));
        if !candidate.matched_intents.is_empty() {
            lines.push(format!(
                "   matched intents: {}",
                candidate.matched_intents.join(", ")
            ));
        }
        lines.push(format!(
            "   trust/risk: {} / {}",
            display_or_none(&candidate.trust_tier),
            display_or_none(&candidate.risk_level)
        ));
        if candidate.canonical_pack {
            lines.push("   canonical pack: yes".to_string());
        }
        if !candidate.prerequisite_hints.is_empty() {
            lines.push(format!(
                "   prerequisites: {}",
                candidate.prerequisite_hints.join(", ")
            ));
        }
        if !candidate.prerequisite_connectors.is_empty() {
            lines.push(format!(
                "   prerequisite connectors: {}",
                candidate.prerequisite_connectors.join(", ")
            ));
        }
        if !candidate.delivery_modes.is_empty() {
            lines.push(format!(
                "   delivery modes: {}",
                candidate.delivery_modes.join(", ")
            ));
        }
        if !candidate.source_links.is_empty() {
            lines.push(format!(
                "   source links: {}",
                candidate.source_links.join(", ")
            ));
        }
        if let Some(approval_behavior) = candidate.approval_behavior.as_deref() {
            lines.push(format!("   approval: {approval_behavior}"));
        }
        if candidate.mobile_safe {
            lines.push("   mobile safe: yes".to_string());
        }
    }

    if let Some(next_cursor) = result.next_cursor.as_deref() {
        lines.push(format!("Next cursor: {next_cursor}"));
    }

    lines.join("\n")
}

fn display_or_none(value: &str) -> &str {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        "none"
    } else {
        trimmed
    }
}

fn parse_skill_discovery_session(value: Option<&str>) -> Result<Option<SessionId>> {
    value
        .map(|session| {
            session
                .parse()
                .with_context(|| format!("invalid session ID `{session}`"))
        })
        .transpose()
}

fn infer_skill_discovery_session_for_cwd(
    sessions: &[SessionInfo],
    cwd: &Path,
) -> Option<SessionId> {
    sessions
        .iter()
        .filter(|session| session.is_alive)
        .filter_map(|session| {
            let session_cwd = session.cwd.as_deref()?;
            let session_path = Path::new(session_cwd);
            cwd.starts_with(session_path)
                .then_some((session_path.components().count(), session.id))
        })
        .max_by_key(|(depth, _)| *depth)
        .map(|(_, session_id)| session_id)
}

async fn resolve_skill_discovery_session(value: Option<&str>) -> Result<Option<SessionId>> {
    if let Some(session_id) = parse_skill_discovery_session(value)? {
        return Ok(Some(session_id));
    }

    let cwd = match std::env::current_dir() {
        Ok(cwd) => cwd,
        Err(_) => return Ok(None),
    };
    let sessions = client::list_sessions().await?;
    Ok(infer_skill_discovery_session_for_cwd(&sessions, &cwd))
}

#[cfg(test)]
mod tests {
    use super::{
        infer_skill_discovery_session_for_cwd, parse_skill_discovery_session,
        render_skill_discovery,
    };
    use std::path::Path;
    use zorai_protocol::SessionInfo;

    #[test]
    fn render_skill_discovery_formats_ranked_candidates() {
        let rendered = render_skill_discovery(&zorai_protocol::SkillDiscoveryResultPublic {
            query: "debug panic".to_string(),
            normalized_intent: "debug panic root cause".to_string(),
            required: true,
            confidence_tier: "strong".to_string(),
            recommended_action: "read_skill systematic-debugging".to_string(),
            requires_approval: false,
            mesh_state: "fresh".to_string(),
            rationale: vec!["matched debug intent".to_string()],
            capability_family: vec!["development".to_string(), "debugging".to_string()],
            explicit_rationale_required: false,
            workspace_tags: vec!["rust".to_string()],
            next_cursor: Some("cursor:skill-2".to_string()),
            candidates: vec![zorai_protocol::SkillDiscoveryCandidatePublic {
                variant_id: "local:systematic-debugging:v1".to_string(),
                skill_name: "systematic-debugging".to_string(),
                variant_name: "v1".to_string(),
                relative_path: "generated/systematic-debugging/SKILL.md".to_string(),
                status: "active".to_string(),
                score: 0.93,
                confidence_tier: "strong".to_string(),
                reasons: vec![
                    "matched debug".to_string(),
                    "workspace rust".to_string(),
                    "14/16 successful uses".to_string(),
                ],
                matched_intents: vec!["debug panic root cause".to_string()],
                matched_trigger_phrases: vec!["panic".to_string()],
                context_tags: vec!["rust".to_string()],
                risk_level: "low".to_string(),
                trust_tier: "trusted_builtin".to_string(),
                source_kind: "builtin".to_string(),
                recommended_action: "read_skill systematic-debugging".to_string(),
                use_count: 16,
                success_count: 14,
                failure_count: 2,
                canonical_pack: false,
                delivery_modes: Vec::new(),
                prerequisite_hints: Vec::new(),
                prerequisite_connectors: Vec::new(),
                source_links: Vec::new(),
                mobile_safe: false,
                approval_behavior: None,
            }],
        });

        assert!(rendered.contains("Confidence: strong"));
        assert!(rendered.contains("Normalized intent: debug panic root cause"));
        assert!(rendered.contains("Next action: read_skill systematic-debugging"));
        assert!(rendered.contains("Mesh state: fresh"));
        assert!(rendered.contains("Rationale: matched debug intent"));
        assert!(rendered.contains("Capability family: development / debugging"));
        assert!(rendered.contains("1. systematic-debugging [active] score=93"));
        assert!(rendered.contains("reasons: matched debug, workspace rust, 14/16 successful uses"));
        assert!(rendered.contains("matched intents: debug panic root cause"));
        assert!(rendered.contains("trust/risk: trusted_builtin / low"));
        assert!(rendered.contains("Next cursor: cursor:skill-2"));
    }

    #[test]
    fn render_skill_discovery_renders_canonical_pack_metadata() {
        let rendered = render_skill_discovery(&zorai_protocol::SkillDiscoveryResultPublic {
            query: "daily brief".to_string(),
            normalized_intent: "daily brief".to_string(),
            required: true,
            confidence_tier: "strong".to_string(),
            recommended_action: "read_skill daily-brief".to_string(),
            requires_approval: false,
            mesh_state: "fresh".to_string(),
            rationale: vec!["matched daily brief".to_string()],
            capability_family: vec!["workflow".to_string()],
            explicit_rationale_required: false,
            workspace_tags: vec!["productivity".to_string()],
            next_cursor: None,
            candidates: vec![zorai_protocol::SkillDiscoveryCandidatePublic {
                variant_id: "fs:workflow-packs/daily-brief/SKILL.md".to_string(),
                skill_name: "daily-brief".to_string(),
                variant_name: "canonical".to_string(),
                relative_path: "workflow-packs/daily-brief/SKILL.md".to_string(),
                status: "active".to_string(),
                score: 0.96,
                confidence_tier: "strong".to_string(),
                reasons: vec!["matched daily brief".to_string()],
                matched_intents: vec!["daily brief".to_string()],
                matched_trigger_phrases: vec![],
                context_tags: vec!["workflow".to_string()],
                risk_level: "low".to_string(),
                trust_tier: "trusted_builtin".to_string(),
                source_kind: "builtin".to_string(),
                recommended_action: "read_skill daily-brief".to_string(),
                use_count: 0,
                success_count: 0,
                failure_count: 0,
                canonical_pack: true,
                delivery_modes: vec!["manual".to_string(), "routine".to_string()],
                prerequisite_hints: vec!["gmail optional".to_string()],
                prerequisite_connectors: Vec::new(),
                source_links: vec!["docs/operating/routines.md".to_string()],
                mobile_safe: true,
                approval_behavior: Some("read-only by default".to_string()),
            }],
        });

        assert!(rendered.contains("canonical pack: yes"));
        assert!(rendered.contains("prerequisites: gmail optional"));
        assert!(rendered.contains("delivery modes: manual, routine"));
        assert!(rendered.contains("source links: docs/operating/routines.md"));
        assert!(rendered.contains("approval: read-only by default"));
        assert!(rendered.contains("mobile safe: yes"));
        assert!(!rendered.contains("prerequisite connectors:"));
    }

    #[test]
    fn infer_skill_discovery_session_matches_exact_cwd() {
        let expected =
            uuid::Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").expect("valid test uuid");
        let sessions = vec![
            SessionInfo {
                id: expected,
                title: Some("repo".to_string()),
                cwd: Some("/workspace/repo".to_string()),
                cols: 80,
                rows: 24,
                created_at: 1,
                workspace_id: None,
                exit_code: None,
                is_alive: true,
                active_command: None,
            },
            SessionInfo {
                id: uuid::Uuid::parse_str("550e8400-e29b-41d4-a716-446655440001")
                    .expect("valid test uuid"),
                title: Some("other".to_string()),
                cwd: Some("/workspace/repo/subdir".to_string()),
                cols: 80,
                rows: 24,
                created_at: 2,
                workspace_id: None,
                exit_code: None,
                is_alive: true,
                active_command: None,
            },
        ];

        let selected =
            infer_skill_discovery_session_for_cwd(&sessions, Path::new("/workspace/repo"));

        assert_eq!(selected, Some(expected));
    }

    #[test]
    fn infer_skill_discovery_session_prefers_nearest_ancestor_cwd() {
        let repo_root =
            uuid::Uuid::parse_str("550e8400-e29b-41d4-a716-446655440010").expect("valid test uuid");
        let subdir =
            uuid::Uuid::parse_str("550e8400-e29b-41d4-a716-446655440011").expect("valid test uuid");
        let sessions = vec![
            SessionInfo {
                id: repo_root,
                title: Some("repo".to_string()),
                cwd: Some("/workspace/repo".to_string()),
                cols: 80,
                rows: 24,
                created_at: 1,
                workspace_id: None,
                exit_code: None,
                is_alive: true,
                active_command: None,
            },
            SessionInfo {
                id: subdir,
                title: Some("subdir".to_string()),
                cwd: Some("/workspace/repo/services/api".to_string()),
                cols: 80,
                rows: 24,
                created_at: 2,
                workspace_id: None,
                exit_code: None,
                is_alive: true,
                active_command: None,
            },
        ];

        let selected = infer_skill_discovery_session_for_cwd(
            &sessions,
            Path::new("/workspace/repo/services/api/src"),
        );

        assert_eq!(selected, Some(subdir));
    }

    #[test]
    fn parse_skill_discovery_session_rejects_invalid_uuid() {
        let result = parse_skill_discovery_session(Some("not-a-uuid"));

        assert!(result.is_err(), "invalid session id should be rejected");
    }

    #[test]
    fn parse_skill_discovery_session_returns_uuid() {
        let parsed = parse_skill_discovery_session(Some("550e8400-e29b-41d4-a716-446655440000"))
            .expect("valid session id should parse");

        assert_eq!(
            parsed,
            Some(
                uuid::Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000")
                    .expect("valid test uuid")
            )
        );
    }
}
