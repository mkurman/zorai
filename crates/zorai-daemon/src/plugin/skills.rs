//! Skill bundling for plugins: copies YAML skill files on install, removes on uninstall.

use anyhow::Result;
use std::path::Path;

use super::manifest::PluginManifest;

/// Copy bundled skill files from plugin dir to the standard skills directory.
/// Source: `plugins_dir/{plugin_name}/{skill_path}`
/// Target: `skills_root/plugins/{plugin_name}/{filename}`
/// Returns count of successfully copied files.
pub(crate) fn install_bundled_skills(
    plugins_dir: &Path,
    plugin_name: &str,
    manifest: &PluginManifest,
    skills_root: &Path,
) -> Result<usize> {
    let skill_paths = match &manifest.skills {
        Some(paths) if !paths.is_empty() => paths,
        _ => return Ok(0),
    };

    let target_dir = skills_root.join("plugins").join(plugin_name);
    std::fs::create_dir_all(&target_dir)?;

    let mut copied = 0usize;
    for skill_path in skill_paths {
        let source = plugins_dir.join(plugin_name).join(skill_path);
        if !source.exists() {
            tracing::warn!(
                plugin = %plugin_name,
                skill = %skill_path,
                "bundled skill file not found, skipping"
            );
            continue;
        }
        let filename = Path::new(skill_path)
            .file_name()
            .unwrap_or_else(|| std::ffi::OsStr::new(skill_path));
        let dest = target_dir.join(filename);
        if crate::agent::skill_library::is_skill_path_locked(skills_root, &dest) {
            tracing::info!(
                plugin = %plugin_name,
                skill = %dest.display(),
                "skipping locked bundled skill"
            );
            continue;
        }
        std::fs::copy(&source, &dest)?;
        copied += 1;
    }

    tracing::info!(
        plugin = %plugin_name,
        count = copied,
        "installed bundled skills"
    );
    Ok(copied)
}

/// Remove skill directory for a plugin.
/// Target: `skills_root/plugins/{plugin_name}/`
pub(crate) fn remove_bundled_skills(plugin_name: &str, skills_root: &Path) -> Result<()> {
    let target_dir = skills_root.join("plugins").join(plugin_name);
    if crate::agent::skill_library::is_skill_path_locked(skills_root, &target_dir) {
        tracing::info!(plugin = %plugin_name, "keeping locked bundled skills");
        return Ok(());
    }
    if target_dir.exists() {
        std::fs::remove_dir_all(&target_dir)?;
        tracing::info!(plugin = %plugin_name, "removed bundled skills");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn make_manifest(name: &str, skills: Option<Vec<String>>) -> PluginManifest {
        PluginManifest {
            name: name.to_string(),
            version: "1.0.0".to_string(),
            schema_version: 1,
            description: None,
            author: None,
            license: None,
            zorai_version: None,
            settings: None,
            api: None,
            commands: None,
            skills,
            auth: None,
            connector: None,
            python: None,
            extra: HashMap::new(),
        }
    }

    #[test]
    fn install_bundled_skills_copies_yaml_files() {
        let tmp = tempfile::TempDir::new().unwrap();
        let plugins_dir = tmp.path().join("plugins");
        let skills_root = tmp.path().join("skills");
        std::fs::create_dir_all(&skills_root).unwrap();

        let plugin_dir = plugins_dir.join("my-plugin");
        std::fs::create_dir_all(plugin_dir.join("skills")).unwrap();
        std::fs::write(plugin_dir.join("skills/search.yaml"), "name: search").unwrap();
        std::fs::write(plugin_dir.join("skills/send.yaml"), "name: send").unwrap();

        let manifest = make_manifest(
            "my-plugin",
            Some(vec![
                "skills/search.yaml".to_string(),
                "skills/send.yaml".to_string(),
            ]),
        );

        let count =
            install_bundled_skills(&plugins_dir, "my-plugin", &manifest, &skills_root).unwrap();
        assert_eq!(count, 2);

        let target_dir = skills_root.join("plugins").join("my-plugin");
        assert!(target_dir.join("search.yaml").exists());
        assert!(target_dir.join("send.yaml").exists());
    }

    #[test]
    fn install_bundled_skills_noop_when_none() {
        let tmp = tempfile::TempDir::new().unwrap();
        let plugins_dir = tmp.path().join("plugins");
        let skills_root = tmp.path().join("skills");

        let manifest = make_manifest("my-plugin", None);
        let count =
            install_bundled_skills(&plugins_dir, "my-plugin", &manifest, &skills_root).unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn install_bundled_skills_noop_when_empty() {
        let tmp = tempfile::TempDir::new().unwrap();
        let plugins_dir = tmp.path().join("plugins");
        let skills_root = tmp.path().join("skills");

        let manifest = make_manifest("my-plugin", Some(vec![]));
        let count =
            install_bundled_skills(&plugins_dir, "my-plugin", &manifest, &skills_root).unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn remove_bundled_skills_removes_dir() {
        let tmp = tempfile::TempDir::new().unwrap();
        let skills_root = tmp.path().join("skills");
        let target_dir = skills_root.join("plugins").join("my-plugin");
        std::fs::create_dir_all(&target_dir).unwrap();
        std::fs::write(target_dir.join("search.yaml"), "name: search").unwrap();

        remove_bundled_skills("my-plugin", &skills_root).unwrap();
        assert!(!target_dir.exists());
    }

    #[test]
    fn remove_bundled_skills_noop_when_missing() {
        let tmp = tempfile::TempDir::new().unwrap();
        let skills_root = tmp.path().join("skills");
        remove_bundled_skills("nonexistent-plugin", &skills_root).unwrap();
    }
}
