use super::*;

fn request<'a>(
    dir: &'a Path,
    shell: &'a str,
    input: serde_json::Value,
    timeout_secs: u64,
) -> LocalRunRequest<'a> {
    LocalRunRequest {
        plugin_name: "voice-lab",
        plugin_dir: dir,
        data_dir: dir,
        shell,
        input,
        timeout_secs,
    }
}

#[tokio::test]
async fn panel_parameters_reach_the_script_on_stdin_and_stdout_is_the_result() {
    let dir = tempfile::tempdir().unwrap();
    let shell =
        "python3 -c 'import json,sys; print(json.load(sys.stdin)[\"params\"][\"text\"].upper())'";
    let result = run_local_endpoint(request(
        dir.path(),
        shell,
        serde_json::json!({ "params": { "text": "hi; rm -rf ~" } }),
        30,
    ))
    .await
    .unwrap();
    assert_eq!(result, "HI; RM -RF ~");
}

#[tokio::test]
async fn environment_setup_chatter_never_corrupts_the_result() {
    let dir = tempfile::tempdir().unwrap();
    let shell = local_run_shell(
        "echo 'Successfully installed numpy'",
        "echo '{\"ok\":true}'",
    );
    let result = run_local_endpoint(request(dir.path(), &shell, serde_json::json!({}), 30))
        .await
        .unwrap();
    assert_eq!(result, "{\"ok\":true}");
}

#[tokio::test]
async fn a_failing_script_reports_its_exit_code_and_error_output() {
    let dir = tempfile::tempdir().unwrap();
    let error = run_local_endpoint(request(
        dir.path(),
        "echo 'ffmpeg not found' >&2; exit 3",
        serde_json::json!({}),
        30,
    ))
    .await
    .unwrap_err();
    match error {
        PluginApiError::LocalRunFailed { code, stderr } => {
            assert_eq!(code, Some(3));
            assert!(stderr.contains("ffmpeg not found"));
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[tokio::test]
async fn a_timeout_stops_the_script_and_everything_it_started() {
    let dir = tempfile::tempdir().unwrap();
    let marker = dir.path().join("grandchild-survived");
    let shell = format!("(sleep 3; touch {}) & sleep 30", marker.display());
    let started = std::time::Instant::now();
    let error = run_local_endpoint(request(dir.path(), &shell, serde_json::json!({}), 1))
        .await
        .unwrap_err();
    assert!(matches!(error, PluginApiError::LocalRunTimeout { secs: 1 }));
    assert!(started.elapsed() < std::time::Duration::from_secs(10));
    tokio::time::sleep(std::time::Duration::from_secs(4)).await;
    assert!(
        !marker.exists(),
        "a background process outlived the timeout"
    );
}

#[test]
fn approval_covers_only_the_version_the_operator_approved() {
    let dir = tempfile::tempdir().unwrap();
    let data_dir = dir.path().join("plugin-data").join("voice-lab");
    assert!(!local_run_approved(&data_dir, "1.0.0"));
    approve_local_run(&data_dir, "1.0.0").unwrap();
    assert!(local_run_approved(&data_dir, "1.0.0"));
    assert!(!local_run_approved(&data_dir, "1.1.0"));
}

#[test]
fn timeouts_default_to_two_minutes_and_are_capped_at_an_hour() {
    let run = |timeout_secs| EndpointRunDef {
        python: Default::default(),
        timeout_secs,
    };
    assert_eq!(local_run_timeout_secs(&run(None)), 120);
    assert_eq!(local_run_timeout_secs(&run(Some(7200))), 3600);
}

#[tokio::test]
async fn a_plugin_endpoint_runs_locally_only_after_its_version_is_approved() {
    let root = tempfile::TempDir::new().unwrap();
    let plugins_dir = root.path().join("plugins");
    let plugin_dir = plugins_dir.join("voice-lab");
    std::fs::create_dir_all(&plugin_dir).unwrap();
    std::fs::write(
        plugin_dir.join("plugin.json"),
        serde_json::json!({
            "name": "voice-lab",
            "version": "0.1.0",
            "schema_version": 1,
            "api": { "endpoints": { "echo": { "run": {
                "command": "python3 -c 'import json,os,sys; d=json.load(sys.stdin); print(d[\"params\"][\"line\"], os.path.basename(os.environ[\"ZORAI_PLUGIN_DATA_DIR\"]))'",
                "timeout_secs": 30
            } } } }
        })
        .to_string(),
    )
    .unwrap();
    let history = std::sync::Arc::new(
        crate::history::HistoryStore::new_test_store(root.path())
            .await
            .unwrap(),
    );
    let manager = crate::plugin::PluginManager::new(history, plugins_dir.clone());
    assert_eq!(manager.load_all_from_disk().await, (1, 0));

    let params = serde_json::json!({ "line": "hello" });
    let refused = manager
        .api_call("voice-lab", "echo", params.clone())
        .await
        .unwrap_err();
    assert!(
        matches!(refused, PluginApiError::LocalRunNotApproved { ref version, .. } if version == "0.1.0")
    );

    approve_local_run(&plugin_data_dir(&plugins_dir, "voice-lab"), "0.1.0").unwrap();
    assert_eq!(
        manager.api_call("voice-lab", "echo", params).await.unwrap(),
        "hello voice-lab"
    );
}

#[test]
fn plugins_get_the_cli_that_ships_next_to_the_running_daemon() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = dir.path().join("zorai-daemon");
    std::fs::write(&daemon, "").unwrap();
    assert_eq!(sibling_cli_path(&daemon), None);
    let cli = dir
        .path()
        .join(if cfg!(windows) { "zorai.exe" } else { "zorai" });
    std::fs::write(&cli, "").unwrap();
    assert_eq!(sibling_cli_path(&daemon), Some(cli));
}
