use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use grep_matcher::Matcher;
use grep_regex::RegexMatcherBuilder;
use grep_searcher::sinks::UTF8;
use grep_searcher::{BinaryDetection, SearcherBuilder};
use ignore::{WalkBuilder, WalkState};

const MAX_FILE_BYTES: u64 = 1024 * 1024;
const PREVIEW_CHARS: usize = 240;

#[derive(Debug, Clone, Default, serde::Deserialize)]
#[serde(default)]
pub(crate) struct CodeSearchOptions {
    pub case_sensitive: bool,
    pub whole_word: bool,
    pub regex: bool,
    pub include: Vec<String>,
    pub exclude: Vec<String>,
    pub use_ignore_files: Option<bool>,
    pub max_results: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub(crate) struct CodeSearchHit {
    pub path: String,
    pub line: u64,
    pub column: usize,
    pub preview: String,
}

fn overrides(root: &Path, options: &CodeSearchOptions) -> Result<ignore::overrides::Override> {
    let mut builder = ignore::overrides::OverrideBuilder::new(root);
    for glob in options
        .include
        .iter()
        .filter(|glob| !glob.trim().is_empty())
    {
        builder.add(glob.trim())?;
    }
    for glob in options
        .exclude
        .iter()
        .filter(|glob| !glob.trim().is_empty())
    {
        builder.add(&format!("!{}", glob.trim()))?;
    }
    for directory in [
        "node_modules",
        "target",
        "dist",
        "build",
        ".venv",
        "__pycache__",
        ".next",
        ".cache",
    ] {
        builder.add(&format!("!**/{directory}/**"))?;
    }
    Ok(builder.build()?)
}

pub(crate) fn search_workspace(
    root: &Path,
    query: &str,
    options: &CodeSearchOptions,
) -> Result<Vec<CodeSearchHit>> {
    if query.trim().is_empty() {
        return Ok(Vec::new());
    }
    let matcher = RegexMatcherBuilder::new()
        .case_insensitive(!options.case_sensitive)
        .word(options.whole_word)
        .fixed_strings(!options.regex)
        .build(query)
        .context("invalid search pattern")?;
    let use_ignore = options.use_ignore_files.unwrap_or(true);
    let walker = WalkBuilder::new(root)
        .standard_filters(use_ignore)
        .hidden(use_ignore)
        .overrides(overrides(root, options)?)
        .max_filesize(Some(MAX_FILE_BYTES))
        .filter_entry(|entry| entry.file_name() != ".git")
        .build_parallel();
    let max_results = options.max_results.unwrap_or(2000).clamp(1, 20_000);
    let hits = Arc::new(Mutex::new(Vec::new()));
    let found = Arc::new(AtomicUsize::new(0));
    let root: PathBuf = root.to_path_buf();
    walker.run(|| {
        let matcher = matcher.clone();
        let hits = Arc::clone(&hits);
        let found = Arc::clone(&found);
        let root = root.clone();
        let mut searcher = SearcherBuilder::new()
            .binary_detection(BinaryDetection::quit(b'\x00'))
            .line_number(true)
            .build();
        Box::new(move |entry| {
            if found.load(Ordering::Relaxed) >= max_results {
                return WalkState::Quit;
            }
            let Ok(entry) = entry else {
                return WalkState::Continue;
            };
            if !entry.file_type().is_some_and(|kind| kind.is_file()) {
                return WalkState::Continue;
            }
            let relative = entry
                .path()
                .strip_prefix(&root)
                .unwrap_or(entry.path())
                .to_string_lossy()
                .replace('\\', "/");
            let mut local = Vec::new();
            let _ = searcher.search_path(
                &matcher,
                entry.path(),
                UTF8(|line_number, line| {
                    let line = line.trim_end_matches(['\n', '\r']);
                    let mut at = 0;
                    while let Ok(Some(found_match)) = matcher.find_at(line.as_bytes(), at) {
                        if found_match.start() == found_match.end() {
                            break;
                        }
                        local.push(CodeSearchHit {
                            path: relative.clone(),
                            line: line_number,
                            column: line[..found_match.start()].encode_utf16().count() + 1,
                            preview: line.chars().take(PREVIEW_CHARS).collect(),
                        });
                        at = found_match.end();
                    }
                    Ok(found.load(Ordering::Relaxed) + local.len() < max_results)
                }),
            );
            if !local.is_empty() {
                found.fetch_add(local.len(), Ordering::Relaxed);
                hits.lock().expect("search hits lock").extend(local);
            }
            WalkState::Continue
        })
    });
    let mut hits = Arc::try_unwrap(hits)
        .map(|mutex| mutex.into_inner().unwrap_or_default())
        .unwrap_or_default();
    hits.sort_by(|left, right| {
        left.path
            .cmp(&right.path)
            .then(left.line.cmp(&right.line))
            .then(left.column.cmp(&right.column))
    });
    hits.truncate(max_results);
    Ok(hits)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn workspace() -> tempfile::TempDir {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(root.path().join("src")).unwrap();
        std::fs::create_dir_all(root.path().join("node_modules/pkg")).unwrap();
        std::fs::write(
            root.path().join("src/lib.rs"),
            "fn needle() {}\nlet x = Needle;\n// ź needle\n",
        )
        .unwrap();
        std::fs::write(root.path().join("node_modules/pkg/index.js"), "needle").unwrap();
        std::fs::write(root.path().join("src/data.bin"), b"needle\x00\x01").unwrap();
        root
    }

    #[test]
    fn finds_every_match_with_editor_columns_and_skips_dependencies_and_binaries() {
        let root = workspace();
        let hits = search_workspace(root.path(), "needle", &CodeSearchOptions::default()).unwrap();
        assert_eq!(
            hits.iter()
                .map(|hit| (hit.path.as_str(), hit.line, hit.column))
                .collect::<Vec<_>>(),
            vec![
                ("src/lib.rs", 1, 4),
                ("src/lib.rs", 2, 9),
                ("src/lib.rs", 3, 6)
            ]
        );
    }

    #[test]
    fn honours_case_whole_word_regex_and_include_globs() {
        let root = workspace();
        let case = CodeSearchOptions {
            case_sensitive: true,
            ..Default::default()
        };
        assert_eq!(
            search_workspace(root.path(), "Needle", &case)
                .unwrap()
                .len(),
            1
        );
        let regex = CodeSearchOptions {
            regex: true,
            ..Default::default()
        };
        assert_eq!(
            search_workspace(root.path(), "fn \\w+\\(", &regex)
                .unwrap()
                .len(),
            1
        );
        let include = CodeSearchOptions {
            include: vec!["*.js".into()],
            use_ignore_files: Some(false),
            ..Default::default()
        };
        assert!(search_workspace(root.path(), "needle", &include)
            .unwrap()
            .is_empty());
        assert!(search_workspace(root.path(), "(", &regex).is_err());
    }
}

#[cfg(test)]
mod bench {
    #[test]
    #[ignore]
    fn search_latency_on_this_repository() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        for query in ["fn search_workspace", "useState", "ZORAI_NONEXISTENT_TOKEN"] {
            let started = std::time::Instant::now();
            let hits = super::search_workspace(&root, query, &super::CodeSearchOptions { max_results: Some(2000), ..Default::default() }).unwrap();
            println!("{query}: {} hits in {} ms", hits.len(), started.elapsed().as_millis());
        }
    }
}
