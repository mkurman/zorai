use std::collections::BTreeMap;
use std::path::Path;
use std::sync::RwLock;

use super::types::SkillLibraryConfig;

static SKILL_LIBRARY: RwLock<SkillLibraryConfig> = RwLock::new(SkillLibraryConfig {
    locked: Vec::new(),
    disabled: Vec::new(),
    preferred: BTreeMap::new(),
});

pub(crate) fn normalize_skill_key(raw: &str) -> Option<String> {
    let normalized = raw.trim().replace('\\', "/");
    let parts = normalized
        .split('/')
        .filter(|part| !part.is_empty() && *part != ".")
        .collect::<Vec<_>>();
    if parts.is_empty() || parts.iter().any(|part| *part == "..") {
        return None;
    }
    Some(parts.join("/"))
}

fn normalized_keys(keys: &[String]) -> Vec<String> {
    keys.iter()
        .filter_map(|key| normalize_skill_key(key))
        .collect()
}

pub(crate) fn set_skill_library(config: &SkillLibraryConfig) {
    let preferred = config
        .preferred
        .iter()
        .map(|(agent_id, skills)| {
            let mut names = skills
                .iter()
                .map(|skill| skill.trim().to_string())
                .filter(|skill| !skill.is_empty())
                .collect::<Vec<_>>();
            names.dedup();
            (agent_id.trim().to_ascii_lowercase(), names)
        })
        .filter(|(agent_id, names)| !agent_id.is_empty() && !names.is_empty())
        .collect();
    let next = SkillLibraryConfig {
        locked: normalized_keys(&config.locked),
        disabled: normalized_keys(&config.disabled),
        preferred,
    };
    *SKILL_LIBRARY
        .write()
        .unwrap_or_else(|error| error.into_inner()) = next;
}

fn covered_by(keys: &[String], relative: &str) -> bool {
    let Some(relative) = normalize_skill_key(relative) else {
        return false;
    };
    keys.iter().any(|key| {
        relative == *key
            || relative
                .strip_prefix(key.as_str())
                .is_some_and(|rest| rest.starts_with('/'))
    })
}

fn relative_to_root(skills_root: &Path, path: &Path) -> Option<String> {
    path.strip_prefix(skills_root)
        .ok()
        .map(|relative| relative.to_string_lossy().into_owned())
}

pub(crate) fn is_skill_relative_path_disabled(relative: &str) -> bool {
    let library = SKILL_LIBRARY
        .read()
        .unwrap_or_else(|error| error.into_inner());
    covered_by(&library.disabled, relative)
}

pub(crate) fn is_skill_relative_path_locked(relative: &str) -> bool {
    let library = SKILL_LIBRARY
        .read()
        .unwrap_or_else(|error| error.into_inner());
    covered_by(&library.locked, relative)
}

pub(crate) fn is_skill_path_disabled(skills_root: &Path, path: &Path) -> bool {
    relative_to_root(skills_root, path)
        .is_some_and(|relative| is_skill_relative_path_disabled(&relative))
}

pub(crate) fn is_skill_path_locked(skills_root: &Path, path: &Path) -> bool {
    relative_to_root(skills_root, path)
        .is_some_and(|relative| is_skill_relative_path_locked(&relative))
}

pub(crate) fn preferred_skills_for_agent(agent_id: &str) -> Vec<String> {
    let library = SKILL_LIBRARY
        .read()
        .unwrap_or_else(|error| error.into_inner());
    library
        .preferred
        .get(&agent_id.trim().to_ascii_lowercase())
        .cloned()
        .unwrap_or_default()
}

pub(crate) fn preferred_skills_prompt_section(
    agent_id: &str,
    available_skills: &[String],
) -> Option<String> {
    let preferred = preferred_skills_for_agent(agent_id)
        .into_iter()
        .filter(|skill| {
            available_skills
                .iter()
                .any(|available| available.eq_ignore_ascii_case(skill))
        })
        .collect::<Vec<_>>();
    if preferred.is_empty() {
        return None;
    }
    let mut section = String::from(
        "## Preferred Skills\n\
         The operator prefers these skills for your work. They are not preloaded: when a task matches one, \
         choose it over similar skills and load it with `read_skill` before relying on it.\n",
    );
    for skill in preferred {
        section.push_str(&format!("- {skill}\n"));
    }
    Some(section)
}

#[cfg(test)]
#[path = "tests/skill_library.rs"]
mod tests;
