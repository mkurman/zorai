use super::*;
use serde::{Deserialize, Serialize};

/// Configuration for idle-time memory consolidation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsolidationConfig {
    /// Whether memory consolidation is enabled.
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Maximum wall-clock seconds a single consolidation tick may use. Per D-02.
    #[serde(default = "default_consolidation_budget_secs")]
    pub budget_secs: u64,
    /// Seconds of operator inactivity before consolidation may begin. Per D-01.
    #[serde(default = "default_consolidation_idle_threshold_secs")]
    pub idle_threshold_secs: u64,
    /// Days to keep tombstoned (superseded) memory facts before permanent deletion. Per D-05.
    #[serde(default = "default_consolidation_tombstone_ttl_days")]
    pub tombstone_ttl_days: u64,
    /// Number of successful repetitions before a heuristic is promoted. Per D-07.
    #[serde(default = "default_consolidation_heuristic_promotion_threshold")]
    pub heuristic_promotion_threshold: u32,
    /// Half-life in hours for exponential memory fact decay. Per D-04.
    #[serde(default = "default_consolidation_memory_decay_half_life_hours")]
    pub memory_decay_half_life_hours: f64,
    /// Whether to auto-resume interrupted goal runs on daemon restart. Per D-11.
    #[serde(default)]
    pub auto_resume_goal_runs: bool,
    /// Confidence threshold below which decayed facts are tombstoned. Per MEMO-02.
    #[serde(default = "default_consolidation_fact_decay_supersede_threshold")]
    pub fact_decay_supersede_threshold: f64,
}

fn default_consolidation_budget_secs() -> u64 {
    30
}
fn default_consolidation_idle_threshold_secs() -> u64 {
    300
}
fn default_consolidation_tombstone_ttl_days() -> u64 {
    7
}
fn default_consolidation_heuristic_promotion_threshold() -> u32 {
    3
}
fn default_consolidation_memory_decay_half_life_hours() -> f64 {
    69.0
}
fn default_consolidation_fact_decay_supersede_threshold() -> f64 {
    0.2
}

impl Default for ConsolidationConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            budget_secs: default_consolidation_budget_secs(),
            idle_threshold_secs: default_consolidation_idle_threshold_secs(),
            tombstone_ttl_days: default_consolidation_tombstone_ttl_days(),
            heuristic_promotion_threshold: default_consolidation_heuristic_promotion_threshold(),
            memory_decay_half_life_hours: default_consolidation_memory_decay_half_life_hours(),
            auto_resume_goal_runs: false,
            fact_decay_supersede_threshold: default_consolidation_fact_decay_supersede_threshold(),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct MemoryLimitsConfig {
    #[serde(default = "default_memory_limits_soul_chars")]
    pub soul_chars: usize,
    #[serde(default = "default_memory_limits_memory_chars")]
    pub memory_chars: usize,
    #[serde(default = "default_memory_limits_user_chars")]
    pub user_chars: usize,
}

fn default_memory_limits_soul_chars() -> usize {
    2_000
}
fn default_memory_limits_memory_chars() -> usize {
    3_600
}
fn default_memory_limits_user_chars() -> usize {
    1_800
}

impl Default for MemoryLimitsConfig {
    fn default() -> Self {
        Self {
            soul_chars: default_memory_limits_soul_chars(),
            memory_chars: default_memory_limits_memory_chars(),
            user_chars: default_memory_limits_user_chars(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct SkillLibraryConfig {
    #[serde(default)]
    pub locked: Vec<String>,
    #[serde(default)]
    pub disabled: Vec<String>,
    #[serde(default)]
    pub preferred: std::collections::BTreeMap<String, Vec<String>>,
}

/// Maturity stage of a skill variant as it progresses through discovery.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum SkillMaturityStatus {
    Draft,
    Testing,
    Active,
    Proven,
    #[serde(rename = "promoted_to_canonical")]
    PromotedToCanonical,
}

impl SkillMaturityStatus {
    /// Return the snake_case string representation.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Testing => "testing",
            Self::Active => "active",
            Self::Proven => "proven",
            Self::PromotedToCanonical => "promoted_to_canonical",
        }
    }

    /// Parse from a status string, supporting both snake_case and legacy kebab-case.
    pub fn from_status_str(s: &str) -> Option<Self> {
        match s {
            "draft" => Some(Self::Draft),
            "testing" => Some(Self::Testing),
            "active" => Some(Self::Active),
            "proven" => Some(Self::Proven),
            "promoted_to_canonical" | "promoted-to-canonical" => Some(Self::PromotedToCanonical),
            _ => None,
        }
    }
}

/// Thresholds for deciding whether an execution trace qualifies as a skill candidate.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillDiscoveryConfig {
    /// Minimum number of tool calls in the trace to consider it complex enough.
    #[serde(default = "default_sd_min_tool_count")]
    pub min_tool_count: usize,
    /// Minimum number of replan events to indicate adaptive problem-solving.
    #[serde(default = "default_sd_min_replan_count")]
    pub min_replan_count: u32,
    /// Minimum quality score (0.0-1.0) to consider the trace successful enough.
    #[serde(default = "default_sd_min_quality_score")]
    pub min_quality_score: f64,
    /// Jaccard similarity threshold below which a sequence is considered novel.
    #[serde(default = "default_sd_novelty_threshold")]
    pub novelty_similarity_threshold: f64,
}

fn default_sd_min_tool_count() -> usize {
    8
}
fn default_sd_min_replan_count() -> u32 {
    1
}
fn default_sd_min_quality_score() -> f64 {
    0.8
}
fn default_sd_novelty_threshold() -> f64 {
    0.7
}

impl Default for SkillDiscoveryConfig {
    fn default() -> Self {
        Self {
            min_tool_count: default_sd_min_tool_count(),
            min_replan_count: default_sd_min_replan_count(),
            min_quality_score: default_sd_min_quality_score(),
            novelty_similarity_threshold: default_sd_novelty_threshold(),
        }
    }
}

/// Runtime skill recommender thresholds and behavior controls.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillRecommendationConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_skill_recommendation_discovery_backend")]
    pub discovery_backend: String,
    #[serde(default = "default_true")]
    pub require_read_on_strong_match: bool,
    #[serde(default = "default_skill_recommendation_strong_match_threshold")]
    pub strong_match_threshold: f64,
    #[serde(default = "default_skill_recommendation_weak_match_threshold")]
    pub weak_match_threshold: f64,
    #[serde(default = "default_skill_recommendation_novelty_distance_weight")]
    pub novelty_distance_weight: f64,
    #[serde(default = "default_true")]
    pub background_community_search: bool,
    #[serde(default = "default_skill_recommendation_community_preapprove_timeout_secs")]
    pub community_preapprove_timeout_secs: u64,
    #[serde(default = "default_skill_recommendation_suggest_global_enable_after_approvals")]
    pub suggest_global_enable_after_approvals: u32,
    #[serde(default)]
    pub llm_normalize_on_no_match: bool,
    #[serde(default)]
    pub llm_semantic_search_on_no_match: bool,
    #[serde(default = "default_skill_recommendation_llm_semantic_search_max_skills")]
    pub llm_semantic_search_max_skills: u32,
}

fn default_skill_recommendation_discovery_backend() -> String {
    "mesh".to_string()
}

fn default_skill_recommendation_strong_match_threshold() -> f64 {
    0.85
}

fn default_skill_recommendation_weak_match_threshold() -> f64 {
    0.60
}

fn default_skill_recommendation_novelty_distance_weight() -> f64 {
    0.05
}

fn default_skill_recommendation_community_preapprove_timeout_secs() -> u64 {
    30
}

fn default_skill_recommendation_suggest_global_enable_after_approvals() -> u32 {
    3
}

fn default_skill_recommendation_llm_semantic_search_max_skills() -> u32 {
    64
}

impl Default for SkillRecommendationConfig {
    fn default() -> Self {
        Self {
            enabled: default_true(),
            discovery_backend: default_skill_recommendation_discovery_backend(),
            require_read_on_strong_match: default_true(),
            strong_match_threshold: default_skill_recommendation_strong_match_threshold(),
            weak_match_threshold: default_skill_recommendation_weak_match_threshold(),
            novelty_distance_weight: default_skill_recommendation_novelty_distance_weight(),
            background_community_search: default_true(),
            community_preapprove_timeout_secs:
                default_skill_recommendation_community_preapprove_timeout_secs(),
            suggest_global_enable_after_approvals:
                default_skill_recommendation_suggest_global_enable_after_approvals(),
            llm_normalize_on_no_match: false,
            llm_semantic_search_on_no_match: false,
            llm_semantic_search_max_skills:
                default_skill_recommendation_llm_semantic_search_max_skills(),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RoutingMode {
    Probabilistic,
    Deterministic,
}

impl Default for RoutingMode {
    fn default() -> Self {
        Self::Probabilistic
    }
}

/// Controls learned specialist routing behavior.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub method: RoutingMode,
    #[serde(default = "default_routing_bayesian_alpha")]
    pub bayesian_alpha: f64,
    #[serde(default = "default_routing_confidence_threshold")]
    pub confidence_threshold: f64,
    #[serde(default = "default_routing_recency_half_life_hours")]
    pub recency_decay_half_life_hours: f64,
    #[serde(default = "default_routing_confidence_ema_alpha")]
    pub confidence_ema_alpha: f64,
}

fn default_routing_bayesian_alpha() -> f64 {
    1.0
}

fn default_routing_confidence_threshold() -> f64 {
    0.3
}

fn default_routing_recency_half_life_hours() -> f64 {
    168.0
}

fn default_routing_confidence_ema_alpha() -> f64 {
    0.3
}

impl Default for RoutingConfig {
    fn default() -> Self {
        Self {
            enabled: default_true(),
            method: RoutingMode::default(),
            bayesian_alpha: default_routing_bayesian_alpha(),
            confidence_threshold: default_routing_confidence_threshold(),
            recency_decay_half_life_hours: default_routing_recency_half_life_hours(),
            confidence_ema_alpha: default_routing_confidence_ema_alpha(),
        }
    }
}

/// Success thresholds for promoting a skill variant through maturity stages.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillPromotionConfig {
    /// Successful uses required to advance Testing -> Active.
    #[serde(default = "default_sp_testing_to_active")]
    pub testing_to_active: u32,
    /// Successful uses required to advance Active -> Proven.
    #[serde(default = "default_sp_active_to_proven")]
    pub active_to_proven: u32,
    /// Successful uses required to advance Proven -> PromotedToCanonical.
    #[serde(default = "default_sp_proven_to_canonical")]
    pub proven_to_canonical: u32,
}

fn default_sp_testing_to_active() -> u32 {
    3
}
fn default_sp_active_to_proven() -> u32 {
    5
}
fn default_sp_proven_to_canonical() -> u32 {
    10
}

impl Default for SkillPromotionConfig {
    fn default() -> Self {
        Self {
            testing_to_active: default_sp_testing_to_active(),
            active_to_proven: default_sp_active_to_proven(),
            proven_to_canonical: default_sp_proven_to_canonical(),
        }
    }
}

/// Outcome of a single consolidation tick.
#[derive(Debug, Clone, Default)]
pub struct ConsolidationResult {
    pub traces_reviewed: usize,
    pub distillation_ran: bool,
    pub distillation_threads_analyzed: usize,
    pub distillation_candidates_generated: usize,
    pub distillation_auto_applied: usize,
    pub distillation_queued_for_review: usize,
    pub forge_ran: bool,
    pub forge_traces_analyzed: usize,
    pub forge_patterns_detected: usize,
    pub forge_hints_generated: usize,
    pub forge_hints_auto_applied: usize,
    pub forge_hints_logged_only: usize,
    pub facts_decayed: usize,
    pub tombstones_purged: usize,
    pub facts_refined: usize,
    pub skipped_reason: Option<String>,
    /// Skill discovery fields (Phase 6).
    pub skill_candidates_flagged: usize,
    pub skills_drafted: usize,
    pub skills_tested: usize,
    pub skills_promoted: usize,
}
