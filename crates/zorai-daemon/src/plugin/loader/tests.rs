use super::*;

fn make_validator() -> jsonschema::Validator {
    super::super::schema::compile_schema_v1()
}

fn minimal_manifest_json() -> String {
    r#"{"name":"test","version":"1.0.0","schema_version":1}"#.to_string()
}

#[test]
fn validate_manifest_rejects_oversized() {
    let validator = make_validator();
    let big = vec![b'{'; MAX_MANIFEST_SIZE + 1];
    let err = validate_manifest(&big, &validator).unwrap_err();
    assert!(
        err.to_string().contains("100KB"),
        "error should mention 100KB limit: {}",
        err
    );
}

#[test]
fn validate_manifest_rejects_missing_name() {
    let validator = make_validator();
    let json = r#"{"version":"1.0.0","schema_version":1}"#;
    let err = validate_manifest(json.as_bytes(), &validator).unwrap_err();
    assert!(
        err.to_string().contains("name"),
        "error should mention 'name': {}",
        err
    );
}

#[test]
fn validate_manifest_rejects_invalid_name_pattern() {
    let validator = make_validator();
    let json = r#"{"name":"../escape","version":"1.0.0","schema_version":1}"#;
    let err = validate_manifest(json.as_bytes(), &validator).unwrap_err();
    assert!(
        err.to_string().contains("escape")
            || err.to_string().contains("..")
            || err.to_string().contains("pattern")
            || err.to_string().contains("validation failed"),
        "error should mention invalid name: {}",
        err
    );
}

#[test]
fn validate_manifest_accepts_minimal() {
    let validator = make_validator();
    let json = minimal_manifest_json();
    let (manifest, raw) = validate_manifest(json.as_bytes(), &validator).unwrap();
    assert_eq!(manifest.name, "test");
    assert_eq!(manifest.version, "1.0.0");
    assert_eq!(manifest.schema_version, 1);
    assert!(!raw.is_empty());
}

#[test]
fn validate_manifest_rejects_too_many_endpoints() {
    let validator = make_validator();
    let mut endpoints = serde_json::Map::new();
    for i in 0..=MAX_ENDPOINTS {
        endpoints.insert(
            format!("ep_{i}"),
            serde_json::json!({"method":"GET","path":"/test"}),
        );
    }
    let manifest = serde_json::json!({
        "name": "test",
        "version": "1.0.0",
        "schema_version": 1,
        "api": { "endpoints": endpoints }
    });
    let raw = serde_json::to_vec(&manifest).unwrap();
    let err = validate_manifest(&raw, &validator).unwrap_err();
    assert!(
        err.to_string().contains("endpoints"),
        "error should mention endpoints: {}",
        err
    );
}

#[test]
fn validate_manifest_rejects_too_many_settings() {
    let validator = make_validator();
    let mut settings = serde_json::Map::new();
    for i in 0..=MAX_SETTINGS {
        settings.insert(
            format!("setting_{i}"),
            serde_json::json!({"type":"string","label":"Test"}),
        );
    }
    let manifest = serde_json::json!({
        "name": "test",
        "version": "1.0.0",
        "schema_version": 1,
        "settings": settings
    });
    let raw = serde_json::to_vec(&manifest).unwrap();
    let err = validate_manifest(&raw, &validator).unwrap_err();
    assert!(
        err.to_string().contains("settings"),
        "error should mention settings: {}",
        err
    );
}

#[test]
fn validate_manifest_accepts_max_endpoints_and_settings() {
    let validator = make_validator();
    let mut endpoints = serde_json::Map::new();
    for i in 0..MAX_ENDPOINTS {
        endpoints.insert(
            format!("ep_{i}"),
            serde_json::json!({"method":"GET","path":"/test"}),
        );
    }
    let mut settings = serde_json::Map::new();
    for i in 0..MAX_SETTINGS {
        settings.insert(
            format!("setting_{i}"),
            serde_json::json!({"type":"string","label":"Test"}),
        );
    }
    let manifest = serde_json::json!({
        "name": "test",
        "version": "1.0.0",
        "schema_version": 1,
        "api": { "endpoints": endpoints },
        "settings": settings
    });
    let raw = serde_json::to_vec(&manifest).unwrap();
    let (manifest, _) = validate_manifest(&raw, &validator).unwrap();
    assert_eq!(
        manifest.api.as_ref().unwrap().endpoints.len(),
        MAX_ENDPOINTS
    );
    assert_eq!(manifest.settings.as_ref().unwrap().len(), MAX_SETTINGS);
}

#[test]
fn validate_manifest_accepts_python_defaults_and_command() {
    let validator = make_validator();
    let manifest = serde_json::json!({
        "name": "python-plugin",
        "version": "1.0.0",
        "schema_version": 1,
        "python": {
            "run_path": "runtime",
            "source": "https://example.com/tool.py",
            "env": true,
            "dependencies": ["requests>=2.32"]
        },
        "commands": {
            "sync": {
                "description": "Run sync job",
                "python": {
                    "command": "python sync.py"
                }
            }
        }
    });
    let raw = serde_json::to_vec(&manifest).unwrap();
    let (manifest, _) = validate_manifest(&raw, &validator).unwrap();
    let command = manifest
        .commands
        .as_ref()
        .and_then(|commands| commands.get("sync"))
        .and_then(|entry| entry.python.as_ref())
        .expect("python command should deserialize");
    assert_eq!(command.command, "python sync.py");
}

#[test]
fn validate_manifest_rejects_python_source_that_is_not_url_or_absolute_path() {
    let validator = make_validator();
    let manifest = serde_json::json!({
        "name": "python-plugin",
        "version": "1.0.0",
        "schema_version": 1,
        "commands": {
            "sync": {
                "description": "Run sync job",
                "python": {
                    "command": "python sync.py",
                    "source": "relative/script.py"
                }
            }
        }
    });
    let raw = serde_json::to_vec(&manifest).unwrap();
    let err = validate_manifest(&raw, &validator).unwrap_err();
    assert!(
        err.to_string().contains("source"),
        "error should mention python source: {}",
        err
    );
}

#[test]
fn validate_plugin_name_accepts_simple() {
    assert_eq!(validate_plugin_name("my-plugin").unwrap(), "my-plugin");
    assert_eq!(validate_plugin_name("test").unwrap(), "test");
    assert_eq!(validate_plugin_name("a1").unwrap(), "a1");
    assert_eq!(
        validate_plugin_name("my.plugin.name").unwrap(),
        "my.plugin.name"
    );
}

#[test]
fn validate_plugin_name_accepts_scoped() {
    assert_eq!(
        validate_plugin_name("@scope/my-plugin").unwrap(),
        "my-plugin"
    );
}

#[test]
fn validate_plugin_name_rejects_path_traversal() {
    assert!(validate_plugin_name("../escape").is_err());
    assert!(validate_plugin_name("..").is_err());
    assert!(validate_plugin_name("foo\\bar").is_err());
}

#[test]
fn validate_plugin_name_rejects_empty() {
    assert!(validate_plugin_name("").is_err());
}

#[test]
fn scan_plugins_dir_loads_valid_manifest() {
    let tmp = tempfile::TempDir::new().unwrap();
    let plugin_dir = tmp.path().join("my-plugin");
    std::fs::create_dir_all(&plugin_dir).unwrap();
    std::fs::write(plugin_dir.join("plugin.json"), minimal_manifest_json()).unwrap();

    let validator = make_validator();
    let result = scan_plugins_dir(tmp.path(), &validator);
    assert_eq!(result.loaded.len(), 1);
    assert_eq!(result.loaded[0].manifest.name, "test");
    assert_eq!(result.loaded[0].dir_name, "my-plugin");
    assert!(result.skipped.is_empty());
}

#[test]
fn scan_plugins_dir_skips_invalid_manifest() {
    let tmp = tempfile::TempDir::new().unwrap();
    let plugin_dir = tmp.path().join("bad-plugin");
    std::fs::create_dir_all(&plugin_dir).unwrap();
    std::fs::write(plugin_dir.join("plugin.json"), "not json").unwrap();

    let validator = make_validator();
    let result = scan_plugins_dir(tmp.path(), &validator);
    assert!(result.loaded.is_empty());
    assert_eq!(result.skipped.len(), 1);
    assert_eq!(result.skipped[0].0, "bad-plugin");
}

fn repo_plugin_manifest_path(relative: &str) -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(relative)
}

#[test]
fn validate_repo_gitlab_manifest_loads() {
    let validator = make_validator();
    let path = repo_plugin_manifest_path("plugins/zorai-plugin-gitlab/gitlab/plugin.json");
    let raw = std::fs::read(&path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));

    let (manifest, _) = validate_manifest(&raw, &validator).expect("gitlab manifest should load");
    assert_eq!(manifest.name, "gitlab");
    assert_eq!(manifest.version, "1.1.0");
    assert!(
        manifest.api.is_some(),
        "gitlab plugin should declare api section"
    );
    assert!(
        manifest.commands.is_some(),
        "gitlab plugin should declare commands"
    );
    let endpoints = &manifest.api.as_ref().expect("gitlab api").endpoints;
    assert!(endpoints.contains_key("get_repo"));
    assert!(endpoints.contains_key("list_issues"));
    assert!(endpoints.contains_key("list_merge_requests"));
}

#[test]
fn validate_repo_linear_manifest_loads() {
    let validator = make_validator();
    let path = repo_plugin_manifest_path("plugins/zorai-plugin-linear/linear/plugin.json");
    let raw = std::fs::read(&path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));

    let (manifest, _) = validate_manifest(&raw, &validator).expect("linear manifest should load");
    assert_eq!(manifest.name, "linear");
    assert_eq!(manifest.version, "1.1.0");
    assert!(
        manifest.api.is_some(),
        "linear plugin should declare api section"
    );
    assert!(
        manifest.commands.is_some(),
        "linear plugin should declare commands"
    );
    let endpoints = &manifest.api.as_ref().expect("linear api").endpoints;
    assert!(endpoints.contains_key("get_issue"));
    assert!(endpoints.contains_key("list_issues"));
    assert!(endpoints.contains_key("list_projects"));
}

#[test]
fn validate_repo_jira_manifest_loads() {
    let validator = make_validator();
    let path = repo_plugin_manifest_path("plugins/zorai-plugin-jira/jira/plugin.json");
    let raw = std::fs::read(&path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));

    let (manifest, _) = validate_manifest(&raw, &validator).expect("jira manifest should load");
    assert_eq!(manifest.name, "jira");
    assert_eq!(manifest.version, "1.1.0");
    assert!(
        manifest.api.is_some(),
        "jira plugin should declare api section"
    );
    assert!(
        manifest.commands.is_some(),
        "jira plugin should declare commands"
    );
    let endpoints = &manifest.api.as_ref().expect("jira api").endpoints;
    assert!(endpoints.contains_key("get_issue"));
    assert!(endpoints.contains_key("list_issues"));
    assert!(endpoints.contains_key("list_projects"));
}

#[test]
fn scan_plugins_dir_skips_dir_without_manifest() {
    let tmp = tempfile::TempDir::new().unwrap();
    let plugin_dir = tmp.path().join("no-manifest");
    std::fs::create_dir_all(&plugin_dir).unwrap();

    let validator = make_validator();
    let result = scan_plugins_dir(tmp.path(), &validator);
    assert!(result.loaded.is_empty());
    assert_eq!(result.skipped.len(), 1);
}

#[test]
fn scan_plugins_dir_handles_nonexistent_dir() {
    let validator = make_validator();
    let result = scan_plugins_dir(std::path::Path::new("/nonexistent/path"), &validator);
    assert!(result.loaded.is_empty());
    assert!(result.skipped.is_empty());
}

fn project_root() -> std::path::PathBuf {
    let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest_dir
        .ancestors()
        .find(|path| path.join("plugins/zorai-plugin-gmail-calendar").is_dir())
        .expect("could not locate project root with plugins/ directory")
        .to_path_buf()
}

#[test]
fn gmail_manifest_validates_through_plugin_loader() {
    let root = project_root();
    let path = root.join("plugins/zorai-plugin-gmail-calendar/gmail/plugin.json");
    let raw = std::fs::read(&path).expect("gmail plugin.json should exist");
    let validator = make_validator();

    let (manifest, _) =
        validate_manifest(&raw, &validator).expect("gmail manifest should pass validation");

    assert_eq!(manifest.name, "gmail");
    assert_eq!(manifest.version, "1.3.0");
    assert_eq!(manifest.schema_version, 1);

    let auth = manifest
        .auth
        .as_ref()
        .expect("gmail should have auth section");
    assert_eq!(auth.auth_type, "oauth2");
    assert!(auth.pkce);
    assert!(auth.authorization_url.is_some());
    assert!(auth.token_url.is_some());
    assert_eq!(auth.scopes.as_ref().unwrap().len(), 3);

    let api = manifest
        .api
        .as_ref()
        .expect("gmail should have api section");
    assert!(
        api.endpoints.len() >= 3,
        "gmail should have at least 3 endpoints, got: {:?}",
        api.endpoints.keys().collect::<Vec<_>>()
    );
    assert!(api.endpoints.contains_key("list_inbox"));
    assert!(api.endpoints.contains_key("get_message"));
    assert!(api.endpoints.contains_key("search_messages"));

    let commands = manifest
        .commands
        .as_ref()
        .expect("gmail should have commands");
    assert!(
        commands.len() >= 2,
        "gmail should have at least 2 commands, got: {:?}",
        commands.keys().collect::<Vec<_>>()
    );
    assert!(commands.contains_key("inbox"));
    assert!(commands.contains_key("search"));

    let skills = manifest.skills.as_ref().expect("gmail should have skills");
    assert_eq!(skills.len(), 1);
}

#[test]
fn calendar_manifest_validates_through_plugin_loader() {
    let root = project_root();
    let path = root.join("plugins/zorai-plugin-gmail-calendar/calendar/plugin.json");
    let raw = std::fs::read(&path).expect("calendar plugin.json should exist");
    let validator = make_validator();

    let (manifest, _) =
        validate_manifest(&raw, &validator).expect("calendar manifest should pass validation");

    assert_eq!(manifest.name, "calendar");
    assert_eq!(manifest.version, "1.3.0");
    assert_eq!(manifest.schema_version, 1);

    let auth = manifest
        .auth
        .as_ref()
        .expect("calendar should have auth section");
    assert_eq!(auth.auth_type, "oauth2");
    assert!(auth.pkce);
    assert!(auth.authorization_url.is_some());
    assert!(auth.token_url.is_some());
    assert_eq!(auth.scopes.as_ref().unwrap().len(), 1);

    let api = manifest
        .api
        .as_ref()
        .expect("calendar should have api section");
    assert!(
        !api.endpoints.is_empty(),
        "calendar should have at least 1 endpoint, got: {:?}",
        api.endpoints.keys().collect::<Vec<_>>()
    );
    assert!(api.endpoints.contains_key("list_events"));

    let commands = manifest
        .commands
        .as_ref()
        .expect("calendar should have commands");
    assert!(
        !commands.is_empty(),
        "calendar should have at least 1 command, got: {:?}",
        commands.keys().collect::<Vec<_>>()
    );
    assert!(commands.contains_key("today"));

    let skills = manifest
        .skills
        .as_ref()
        .expect("calendar should have skills");
    assert_eq!(skills.len(), 1);
}

#[test]
fn autosearch_manifest_validates_through_plugin_loader() {
    let root = project_root();
    let path = root.join("plugins/zorai-plugin-autosearch/autosearch/plugin.json");
    let raw = std::fs::read(&path).expect("autosearch plugin.json should exist");
    let validator = make_validator();

    let (manifest, _) =
        validate_manifest(&raw, &validator).expect("autosearch manifest should pass validation");

    assert_eq!(manifest.name, "autosearch");
    assert_eq!(manifest.version, "1.0.0");
    assert_eq!(manifest.schema_version, 1);

    let settings = manifest
        .settings
        .as_ref()
        .expect("autosearch should have settings");
    assert!(settings.contains_key("workspace_path"));
    assert!(settings.contains_key("default_command_timeout_sec"));

    assert!(
        manifest.api.is_none(),
        "autosearch MVP should not declare api endpoints"
    );

    let python = manifest
        .python
        .as_ref()
        .expect("autosearch should have top-level python defaults");
    assert_eq!(python.run_path, None);
    assert_eq!(python.source, None);
    assert_eq!(python.dependencies.len(), 0);

    let commands = manifest
        .commands
        .as_ref()
        .expect("autosearch should have commands");
    assert!(commands.contains_key("check"));
    assert!(commands.contains_key("prepare"));
    assert!(commands.contains_key("train"));
    assert!(commands["check"].python.is_some());
    assert!(commands["prepare"].python.is_some());
    assert!(commands["train"].python.is_some());

    let skills = manifest
        .skills
        .as_ref()
        .expect("autosearch should have skills");
    assert_eq!(skills.len(), 1);
}

#[test]
fn github_manifest_validates_through_plugin_loader() {
    let root = project_root();
    let path = root.join("plugins/zorai-plugin-github/github/plugin.json");
    let raw = std::fs::read(&path).expect("github plugin.json should exist");
    let validator = make_validator();

    let (manifest, _) =
        validate_manifest(&raw, &validator).expect("github manifest should pass validation");

    assert_eq!(manifest.name, "github");
    assert_eq!(manifest.version, "1.1.0");
    assert_eq!(manifest.schema_version, 1);
    assert!(
        manifest.auth.is_none(),
        "github MVP should not require oauth"
    );

    let settings = manifest
        .settings
        .as_ref()
        .expect("github should have settings");
    let token_setting = settings
        .get("token")
        .expect("github should declare token setting");
    assert_eq!(token_setting.field_type, "string");
    assert!(token_setting.required);
    assert!(token_setting.secret);

    let api = manifest
        .api
        .as_ref()
        .expect("github should have api section");
    assert_eq!(api.base_url.as_deref(), Some("https://api.github.com"));
    assert_eq!(
        api.rate_limit
            .as_ref()
            .and_then(|limit| limit.requests_per_minute),
        Some(60)
    );
    assert!(api.endpoints.contains_key("get_repo"));
    assert!(api.endpoints.contains_key("list_issues"));
    assert!(api.endpoints.contains_key("list_pull_requests"));

    let repo_endpoint = api
        .endpoints
        .get("get_repo")
        .expect("github should declare get_repo endpoint");
    assert_eq!(repo_endpoint.method, "GET");
    assert_eq!(
        repo_endpoint.path,
        "/repos/{{params.owner}}/{{params.repo}}"
    );
    assert!(repo_endpoint.headers.is_some());
    assert!(repo_endpoint.response_template.is_some());

    let issues_endpoint = api
        .endpoints
        .get("list_issues")
        .expect("github should declare list_issues endpoint");
    assert_eq!(issues_endpoint.method, "GET");
    assert_eq!(
        issues_endpoint.path,
        r#"/repos/{{params.owner}}/{{params.repo}}/issues?state={{default params.state "open"}}&per_page={{default params.per_page "20"}}"#
    );
    assert!(issues_endpoint.headers.is_some());
    assert!(issues_endpoint.response_template.is_some());

    let pulls_endpoint = api
        .endpoints
        .get("list_pull_requests")
        .expect("github should declare list_pull_requests endpoint");
    assert_eq!(pulls_endpoint.method, "GET");
    assert_eq!(
        pulls_endpoint.path,
        r#"/repos/{{params.owner}}/{{params.repo}}/pulls?state={{default params.state "open"}}&per_page={{default params.per_page "20"}}"#
    );
    assert!(pulls_endpoint.headers.is_some());
    assert!(pulls_endpoint.response_template.is_some());

    let commands = manifest
        .commands
        .as_ref()
        .expect("github should have commands");
    let repo = commands
        .get("repo")
        .expect("github should expose repo command");
    assert_eq!(repo.action.as_deref(), Some("get_repo"));
    let issues = commands
        .get("issues")
        .expect("github should expose issues command");
    assert_eq!(issues.action.as_deref(), Some("list_issues"));
    let pulls = commands
        .get("pulls")
        .expect("github should expose pulls command");
    assert_eq!(pulls.action.as_deref(), Some("list_pull_requests"));
}

#[test]
fn core_connector_manifests_expose_readiness_probes_and_normalized_primitives() {
    let validator = make_validator();
    let cases: &[(&str, &str, &[&str], &[&str])] = &[
        (
            "plugins/zorai-plugin-github/github/plugin.json",
            "github",
            &[
                "check_health",
                "list_work_items",
                "fetch_work_item_context",
                "comment_on_work_item",
            ],
            &["comment", "merge"],
        ),
        (
            "plugins/zorai-plugin-gitlab/gitlab/plugin.json",
            "gitlab",
            &[
                "check_health",
                "list_work_items",
                "fetch_work_item_context",
                "comment_on_work_item",
            ],
            &["comment", "merge"],
        ),
        (
            "plugins/zorai-plugin-linear/linear/plugin.json",
            "linear",
            &[
                "check_health",
                "list_work_items",
                "fetch_work_item_context",
                "create_work_item",
            ],
            &["create", "transition"],
        ),
        (
            "plugins/zorai-plugin-jira/jira/plugin.json",
            "jira",
            &[
                "check_health",
                "list_work_items",
                "fetch_work_item_context",
                "create_work_item",
            ],
            &["create", "transition"],
        ),
        (
            "plugins/zorai-plugin-gmail-calendar/gmail/plugin.json",
            "gmail",
            &[
                "check_health",
                "list_threads",
                "fetch_thread_context",
                "draft_message",
                "reply_in_thread",
            ],
            &["draft", "reply"],
        ),
        (
            "plugins/zorai-plugin-gmail-calendar/calendar/plugin.json",
            "calendar",
            &[
                "check_health",
                "list_schedule_items",
                "fetch_schedule_context",
                "schedule_follow_up",
                "meeting_prep",
            ],
            &["update", "rsvp", "prep"],
        ),
    ];

    for &(relative_path, expected_kind, expected_endpoints, expected_commands) in cases {
        let path = repo_plugin_manifest_path(relative_path);
        let raw = std::fs::read(&path)
            .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
        let (manifest, _) = validate_manifest(&raw, &validator)
            .unwrap_or_else(|error| panic!("{} should validate: {error}", path.display()));

        let connector = manifest
            .connector
            .as_ref()
            .unwrap_or_else(|| panic!("{} should declare connector metadata", path.display()));
        assert_eq!(connector.kind, expected_kind);
        assert!(
            connector.readiness_endpoint.is_some(),
            "{} should declare readiness_endpoint",
            path.display()
        );
        assert!(
            !connector.workflow_primitives.is_empty(),
            "{} should declare workflow primitives",
            path.display()
        );

        let api = manifest
            .api
            .as_ref()
            .expect("connector should have api section");
        for endpoint in expected_endpoints {
            assert!(
                api.endpoints.contains_key(*endpoint),
                "{} should expose endpoint {}",
                path.display(),
                endpoint
            );
        }

        let commands = manifest
            .commands
            .as_ref()
            .expect("connector should have commands");
        for command in expected_commands {
            assert!(
                commands.contains_key(*command),
                "{} should expose command {}",
                path.display(),
                command
            );
        }
    }
}

#[test]
fn validate_repo_voice_lab_manifest_loads_with_local_endpoints() {
    let validator = make_validator();
    let path = repo_plugin_manifest_path("plugins/zorai-plugin-voice-lab/voice-lab/plugin.json");
    let raw = std::fs::read(&path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));

    let (manifest, _) =
        validate_manifest(&raw, &validator).expect("voice-lab manifest should load");
    assert_eq!(manifest.name, "voice-lab");
    let endpoints = &manifest
        .api
        .expect("voice-lab declares endpoints")
        .endpoints;
    for name in [
        "check",
        "setup",
        "render",
        "transcribe",
        "status",
        "cancel",
        "projects",
        "save",
    ] {
        let run = endpoints
            .get(name)
            .and_then(|endpoint| endpoint.run.as_ref())
            .unwrap_or_else(|| panic!("{name} should be a local endpoint"));
        assert!(run.python.command.contains("scripts/voice_lab.py"));
    }
}

#[test]
fn validate_repo_lab_manifests_run_only_their_own_scripts() {
    let validator = make_validator();
    for (relative, name, script, commands) in [
        (
            "plugins/zorai-plugin-video-lab/video-lab/plugin.json",
            "video-lab",
            "scripts/video_lab.py",
            &["check", "animatic", "estimate", "render", "cut", "demo"][..],
        ),
        (
            "plugins/zorai-plugin-image-lab/image-lab/plugin.json",
            "image-lab",
            "scripts/image_lab.py",
            &["check", "sheet", "fix", "uikit", "figure"][..],
        ),
        (
            "plugins/zorai-plugin-ml-research/ml-research/plugin.json",
            "ml-research",
            "scripts/ml_research.py",
            &["check", "lock", "sweep", "ablate", "report"][..],
        ),
    ] {
        let raw = std::fs::read(repo_plugin_manifest_path(relative))
            .unwrap_or_else(|error| panic!("failed to read {relative}: {error}"));
        let (manifest, _) = validate_manifest(&raw, &validator)
            .unwrap_or_else(|error| panic!("{relative} should validate: {error}"));
        assert_eq!(manifest.name, name);
        for (endpoint, definition) in &manifest.api.as_ref().expect("endpoints").endpoints {
            let run = definition
                .run
                .as_ref()
                .unwrap_or_else(|| panic!("{name}.{endpoint} should be a local endpoint"));
            assert!(run.python.command.contains(script), "{name}.{endpoint}");
        }
        let declared = manifest.commands.as_ref().expect("slash commands");
        for command in commands {
            assert!(
                declared.contains_key(*command),
                "{name} should offer /{name}.{command}"
            );
        }
    }
}

#[test]
fn validate_manifest_rejects_an_endpoint_with_neither_a_url_nor_a_local_command() {
    let validator = make_validator();
    let json = r#"{"name":"broken","version":"1","schema_version":1,"api":{"endpoints":{"x":{"params":{}}}}}"#;
    assert!(validate_manifest(json.as_bytes(), &validator).is_err());
}

#[tokio::test]
async fn gmail_and_calendar_panel_endpoints_build_valid_google_urls() {
    let validator = make_validator();
    let load = |path: &str| {
        let raw = std::fs::read(repo_plugin_manifest_path(path)).unwrap();
        validate_manifest(&raw, &validator).unwrap().0
    };
    let registry = crate::plugin::template::create_registry();
    let render = |manifest: &crate::plugin::manifest::PluginManifest,
                  endpoint: &str,
                  params: serde_json::Value| {
        let api = manifest.api.clone().unwrap();
        let endpoint = api.endpoints.get(endpoint).unwrap().clone();
        let context = serde_json::json!({ "params": params, "settings": {}, "auth": { "access_token": "t" } });
        let registry = registry.clone();
        async move {
            crate::plugin::template::render_request(&registry, &api, &endpoint, &context)
                .await
                .unwrap()
                .url
        }
    };
    let gmail = load("plugins/zorai-plugin-gmail-calendar/gmail/plugin.json");
    let first_page = render(
        &gmail,
        "ui_messages",
        serde_json::json!({ "query": "is:unread in:inbox" }),
    )
    .await;
    assert!(
        first_page.ends_with("/messages?maxResults=25&q=is%3Aunread%20in%3Ainbox"),
        "{first_page}"
    );
    let next_page = render(
        &gmail,
        "ui_messages",
        serde_json::json!({ "page_token": "abc/123" }),
    )
    .await;
    assert!(
        next_page.contains("q=in%3Ainbox&pageToken=abc%2F123"),
        "{next_page}"
    );

    let calendar = load("plugins/zorai-plugin-gmail-calendar/calendar/plugin.json");
    let events = render(
        &calendar,
        "ui_events",
        serde_json::json!({
            "calendar_id": "pl.polish#holiday@group.v.calendar.google.com",
            "time_min": "2026-11-09T00:00:00+01:00",
            "time_max": "2026-11-16T00:00:00+01:00"
        }),
    )
    .await;
    assert!(
        !events.contains('#'),
        "a raw # would cut the URL short: {events}"
    );
    assert!(events.contains("%23holiday%40group"), "{events}");
    assert!(
        events.contains("T00%3A00%3A00%2B01%3A00&timeMax="),
        "{events}"
    );

    let api = calendar.api.clone().unwrap();
    let context = |params: serde_json::Value| serde_json::json!({ "params": params, "settings": {}, "auth": { "access_token": "t" } });
    let update = api.endpoints.get("update_event").unwrap();
    assert_eq!(
        update.method, "PATCH",
        "PUT would wipe attendees and reminders"
    );
    let patched = crate::plugin::template::render_request(
        &registry,
        &api,
        update,
        &context(serde_json::json!({
            "calendar_id": "team#ops@group.calendar.google.com",
            "event_id": "abc_123",
            "patch": { "summary": "Moved \"sync\"" }
        })),
    )
    .await
    .unwrap();
    assert!(!patched.url.contains('#'), "{}", patched.url);
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(patched.body.as_deref().unwrap()).unwrap(),
        serde_json::json!({ "summary": "Moved \"sync\"" })
    );
    for endpoint in [
        "delete_event",
        "update_attendance",
        "get_event",
        "create_event",
    ] {
        let rendered = crate::plugin::template::render_request(
            &registry,
            &api,
            api.endpoints.get(endpoint).unwrap(),
            &context(serde_json::json!({
                "calendar_id": "team#ops@group.calendar.google.com",
                "event_id": "abc_123",
                "email": "me@example.com",
                "summary": "x",
                "start_time": "2026-11-13T10:00:00Z",
                "end_time": "2026-11-13T11:00:00Z",
                "location": "",
                "description": ""
            })),
        )
        .await
        .unwrap();
        assert!(!rendered.url.contains('#'), "{endpoint}: {}", rendered.url);
    }
}

#[test]
fn every_plugin_shipped_in_the_repo_passes_the_schema_so_bundle_sync_never_skips_one() {
    let validator = make_validator();
    let found = crate::plugin::bundled::find_bundled_plugins(&repo_plugin_manifest_path("plugins"));
    assert!(
        found.len() > 30,
        "expected the repo's bundled plugins, found {}",
        found.len()
    );
    let failures: Vec<String> = found
        .iter()
        .filter_map(|dir| {
            let raw = std::fs::read(dir.join("plugin.json")).ok()?;
            validate_manifest(&raw, &validator)
                .err()
                .map(|error| format!("{}: {error}", dir.display()))
        })
        .collect();
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
