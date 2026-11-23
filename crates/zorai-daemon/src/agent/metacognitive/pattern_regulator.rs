use serde::{Deserialize, Serialize};

use crate::agent::engine::AgentEngine;
use crate::agent::explanation::ConfidenceBand;
use crate::agent::generate_message_id;
use crate::agent::metacognitive::introspector::{
    BiasSignal, InterventionStrength, IntrospectionOutcome,
};
use crate::agent::now_millis;
use crate::agent::types::{AgentMessage, AgentMessageKind, MessageRole, ToolCall};

const META_COGNITIVE_INTERVENTION_PREFIX: &str = "Meta-cognitive intervention:";
const META_COGNITIVE_REPEAT_PREFIX: &str = "Repeated count:";
const META_COGNITIVE_GROUP_PREFIX: &str = "Grouped count:";
const META_COGNITIVE_RECENT_SCAN_LIMIT: usize = 16;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum InterventionAction {
    Allow,
    Warn,
    Block,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RegulationDecision {
    pub action: InterventionAction,
    pub summary: String,
    #[serde(default)]
    pub warnings: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system_message: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confidence_adjustment: Option<f64>,
}

pub fn regulate(outcome: &IntrospectionOutcome, tool_call: &ToolCall) -> RegulationDecision {
    match outcome.strength {
        InterventionStrength::None => RegulationDecision {
            action: InterventionAction::Allow,
            summary: "No metacognitive intervention needed.".to_string(),
            warnings: Vec::new(),
            system_message: None,
            confidence_adjustment: None,
        },
        InterventionStrength::Warn => RegulationDecision {
            action: InterventionAction::Warn,
            summary: format!(
                "Meta-cognitive warning before `{}`: {}",
                tool_call.function.name,
                joined_warnings(&outcome.signals)
            ),
            warnings: outcome
                .signals
                .iter()
                .map(render_warning)
                .collect::<Vec<_>>(),
            system_message: Some(build_reflection_message(tool_call, &outcome.signals, false)),
            confidence_adjustment: outcome.confidence_adjustment,
        },
        InterventionStrength::Block => RegulationDecision {
            action: InterventionAction::Block,
            summary: format!(
                "Meta-cognitive block before `{}`: {}",
                tool_call.function.name,
                joined_warnings(&outcome.signals)
            ),
            warnings: outcome
                .signals
                .iter()
                .map(render_warning)
                .collect::<Vec<_>>(),
            system_message: Some(build_reflection_message(tool_call, &outcome.signals, true)),
            confidence_adjustment: outcome.confidence_adjustment,
        },
    }
}

impl AgentEngine {
    pub(crate) async fn append_metacognitive_system_message(&self, thread_id: &str, content: &str) {
        let now = now_millis();
        {
            let mut threads = self.threads.write().await;
            if let Some(thread) = threads.get_mut(thread_id) {
                if let Some(new_signature) = metacognitive_message_signature(content) {
                    if let Some(existing_message) = thread
                        .messages
                        .iter_mut()
                        .rev()
                        .take(META_COGNITIVE_RECENT_SCAN_LIMIT)
                        .find(|message| {
                            message.role == MessageRole::System
                                && metacognitive_message_signature(&message.content).as_ref()
                                    == Some(&new_signature)
                        })
                    {
                        existing_message.content = compact_repeated_metacognitive_message(
                            &existing_message.content,
                            content,
                            &new_signature,
                        );
                        existing_message.timestamp = now;
                        thread.updated_at = now;
                        drop(threads);
                        self.persist_thread_by_id(thread_id).await;
                        return;
                    }
                    if let Some(existing_message) = thread
                        .messages
                        .iter_mut()
                        .rev()
                        .take(META_COGNITIVE_RECENT_SCAN_LIMIT)
                        .find(|message| {
                            message.role == MessageRole::System
                                && is_metacognitive_intervention_content(&message.content)
                        })
                    {
                        existing_message.content = group_metacognitive_message(
                            &existing_message.content,
                            content,
                            &new_signature,
                        );
                        existing_message.timestamp = now;
                        thread.updated_at = now;
                        drop(threads);
                        self.persist_thread_by_id(thread_id).await;
                        return;
                    }
                }
                thread.messages.push(AgentMessage {
                    reasoning_tokens: None,
                    thinking_ms: None,
                    id: generate_message_id(),
                    role: MessageRole::System,
                    content: content.to_string(),
                    content_blocks: Vec::new(),
                    tool_calls: None,
                    tool_call_id: None,
                    tool_name: None,
                    tool_arguments: None,
                    tool_status: None,
                    weles_review: None,
                    input_tokens: 0,
                    output_tokens: 0,
                    cost: None,
                    provider: None,
                    model: None,
                    api_transport: None,
                    response_id: None,
                    upstream_message: None,
                    provider_final_result: None,
                    author_agent_id: None,
                    author_agent_name: None,
                    reasoning: None,
                    message_kind: AgentMessageKind::Normal,
                    compaction_strategy: None,
                    compaction_payload: None,
                    offloaded_payload_id: None,
                    tool_output_preview_path: None,
                    structural_refs: Vec::new(),
                    pinned_for_compaction: false,
                    timestamp: now,
                    feedback: None,
                });
                thread.updated_at = now;
            }
        }
        self.persist_thread_by_id(thread_id).await;
    }

    pub(crate) async fn reinforce_meta_cognitive_bias_occurrence(&self, bias_name: &str) {
        let maybe_model = {
            let mut model = self.meta_cognitive_self_model.write().await;
            if let Some(bias) = model.biases.iter_mut().find(|bias| bias.name == bias_name) {
                bias.occurrence_count = bias.occurrence_count.saturating_add(1);
                model.last_updated_ms = now_millis();
                Some(model.clone())
            } else {
                None
            }
        };

        if let Some(model) = maybe_model {
            if let Err(error) = self.persist_meta_cognitive_self_model(&model).await {
                tracing::warn!(bias_name = %bias_name, "failed to persist meta-cognitive bias reinforcement: {error}");
            }
        }
    }

    pub(crate) async fn record_meta_cognitive_workflow_profile(
        &self,
        tool_sequence: &[String],
        completed: bool,
    ) {
        if tool_sequence.is_empty() {
            return;
        }

        let now = now_millis();
        let model_to_persist = {
            let mut model = self.meta_cognitive_self_model.write().await;
            let profile_name = tool_sequence.join("__");
            let steps = tool_sequence.len() as u32;

            if let Some(profile) = model
                .workflow_profiles
                .iter_mut()
                .find(|profile| profile.name == profile_name)
            {
                let prior_steps = profile.avg_steps.max(1) as f64;
                profile.avg_steps = ((prior_steps + steps as f64) / 2.0).round() as u32;
                let prior_rate = profile.avg_success_rate.clamp(0.0, 1.0);
                let observed = if completed { 1.0 } else { 0.0 };
                profile.avg_success_rate = ((prior_rate + observed) / 2.0).clamp(0.0, 1.0);
                profile.typical_tools = tool_sequence.to_vec();
            } else {
                model
                    .workflow_profiles
                    .push(crate::agent::metacognitive::types::WorkflowProfile {
                        name: profile_name,
                        avg_success_rate: if completed { 1.0 } else { 0.0 },
                        avg_steps: steps,
                        typical_tools: tool_sequence.to_vec(),
                    });
            }
            model.last_updated_ms = now;
            model.clone()
        };

        if let Err(error) = self
            .persist_meta_cognitive_self_model(&model_to_persist)
            .await
        {
            tracing::warn!("failed to persist meta-cognitive workflow profile update: {error}");
        }
    }

    pub(crate) async fn apply_meta_cognitive_calibration_adjustment(
        &self,
        adjustment: f64,
        predicted_band: ConfidenceBand,
    ) {
        let now = now_millis();
        let model_to_persist = {
            let mut model = self.meta_cognitive_self_model.write().await;
            model.calibration_offset = (model.calibration_offset + adjustment).clamp(-0.35, 0.35);
            model.last_updated_ms = now;
            model.clone()
        };
        let predicted_success = matches!(
            predicted_band,
            ConfidenceBand::Confident | ConfidenceBand::Likely
        );
        self.calibration_tracker.write().await.record_observation(
            predicted_band,
            predicted_success,
            now,
        );

        if let Err(error) = self
            .persist_meta_cognitive_self_model(&model_to_persist)
            .await
        {
            tracing::warn!(
                adjustment,
                "failed to persist meta-cognitive calibration update: {error}"
            );
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct MetacognitiveMessageSignature {
    intervention_kind: String,
    planned_tool: String,
    arguments_hash: String,
}

fn metacognitive_message_signature(content: &str) -> Option<MetacognitiveMessageSignature> {
    let trimmed = content.trim_start();
    if !is_metacognitive_intervention_content(trimmed) {
        return None;
    }
    let mut lines = trimmed.lines();
    let header = lines.next()?.trim();
    let intervention_kind = header
        .strip_prefix(META_COGNITIVE_INTERVENTION_PREFIX)?
        .trim()
        .to_string();
    let planned_tool = trimmed
        .lines()
        .find_map(|line| line.trim().strip_prefix("Planned tool:"))
        .map(str::trim)
        .filter(|value| !value.is_empty())?
        .to_string();
    let arguments = trimmed
        .lines()
        .find_map(|line| line.trim().strip_prefix("Arguments:"))
        .map(str::trim)
        .unwrap_or("");
    let arguments_hash = arguments
        .strip_prefix("<same signature omitted; hash=")
        .and_then(|value| value.strip_suffix('>'))
        .map(str::to_string)
        .unwrap_or_else(|| stable_short_hash(arguments));
    Some(MetacognitiveMessageSignature {
        intervention_kind,
        planned_tool,
        arguments_hash,
    })
}

fn is_metacognitive_intervention_content(content: &str) -> bool {
    content
        .trim_start()
        .starts_with(META_COGNITIVE_INTERVENTION_PREFIX)
}

fn stable_short_hash(value: &str) -> String {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in value.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}

fn metacognitive_repeat_count(content: &str) -> u32 {
    content
        .lines()
        .find_map(|line| line.trim().strip_prefix(META_COGNITIVE_REPEAT_PREFIX))
        .and_then(|value| value.trim().parse::<u32>().ok())
        .unwrap_or(1)
}

fn metacognitive_group_count(content: &str) -> u32 {
    content
        .lines()
        .find_map(|line| line.trim().strip_prefix(META_COGNITIVE_GROUP_PREFIX))
        .and_then(|value| value.trim().parse::<u32>().ok())
        .unwrap_or_else(|| metacognitive_repeat_count(content))
}

fn metacognitive_tools(content: &str) -> Vec<String> {
    let mut tools = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if let Some(value) = trimmed.strip_prefix("Tools:") {
            for tool in value
                .split(',')
                .map(str::trim)
                .filter(|tool| !tool.is_empty())
            {
                push_unique_tool(&mut tools, tool);
            }
        } else if let Some(value) = trimmed
            .strip_prefix("Planned tool:")
            .or_else(|| trimmed.strip_prefix("Latest planned tool:"))
            .map(str::trim)
            .filter(|value| !value.is_empty() && *value != "<grouped>")
        {
            push_unique_tool(&mut tools, value);
        }
    }
    tools
}

fn push_unique_tool(tools: &mut Vec<String>, tool: &str) {
    if !tools.iter().any(|existing| existing == tool) {
        tools.push(tool.to_string());
    }
}

fn metacognitive_detected_risks(content: &str) -> String {
    content
        .split_once("Detected risks:")
        .map(|(_, tail)| {
            tail.lines()
                .take_while(|line| {
                    let trimmed = line.trim_start();
                    trimmed.starts_with('-') || trimmed.is_empty()
                })
                .collect::<Vec<_>>()
                .join("\n")
                .trim()
                .to_string()
        })
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "- repeated intervention".to_string())
}

fn group_metacognitive_message(
    existing_content: &str,
    latest_content: &str,
    signature: &MetacognitiveMessageSignature,
) -> String {
    let group_count = metacognitive_group_count(existing_content).saturating_add(1);
    let mut tools = metacognitive_tools(existing_content);
    push_unique_tool(&mut tools, &signature.planned_tool);
    let tools = if tools.is_empty() {
        signature.planned_tool.clone()
    } else {
        tools.join(", ")
    };
    let warnings = metacognitive_detected_risks(latest_content);

    format!(
        "{prefix} grouped advisory notices.\n{group_prefix} {group_count}\nPlanned tool: <grouped>\nTools: {tools}\nLatest planned tool: {latest_tool}\nArguments: <latest omitted; hash={hash}>\nDetected risks:\n{warnings}\nRepeated meta-cognitive interventions are grouped here instead of appended as separate system messages.",
        prefix = META_COGNITIVE_INTERVENTION_PREFIX,
        group_prefix = META_COGNITIVE_GROUP_PREFIX,
        latest_tool = signature.planned_tool,
        hash = signature.arguments_hash,
    )
}

fn compact_repeated_metacognitive_message(
    existing_content: &str,
    latest_content: &str,
    signature: &MetacognitiveMessageSignature,
) -> String {
    let repeat_count = metacognitive_repeat_count(existing_content).saturating_add(1);
    let warnings = metacognitive_detected_risks(latest_content);
    format!(
        "{prefix} {kind}\nPlanned tool: {tool}\nArguments: <same signature omitted; hash={hash}>\n{repeat_prefix} {repeat_count}\nDetected risks:\n{warnings}\nRepeated meta-cognitive interventions for this same tool call are compacted here instead of appended as separate system messages.",
        prefix = META_COGNITIVE_INTERVENTION_PREFIX,
        kind = signature.intervention_kind,
        tool = signature.planned_tool,
        hash = signature.arguments_hash,
        repeat_prefix = META_COGNITIVE_REPEAT_PREFIX,
    )
}

fn render_warning(signal: &BiasSignal) -> String {
    format!("{} — {}", signal.bias_name, signal.rationale)
}

fn joined_warnings(signals: &[BiasSignal]) -> String {
    signals
        .iter()
        .map(|signal| signal.bias_name.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}

fn build_reflection_message(
    tool_call: &ToolCall,
    signals: &[BiasSignal],
    hard_block: bool,
) -> String {
    let header = if hard_block {
        "Meta-cognitive intervention: tool call blocked before execution."
    } else {
        "Meta-cognitive intervention: warning before tool execution."
    };
    let warnings = signals
        .iter()
        .map(|signal| format!("- {}: {}", signal.bias_name, signal.mitigation_prompt))
        .collect::<Vec<_>>()
        .join("\n");
    let action = if hard_block {
        "Do not repeat the same action immediately. First reflect on why this approach may be failing, inspect fresh state, and choose a materially different next step."
    } else {
        "Before continuing, briefly reflect on whether this is the best next step and whether a different approach would reduce risk."
    };

    format!(
        "{header}\nPlanned tool: {}\nArguments: {}\nDetected risks:\n{}\n{}",
        tool_call.function.name, tool_call.function.arguments, warnings, action,
    )
}
