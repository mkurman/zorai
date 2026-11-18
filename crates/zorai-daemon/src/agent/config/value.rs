use super::*;

pub(super) fn is_sensitive_config_key(key: &str) -> bool {
    let normalized = camel_to_snake_key(key);
    let lower = normalized.to_ascii_lowercase();
    matches!(
        normalized.as_str(),
        "api_key"
            | "slack_token"
            | "telegram_token"
            | "discord_token"
            | "whatsapp_token"
            | "firecrawl_api_key"
            | "exa_api_key"
            | "tavily_api_key"
            | "keenable_api_key"
            | "youcom_api_key"
            | "honcho_api_key"
            | "client_secret"
            | "access_token"
            | "refresh_token"
    ) || key.ends_with("_token")
        || key.ends_with("_api_key")
        || lower.contains("oauth")
        || lower.contains("credential")
}

pub(super) fn redact_config_value(value: &Value) -> Value {
    match value {
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(key, value)| {
                    let redacted = if is_sensitive_config_key(key) {
                        Value::String("<redacted>".to_string())
                    } else {
                        redact_config_value(value)
                    };
                    (key.clone(), redacted)
                })
                .collect(),
        ),
        Value::Array(items) => Value::Array(items.iter().map(redact_config_value).collect()),
        other => other.clone(),
    }
}

pub(super) fn top_level_config_keys(value: &Value) -> Vec<String> {
    value
        .as_object()
        .map(|map| map.keys().cloned().collect())
        .unwrap_or_default()
}

pub(super) fn camel_to_snake_key(key: &str) -> String {
    let mut result = String::with_capacity(key.len() + 4);
    let mut prev_is_lower_or_digit = false;
    for ch in key.chars() {
        if ch.is_ascii_uppercase() {
            if prev_is_lower_or_digit {
                result.push('_');
            }
            result.push(ch.to_ascii_lowercase());
            prev_is_lower_or_digit = false;
        } else {
            prev_is_lower_or_digit = ch.is_ascii_lowercase() || ch.is_ascii_digit();
            result.push(ch);
        }
    }
    result
}

pub(crate) fn normalize_config_keys_to_snake_case(value: &mut Value) {
    match value {
        Value::Object(map) => {
            let entries: Vec<(String, Value)> = map.clone().into_iter().collect();
            map.clear();
            for (key, mut child) in entries {
                normalize_config_keys_to_snake_case(&mut child);
                map.insert(camel_to_snake_key(&key), child);
            }
        }
        Value::Array(items) => {
            for item in items {
                normalize_config_keys_to_snake_case(item);
            }
        }
        _ => {}
    }
}

pub(super) fn merge_json_value(base: &mut Value, patch: Value) {
    match (base, patch) {
        (Value::Object(base_map), Value::Object(patch_map)) => {
            for (key, patch_value) in patch_map {
                match base_map.get_mut(&key) {
                    Some(base_value) => merge_json_value(base_value, patch_value),
                    None => {
                        base_map.insert(key, patch_value);
                    }
                }
            }
        }
        (base_slot, patch_value) => {
            *base_slot = patch_value;
        }
    }
}

pub(super) fn escape_pointer_segment(segment: &str) -> String {
    segment.replace('~', "~0").replace('/', "~1")
}

pub(super) fn unescape_pointer_segment(segment: &str) -> String {
    segment.replace("~1", "/").replace("~0", "~")
}

pub(crate) fn flatten_config_value_to_items(
    value: &Value,
    pointer: &str,
    items: &mut Vec<(String, Value)>,
) {
    match value {
        Value::Object(map) if !map.is_empty() => {
            for (key, child) in map {
                let next = format!("{}/{}", pointer, escape_pointer_segment(key));
                flatten_config_value_to_items(child, &next, items);
            }
        }
        other => items.push((pointer.to_string(), other.clone())),
    }
}

pub(super) fn set_config_value_at_pointer(
    root: &mut Value,
    pointer: &str,
    value: Value,
) -> Result<()> {
    if !pointer.starts_with('/') {
        anyhow::bail!("config key path must be a JSON pointer starting with '/'");
    }
    let mut current = root;
    let mut segments = pointer
        .trim_start_matches('/')
        .split('/')
        .filter(|segment| !segment.is_empty())
        .peekable();
    while let Some(segment) = segments.next() {
        let key = unescape_pointer_segment(segment);
        if segments.peek().is_none() {
            let object = current
                .as_object_mut()
                .context("config pointer parent is not an object")?;
            object.insert(key, value);
            return Ok(());
        }
        let object = current
            .as_object_mut()
            .context("config pointer parent is not an object")?;
        current = object
            .entry(key)
            .or_insert_with(|| Value::Object(Default::default()));
        if !current.is_object() {
            *current = Value::Object(Default::default());
        }
    }
    anyhow::bail!("config key path must not point to the root object")
}

pub(crate) fn load_config_from_items(items: Vec<(String, Value)>) -> Result<AgentConfig> {
    let (config, _) = load_config_from_items_with_weles_cleanup(items)?;
    Ok(config)
}

pub(in crate::agent) fn load_config_from_items_with_weles_cleanup(
    items: Vec<(String, Value)>,
) -> Result<(AgentConfig, Vec<SubAgentDefinition>)> {
    if items.is_empty() {
        return Ok((AgentConfig::default(), Vec::new()));
    }
    let mut root = Value::Object(Default::default());
    for (pointer, value) in items {
        set_config_value_at_pointer(&mut root, &pointer, value)?;
    }
    sanitize_config_value(&mut root);
    let mut config = serde_json::from_value(root).unwrap_or_default();
    let collisions = sanitize_weles_collisions_from_config(&mut config);
    Ok((config, collisions))
}
