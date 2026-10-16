use super::*;
use zorai_shared::providers::PROVIDER_ID_CUSTOM;

async fn spawn_retry_reset_regression_server(request_counter: Arc<AtomicUsize>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind retry reset regression server");
    let addr = listener
        .local_addr()
        .expect("retry reset regression server addr");

    tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = listener.accept().await else {
                break;
            };
            let request_counter = request_counter.clone();
            tokio::spawn(async move {
                let attempt = request_counter.fetch_add(1, Ordering::SeqCst);
                let _request =
                    read_http_request(&mut socket, "retry reset regression request").await;

                let response = match attempt {
                    0 | 2 => None,
                    1 => Some(concat!(
                        "HTTP/1.1 200 OK\r\n",
                        "content-type: text/event-stream\r\n",
                        "cache-control: no-cache\r\n",
                        "connection: close\r\n",
                        "\r\n",
                        "data: {\"choices\":[{\"delta\":{\"content\":\"Retry recovered\"}}]}\n\n",
                        "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_retry_reset_1\",\"function\":{\"name\":\"definitely_unknown_tool\",\"arguments\":\"{}\"}}]}}],\"usage\":{\"prompt_tokens\":7,\"completion_tokens\":3}}\n\n",
                        "data: [DONE]\n\n"
                    )),
                    _ => Some(concat!(
                        "HTTP/1.1 200 OK\r\n",
                        "content-type: text/event-stream\r\n",
                        "cache-control: no-cache\r\n",
                        "connection: close\r\n",
                        "\r\n",
                        "data: {\"choices\":[{\"delta\":{\"content\":\"All clear\"}}]}\n\n",
                        "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":7,\"completion_tokens\":3}}\n\n",
                        "data: [DONE]\n\n"
                    )),
                };

                if let Some(response) = response {
                    socket
                        .write_all(response.as_bytes())
                        .await
                        .expect("write retry reset regression response");
                } else {
                    let _ = socket.shutdown().await;
                }
            });
        }
    });

    format!("http://{addr}/v1")
}

async fn spawn_timeout_retry_reset_server(request_counter: Arc<AtomicUsize>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind timeout retry reset server");
    let addr = listener
        .local_addr()
        .expect("timeout retry reset server addr");

    tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = listener.accept().await else {
                break;
            };
            let request_counter = request_counter.clone();
            tokio::spawn(async move {
                let attempt = request_counter.fetch_add(1, Ordering::SeqCst);
                let _request = read_http_request(&mut socket, "timeout retry reset request").await;

                match attempt {
                    0 | 2 => {
                        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                        let _ = socket.shutdown().await;
                    }
                    1 => {
                        let response = concat!(
                            "HTTP/1.1 200 OK\r\n",
                            "content-type: text/event-stream\r\n",
                            "cache-control: no-cache\r\n",
                            "connection: close\r\n",
                            "\r\n",
                            "data: {\"choices\":[{\"delta\":{\"content\":\"Recovered from timeout\"}}]}\n\n",
                            "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_timeout_reset_1\",\"function\":{\"name\":\"definitely_unknown_tool\",\"arguments\":\"{}\"}}]}}],\"usage\":{\"prompt_tokens\":7,\"completion_tokens\":3}}\n\n",
                            "data: [DONE]\n\n"
                        );
                        socket
                            .write_all(response.as_bytes())
                            .await
                            .expect("write timeout retry reset tool-call response");
                    }
                    _ => {
                        let response = concat!(
                            "HTTP/1.1 200 OK\r\n",
                            "content-type: text/event-stream\r\n",
                            "cache-control: no-cache\r\n",
                            "connection: close\r\n",
                            "\r\n",
                            "data: {\"choices\":[{\"delta\":{\"content\":\"Timeout path recovered\"}}]}\n\n",
                            "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":7,\"completion_tokens\":3}}\n\n",
                            "data: [DONE]\n\n"
                        );
                        socket
                            .write_all(response.as_bytes())
                            .await
                            .expect("write timeout retry reset final response");
                    }
                }
            });
        }
    });

    format!("http://{addr}/v1")
}

#[tokio::test]
async fn successful_retry_resets_scheduled_retry_cycle_before_later_failures() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let request_counter = Arc::new(AtomicUsize::new(0));
    let mut config = AgentConfig::default();
    config.provider = PROVIDER_ID_CUSTOM.to_string();
    config.base_url = spawn_retry_reset_regression_server(request_counter.clone()).await;
    config.model = "gpt-4.1".to_string();
    config.api_key = "test-key".to_string();
    config.auth_source = AuthSource::ApiKey;
    config.api_transport = ApiTransport::ChatCompletions;
    config.auto_retry = true;
    config.max_retries = 3;
    config.retry_delay_ms = 5;
    config.max_tool_loops = 4;

    let engine = AgentEngine::new_test(manager, config, root.path()).await;
    let mut events = engine.subscribe();
    let thread_id = "thread-retry-reset-after-success";
    {
        let mut threads = engine.threads.write().await;
        threads.insert(
            thread_id.to_string(),
            crate::agent::types::AgentThread {
                id: thread_id.to_string(),
                agent_name: None,
                title: "Retry reset after success".to_string(),
                messages: vec![crate::agent::types::AgentMessage::user("hello", 1)],
                pinned: false,
                upstream_thread_id: None,
                upstream_transport: None,
                upstream_provider: None,
                upstream_model: None,
                upstream_assistant_id: None,
                total_input_tokens: 0,
                total_output_tokens: 0,
                created_at: 1,
                updated_at: 1,
            },
        );
    }

    let mut send_task = tokio::spawn({
        let engine = engine.clone();
        async move {
            engine
                .send_message_inner(
                    Some(thread_id),
                    "hello",
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    true,
                )
                .await
        }
    });

    let mut retry_attempts = Vec::new();
    let done_thread_id = thread_id.to_string();
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        loop {
            match events.recv().await {
                Ok(AgentEvent::RetryStatus {
                    thread_id: event_thread_id,
                    attempt,
                    ..
                }) if event_thread_id == thread_id && attempt > 0 => {
                    retry_attempts.push(attempt);
                }
                Ok(AgentEvent::Done {
                    thread_id: event_thread_id,
                    ..
                }) if event_thread_id == done_thread_id => {
                    break;
                }
                Ok(_) => {}
                Err(_) => {}
            }
        }
    })
    .await
    .expect("send loop should emit retry and done events");

    let outcome = tokio::time::timeout(std::time::Duration::from_secs(2), &mut send_task)
        .await
        .expect("send loop should finish after the final recovery")
        .expect("send task join should succeed")
        .expect("send loop should recover after both transient failures");

    assert_eq!(outcome.thread_id, thread_id);
    assert_eq!(request_counter.load(Ordering::SeqCst), 4);

    assert_eq!(
        retry_attempts,
        vec![1, 1],
        "a later transient failure in the same turn should restart retry counting after the stream recovers"
    );
}

#[tokio::test]
async fn successful_timeout_recovery_resets_stream_timeout_counter_before_later_timeouts() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let request_counter = Arc::new(AtomicUsize::new(0));
    let mut config = AgentConfig::default();
    config.provider = PROVIDER_ID_CUSTOM.to_string();
    config.base_url = spawn_timeout_retry_reset_server(request_counter.clone()).await;
    config.model = "gpt-4.1".to_string();
    config.api_key = "test-key".to_string();
    config.auth_source = AuthSource::ApiKey;
    config.api_transport = ApiTransport::ChatCompletions;
    config.auto_retry = true;
    config.max_retries = 0;
    config.max_tool_loops = 4;

    let engine = AgentEngine::new_test(manager, config, root.path()).await;
    let mut events = engine.subscribe();
    let thread_id = "thread-timeout-reset-after-success";
    {
        let mut threads = engine.threads.write().await;
        threads.insert(
            thread_id.to_string(),
            crate::agent::types::AgentThread {
                id: thread_id.to_string(),
                agent_name: None,
                title: "Timeout reset after success".to_string(),
                messages: vec![crate::agent::types::AgentMessage::user("hello", 1)],
                pinned: false,
                upstream_thread_id: None,
                upstream_transport: None,
                upstream_provider: None,
                upstream_model: None,
                upstream_assistant_id: None,
                total_input_tokens: 0,
                total_output_tokens: 0,
                created_at: 1,
                updated_at: 1,
            },
        );
    }

    let mut send_task = tokio::spawn({
        let engine = engine.clone();
        async move {
            engine
                .send_message_inner(
                    Some(thread_id),
                    "hello",
                    None,
                    None,
                    None,
                    None,
                    None,
                    Some(std::time::Duration::from_millis(20)),
                    None,
                    true,
                )
                .await
        }
    });

    let mut timeout_attempts = Vec::new();
    let done_thread_id = thread_id.to_string();
    tokio::time::timeout(std::time::Duration::from_secs(8), async {
        loop {
            match events.recv().await {
                Ok(AgentEvent::RetryStatus {
                    thread_id: event_thread_id,
                    failure_class,
                    attempt,
                    ..
                }) if event_thread_id == thread_id && failure_class == "timeout" => {
                    timeout_attempts.push(attempt);
                }
                Ok(AgentEvent::Done {
                    thread_id: event_thread_id,
                    ..
                }) if event_thread_id == done_thread_id => {
                    break;
                }
                Ok(_) => {}
                Err(_) => {}
            }
        }
    })
    .await
    .expect("send loop should emit timeout retry and done events");

    let outcome = tokio::time::timeout(std::time::Duration::from_secs(2), &mut send_task)
        .await
        .expect("send loop should finish after the final timeout recovery")
        .expect("send task join should succeed")
        .expect("send loop should recover after both timeouts");

    assert_eq!(outcome.thread_id, thread_id);
    assert_eq!(request_counter.load(Ordering::SeqCst), 4);
    assert_eq!(
        timeout_attempts,
        vec![1, 1],
        "a later timeout in the same turn should restart the timeout counter after the stream recovers"
    );
}

#[tokio::test]
async fn repeated_waiting_timeouts_escalate_to_fresh_runner() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let request_counter = Arc::new(AtomicUsize::new(0));
    let success_request_started = Arc::new(tokio::sync::Notify::new());
    let release_success_request = Arc::new(tokio::sync::Notify::new());
    let mut config = AgentConfig::default();
    config.provider = PROVIDER_ID_CUSTOM.to_string();
    config.base_url = spawn_timeout_failures_then_blocking_success_server(
        request_counter.clone(),
        4,
        success_request_started.clone(),
        release_success_request.clone(),
    )
    .await;
    config.model = "gpt-4.1".to_string();
    config.api_key = "test-key".to_string();
    config.auth_source = AuthSource::ApiKey;
    config.api_transport = ApiTransport::ChatCompletions;
    config.auto_retry = true;
    config.max_retries = 0;
    config.max_tool_loops = 6;

    let engine = AgentEngine::new_test(manager, config, root.path()).await;
    let thread_id = "thread-timeout-wait-fresh-runner";
    {
        let mut threads = engine.threads.write().await;
        threads.insert(
            thread_id.to_string(),
            crate::agent::types::AgentThread {
                id: thread_id.to_string(),
                agent_name: None,
                title: "Timeout wait fresh runner".to_string(),
                messages: vec![crate::agent::types::AgentMessage::user("hello", 1)],
                pinned: false,
                upstream_thread_id: None,
                upstream_transport: None,
                upstream_provider: None,
                upstream_model: None,
                upstream_assistant_id: None,
                total_input_tokens: 0,
                total_output_tokens: 0,
                created_at: 1,
                updated_at: 1,
            },
        );
    }

    let mut send_task = tokio::spawn({
        let engine = engine.clone();
        async move {
            engine
                .send_message_inner(
                    Some(thread_id),
                    "hello",
                    None,
                    None,
                    None,
                    None,
                    None,
                    Some(std::time::Duration::from_millis(20)),
                    None,
                    true,
                )
                .await
        }
    });

    let initial_generation = tokio::time::timeout(std::time::Duration::from_secs(1), async {
        loop {
            if request_counter.load(Ordering::SeqCst) >= 1 {
                let streams = engine.stream_cancellations.lock().await;
                if let Some(entry) = streams.get(thread_id) {
                    break entry.generation;
                }
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("initial timeout stream generation should be registered");

    tokio::time::timeout(
        std::time::Duration::from_secs(3),
        success_request_started.notified(),
    )
    .await
    .expect("fresh timeout recovery request should start after repeated waiting timeouts");

    let refreshed_generation = {
        let streams = engine.stream_cancellations.lock().await;
        streams
            .get(thread_id)
            .map(|entry| entry.generation)
            .expect("fresh timeout recovery stream should replace the active stream entry")
    };
    assert!(
        refreshed_generation > initial_generation,
        "repeated waiting timeouts should replace the broken stream with a fresh runner"
    );

    release_success_request.notify_waiters();

    let outcome = tokio::time::timeout(std::time::Duration::from_secs(2), &mut send_task)
        .await
        .expect("fresh timeout recovery loop should finish")
        .expect("send task join should succeed")
        .expect("fresh timeout recovery loop should succeed");
    assert_eq!(outcome.thread_id, thread_id);
    assert_eq!(request_counter.load(Ordering::SeqCst), 5);
}

#[tokio::test]
async fn timeout_retry_clears_the_countdown_once_the_retry_request_is_in_flight() {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind silent provider");
    let addr = listener.local_addr().expect("silent provider addr");
    tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = listener.accept().await else {
                break;
            };
            tokio::spawn(async move {
                let _request = read_http_request(&mut socket, "silent provider request").await;
                tokio::time::sleep(std::time::Duration::from_secs(30)).await;
            });
        }
    });

    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.provider = PROVIDER_ID_CUSTOM.to_string();
    config.base_url = format!("http://{addr}/v1");
    config.model = "gpt-4.1".to_string();
    config.api_key = "test-key".to_string();
    config.auth_source = AuthSource::ApiKey;
    config.api_transport = ApiTransport::ChatCompletions;
    config.auto_retry = true;
    config.max_retries = 0;

    let engine = AgentEngine::new_test(manager, config, root.path()).await;
    let mut events = engine.subscribe();
    let thread_id = "thread-timeout-retry-countdown";
    let send_task = tokio::spawn({
        let engine = engine.clone();
        async move {
            engine
                .send_message_inner(
                    Some(thread_id),
                    "hello",
                    None,
                    None,
                    None,
                    None,
                    None,
                    Some(std::time::Duration::from_millis(20)),
                    None,
                    true,
                )
                .await
        }
    });

    let mut phases = Vec::new();
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        while phases.iter().filter(|(_, attempt)| *attempt == 2).count() == 0 {
            if let Ok(AgentEvent::RetryStatus {
                thread_id: event_thread_id,
                phase,
                attempt,
                ..
            }) = events.recv().await
            {
                if event_thread_id == thread_id {
                    phases.push((phase, attempt));
                }
            }
        }
    })
    .await
    .expect("the silent provider should time out twice");
    send_task.abort();

    let first_retry = phases
        .iter()
        .position(|(phase, attempt)| phase == "retrying" && *attempt == 1)
        .expect("first timeout schedules a retry");
    let second_retry = phases
        .iter()
        .position(|(_, attempt)| *attempt == 2)
        .expect("second timeout schedules another retry");
    assert!(
        phases[first_retry + 1..second_retry]
            .iter()
            .any(|(phase, _)| phase == "cleared"),
        "the 'retry in Ns' countdown must clear when the retry starts, or it stays stale for the whole next attempt: {phases:?}"
    );
}

#[tokio::test]
async fn stream_timeout_that_trips_the_circuit_breaker_fails_loudly_instead_of_a_silent_interrupt() {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind silent provider");
    let addr = listener.local_addr().expect("silent provider addr");
    tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = listener.accept().await else {
                break;
            };
            tokio::spawn(async move {
                let _request = read_http_request(&mut socket, "silent provider request").await;
                tokio::time::sleep(std::time::Duration::from_secs(30)).await;
            });
        }
    });

    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.provider = PROVIDER_ID_CUSTOM.to_string();
    config.base_url = format!("http://{addr}/v1");
    config.model = "gpt-4.1".to_string();
    config.api_key = "test-key".to_string();
    config.auth_source = AuthSource::ApiKey;
    config.api_transport = ApiTransport::ChatCompletions;
    config.auto_retry = true;
    config.max_retries = 0;

    let engine = AgentEngine::new_test(manager, config, root.path()).await;
    for _ in 0..4 {
        engine.record_llm_outcome(PROVIDER_ID_CUSTOM, false).await;
    }
    let mut events = engine.subscribe();
    let thread_id = "thread-timeout-trips-breaker";
    let send_task = tokio::spawn({
        let engine = engine.clone();
        async move {
            engine
                .send_message_inner(
                    Some(thread_id),
                    "hello",
                    None,
                    None,
                    None,
                    None,
                    None,
                    Some(std::time::Duration::from_millis(20)),
                    None,
                    true,
                )
                .await
        }
    });

    let outcome = tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            match events.recv().await {
                Ok(AgentEvent::Error {
                    thread_id: event_thread_id,
                    message,
                }) if event_thread_id == thread_id => break Ok(message),
                Ok(AgentEvent::TurnInterrupted {
                    thread_id: event_thread_id,
                }) if event_thread_id == thread_id => break Err(()),
                _ => {}
            }
        }
    })
    .await
    .expect("the turn must end once the breaker trips");
    send_task.abort();

    let message = outcome.expect(
        "a provider outage must be reported to the operator, not shown as if they pressed stop",
    );
    assert!(message.contains("circuit breaker"), "{message}");
}

#[tokio::test]
async fn usage_limit_waits_do_not_use_up_the_tool_loop_budget() {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind usage-limit provider");
    let addr = listener.local_addr().expect("usage-limit provider addr");
    let request_counter = Arc::new(AtomicUsize::new(0));
    tokio::spawn({
        let request_counter = request_counter.clone();
        async move {
            loop {
                let Ok((mut socket, _)) = listener.accept().await else {
                    break;
                };
                let attempt = request_counter.fetch_add(1, Ordering::SeqCst);
                tokio::spawn(async move {
                    let _request = read_http_request(&mut socket, "usage-limit request").await;
                    let response = if attempt < 3 {
                        let body = r#"{"error":{"type":"usage_limit_reached","message":"The usage limit has been reached"}}"#;
                        format!(
                            "HTTP/1.1 429 Too Many Requests\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                            body.len()
                        )
                    } else {
                        concat!(
                            "HTTP/1.1 200 OK\r\n",
                            "content-type: text/event-stream\r\n",
                            "cache-control: no-cache\r\n",
                            "connection: close\r\n",
                            "\r\n",
                            "data: {\"choices\":[{\"delta\":{\"content\":\"Back after the reset\"}}]}\n\n",
                            "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":7,\"completion_tokens\":3}}\n\n",
                            "data: [DONE]\n\n"
                        )
                        .to_string()
                    };
                    let _ = socket.write_all(response.as_bytes()).await;
                });
            }
        }
    });

    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.provider = PROVIDER_ID_CUSTOM.to_string();
    config.base_url = format!("http://{addr}/v1");
    config.model = "gpt-4.1".to_string();
    config.api_key = "test-key".to_string();
    config.auth_source = AuthSource::ApiKey;
    config.api_transport = ApiTransport::ChatCompletions;
    config.auto_retry = true;
    config.max_retries = 0;
    config.max_tool_loops = 2;

    let engine = AgentEngine::new_test(manager, config, root.path()).await;
    let mut events = engine.subscribe();
    let thread_id = "thread-usage-limit-loop-budget";
    let send_task = tokio::spawn({
        let engine = engine.clone();
        async move {
            engine
                .send_message_inner(
                    Some(thread_id),
                    "hello",
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    true,
                )
                .await
        }
    });

    let ended = tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            match events.recv().await {
                Ok(AgentEvent::Done {
                    thread_id: event_thread_id,
                    ..
                }) if event_thread_id == thread_id => break Ok(()),
                Ok(AgentEvent::Error {
                    thread_id: event_thread_id,
                    message,
                }) if event_thread_id == thread_id => break Err(message),
                _ => {}
            }
        }
    })
    .await
    .expect("the turn should end");
    send_task.abort();

    assert_eq!(
        ended,
        Ok(()),
        "waiting out a usage limit is not a tool round and must not end the turn with a loop-limit error"
    );
    assert_eq!(request_counter.load(Ordering::SeqCst), 4);
}
