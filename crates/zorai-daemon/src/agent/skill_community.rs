//! Community skill packaging, import/export, and publish preparation.

use std::fs::{self, File};
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use flate2::Compression;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tar::{Archive, Builder};

use crate::agent::skill_security::{scan_skill_content, ScanReport, ScanTier, ScanVerdict};
use crate::history::{HistoryStore, ProvenanceEventRecord, SkillVariantRecord};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SkillFormat {
    ZoraiNative,
    AgentSkillsIo,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ZoraiSkillFrontmatter {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compatibility: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_yaml::Value>,
    #[serde(default)]
    pub allowed_tools: Vec<String>,
    #[serde(default)]
    pub zorai: ZoraiExtensions,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentSkillsFrontmatter {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compatibility: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_yaml::Value>,
    #[serde(default)]
    pub allowed_tools: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct ZoraiExtensions {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub maturity_status: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provenance_hash: Option<String>,
    #[serde(default)]
    pub context_tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variant_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin_trace: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub success_rate: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub use_count: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PublishMetadata {
    pub publisher_id: String,
    pub origin_trace_summary: String,
    pub success_rate: f64,
    pub use_count: u32,
    pub created_at: u64,
    pub content_hash: String,
    pub zorai_version: String,
    pub maturity_at_publish: String,
}

#[derive(Debug, Clone)]
pub(crate) enum ImportResult {
    Success {
        variant_id: String,
        scan_verdict: String,
    },
    Blocked {
        report_summary: String,
        findings_count: u32,
    },
    NeedsForce {
        report_summary: String,
        findings_count: u32,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ImportDecision {
    Import,
    Blocked,
    NeedsForce,
}

pub(super) fn split_frontmatter(content: &str) -> Option<(&str, &str)> {
    let rest = content.strip_prefix("---\n")?;
    let split_at = rest.find("\n---\n")?;
    let frontmatter = &rest[..split_at];
    let body = &rest[split_at + 5..];
    Some((frontmatter, body))
}

pub(super) fn detect_skill_format(frontmatter: &str) -> SkillFormat {
    if frontmatter.lines().any(|line| line.trim() == "zorai:") {
        SkillFormat::ZoraiNative
    } else {
        SkillFormat::AgentSkillsIo
    }
}

pub(super) fn sanitize_name_for_agentskills(name: &str) -> String {
    let mut normalized = name.to_ascii_lowercase().replace('_', "-");
    while normalized.contains("--") {
        normalized = normalized.replace("--", "-");
    }
    normalized.trim_matches('-').to_string()
}

pub(super) fn to_agentskills_format(zorai: &ZoraiSkillFrontmatter, body: &str) -> String {
    let export = AgentSkillsFrontmatter {
        name: sanitize_name_for_agentskills(&zorai.name),
        description: zorai.description.clone(),
        license: zorai.license.clone(),
        compatibility: zorai.compatibility.clone(),
        metadata: zorai.metadata.clone(),
        allowed_tools: zorai.allowed_tools.clone(),
    };
    let frontmatter = serde_yaml::to_string(&export).expect("serialize agentskills frontmatter");
    format!("---\n{}---\n{}", frontmatter, body)
}

pub(super) fn from_agentskills_format(content: &str) -> Result<ZoraiSkillFrontmatter> {
    let (frontmatter, _) =
        split_frontmatter(content).ok_or_else(|| anyhow!("missing YAML frontmatter"))?;
    let parsed: AgentSkillsFrontmatter =
        serde_yaml::from_str(frontmatter).context("parse agentskills.io frontmatter")?;

    Ok(ZoraiSkillFrontmatter {
        name: parsed.name,
        description: parsed.description,
        license: parsed.license,
        compatibility: parsed.compatibility,
        metadata: parsed.metadata,
        allowed_tools: parsed.allowed_tools,
        zorai: ZoraiExtensions::default(),
    })
}

pub(super) fn content_hash(content: &str) -> String {
    hex_sha256(content.as_bytes())
}

pub(super) fn publisher_id(machine_id: &str) -> String {
    hex_sha256(format!("zorai-publisher:{machine_id}").as_bytes())[..16].to_string()
}

pub(super) fn pack_skill(skill_dir: &Path, output: &Path) -> Result<()> {
    let tar_gz =
        File::create(output).with_context(|| format!("create archive {}", output.display()))?;
    let encoder = GzEncoder::new(tar_gz, Compression::default());
    let mut builder = Builder::new(encoder);

    for entry in fs::read_dir(skill_dir)
        .with_context(|| format!("read skill dir {}", skill_dir.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        if path.is_file() {
            let name = path
                .file_name()
                .ok_or_else(|| anyhow!("invalid file name in {}", path.display()))?;
            builder
                .append_path_with_name(&path, name)
                .with_context(|| format!("append {} to archive", path.display()))?;
        }
    }

    let encoder = builder.into_inner().context("finish tar archive")?;
    encoder.finish().context("finish gzip archive")?;
    Ok(())
}

pub(crate) fn unpack_skill(archive: &Path, target_dir: &Path) -> Result<()> {
    fs::create_dir_all(target_dir)
        .with_context(|| format!("create target dir {}", target_dir.display()))?;
    let tar_gz =
        File::open(archive).with_context(|| format!("open archive {}", archive.display()))?;
    let decoder = GzDecoder::new(tar_gz);
    let mut archive = Archive::new(decoder);
    archive
        .unpack(target_dir)
        .with_context(|| format!("unpack archive into {}", target_dir.display()))?;
    Ok(())
}

pub(crate) async fn import_community_skill(
    history: &HistoryStore,
    skill_content: &str,
    skill_name: &str,
    source_description: &str,
    tool_whitelist: &[String],
    force: bool,
    publisher_verified: bool,
    skills_root: &Path,
) -> Result<ImportResult> {
    let report = build_scan_report(skill_content, tool_whitelist, publisher_verified);
    let findings_count = report.findings.len() as u32;
    let summary = scan_report_summary(&report);

    match decide_import(report.verdict, force) {
        ImportDecision::Blocked => {
            record_import_provenance(
                history,
                "community_skill_import_blocked",
                &summary,
                &report,
                skill_name,
                source_description,
                publisher_verified,
                force,
                None,
            )
            .await?;
            Ok(ImportResult::Blocked {
                report_summary: summary,
                findings_count,
            })
        }
        ImportDecision::NeedsForce => {
            record_import_provenance(
                history,
                "community_skill_import_warned",
                &summary,
                &report,
                skill_name,
                source_description,
                publisher_verified,
                force,
                None,
            )
            .await?;
            Ok(ImportResult::NeedsForce {
                report_summary: summary,
                findings_count,
            })
        }
        ImportDecision::Import => {
            let skill_dir = skills_root.join("community").join(skill_name);
            if crate::agent::skill_library::is_skill_path_locked(
                skills_root,
                &skill_dir.join("SKILL.md"),
            ) {
                anyhow::bail!("community skill '{skill_name}' is locked and cannot be replaced");
            }
            tokio::fs::create_dir_all(&skill_dir)
                .await
                .with_context(|| format!("create community skill dir {}", skill_dir.display()))?;
            let skill_path = skill_dir.join("SKILL.md");
            tokio::fs::write(&skill_path, skill_content)
                .await
                .with_context(|| format!("write imported skill {}", skill_path.display()))?;

            let variant = history.register_skill_document(&skill_path).await?;
            history
                .update_skill_variant_status(&variant.variant_id, "draft")
                .await?;

            let event_type = if report.verdict == ScanVerdict::Warn {
                "community_skill_import_forced"
            } else {
                "community_skill_import_passed"
            };
            record_import_provenance(
                history,
                event_type,
                &summary,
                &report,
                skill_name,
                source_description,
                publisher_verified,
                force,
                Some(&variant.variant_id),
            )
            .await?;

            Ok(ImportResult::Success {
                variant_id: variant.variant_id,
                scan_verdict: format_scan_verdict(report.verdict),
            })
        }
    }
}

pub(crate) fn export_skill(
    skill_content: &str,
    format: &str,
    output_dir: &Path,
    skill_name: &str,
) -> Result<String> {
    let output_name = if format == "agentskills" {
        let (frontmatter, body) =
            split_frontmatter(skill_content).ok_or_else(|| anyhow!("missing YAML frontmatter"))?;
        let zorai: ZoraiSkillFrontmatter = match detect_skill_format(frontmatter) {
            SkillFormat::ZoraiNative => {
                serde_yaml::from_str(frontmatter).context("parse zorai frontmatter")?
            }
            SkillFormat::AgentSkillsIo => from_agentskills_format(skill_content)?,
        };
        let converted = to_agentskills_format(&zorai, body);
        let sanitized_name = sanitize_name_for_agentskills(skill_name);
        let destination = output_dir.join(&sanitized_name);
        fs::create_dir_all(&destination)
            .with_context(|| format!("create export dir {}", destination.display()))?;
        let skill_path = destination.join("SKILL.md");
        fs::write(&skill_path, converted)
            .with_context(|| format!("write export {}", skill_path.display()))?;
        skill_path
    } else {
        let destination = output_dir.join(skill_name);
        fs::create_dir_all(&destination)
            .with_context(|| format!("create export dir {}", destination.display()))?;
        let skill_path = destination.join("SKILL.md");
        fs::write(&skill_path, skill_content)
            .with_context(|| format!("write export {}", skill_path.display()))?;
        skill_path
    };

    Ok(output_name.to_string_lossy().to_string())
}

pub(crate) fn prepare_publish(
    skill_dir: &Path,
    variant: &SkillVariantRecord,
    machine_id: &str,
) -> Result<(PathBuf, PublishMetadata)> {
    let skill_path = skill_dir.join("SKILL.md");
    let content = fs::read_to_string(&skill_path)
        .with_context(|| format!("read skill file {}", skill_path.display()))?;

    let metadata = PublishMetadata {
        publisher_id: publisher_id(machine_id),
        origin_trace_summary: format!("Published local skill {}", variant.skill_name),
        success_rate: variant.success_rate(),
        use_count: variant.use_count,
        created_at: variant.created_at,
        content_hash: content_hash(&content),
        zorai_version: env!("CARGO_PKG_VERSION").to_string(),
        maturity_at_publish: variant.status.clone(),
    };

    let temp_root = std::env::temp_dir().join(format!(
        "zorai-publish-src-{}-{}",
        variant.skill_name, variant.variant_id
    ));
    if temp_root.exists() {
        let _ = fs::remove_dir_all(&temp_root);
    }
    fs::create_dir_all(&temp_root)
        .with_context(|| format!("create publish temp dir {}", temp_root.display()))?;
    let tarball_path = temp_root.join(format!("{}.tar.gz", variant.skill_name));
    pack_skill(skill_dir, &tarball_path)?;
    let persisted_path = std::env::temp_dir().join(format!(
        "zorai-publish-{}-{}.tar.gz",
        variant.skill_name, variant.variant_id
    ));
    fs::copy(&tarball_path, &persisted_path)
        .with_context(|| format!("persist tarball {}", persisted_path.display()))?;
    Ok((persisted_path, metadata))
}

fn hex_sha256(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

fn build_scan_report(
    skill_content: &str,
    tool_whitelist: &[String],
    publisher_verified: bool,
) -> ScanReport {
    let skip_llm_tier = publisher_verified;
    scan_skill_content(skill_content, tool_whitelist, skip_llm_tier)
}

fn decide_import(scan_verdict: ScanVerdict, force: bool) -> ImportDecision {
    match (scan_verdict, force) {
        (ScanVerdict::Block, _) => ImportDecision::Blocked,
        (ScanVerdict::Warn, false) => ImportDecision::NeedsForce,
        (ScanVerdict::Warn, true) | (ScanVerdict::Pass, _) => ImportDecision::Import,
    }
}

fn scan_report_summary(report: &ScanReport) -> String {
    let critical = report
        .findings
        .iter()
        .filter(|finding| {
            finding.severity == crate::agent::skill_security::FindingSeverity::Critical
        })
        .count();
    let suspicious = report
        .findings
        .iter()
        .filter(|finding| {
            finding.severity == crate::agent::skill_security::FindingSeverity::Suspicious
        })
        .count();
    format!(
        "Scan verdict={} critical={} suspicious={} findings={}",
        format_scan_verdict(report.verdict),
        critical,
        suspicious,
        report.findings.len()
    )
}

fn format_scan_verdict(verdict: ScanVerdict) -> String {
    match verdict {
        ScanVerdict::Pass => "pass",
        ScanVerdict::Warn => "warn",
        ScanVerdict::Block => "block",
    }
    .to_string()
}

async fn record_import_provenance(
    history: &HistoryStore,
    event_type: &str,
    summary: &str,
    report: &ScanReport,
    skill_name: &str,
    source_description: &str,
    publisher_verified: bool,
    force: bool,
    variant_id: Option<&str>,
) -> Result<()> {
    let details = serde_json::json!({
        "skill_name": skill_name,
        "source": source_description,
        "publisher_verified": publisher_verified,
        "force": force,
        "variant_id": variant_id,
        "scan_verdict": format_scan_verdict(report.verdict),
        "findings_count": report.findings.len(),
        "tier_results": report.tier_results.iter().map(|tier| serde_json::json!({
            "tier": match tier.tier {
                ScanTier::PatternBlocklist => "pattern_blocklist",
                ScanTier::StructuralValidation => "structural_validation",
                ScanTier::LlmReview => "llm_review",
            },
            "verdict": format_scan_verdict(tier.verdict),
            "findings_count": tier.findings_count,
            "skipped": tier.skipped,
        })).collect::<Vec<_>>()
    });

    let record = ProvenanceEventRecord {
        event_type,
        summary,
        details: &details,
        agent_id: "community-skill-import",
        goal_run_id: None,
        task_id: None,
        thread_id: None,
        approval_id: None,
        causal_trace_id: None,
        compliance_mode: "community-skill-security",
        sign: true,
        created_at: current_timestamp(),
    };
    history.record_provenance_event(&record).await
}

fn current_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
#[path = "skill_community/tests.rs"]
mod tests;
