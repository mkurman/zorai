use super::gemini_interactions::reasoning_for_storage;
use super::*;

#[derive(Default)]
struct StepBuild {
    kind: String,
    text: String,
    signature: String,
    name: String,
    id: String,
    arguments: Option<serde_json::Value>,
    arguments_text: String,
}

#[derive(Default)]
pub(crate) struct GeminiStreamAssembler {
    pending_event: String,
    pending_data: String,
    steps: Vec<StepBuild>,
    content: String,
    summary: String,
    signature: String,
    tool_calls: Vec<ToolCall>,
    input_tokens: u64,
    output_tokens: u64,
    cache_read_input_tokens: u64,
    response_id: Option<String>,
    status: String,
    error: Option<String>,
    saw_event: bool,
    raw_json: String,
}

impl GeminiStreamAssembler {
    pub(crate) fn push_line(&mut self, line: &str) -> Result<Vec<CompletionChunk>> {
        if line.is_empty() {
            return self.dispatch_pending();
        }
        if let Some(event) = line.strip_prefix("event:") {
            self.pending_event = event.trim().to_string();
            self.saw_event = true;
            return Ok(Vec::new());
        }
        if let Some(data) = line.strip_prefix("data:") {
            if !self.pending_data.is_empty() {
                self.pending_data.push('\n');
            }
            self.pending_data.push_str(data.trim());
            self.saw_event = true;
            return Ok(Vec::new());
        }
        if !self.saw_event {
            self.raw_json.push_str(line);
        }
        Ok(Vec::new())
    }

    pub(crate) fn finish(&mut self) -> Result<Vec<CompletionChunk>> {
        let mut chunks = self.dispatch_pending()?;
        if !self.saw_event && !self.raw_json.trim().is_empty() {
            let payload: serde_json::Value = serde_json::from_str(self.raw_json.trim())?;
            self.ingest_interaction(&payload)?;
        }
        if let Some(error) = self.error.clone() {
            anyhow::bail!(error);
        }
        if self.status == "failed" {
            anyhow::bail!("Gemini interaction failed");
        }
        self.collect_from_steps();
        chunks.push(self.terminal_chunk());
        Ok(chunks)
    }

    fn dispatch_pending(&mut self) -> Result<Vec<CompletionChunk>> {
        if self.pending_data.is_empty() {
            self.pending_event.clear();
            return Ok(Vec::new());
        }
        let data = std::mem::take(&mut self.pending_data);
        let event = std::mem::take(&mut self.pending_event);
        let payload: serde_json::Value =
            serde_json::from_str(&data).unwrap_or(serde_json::Value::Null);
        let event = if event.is_empty() {
            payload
                .get("event_type")
                .and_then(|value| value.as_str())
                .unwrap_or("")
                .to_string()
        } else {
            event
        };
        self.apply_event(&event, &payload)
    }

    fn apply_event(
        &mut self,
        event: &str,
        payload: &serde_json::Value,
    ) -> Result<Vec<CompletionChunk>> {
        match event {
            "interaction.created" | "interaction.completed" => {
                if let Some(interaction) = payload.get("interaction") {
                    self.ingest_interaction(interaction)?;
                } else {
                    self.ingest_interaction(payload)?;
                }
                Ok(Vec::new())
            }
            "step.start" => {
                self.ensure_step(payload).kind = payload
                    .pointer("/step/type")
                    .and_then(|value| value.as_str())
                    .unwrap_or("")
                    .to_string();
                copy_step_fields(
                    self.ensure_step(payload),
                    payload.get("step").unwrap_or(payload),
                );
                Ok(Vec::new())
            }
            "step.delta" => self.apply_delta(payload),
            "step.stop" => Ok(Vec::new()),
            _ => Ok(Vec::new()),
        }
    }

    fn apply_delta(&mut self, payload: &serde_json::Value) -> Result<Vec<CompletionChunk>> {
        let delta = payload
            .get("delta")
            .cloned()
            .unwrap_or_else(|| payload.clone());
        let index = payload
            .get("index")
            .and_then(|value| value.as_u64())
            .unwrap_or(0) as usize;
        if self.steps.len() <= index {
            self.steps.resize_with(index + 1, StepBuild::default);
        }
        let delta_type = delta
            .get("type")
            .and_then(|value| value.as_str())
            .unwrap_or("");
        if let Some(signature) = delta.get("signature").and_then(|value| value.as_str()) {
            self.steps[index].signature = signature.to_string();
            if self.steps[index].kind.is_empty() {
                self.steps[index].kind = "thought".to_string();
            }
        }
        if let Some(name) = delta.get("name").and_then(|value| value.as_str()) {
            self.steps[index].name = name.to_string();
        }
        if let Some(arguments) = delta.get("arguments") {
            self.steps[index].arguments = Some(arguments.clone());
        }
        let Some(text) = delta.get("text").and_then(|value| value.as_str()) else {
            return Ok(Vec::new());
        };
        let kind = self.steps[index].kind.clone();
        self.steps[index].text.push_str(text);
        if kind == "function_call" {
            self.steps[index].arguments_text.push_str(text);
            return Ok(Vec::new());
        }
        if kind == "thought" {
            self.summary.push_str(text);
            return Ok(vec![CompletionChunk::Delta {
                content: String::new(),
                reasoning: Some(text.to_string()),
            }]);
        }
        if kind == "model_output" || delta_type == "text" {
            self.content.push_str(text);
            return Ok(vec![CompletionChunk::Delta {
                content: text.to_string(),
                reasoning: None,
            }]);
        }
        Ok(Vec::new())
    }

    fn ensure_step(&mut self, payload: &serde_json::Value) -> &mut StepBuild {
        let index = payload
            .get("index")
            .and_then(|value| value.as_u64())
            .unwrap_or(0) as usize;
        if self.steps.len() <= index {
            self.steps.resize_with(index + 1, StepBuild::default);
        }
        &mut self.steps[index]
    }

    pub(crate) fn ingest_interaction(&mut self, interaction: &serde_json::Value) -> Result<()> {
        if let Some(id) = interaction.get("id").and_then(|value| value.as_str()) {
            self.response_id = Some(id.to_string());
        }
        if let Some(status) = interaction.get("status").and_then(|value| value.as_str()) {
            self.status = status.to_string();
        }
        if let Some(error) = interaction.get("error").and_then(|value| {
            value.as_str().map(ToOwned::to_owned).or_else(|| {
                value
                    .get("message")
                    .and_then(|message| message.as_str())
                    .map(ToOwned::to_owned)
            })
        }) {
            self.error = Some(error);
        }
        if let Some(usage) = interaction.get("usage") {
            if let Some(input) = usage
                .get("total_input_tokens")
                .and_then(|value| value.as_u64())
            {
                self.input_tokens = input;
            }
            if let Some(output) = usage
                .get("total_output_tokens")
                .and_then(|value| value.as_u64())
            {
                self.output_tokens = output;
            }
            if let Some(cached) = cached_input_tokens(usage) {
                self.cache_read_input_tokens = cached;
            }
        }
        if let Some(steps) = interaction.get("steps").and_then(|value| value.as_array()) {
            for (index, step) in steps.iter().enumerate() {
                if self.steps.len() <= index {
                    self.steps.resize_with(index + 1, StepBuild::default);
                }
                let kind = step
                    .get("type")
                    .and_then(|value| value.as_str())
                    .unwrap_or("");
                if self.steps[index].kind.is_empty() {
                    self.steps[index].kind = kind.to_string();
                }
                copy_step_fields(&mut self.steps[index], step);
            }
        }
        Ok(())
    }

    fn collect_from_steps(&mut self) {
        if self.tool_calls.is_empty() {
            for step in &self.steps {
                if step.kind != "function_call" {
                    continue;
                }
                let name = step.name.trim();
                if name.is_empty() {
                    continue;
                }
                let mut arguments = step.arguments.clone().unwrap_or(serde_json::Value::Null);
                if tool_arguments_are_empty(&arguments) && !step.arguments_text.trim().is_empty() {
                    arguments = serde_json::from_str(&step.arguments_text)
                        .unwrap_or_else(|_| serde_json::json!({ "_raw": step.arguments_text }));
                }
                let id = if step.id.trim().is_empty() {
                    format!("gemini_call_{}", self.tool_calls.len())
                } else {
                    step.id.clone()
                };
                self.tool_calls.push(ToolCall::with_default_weles_review(
                    id,
                    ToolFunction {
                        name: name.to_string(),
                        arguments: tool_arguments_json(&arguments),
                    },
                ));
            }
        }
        if self.signature.is_empty() {
            if let Some(signature) = self
                .steps
                .iter()
                .find(|step| step.kind == "thought" && !step.signature.is_empty())
                .map(|step| step.signature.clone())
            {
                self.signature = signature;
            }
        }
        if self.summary.is_empty() {
            self.summary = self
                .steps
                .iter()
                .find(|step| step.kind == "thought")
                .map(|step| step.text.clone())
                .unwrap_or_default();
        }
        if self.content.is_empty() {
            self.content = self
                .steps
                .iter()
                .filter(|step| step.kind == "model_output")
                .map(|step| step.text.as_str())
                .collect::<Vec<_>>()
                .join("");
        }
    }

    fn terminal_chunk(&self) -> CompletionChunk {
        let reasoning = reasoning_for_storage(&self.summary, &self.signature);
        if self.tool_calls.is_empty() {
            CompletionChunk::Done {
                content: self.content.clone(),
                reasoning,
                input_tokens: self.input_tokens,
                output_tokens: self.output_tokens,
                cost_usd: None,
                stop_reason: Some(self.status.clone()).filter(|status| !status.is_empty()),
                stop_sequence: None,
                cache_creation_input_tokens: None,
                cache_read_input_tokens: (self.cache_read_input_tokens > 0)
                    .then_some(self.cache_read_input_tokens),
                server_tool_use: None,
                response_id: self.response_id.clone(),
                request_id: None,
                upstream_model: None,
                upstream_role: None,
                upstream_message_type: None,
                upstream_container: None,
                upstream_message: None,
                provider_final_result: None,
                upstream_thread_id: None,
            }
        } else {
            CompletionChunk::ToolCalls {
                tool_calls: self.tool_calls.clone(),
                content: Some(self.content.clone()),
                reasoning,
                input_tokens: Some(self.input_tokens),
                output_tokens: Some(self.output_tokens),
                stop_reason: Some("requires_action".to_string()),
                stop_sequence: None,
                response_id: self.response_id.clone(),
                request_id: None,
                upstream_model: None,
                upstream_role: None,
                upstream_message_type: None,
                upstream_container: None,
                upstream_message: None,
                provider_final_result: None,
                upstream_thread_id: None,
                cache_creation_input_tokens: None,
                cache_read_input_tokens: (self.cache_read_input_tokens > 0)
                    .then_some(self.cache_read_input_tokens),
                server_tool_use: None,
            }
        }
    }
}

fn tool_arguments_are_empty(value: &serde_json::Value) -> bool {
    match value {
        serde_json::Value::Null => true,
        serde_json::Value::Object(map) => map.is_empty(),
        serde_json::Value::String(text) => {
            let trimmed = text.trim();
            trimmed.is_empty() || trimmed == "{}" || trimmed == "[]"
        }
        _ => false,
    }
}

fn tool_arguments_json(value: &serde_json::Value) -> String {
    let mut current = value.clone();
    for _ in 0..4 {
        let serde_json::Value::String(text) = &current else {
            break;
        };
        match serde_json::from_str::<serde_json::Value>(text) {
            Ok(parsed) if parsed.is_object() || parsed.is_array() || parsed.is_string() => {
                current = parsed;
            }
            _ => break,
        }
    }
    match current {
        serde_json::Value::String(text) => serde_json::json!({ "_raw": text }).to_string(),
        other => other.to_string(),
    }
}

fn cached_input_tokens(usage: &serde_json::Value) -> Option<u64> {
    [
        "total_cached_tokens",
        "cached_content_token_count",
        "cached_tokens",
    ]
    .into_iter()
    .find_map(|key| usage.get(key).and_then(|value| value.as_u64()))
    .or_else(|| {
        usage
            .pointer("/input_tokens_details/cached_tokens")
            .and_then(|value| value.as_u64())
    })
}

fn copy_step_fields(step: &mut StepBuild, source: &serde_json::Value) {
    if let Some(kind) = source.get("type").and_then(|value| value.as_str()) {
        if step.kind.is_empty() {
            step.kind = kind.to_string();
        }
    }
    if let Some(id) = source.get("id").and_then(|value| value.as_str()) {
        step.id = id.to_string();
    }
    if let Some(name) = source.get("name").and_then(|value| value.as_str()) {
        step.name = name.to_string();
    }
    if let Some(signature) = source.get("signature").and_then(|value| value.as_str()) {
        step.signature = signature.to_string();
    }
    if let Some(arguments) = source.get("arguments") {
        step.arguments = Some(arguments.clone());
    }
    if step.text.is_empty() {
        if let Some(text) = source.get("text").and_then(|value| value.as_str()) {
            step.text = text.to_string();
        }
        if let Some(parts) = source.get("content").and_then(|value| value.as_array()) {
            step.text = parts
                .iter()
                .filter_map(|part| part.get("text").and_then(|value| value.as_str()))
                .collect::<Vec<_>>()
                .join("");
            if step.text.is_empty() {
                if let Some(image) = parts
                    .iter()
                    .find(|part| part.get("type").and_then(|value| value.as_str()) == Some("image"))
                {
                    let mime = image
                        .get("mime_type")
                        .and_then(|value| value.as_str())
                        .unwrap_or("image/png");
                    step.text = format!("[Generated image {mime}]");
                }
            }
        }
    }
}
