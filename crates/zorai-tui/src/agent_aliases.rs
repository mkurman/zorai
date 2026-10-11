use std::collections::BTreeMap;
use std::sync::RwLock;

type AliasTable = RwLock<BTreeMap<String, String>>;

#[cfg(not(test))]
static AGENT_ALIASES: AliasTable = RwLock::new(BTreeMap::new());

#[cfg(test)]
thread_local! {
    static AGENT_ALIASES: AliasTable = const { RwLock::new(BTreeMap::new()) };
}

fn with_table<R>(f: impl FnOnce(&AliasTable) -> R) -> R {
    #[cfg(not(test))]
    {
        f(&AGENT_ALIASES)
    }
    #[cfg(test)]
    {
        AGENT_ALIASES.with(f)
    }
}

pub(crate) const ALIASABLE_AGENT_IDS: [&str; 3] = ["swarog", "rarog", "weles"];

pub(crate) fn aliases_from_raw(raw: &serde_json::Value) -> BTreeMap<String, String> {
    let mut map = BTreeMap::new();
    let Some(entries) = raw.get("agent_aliases").and_then(|value| value.as_object()) else {
        return map;
    };
    for id in ALIASABLE_AGENT_IDS {
        if let Some(alias) = entries
            .get(id)
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            map.insert(id.to_string(), alias.to_string());
        }
    }
    map
}

pub(crate) fn set_from_raw(raw: &serde_json::Value) {
    let map = aliases_from_raw(raw);
    with_table(|table| {
        if let Ok(mut guard) = table.write() {
            *guard = map;
        }
    });
}

pub(crate) fn current() -> BTreeMap<String, String> {
    with_table(|table| table.read().map(|guard| guard.clone()).unwrap_or_default())
}

pub(crate) fn aliasable_agent_id(name_or_id: &str) -> Option<&'static str> {
    match name_or_id.trim().to_ascii_lowercase().as_str() {
        "swarog" | "svarog" => Some("swarog"),
        "rarog" => Some("rarog"),
        "weles" | "veles" => Some("weles"),
        _ => None,
    }
}

pub(crate) fn display_agent_name_in(name_or_id: &str, map: &BTreeMap<String, String>) -> String {
    aliasable_agent_id(name_or_id)
        .and_then(|id| map.get(id))
        .map(|alias| alias.trim())
        .filter(|alias| !alias.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| name_or_id.to_string())
}

pub(crate) fn display_agent_name(name_or_id: &str) -> String {
    with_table(|table| match table.read() {
        Ok(guard) => display_agent_name_in(name_or_id, &guard),
        Err(_) => name_or_id.to_string(),
    })
}

pub(crate) fn agent_id_for_alias_in(typed: &str, map: &BTreeMap<String, String>) -> Option<String> {
    let typed = typed.trim();
    if typed.is_empty() {
        return None;
    }
    map.iter()
        .find(|(_, alias)| alias.trim().eq_ignore_ascii_case(typed))
        .map(|(id, _)| id.clone())
}

pub(crate) fn agent_id_for_alias(typed: &str) -> Option<String> {
    with_table(|table| {
        table
            .read()
            .ok()
            .and_then(|guard| agent_id_for_alias_in(typed, &guard))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn aliases() -> BTreeMap<String, String> {
        aliases_from_raw(&serde_json::json!({
            "agent_aliases": {"swarog": "Atlas", "rarog": "", "weles": "Judge"}
        }))
    }

    #[test]
    fn alias_replaces_builtin_by_id_or_stored_name_so_old_threads_show_it() {
        let map = aliases();
        assert_eq!(display_agent_name_in("swarog", &map), "Atlas");
        assert_eq!(display_agent_name_in("Svarog", &map), "Atlas");
        assert_eq!(display_agent_name_in(" Swarog ", &map), "Atlas");
        assert_eq!(display_agent_name_in("Weles", &map), "Judge");
        assert_eq!(display_agent_name_in("veles", &map), "Judge");
    }

    #[test]
    fn empty_alias_keeps_canonical_name() {
        let map = aliases();
        assert!(!map.contains_key("rarog"));
        assert_eq!(display_agent_name_in("Rarog", &map), "Rarog");
    }

    #[test]
    fn subagents_and_personas_are_never_aliased() {
        let map = aliases();
        assert_eq!(display_agent_name_in("Perun", &map), "Perun");
        assert_eq!(display_agent_name_in("Radogost", &map), "Radogost");
        assert_eq!(display_agent_name_in("my-reviewer", &map), "my-reviewer");
    }

    #[test]
    fn typed_alias_resolves_to_canonical_id_case_insensitively() {
        let map = aliases();
        assert_eq!(
            agent_id_for_alias_in("atlas", &map).as_deref(),
            Some("swarog")
        );
        assert_eq!(
            agent_id_for_alias_in("JUDGE", &map).as_deref(),
            Some("weles")
        );
        assert_eq!(agent_id_for_alias_in("Rarog", &map), None);
        assert_eq!(agent_id_for_alias_in("", &map), None);
    }

    #[test]
    fn at_directive_with_alias_targets_canonical_agent_like_builtin_name() {
        let map = aliases();
        let mut known = vec!["svarog".to_string(), "weles".to_string()];
        known.extend(map.values().cloned());
        let directive =
            crate::state::input_refs::parse_leading_agent_directive("@Atlas do X", &known)
                .expect("alias should be a known directive target");
        assert_eq!(
            agent_id_for_alias_in(&directive.agent_alias, &map).as_deref(),
            Some("swarog")
        );
        assert_eq!(directive.body, "do X");
        assert!(crate::state::input_refs::parse_leading_agent_directive(
            "@Atlas do X",
            &["svarog".to_string()]
        )
        .is_none());
    }
}
