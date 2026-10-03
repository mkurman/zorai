use super::super::*;
use crate::agent::types::AgentEvent;

fn context(thread: &str) -> TurnObservationContext {
    TurnObservationContext {
        relationships: MlflowTraceRelationships {
            thread_id: thread.into(),
            client_surface: Some("desktop".into()),
            user_message_id: Some("u1".into()),
            ..Default::default()
        },
        input: capture_text(
            "hello",
            MlflowCaptureMode::Guarded,
            MlflowContentKind::User,
            100,
        ),
        user_message_timestamp_ms: Some(900),
        autonomous: false,
        input_tokens: 0,
        output_tokens: 0,
        cache_read_input_tokens: 0,
        cache_creation_input_tokens: 0,
    }
}

fn done(thread: &str, id: &str) -> AgentEvent {
    AgentEvent::Done {
        thread_id: thread.into(),
        input_tokens: 10,
        output_tokens: 4,
        cost: Some(0.01),
        provider: Some("openai".into()),
        model: Some("gpt".into()),
        tps: None,
        generation_ms: Some(100),
        reasoning: None,
        upstream_message: None,
        provider_final_result: None,
        message_id: Some(id.into()),
    }
}

#[test]
fn simple_turn_reconstructs_root_and_inferred_llm_span() {
    let mut assembler = TurnTraceAssembler::new(MlflowTracingConfig::default());
    assembler.observe(
        1_000,
        AgentEvent::Delta {
            thread_id: "t1".into(),
            content: "Hello".into(),
        },
        context("t1"),
    );
    assembler.observe(1_100, done("t1", "a1"), context("t1"));
    let trace = assembler.drain_completed().pop().unwrap();
    assert_eq!(trace.relationships.thread_id, "t1");
    assert_eq!(trace.started_at_ms, 900);
    assert_eq!(trace.ended_at_ms, 1_100);
    assert_eq!(trace.output.as_ref().unwrap().value, "Hello");
    assert_eq!(trace.spans.len(), 1);
    assert_eq!(trace.spans[0].kind, MlflowSpanKind::Llm);
    assert!(trace.spans[0].timing_inferred);
    assert_eq!(trace.input_tokens, 10);
    assert_eq!(trace.output_tokens, 4);
}

#[test]
fn tool_span_uses_exact_call_result_boundaries() {
    let mut assembler = TurnTraceAssembler::new(MlflowTracingConfig::default());
    assembler.observe(
        1_000,
        AgentEvent::ToolCall {
            thread_id: "t1".into(),
            call_id: "c1".into(),
            name: "read_file".into(),
            arguments: "{\"path\":\"a\"}".into(),
            weles_review: None,
            message_id: None,
            turn_input_tokens: None,
            turn_output_tokens: None,
            turn_cost_usd: None,
        },
        context("t1"),
    );
    assembler.observe(
        1_250,
        AgentEvent::ToolResult {
            thread_id: "t1".into(),
            call_id: "c1".into(),
            name: "read_file".into(),
            content: "ok".into(),
            is_error: false,
            weles_review: None,
            message_id: None,
        },
        context("t1"),
    );
    assembler.observe(1_300, done("t1", "a1"), context("t1"));
    let trace = assembler.drain_completed().pop().unwrap();
    let tool = trace
        .spans
        .iter()
        .find(|span| span.kind == MlflowSpanKind::Tool)
        .unwrap();
    assert_eq!(tool.ended_at_ms - tool.started_at_ms, 250);
    assert!(!tool.timing_inferred);
    assert_eq!(tool.call_id.as_deref(), Some("c1"));
}

#[test]
fn stale_and_interleaved_turns_are_isolated() {
    let mut config = MlflowTracingConfig::default();
    config.stale_turn_timeout_ms = 10_000;
    let mut assembler = TurnTraceAssembler::new(config);
    assembler.observe(
        1_000,
        AgentEvent::Delta {
            thread_id: "a".into(),
            content: "A".into(),
        },
        context("a"),
    );
    assembler.observe(
        2_000,
        AgentEvent::Delta {
            thread_id: "b".into(),
            content: "B".into(),
        },
        context("b"),
    );
    assembler.observe(2_100, done("b", "b1"), context("b"));
    assembler.expire_stale(11_000);
    let traces = assembler.drain_completed();
    assert_eq!(traces.len(), 2);
    assert!(traces
        .iter()
        .any(|trace| trace.relationships.thread_id == "a"
            && trace.partial_reason.as_deref() == Some("stale_timeout")));
    assert!(traces.iter().any(
        |trace| trace.relationships.thread_id == "b" && trace.outcome == MlflowTraceOutcome::Ok
    ));
}

#[test]
fn broadcast_lag_closes_active_turn_as_partial() {
    let mut assembler = TurnTraceAssembler::new(MlflowTracingConfig::default());
    assembler.observe(
        1_000,
        AgentEvent::Delta {
            thread_id: "t".into(),
            content: "x".into(),
        },
        context("t"),
    );
    assembler.mark_broadcast_lag(1_050);
    assert_eq!(assembler.active_len(), 0);
    let trace = assembler.drain_completed().pop().unwrap();
    assert_eq!(trace.partial_reason.as_deref(), Some("broadcast_lag"));
}

#[test]
fn configured_span_limit_is_hard() {
    let mut config = MlflowTracingConfig::default();
    config.max_spans_per_trace = 1;
    let mut assembler = TurnTraceAssembler::new(config);
    assembler.observe(
        1_000,
        AgentEvent::Delta {
            thread_id: "t".into(),
            content: "thinking".into(),
        },
        context("t"),
    );
    assembler.observe(
        1_050,
        AgentEvent::ToolCall {
            thread_id: "t".into(),
            call_id: "c1".into(),
            name: "read_file".into(),
            arguments: "{}".into(),
            weles_review: None,
            message_id: None,
            turn_input_tokens: None,
            turn_output_tokens: None,
            turn_cost_usd: None,
        },
        context("t"),
    );
    assembler.observe(1_100, done("t", "a1"), context("t"));
    let trace = assembler.drain_completed().pop().unwrap();
    assert_eq!(trace.spans.len(), 1);
}

fn done_without_usage(thread: &str, id: &str) -> AgentEvent {
    AgentEvent::Done {
        thread_id: thread.into(),
        input_tokens: 0,
        output_tokens: 0,
        cost: None,
        provider: Some("openai".into()),
        model: Some("gpt".into()),
        tps: None,
        generation_ms: Some(100),
        reasoning: None,
        upstream_message: None,
        provider_final_result: None,
        message_id: Some(id.into()),
    }
}

#[test]
fn zero_token_done_uses_persisted_turn_usage() {
    let mut assembler = TurnTraceAssembler::new(MlflowTracingConfig::default());
    assembler.observe(
        1_000,
        AgentEvent::Delta {
            thread_id: "t1".into(),
            content: "Hello".into(),
        },
        context("t1"),
    );
    let mut done_context = context("t1");
    done_context.input_tokens = 128;
    done_context.output_tokens = 32;
    done_context.cache_read_input_tokens = 40;
    done_context.cache_creation_input_tokens = 8;
    assembler.observe(1_100, done_without_usage("t1", "a1"), done_context);
    let trace = assembler.drain_completed().pop().unwrap();
    assert_eq!(
        trace.input_tokens, 128,
        "MLflow usage must come from persisted assistant messages when Done reports 0"
    );
    assert_eq!(trace.output_tokens, 32);
    assert_eq!(
        trace.cache_read_input_tokens, 40,
        "cache read must survive a zero-usage Done event"
    );
    assert_eq!(
        trace.cache_creation_input_tokens, 8,
        "cache write must survive a zero-usage Done event"
    );
}

#[test]
fn later_user_context_does_not_overwrite_frozen_turn_input() {
    let mut assembler = TurnTraceAssembler::new(MlflowTracingConfig::default());
    assembler.observe(
        1_000,
        AgentEvent::Delta {
            thread_id: "t1".into(),
            content: "A1".into(),
        },
        context("t1"),
    );
    let mut second = context("t1");
    second.input = capture_text(
        "second user",
        MlflowCaptureMode::Guarded,
        MlflowContentKind::User,
        100,
    );
    second.relationships.user_message_id = Some("u2".into());
    assembler.observe(
        1_050,
        AgentEvent::Delta {
            thread_id: "t1".into(),
            content: " more".into(),
        },
        second,
    );
    assembler.observe(1_100, done("t1", "a1"), context("t1"));
    let trace = assembler.drain_completed().pop().unwrap();
    assert_eq!(
        trace.input.as_ref().unwrap().value,
        "hello",
        "turn input is frozen at the first observed event, not the latest user message"
    );
    assert_eq!(trace.output.as_ref().unwrap().value, "A1 more");
}

#[test]
fn interrupt_exports_partial_turn_including_open_tools() {
    let mut assembler = TurnTraceAssembler::new(MlflowTracingConfig::default());
    assembler.observe(
        1_000,
        AgentEvent::Delta {
            thread_id: "t1".into(),
            content: "plan".into(),
        },
        context("t1"),
    );
    assembler.observe(
        1_050,
        AgentEvent::ToolCall {
            thread_id: "t1".into(),
            call_id: "c1".into(),
            name: "read_file".into(),
            arguments: "{}".into(),
            weles_review: None,
            message_id: None,
            turn_input_tokens: None,
            turn_output_tokens: None,
            turn_cost_usd: None,
        },
        context("t1"),
    );
    assert!(assembler.interrupt_turn("t1", 1_080));
    let trace = assembler.drain_completed().pop().unwrap();
    assert_eq!(trace.partial_reason.as_deref(), Some("interrupted"));
    assert!(
        trace
            .spans
            .iter()
            .any(|span| span.kind == MlflowSpanKind::Tool),
        "send-now must export the tool stack that was in flight"
    );
    assert_eq!(assembler.active_len(), 0);
}

#[test]
fn interrupt_without_active_turn_does_not_emit() {
    let mut assembler = TurnTraceAssembler::new(MlflowTracingConfig::default());
    assert!(!assembler.interrupt_turn("t1", 1_000));
    assert!(assembler.drain_completed().is_empty());
}

#[test]
fn interrupt_then_follow_up_starts_a_new_turn() {
    let mut assembler = TurnTraceAssembler::new(MlflowTracingConfig::default());
    assembler.observe(
        1_000,
        AgentEvent::Delta {
            thread_id: "t1".into(),
            content: "A1".into(),
        },
        context("t1"),
    );
    assembler.interrupt_turn("t1", 1_050);
    assembler.drain_completed();
    let mut follow_up = context("t1");
    follow_up.input = capture_text(
        "follow up",
        MlflowCaptureMode::Guarded,
        MlflowContentKind::User,
        100,
    );
    assembler.observe(
        1_100,
        AgentEvent::Delta {
            thread_id: "t1".into(),
            content: "A2".into(),
        },
        follow_up.clone(),
    );
    assembler.observe(1_200, done("t1", "a2"), follow_up);
    let trace = assembler.drain_completed().pop().unwrap();
    assert_eq!(trace.input.as_ref().unwrap().value, "follow up");
    assert_eq!(trace.output.as_ref().unwrap().value, "A2");
}

fn error_event(thread: &str, message: &str) -> AgentEvent {
    AgentEvent::Error {
        thread_id: thread.into(),
        message: message.into(),
    }
}

fn retry_event(thread: &str, attempt: u32) -> AgentEvent {
    AgentEvent::RetryStatus {
        thread_id: thread.into(),
        phase: "provider".into(),
        attempt,
        max_retries: 3,
        delay_ms: 25,
        failure_class: "timeout".into(),
        message: "retry".into(),
    }
}

fn approval_event(thread: &str) -> AgentEvent {
    AgentEvent::ApprovalRequired {
        thread_id: thread.into(),
        approval_id: "ap1".into(),
        command: "ls".into(),
        rationale: None,
        reasons: Vec::new(),
        risk_level: "low".into(),
        blast_radius: "low".into(),
    }
}

#[test]
fn error_then_done_does_not_open_a_second_trace() {
    let mut assembler = TurnTraceAssembler::new(MlflowTracingConfig::default());
    assembler.observe(
        1_000,
        AgentEvent::Delta {
            thread_id: "t1".into(),
            content: "Hello".into(),
        },
        context("t1"),
    );
    assembler.observe(
        1_050,
        error_event("t1", "Tool execution limit reached"),
        context("t1"),
    );
    assembler.observe(1_060, done("t1", "a1"), context("t1"));
    let traces = assembler.drain_completed();
    assert_eq!(
        traces.len(),
        1,
        "deferred Done after Error must not finish a second empty turn that would steal the next FIFO anchor"
    );
    assert_eq!(traces[0].outcome, MlflowTraceOutcome::Error);
    assert_eq!(assembler.active_len(), 0);
}

#[test]
fn late_retry_and_approval_do_not_open_empty_turns() {
    let mut assembler = TurnTraceAssembler::new(MlflowTracingConfig::default());
    assembler.observe(
        1_000,
        AgentEvent::Delta {
            thread_id: "t1".into(),
            content: "Hello".into(),
        },
        context("t1"),
    );
    assembler.observe(1_100, done("t1", "a1"), context("t1"));
    assembler.drain_completed();
    assembler.observe(1_200, retry_event("t1", 1), context("t1"));
    assembler.observe(1_250, approval_event("t1"), context("t1"));
    assert_eq!(assembler.active_len(), 0);
    assert!(
        assembler.drain_completed().is_empty(),
        "late approval/retry after the real turn finished must not export a stale empty trace"
    );
}

#[test]
fn retry_and_approval_during_a_live_turn_are_recorded_as_events() {
    let mut assembler = TurnTraceAssembler::new(MlflowTracingConfig::default());
    assembler.observe(
        1_000,
        AgentEvent::Delta {
            thread_id: "t1".into(),
            content: "Hello".into(),
        },
        context("t1"),
    );
    assembler.observe(1_020, retry_event("t1", 1), context("t1"));
    assembler.observe(1_040, approval_event("t1"), context("t1"));
    assembler.observe(1_100, done("t1", "a1"), context("t1"));
    let trace = assembler.drain_completed().pop().unwrap();
    assert_eq!(
        trace
            .events
            .iter()
            .map(|event| event.name.as_str())
            .collect::<Vec<_>>(),
        vec!["retry_status", "approval_required"]
    );
    assert_eq!(trace.dropped_events, 0);
}

#[test]
fn max_events_per_span_drops_extra_retry_events() {
    let mut config = MlflowTracingConfig::default();
    config.max_events_per_span = 2;
    let mut assembler = TurnTraceAssembler::new(config);
    assembler.observe(
        1_000,
        AgentEvent::Delta {
            thread_id: "t1".into(),
            content: "Hello".into(),
        },
        context("t1"),
    );
    assembler.observe(1_010, retry_event("t1", 1), context("t1"));
    assembler.observe(1_020, retry_event("t1", 2), context("t1"));
    assembler.observe(1_030, retry_event("t1", 3), context("t1"));
    assembler.observe(1_100, done("t1", "a1"), context("t1"));
    let trace = assembler.drain_completed().pop().unwrap();
    assert_eq!(trace.events.len(), 2);
    assert_eq!(trace.dropped_events, 1);
}

#[test]
fn max_trace_bytes_shrinks_exported_payloads() {
    let mut config = MlflowTracingConfig::default();
    config.max_trace_bytes = 16 * 1024;
    let mut assembler = TurnTraceAssembler::new(config);
    assembler.observe(
        1_000,
        AgentEvent::Delta {
            thread_id: "t1".into(),
            content: "assistant reply with spaces. ".repeat(2_000),
        },
        context("t1"),
    );
    assembler.observe(1_100, done("t1", "a1"), context("t1"));
    let trace = assembler.drain_completed().pop().unwrap();
    let encoded = encode_otlp_batch(std::slice::from_ref(&trace)).unwrap();
    assert!(
        encoded.len() <= 16 * 1024,
        "encoded OTLP must honor max_trace_bytes, got {}",
        encoded.len()
    );
    assert_eq!(trace.partial_reason.as_deref(), Some("trace_bytes"));
    assert!(
        trace.output.as_ref().is_some_and(|output| output.truncated),
        "byte-budget enforcement must mark captured output as truncated"
    );
}
