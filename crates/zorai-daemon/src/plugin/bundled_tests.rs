use super::*;
use std::sync::Arc;

fn write_plugin(dir: &Path, name: &str, version: &str, with_skill: bool) {
    std::fs::create_dir_all(dir.join("skills")).unwrap();
    let mut manifest = serde_json::json!({
        "name": name,
        "version": version,
        "schema_version": 1,
        "commands": { "check": { "description": "Check", "python": { "command": "python3 -c 1" } } }
    });
    if with_skill {
        manifest["skills"] = serde_json::json!(["skills/guide.md"]);
        std::fs::write(
            dir.join("skills/guide.md"),
            "---\nname: guide\n---\n# Guide\n",
        )
        .unwrap();
    }
    std::fs::write(dir.join("plugin.json"), manifest.to_string()).unwrap();
}

async fn manager(root: &Path) -> crate::plugin::PluginManager {
    let history = Arc::new(
        crate::history::HistoryStore::new_test_store(root)
            .await
            .unwrap(),
    );
    crate::plugin::PluginManager::new(history, root.join("home/.zorai/plugins"))
}

#[test]
fn bundle_discovery_finds_nested_plugins_and_skips_tests_and_caches() {
    let root = tempfile::tempdir().unwrap();
    write_plugin(
        &root.path().join("zorai-plugin-gmail-calendar/gmail"),
        "gmail",
        "1",
        false,
    );
    write_plugin(
        &root.path().join("zorai-plugin-gmail-calendar/calendar"),
        "calendar",
        "1",
        false,
    );
    write_plugin(
        &root.path().join("zorai-plugin-voice-lab/voice-lab"),
        "voice-lab",
        "1",
        false,
    );
    write_plugin(
        &root.path().join("zorai-plugin-science/tests/fixture"),
        "fixture",
        "1",
        false,
    );
    write_plugin(
        &root.path().join("zorai-plugin-x/node_modules/dep"),
        "dep",
        "1",
        false,
    );
    let names: Vec<String> = find_bundled_plugins(root.path())
        .iter()
        .map(|dir| dir.file_name().unwrap().to_string_lossy().to_string())
        .collect();
    assert_eq!(names, ["calendar", "gmail", "voice-lab"]);
}

#[test]
fn the_bundle_is_found_beside_packaged_and_development_binaries_or_in_the_home_folder() {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    write_plugin(&repo.join("plugins/zorai-plugin-a/a"), "a", "1", false);
    let dev_exe = repo.join("target/debug");
    std::fs::create_dir_all(&dev_exe).unwrap();
    assert_eq!(
        bundled_plugins_dir_from(None, Some(&dev_exe), None).map(|dir| dir.canonicalize().unwrap()),
        Some(repo.join("plugins").canonicalize().unwrap())
    );
    let app = root.path().join("app/resources");
    write_plugin(&app.join("plugins/a"), "a", "1", false);
    std::fs::create_dir_all(app.join("bin")).unwrap();
    assert_eq!(
        bundled_plugins_dir_from(None, Some(&app.join("bin")), None)
            .map(|dir| dir.canonicalize().unwrap()),
        Some(app.join("plugins").canonicalize().unwrap())
    );
    let home = root.path().join("home-bundle");
    write_plugin(&home.join("a"), "a", "1", false);
    assert_eq!(
        bundled_plugins_dir_from(None, Some(&root.path().join("nowhere")), Some(home.clone())),
        Some(home.clone())
    );
    assert_eq!(
        bundled_plugins_dir_from(
            Some(root.path().join("missing")),
            Some(&dev_exe),
            Some(home)
        ),
        None
    );
}

#[tokio::test]
async fn bundled_plugins_arrive_disabled_and_stay_out_of_the_agents_way_until_enabled() {
    let root = tempfile::tempdir().unwrap();
    let bundle = root.path().join("bundle");
    write_plugin(&bundle.join("pkg/gmail"), "gmail", "1.0.0", true);
    let manager = manager(root.path()).await;

    let report = manager.sync_bundled_plugins(&bundle).await;
    assert_eq!(report.installed, ["gmail"]);
    assert_eq!(manager.load_all_from_disk().await, (1, 0));
    let listed = manager.list_plugins().await;
    assert!(!listed[0].enabled);
    assert!(manager.list_commands().await.is_empty());
    let skills_dir = root.path().join("home/.zorai/skills");
    let skill_files = |dir: &Path| walk_count(dir);
    assert_eq!(
        skill_files(&skills_dir),
        0,
        "disabled plugins must not add skills"
    );

    manager.set_enabled("gmail", true).await.unwrap();
    assert_eq!(manager.list_commands().await.len(), 1);
    assert!(skill_files(&skills_dir) > 0);
    manager.set_enabled("gmail", false).await.unwrap();
    assert!(manager.list_commands().await.is_empty());
    assert_eq!(skill_files(&skills_dir), 0);

    assert_eq!(
        manager.sync_bundled_plugins(&bundle).await,
        BundleSyncReport::default(),
        "a second start changes nothing"
    );
}

#[tokio::test]
async fn a_newer_bundle_updates_its_plugins_but_keeps_choices_and_never_touches_user_installs() {
    let root = tempfile::tempdir().unwrap();
    let bundle = root.path().join("bundle");
    write_plugin(&bundle.join("pkg/gmail"), "gmail", "1.0.0", false);
    write_plugin(&bundle.join("pkg/jira"), "jira", "1.0.0", false);
    write_plugin(&bundle.join("pkg/linear"), "linear", "1.0.0", false);
    let manager = manager(root.path()).await;
    let plugins_dir = root.path().join("home/.zorai/plugins");

    write_plugin(&plugins_dir.join("jira"), "jira", "0.9.0-mine", false);
    manager
        .register_plugin("jira", "local:/home/me/jira")
        .await
        .unwrap();
    manager.sync_bundled_plugins(&bundle).await;
    manager.load_all_from_disk().await;
    manager.set_enabled("gmail", true).await.unwrap();
    std::fs::create_dir_all(plugins_dir.join("gmail/.python/env")).unwrap();
    manager.unregister_plugin("linear").await.unwrap();
    std::fs::remove_dir_all(plugins_dir.join("linear")).ok();

    write_plugin(&bundle.join("pkg/gmail"), "gmail", "1.1.0", false);
    write_plugin(&bundle.join("pkg/jira"), "jira", "1.1.0", false);
    write_plugin(&bundle.join("pkg/linear"), "linear", "1.1.0", false);
    let report = manager.sync_bundled_plugins(&bundle).await;
    assert_eq!(
        report,
        BundleSyncReport {
            installed: vec![],
            updated: vec!["gmail".to_string()]
        }
    );
    manager.load_all_from_disk().await;

    let listed = manager.list_plugins().await;
    let gmail = listed.iter().find(|plugin| plugin.name == "gmail").unwrap();
    assert_eq!(gmail.version, "1.1.0");
    assert!(
        gmail.enabled,
        "the user's choice to enable it survives the update"
    );
    assert!(
        plugins_dir.join("gmail/.python/env").is_dir(),
        "downloaded environments are kept"
    );
    let jira = listed.iter().find(|plugin| plugin.name == "jira").unwrap();
    assert_eq!(
        jira.version, "0.9.0-mine",
        "plugins the user installed are never replaced"
    );
    assert!(
        listed.iter().all(|plugin| plugin.name != "linear"),
        "a removed plugin does not come back"
    );
}

fn walk_count(dir: &Path) -> usize {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };
    entries
        .flatten()
        .map(|entry| {
            if entry.path().is_dir() {
                walk_count(&entry.path())
            } else {
                1
            }
        })
        .sum()
}

#[tokio::test]
async fn two_plugins_can_both_offer_a_check_command_because_each_lives_under_its_own_name() {
    let root = tempfile::tempdir().unwrap();
    let manager = manager(root.path()).await;
    let plugins_dir = root.path().join("home/.zorai/plugins");
    for name in ["voice-lab", "video-lab"] {
        write_plugin(&plugins_dir.join(name), name, "1.0.0", true);
        manager
            .register_plugin(name, &format!("local:/repo/{name}"))
            .await
            .unwrap();
    }

    let mut commands: Vec<String> = manager
        .list_commands()
        .await
        .into_iter()
        .map(|command| command.command)
        .collect();
    commands.sort();
    assert_eq!(commands, vec!["/video-lab.check", "/voice-lab.check"]);
}
