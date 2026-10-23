use super::super::{
    execute_onecontext_search_with_runner, execute_search_files_with_runner,
    execute_web_search_with_runner,
};
use super::part1::{exit_status_with_code, successful_exit_status};
use std::fs;
use std::sync::{Arc, Mutex};
use tempfile::tempdir;
use tokio::time::Duration;

#[cfg(windows)]
use std::os::windows::process::ExitStatusExt;

#[tokio::test]
async fn onecontext_search_runtime_clamps_timeout_override_on_caller_path() {
    let observed_timeout = Arc::new(Mutex::new(None));
    let observed_timeout_clone = observed_timeout.clone();

    execute_onecontext_search_with_runner(
        &serde_json::json!({ "query": "timeout policy", "timeout_seconds": 999 }),
        true,
        move |request| {
            let observed_timeout = observed_timeout_clone.clone();
            async move {
                *observed_timeout
                    .lock()
                    .expect("timeout lock should succeed") = Some(request.timeout_seconds);
                Ok::<std::process::Output, anyhow::Error>(std::process::Output {
                    status: successful_exit_status(),
                    stdout: b"match".to_vec(),
                    stderr: Vec::new(),
                })
            }
        },
    )
    .await
    .expect("onecontext search should succeed");

    assert_eq!(
        *observed_timeout
            .lock()
            .expect("timeout lock should succeed"),
        Some(600)
    );
}

#[tokio::test]
async fn search_files_runtime_uses_default_timeout_on_caller_path() {
    let observed_timeout = Arc::new(Mutex::new(None));
    let observed_timeout_clone = observed_timeout.clone();

    let result = execute_search_files_with_runner(
        &serde_json::json!({ "pattern": "needle" }),
        move |request| {
            let observed_timeout = observed_timeout_clone.clone();
            async move {
                *observed_timeout
                    .lock()
                    .expect("timeout lock should succeed") = Some(request.timeout_seconds);
                Ok::<super::SearchFilesCommandOutput, anyhow::Error>(
                    super::SearchFilesCommandOutput {
                        status: successful_exit_status(),
                        stdout: Vec::new(),
                        stderr: Vec::new(),
                        truncated: false,
                    },
                )
            }
        },
    )
    .await
    .expect("search_files should succeed");

    assert_eq!(
        *observed_timeout
            .lock()
            .expect("timeout lock should succeed"),
        Some(120)
    );
    assert_eq!(result, "No matches found.");
}

#[test]
fn search_files_rejects_empty_pattern() {
    let error = super::search_files_request(&serde_json::json!({ "pattern": "   " }))
        .err()
        .expect("blank patterns should be rejected");

    assert!(error.to_string().contains("must not be empty"));
}

#[test]
fn search_files_clamps_max_results() {
    let request = super::search_files_request(&serde_json::json!({
        "pattern": "needle",
        "max_results": 9_999
    }))
    .expect("request should parse");

    assert!(request.max_results <= 200);
}

#[test]
fn search_files_clamps_zero_max_results_to_one() {
    let request = super::search_files_request(&serde_json::json!({
        "pattern": "needle",
        "max_results": 0
    }))
    .expect("request should parse");

    assert_eq!(request.max_results, 1);
}

#[test]
fn search_files_resolves_relative_path_to_absolute_root() {
    let request = super::search_files_request(&serde_json::json!({
        "pattern": "needle",
        "path": "nested/src"
    }))
    .expect("request should parse");

    let expected = std::env::current_dir()
        .expect("current dir should resolve")
        .join("nested/src");
    assert_eq!(std::path::PathBuf::from(&request.path), expected);
    assert!(std::path::Path::new(&request.path).is_absolute());
}

#[test]
fn search_files_rejects_invalid_max_results_type() {
    for invalid in [serde_json::json!(-1), serde_json::json!("many")] {
        let error = super::search_files_request(&serde_json::json!({
            "pattern": "needle",
            "max_results": invalid,
        }))
        .err()
        .expect("invalid max_results should be rejected");

        assert!(error
            .to_string()
            .contains("'max_results' must be a non-negative integer"));
    }
}

#[tokio::test]
async fn search_files_reports_invalid_regex() {
    let error = execute_search_files_with_runner(
        &serde_json::json!({ "pattern": "[", "regex": true }),
        |_| async move {
            Ok::<super::SearchFilesCommandOutput, anyhow::Error>(super::SearchFilesCommandOutput {
                status: exit_status_with_code(2),
                stdout: Vec::new(),
                stderr: b"regex parse error:\n    [\n    ^\nerror: unclosed character class"
                    .to_vec(),
                truncated: false,
            })
        },
    )
    .await
    .expect_err("invalid regex should return an error");

    assert!(error.to_string().contains("invalid regex"));
}

#[tokio::test]
async fn search_files_passes_file_pattern_and_root_path_to_runner() {
    let observed_request = Arc::new(Mutex::new(None));
    let observed_request_clone = observed_request.clone();

    let result = execute_search_files_with_runner(
        &serde_json::json!({
            "pattern": "needle",
            "path": "/tmp/workspace",
            "file_pattern": "*.rs"
        }),
        move |request| {
            let observed_request = observed_request_clone.clone();
            async move {
                *observed_request
                    .lock()
                    .expect("request lock should succeed") = Some(request);
                Ok::<super::SearchFilesCommandOutput, anyhow::Error>(
                    super::SearchFilesCommandOutput {
                        status: successful_exit_status(),
                        stdout: Vec::new(),
                        stderr: Vec::new(),
                        truncated: false,
                    },
                )
            }
        },
    )
    .await
    .expect("search_files should succeed");

    let request = observed_request
        .lock()
        .expect("request lock should succeed")
        .clone()
        .expect("runner should receive a request");
    assert_eq!(request.file_pattern.as_deref(), Some("*.rs"));
    assert_eq!(request.path, "/tmp/workspace");
    assert_eq!(result, "No matches found.");
}

#[tokio::test]
async fn search_files_subprocess_supports_nested_glob_with_absolute_root_path() {
    let root = tempdir().expect("tempdir should succeed");
    let nested_dir = root.path().join("src/nested");
    fs::create_dir_all(&nested_dir).expect("nested dir should be created");
    fs::write(
        nested_dir.join("lib.rs"),
        "const NEEDLE: &str = \"needle\";\n",
    )
    .expect("matching rust file should be written");
    fs::write(root.path().join("src/ignore.txt"), "needle\n")
        .expect("non-rust file should be written");
    fs::write(root.path().join("top.rs"), "needle\n")
        .expect("outside-glob rust file should be written");

    let result = execute_search_files_with_runner(
        &serde_json::json!({
            "pattern": "needle",
            "path": root.path().to_string_lossy(),
            "file_pattern": "src/**/*.rs",
        }),
        super::run_search_files_subprocess,
    )
    .await
    .expect("nested glob search should succeed");

    assert!(result.contains("src/nested/lib.rs:1:"));
    assert!(!result.contains("src/ignore.txt"));
    assert!(!result.contains("top.rs"));
}

#[tokio::test]
async fn search_files_subprocess_supports_literal_function_call_pattern() {
    let root = tempdir().expect("tempdir should succeed");
    fs::write(
        root.path().join("gateway.rs"),
        "client.request_gateway_send(request).await?;\n",
    )
    .expect("rust source file should be written");

    let result = execute_search_files_with_runner(
        &serde_json::json!({
            "pattern": "request_gateway_send(",
            "path": root.path().to_string_lossy(),
            "file_pattern": "*.rs",
        }),
        super::run_search_files_subprocess,
    )
    .await
    .expect("literal function-call search should succeed");

    assert!(result.contains("gateway.rs:1:"));
    assert!(result.contains("request_gateway_send("));
}

#[test]
fn search_files_builds_rg_args_with_file_pattern_and_root_path() {
    let args = super::build_search_files_rg_args(&super::SearchFilesRequest {
        pattern: "needle".to_string(),
        path: "/tmp/workspace".to_string(),
        file_pattern: Some("*.rs".to_string()),
        regex: false,
        max_results: 17,
        timeout_seconds: 120,
    });

    assert!(args.contains(&"--glob=*.rs".to_string()));
    assert!(args.contains(&"--fixed-strings".to_string()));
    assert!(args.contains(&"/tmp/workspace".to_string()));
    assert!(args.contains(&"--".to_string()));
    assert_eq!(args.last().map(String::as_str), Some("/tmp/workspace"));
}

#[test]
fn search_files_builds_rg_args_with_positional_separator_before_pattern_and_path() {
    let args = super::build_search_files_rg_args(&super::SearchFilesRequest {
        pattern: "--type-add=bad".to_string(),
        path: "-definitely-a-path".to_string(),
        file_pattern: None,
        regex: false,
        max_results: 5,
        timeout_seconds: 120,
    });

    let separator_index = args
        .iter()
        .position(|arg| arg == "--")
        .expect("rg args should contain positional separator");
    assert_eq!(
        args.get(separator_index + 1).map(String::as_str),
        Some("--type-add=bad")
    );
    assert_eq!(
        args.get(separator_index + 2).map(String::as_str),
        Some("-definitely-a-path")
    );
}

#[tokio::test]
async fn search_files_enforces_global_result_cap() {
    let result = execute_search_files_with_runner(
        &serde_json::json!({ "pattern": "needle", "max_results": 2 }),
        |_| async move {
            Ok::<super::SearchFilesCommandOutput, anyhow::Error>(super::SearchFilesCommandOutput {
                status: successful_exit_status(),
                stdout: b"a:1:needle\nb:2:needle\nc:3:needle\n".to_vec(),
                stderr: Vec::new(),
                truncated: true,
            })
        },
    )
    .await
    .expect("search_files should succeed");

    assert_eq!(result, "a:1:needle\nb:2:needle\n\n... (more matches)");
}

#[tokio::test]
async fn search_files_surfaces_truncated_rg_stderr_failure() {
    let error = execute_search_files_with_runner(
        &serde_json::json!({ "pattern": "needle", "max_results": 2 }),
        |_| async move {
            Ok::<super::SearchFilesCommandOutput, anyhow::Error>(super::SearchFilesCommandOutput {
                status: successful_exit_status(),
                stdout: b"a:1:needle\nb:2:needle\n".to_vec(),
                stderr: b"rg: permission denied".to_vec(),
                truncated: true,
            })
        },
    )
    .await
    .expect_err("truncated stderr failure should surface");

    assert!(error.to_string().contains("search failed"));
    assert!(error.to_string().contains("permission denied"));
}

#[tokio::test]
async fn search_files_reports_missing_rg() {
    let error = execute_search_files_with_runner(
        &serde_json::json!({ "pattern": "needle" }),
        |_| async move {
            Err::<super::SearchFilesCommandOutput, anyhow::Error>(
                std::io::Error::new(std::io::ErrorKind::NotFound, "No such file or directory")
                    .into(),
            )
        },
    )
    .await
    .expect_err("missing rg should return an error");

    assert!(error.to_string().contains("rg"));
}

#[tokio::test]
async fn web_search_runtime_uses_default_timeout_on_caller_path() {
    let observed_timeout = Arc::new(Mutex::new(None));
    let observed_timeout_clone = observed_timeout.clone();

    let result = execute_web_search_with_runner(
        &serde_json::json!({ "query": "timeout policy" }),
        "exa",
        "exa-key",
        "",
        "",
        move |request: super::WebSearchRequest, provider| {
            let observed_timeout = observed_timeout_clone.clone();
            async move {
                *observed_timeout
                    .lock()
                    .expect("timeout lock should succeed") = Some(request.timeout_seconds);
                Ok::<String, anyhow::Error>(format!("provider={provider}; query={}", request.query))
            }
        },
    )
    .await
    .expect("web_search should succeed");

    assert_eq!(
        *observed_timeout
            .lock()
            .expect("timeout lock should succeed"),
        Some(300)
    );
    assert_eq!(result, "provider=exa; query=timeout policy");
}

#[tokio::test]
async fn web_search_runtime_clamps_timeout_override_on_caller_path() {
    let observed_timeout = Arc::new(Mutex::new(None));
    let observed_timeout_clone = observed_timeout.clone();

    execute_web_search_with_runner(
        &serde_json::json!({ "query": "timeout policy", "timeout_seconds": 999 }),
        "tavily",
        "",
        "tavily-key",
        "",
        move |request: super::WebSearchRequest, provider| {
            let observed_timeout = observed_timeout_clone.clone();
            async move {
                *observed_timeout
                    .lock()
                    .expect("timeout lock should succeed") = Some(request.timeout_seconds);
                Ok::<String, anyhow::Error>(format!(
                    "provider={provider}; max_results={}",
                    request.max_results
                ))
            }
        },
    )
    .await
    .expect("web_search should succeed");

    assert_eq!(
        *observed_timeout
            .lock()
            .expect("timeout lock should succeed"),
        Some(600)
    );
}

#[tokio::test]
async fn web_search_runtime_routes_explicit_duckduckgo_provider_without_api_key() {
    let result = execute_web_search_with_runner(
        &serde_json::json!({ "query": "privacy search", "max_results": 2 }),
        "duckduckgo",
        "",
        "",
        "",
        |_request: super::WebSearchRequest, provider| async move {
            Ok::<String, anyhow::Error>(format!("provider={provider}"))
        },
    )
    .await
    .expect("explicit DuckDuckGo search should not require an API key");

    assert_eq!(result, "provider=duckduckgo");
}

#[test]
fn duckduckgo_search_url_includes_region_and_safe_search_params() {
    let url = super::build_duckduckgo_search_url("privacy search", "pl-pl", "off");

    assert_eq!(
        url,
        "https://lite.duckduckgo.com/lite/?q=privacy+search&kl=pl-pl&kp=-2"
    );
}

#[tokio::test]
async fn web_search_runtime_returns_timeout_error_when_runner_exceeds_limit() {
    let error = execute_web_search_with_runner(
        &serde_json::json!({ "query": "timeout policy", "timeout_seconds": 0 }),
        "ddg",
        "",
        "",
        "",
        |_request: super::WebSearchRequest, _provider| async move {
            tokio::time::sleep(Duration::from_millis(10)).await;
            Ok::<String, anyhow::Error>("late result".to_string())
        },
    )
    .await
    .expect_err("runner exceeding timeout should return timeout error");

    assert!(error.to_string().contains("web search timed out"));
    assert!(error.to_string().contains("0"));
}

#[tokio::test]
async fn web_search_runtime_routes_firecrawl_only_when_its_key_is_set() {
    let args = serde_json::json!({ "query": "crawl" });
    for (key, expected) in [("fc-key", "firecrawl"), ("", "ddg")] {
        let provider = execute_web_search_with_runner(
            &args,
            "firecrawl",
            "",
            "",
            key,
            |_request: super::WebSearchRequest, provider| async move {
                Ok::<String, anyhow::Error>(provider.to_string())
            },
        )
        .await
        .expect("search");
        assert_eq!(provider, expected, "key {key:?}");
    }
}

#[test]
fn firecrawl_v2_web_results_are_formatted_like_other_providers() {
    let text = super::format_firecrawl_search_results(
        "rust async",
        &serde_json::json!({
            "success": true,
            "data": { "web": [
                { "title": "Async Book", "url": "https://rust-lang.github.io/async-book/", "description": "Asynchronous programming in Rust" }
            ] }
        }),
    );

    assert!(text.starts_with("Web results for \"rust async\""), "{text}");
    assert!(text.contains("Async Book"), "{text}");
    assert!(
        text.contains("https://rust-lang.github.io/async-book/"),
        "{text}"
    );
    assert!(text.contains("Asynchronous programming in Rust"), "{text}");
    assert_eq!(
        super::format_firecrawl_search_results(
            "nothing",
            &serde_json::json!({ "success": true, "data": { "web": [] } })
        ),
        "No web results found for: nothing"
    );
}
