use std::path::Path;

const RULES_MAX_TOTAL_CHARS: usize = 16_000;
const RULE_MAX_FILES: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProjectRule {
    pub name: String,
    pub description: String,
    pub globs: Vec<String>,
    pub always_apply: bool,
    pub body: String,
}

fn parse_list(value: &str) -> Vec<String> {
    value
        .trim()
        .trim_start_matches('[')
        .trim_end_matches(']')
        .split(',')
        .map(|item| item.trim().trim_matches(['"', '\'']).to_string())
        .filter(|item| !item.is_empty())
        .collect()
}

pub(crate) fn parse_rule(name: &str, text: &str) -> ProjectRule {
    let mut rule = ProjectRule {
        name: name.to_string(),
        description: String::new(),
        globs: Vec::new(),
        always_apply: false,
        body: text.trim().to_string(),
    };
    let normalized = text.replace("\r\n", "\n");
    let Some(rest) = normalized.strip_prefix("---\n") else {
        return rule;
    };
    let Some(end) = rest.find("\n---") else {
        return rule;
    };
    let (front, body) = rest.split_at(end);
    rule.body = body.trim_start_matches("\n---").trim().to_string();
    let mut list_key: Option<&str> = None;
    for line in front.lines() {
        let trimmed = line.trim();
        if let Some(item) = trimmed.strip_prefix("- ") {
            if list_key == Some("globs") {
                rule.globs.extend(parse_list(item));
            }
            continue;
        }
        let Some((key, value)) = trimmed.split_once(':') else {
            continue;
        };
        list_key = None;
        match key.trim() {
            "description" => rule.description = value.trim().trim_matches(['"', '\'']).to_string(),
            "globs" if value.trim().is_empty() => list_key = Some("globs"),
            "globs" => rule.globs = parse_list(value),
            "alwaysApply" | "always_apply" => rule.always_apply = value.trim() == "true",
            _ => {}
        }
    }
    rule
}

fn glob_regex(glob: &str) -> Option<regex::Regex> {
    let mut pattern = String::from("^");
    let mut chars = glob.trim().trim_start_matches("./").chars().peekable();
    while let Some(char) = chars.next() {
        match char {
            '*' if chars.peek() == Some(&'*') => {
                chars.next();
                if chars.peek() == Some(&'/') {
                    chars.next();
                    pattern.push_str("(?:.*/)?");
                } else {
                    pattern.push_str(".*");
                }
            }
            '*' => pattern.push_str("[^/]*"),
            '?' => pattern.push_str("[^/]"),
            other => pattern.push_str(&regex::escape(&other.to_string())),
        }
    }
    pattern.push('$');
    regex::Regex::new(&pattern).ok()
}

pub(crate) fn glob_matches(glob: &str, path: &str) -> bool {
    let path = path.trim_start_matches("./");
    let Some(regex) = glob_regex(glob) else {
        return false;
    };
    if regex.is_match(path) {
        return true;
    }
    !glob.contains('/')
        && path
            .rsplit('/')
            .next()
            .is_some_and(|name| regex.is_match(name))
}

pub(crate) fn rule_applies(rule: &ProjectRule, targets: &[String]) -> bool {
    rule.always_apply
        || (rule.globs.is_empty() && rule.description.is_empty())
        || rule
            .globs
            .iter()
            .any(|glob| targets.iter().any(|target| glob_matches(glob, target)))
}

fn read_rules_dir(dir: &Path, extension: &str, label: &str, rules: &mut Vec<ProjectRule>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut paths: Vec<_> = entries
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some(extension))
        .collect();
    paths.sort();
    for path in paths.into_iter().take(RULE_MAX_FILES) {
        if let Ok(text) = std::fs::read_to_string(&path) {
            let name = format!(
                "{label}/{}",
                path.file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("rule")
            );
            rules.push(parse_rule(&name, &text));
        }
    }
}

pub(crate) fn discover_rules(root: &Path) -> Vec<ProjectRule> {
    let mut rules = Vec::new();
    read_rules_dir(&root.join(".zorai/rules"), "md", ".zorai/rules", &mut rules);
    read_rules_dir(
        &root.join(".cursor/rules"),
        "mdc",
        ".cursor/rules",
        &mut rules,
    );
    if let Ok(text) = std::fs::read_to_string(root.join(".cursorrules")) {
        let mut rule = parse_rule(".cursorrules", &text);
        rule.always_apply = true;
        rules.push(rule);
    }
    rules
}

pub(crate) fn rule_summaries(root: &str, targets: &[String]) -> Vec<serde_json::Value> {
    let root = root.trim();
    if root.is_empty() {
        return Vec::new();
    }
    discover_rules(Path::new(root))
        .into_iter()
        .filter(|rule| !rule.body.is_empty())
        .map(|rule| {
            let applied = rule_applies(&rule, targets);
            serde_json::json!({
                "name": rule.name,
                "description": rule.description,
                "globs": rule.globs,
                "applied": applied,
            })
        })
        .collect()
}

pub(crate) fn rules_prompt_block(root: &str, targets: &[String]) -> Option<String> {
    let root = root.trim();
    if root.is_empty() {
        return None;
    }
    let (active, optional): (Vec<ProjectRule>, Vec<ProjectRule>) = discover_rules(Path::new(root))
        .into_iter()
        .filter(|rule| !rule.body.is_empty())
        .partition(|rule| rule_applies(rule, targets));
    let optional: Vec<ProjectRule> = optional
        .into_iter()
        .filter(|rule| rule.globs.is_empty() && !rule.description.is_empty())
        .collect();
    if active.is_empty() && optional.is_empty() {
        return None;
    }
    let mut block = String::from(
        "## Project Rules\nThe project defines these rules for the files in play. Follow them.\n",
    );
    for rule in active {
        let header = if rule.globs.is_empty() {
            format!("\n### {}\n", rule.name)
        } else {
            format!(
                "\n### {} (applies to {})\n",
                rule.name,
                rule.globs.join(", ")
            )
        };
        if block.len() + header.len() + rule.body.len() > RULES_MAX_TOTAL_CHARS {
            block.push_str("\n(More rules matched but were left out to save context.)\n");
            break;
        }
        block.push_str(&header);
        block.push_str(&rule.body);
        block.push('\n');
    }
    if !optional.is_empty() {
        block.push_str(
            "\nOther project rules; read one with `read_file` when its topic comes up:\n",
        );
        for rule in optional {
            block.push_str(&format!("- `{}`: {}\n", rule.name, rule.description));
        }
    }
    Some(block)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frontmatter_globs_scope_a_rule_to_matching_files_only() {
        let rule = parse_rule(
            "rust.md",
            "---\ndescription: Rust style\nglobs: [\"*.rs\", \"crates/**/Cargo.toml\"]\nalwaysApply: false\n---\nUse anyhow for errors.",
        );
        assert_eq!(rule.globs, vec!["*.rs", "crates/**/Cargo.toml"]);
        assert_eq!(rule.body, "Use anyhow for errors.");
        assert!(rule_applies(
            &rule,
            &["crates/zorai/src/main.rs".to_string()]
        ));
        assert!(rule_applies(&rule, &["crates/a/b/Cargo.toml".to_string()]));
        assert!(!rule_applies(&rule, &["frontend/src/app.tsx".to_string()]));
    }

    #[test]
    fn yaml_list_globs_and_always_apply_are_understood() {
        let rule = parse_rule(
            "ui.md",
            "---\nglobs:\n  - frontend/**/*.tsx\nalwaysApply: false\n---\nNo inline styles.",
        );
        assert!(rule_applies(
            &rule,
            &["frontend/src/zorai/App.tsx".to_string()]
        ));
        assert!(!rule_applies(
            &rule,
            &["frontend/src/zorai/App.ts".to_string()]
        ));
        let always = parse_rule("all.md", "---\nalwaysApply: true\n---\nBe concise.");
        assert!(rule_applies(&always, &[]));
    }

    #[test]
    fn rules_without_frontmatter_apply_everywhere() {
        let rule = parse_rule("plain.md", "Write tests first.");
        assert!(rule_applies(&rule, &[]));
    }

    #[test]
    fn only_matching_rules_reach_the_prompt() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(root.path().join(".zorai/rules")).unwrap();
        std::fs::write(
            root.path().join(".zorai/rules/rust.md"),
            "---\nglobs: [\"*.rs\"]\n---\nRUST RULE",
        )
        .unwrap();
        std::fs::write(
            root.path().join(".zorai/rules/ts.md"),
            "---\nglobs: [\"*.ts\"]\n---\nTS RULE",
        )
        .unwrap();
        let root_str = root.path().to_string_lossy().to_string();
        let block = rules_prompt_block(&root_str, &["src/main.rs".to_string()]).unwrap();
        assert!(block.contains("RUST RULE"));
        assert!(!block.contains("TS RULE"));
        assert!(rules_prompt_block(&root_str, &["README.md".to_string()]).is_none());
        std::fs::write(
            root.path().join(".zorai/rules/db.md"),
            "---\ndescription: Database migrations\n---\nDB RULE",
        )
        .unwrap();
        let block = rules_prompt_block(&root_str, &["README.md".to_string()]).unwrap();
        assert!(block.contains("`.zorai/rules/db.md`: Database migrations"));
        assert!(!block.contains("DB RULE"));
    }
}
