use super::*;
use std::path::PathBuf;
use std::sync::Mutex;

static GLOBAL_POLICY_GUARD: Mutex<()> = Mutex::new(());

fn with_library<T>(config: SkillLibraryConfig, check: impl FnOnce() -> T) -> T {
    let _guard = GLOBAL_POLICY_GUARD
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    set_skill_library(&config);
    let result = check();
    set_skill_library(&SkillLibraryConfig::default());
    result
}

#[test]
fn a_locked_or_disabled_folder_covers_every_skill_inside_it_but_not_lookalike_siblings() {
    let config = SkillLibraryConfig {
        locked: vec!["zz-locked-test/".into()],
        disabled: vec!["./zz-disabled-test\\bio".into()],
        preferred: Default::default(),
    };
    with_library(config, || {
        let root = PathBuf::from("/data/skills");
        assert!(is_skill_path_locked(
            &root,
            &root.join("zz-locked-test/rust/SKILL.md")
        ));
        assert!(is_skill_relative_path_locked("zz-locked-test"));
        assert!(!is_skill_relative_path_locked(
            "zz-locked-test-extra/SKILL.md"
        ));
        assert!(is_skill_relative_path_disabled(
            "zz-disabled-test/bio/SKILL.md"
        ));
        assert!(!is_skill_relative_path_disabled(
            "zz-disabled-test/biology/SKILL.md"
        ));
        assert!(!is_skill_path_disabled(
            &root,
            &PathBuf::from("/elsewhere/zz-disabled-test/bio/SKILL.md")
        ));
    });
}

#[test]
fn keys_that_escape_the_skills_root_are_ignored() {
    assert_eq!(normalize_skill_key("../outside"), None);
    assert_eq!(normalize_skill_key("  "), None);
    assert_eq!(normalize_skill_key("a//b/./c/"), Some("a/b/c".to_string()));
}

#[test]
fn preferred_skills_name_only_available_skills_and_never_inline_their_content() {
    let mut preferred = std::collections::BTreeMap::new();
    preferred.insert(
        "Swarog".to_string(),
        vec![
            "zz-preferred-test".to_string(),
            "zz-missing-test".to_string(),
        ],
    );
    let config = SkillLibraryConfig {
        locked: Vec::new(),
        disabled: Vec::new(),
        preferred,
    };
    with_library(config, || {
        let available = vec!["zz-preferred-test".to_string(), "other".to_string()];
        let section = preferred_skills_prompt_section("swarog", &available).expect("section");
        assert!(section.starts_with("## Preferred Skills"));
        assert!(section.contains("- zz-preferred-test"));
        assert!(section.contains("read_skill"));
        assert!(!section.contains("zz-missing-test"));
        assert!(!section.contains("- other"));
        assert_eq!(preferred_skills_prompt_section("weles", &available), None);
    });
}

#[test]
fn a_disabled_folder_hides_its_skills_from_the_preferred_prompt_section() {
    let root = tempfile::tempdir().expect("tempdir");
    for skill in ["zz-enabled-skill", "zz-off/zz-hidden-skill"] {
        let dir = root.path().join(skill);
        std::fs::create_dir_all(&dir).expect("skill dir");
        std::fs::write(dir.join("SKILL.md"), "# skill").expect("skill doc");
    }
    let mut preferred = std::collections::BTreeMap::new();
    preferred.insert(
        "perun".to_string(),
        vec![
            "zz-enabled-skill".to_string(),
            "zz-hidden-skill".to_string(),
        ],
    );
    let config = SkillLibraryConfig {
        locked: Vec::new(),
        disabled: vec!["zz-off".to_string()],
        preferred,
    };
    with_library(config, || {
        let section = crate::agent::system_prompt::render_preferred_skills(root.path(), "perun")
            .expect("preferred section");
        assert!(section.contains("- zz-enabled-skill"));
        assert!(!section.contains("zz-hidden-skill"));
        assert!(
            crate::agent::system_prompt::render_preferred_skills(root.path(), "swarog").is_none()
        );
    });
}
