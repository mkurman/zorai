use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::SystemTime;

use anyhow::Result;
use ignore::WalkBuilder;

use super::ast_graph::{file_graph, top_level_chunks};
use crate::agent::AgentEngine;

const MAX_FILE_BYTES: u64 = 200 * 1024;
const MAX_CHUNK_CHARS: usize = 6000;
const MAX_FILES: usize = 20_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CodeChunk {
    pub key: String,
    pub title: String,
    pub body: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub(crate) struct CodeIndexEstimate {
    pub files: usize,
    pub chunks: usize,
    pub characters: usize,
    pub approx_tokens: usize,
}

#[derive(Debug, Clone, serde::Serialize)]
pub(crate) struct SemanticCodeHit {
    pub path: String,
    pub line: usize,
    pub title: String,
    pub score: f64,
    pub snippet: String,
}

type FileCache = HashMap<PathBuf, (SystemTime, u64, Vec<CodeChunk>)>;
static CACHE: Mutex<Option<HashMap<PathBuf, FileCache>>> = Mutex::new(None);

pub(crate) fn chunks_for_file(relative: &str, source: &str) -> Vec<CodeChunk> {
    let lines: Vec<&str> = source.lines().collect();
    let take = |text: String| text.chars().take(MAX_CHUNK_CHARS).collect::<String>();
    let Some(graph) = file_graph(relative, source) else {
        return Vec::new();
    };
    let mut seen: HashMap<String, usize> = HashMap::new();
    let mut chunks: Vec<CodeChunk> = top_level_chunks(&graph)
        .into_iter()
        .filter(|symbol| symbol.end_line > symbol.line || graph.language == "css")
        .map(|symbol| {
            let qualified = match &symbol.container {
                Some(container) => format!("{container}::{}", symbol.name),
                None => symbol.name.clone(),
            };
            let count = seen.entry(qualified.clone()).or_insert(0);
            *count += 1;
            let key = if *count == 1 {
                format!("{relative}#{qualified}")
            } else {
                format!("{relative}#{qualified}~{count}")
            };
            let body =
                lines[symbol.line.saturating_sub(1)..symbol.end_line.min(lines.len())].join("\n");
            CodeChunk {
                key,
                title: format!("{relative}:{} {} {qualified}", symbol.line, symbol.kind),
                body: take(format!("// {relative}\n{}", body)),
            }
        })
        .collect();
    if chunks.is_empty() && !source.trim().is_empty() {
        chunks.push(CodeChunk {
            key: relative.to_string(),
            title: format!("{relative}:1 file"),
            body: take(format!("// {relative}\n{source}")),
        });
    }
    chunks
}

pub(crate) fn collect_code_chunks(root: &Path) -> Vec<CodeChunk> {
    let mut guard = CACHE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let cache = guard
        .get_or_insert_with(HashMap::new)
        .entry(root.to_path_buf())
        .or_default();
    let mut next: FileCache = HashMap::new();
    let mut chunks = Vec::new();
    let walker = WalkBuilder::new(root)
        .max_filesize(Some(MAX_FILE_BYTES))
        .filter_entry(|entry| entry.file_name() != ".git")
        .build();
    for entry in walker
        .flatten()
        .filter(|entry| entry.file_type().is_some_and(|kind| kind.is_file()))
        .take(MAX_FILES)
    {
        let path = entry.path().to_path_buf();
        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        let modified = metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH);
        let file_chunks = match cache.remove(&path) {
            Some((cached_modified, cached_len, cached))
                if cached_modified == modified && cached_len == metadata.len() =>
            {
                cached
            }
            _ => {
                let relative = path
                    .strip_prefix(root)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .replace('\\', "/");
                std::fs::read_to_string(&path)
                    .map(|source| chunks_for_file(&relative, &source))
                    .unwrap_or_default()
            }
        };
        chunks.extend(file_chunks.iter().cloned());
        next.insert(path, (modified, metadata.len(), file_chunks));
    }
    *cache = next;
    chunks
}

pub(crate) fn estimate(chunks: &[CodeChunk]) -> CodeIndexEstimate {
    let characters: usize = chunks.iter().map(|chunk| chunk.body.chars().count()).sum();
    let files = chunks
        .iter()
        .map(|chunk| chunk.key.split('#').next().unwrap_or(""))
        .collect::<std::collections::HashSet<_>>()
        .len();
    CodeIndexEstimate {
        files,
        chunks: chunks.len(),
        characters,
        approx_tokens: characters / 4,
    }
}

fn line_from_title(title: &str) -> usize {
    title
        .split_whitespace()
        .next()
        .and_then(|location| location.rsplit(':').next())
        .and_then(|line| line.parse().ok())
        .unwrap_or(1)
}

fn path_from_key(key: &str) -> String {
    key.split('#').next().unwrap_or(key).to_string()
}

pub(crate) async fn execute_search_codebase(args: &serde_json::Value, agent: &AgentEngine) -> Result<String> {
    let query = args.get("query").and_then(|value| value.as_str()).unwrap_or("").trim().to_string();
    if query.is_empty() {
        anyhow::bail!("query is required: describe the code you are looking for");
    }
    let requested = args.get("path").and_then(|value| value.as_str()).map(PathBuf::from).unwrap_or(std::env::current_dir()?);
    let requested = std::fs::canonicalize(&requested).unwrap_or(requested);
    let limit = args.get("limit").and_then(|value| value.as_u64()).unwrap_or(8).clamp(1, 30) as usize;
    let config = agent.get_config().await;
    let root = config
        .code_index
        .roots
        .iter()
        .find(|root| std::fs::canonicalize(root).map(|canonical| requested.starts_with(canonical)).unwrap_or(false))
        .cloned();
    let Some(root) = root else {
        return Ok("The semantic code index is off for this workspace (the user turns it on with the ≈ toggle in the Code view's Search panel). Use search_files or lsp_query instead.".to_string());
    };
    let hits = agent.semantic_code_search(&root, &query, limit).await?;
    if hits.is_empty() {
        return Ok(format!("No indexed code matched '{query}'."));
    }
    Ok(hits
        .iter()
        .map(|hit| format!("## {} (score {:.2})\n{}", hit.title, hit.score, hit.snippet))
        .collect::<Vec<_>>()
        .join("\n\n"))
}

impl AgentEngine {
    pub(crate) async fn sync_code_indexes(&self) -> Result<usize> {
        let config = self.get_config().await;
        let model = config.semantic.embedding.model.trim().to_string();
        if !config.semantic.embedding.enabled || model.is_empty() {
            return Ok(0);
        }
        let mut changed = 0;
        for root in config
            .code_index
            .roots
            .iter()
            .map(PathBuf::from)
            .filter(|root| root.is_dir())
        {
            let walk_root = root.clone();
            let chunks =
                tokio::task::spawn_blocking(move || collect_code_chunks(&walk_root)).await?;
            let summary = self
                .history
                .sync_code_chunks(
                    &root,
                    chunks
                        .into_iter()
                        .map(|chunk| (chunk.key, chunk.title, chunk.body))
                        .collect(),
                    &model,
                    config.semantic.embedding.dimensions,
                )
                .await?;
            changed += summary.changed + summary.removed;
        }
        Ok(changed)
    }

    pub(crate) async fn code_index_status(&self, root: &str) -> serde_json::Value {
        let config = self.get_config().await;
        let walk_root = PathBuf::from(root);
        let estimate =
            tokio::task::spawn_blocking(move || estimate(&collect_code_chunks(&walk_root)))
                .await
                .unwrap_or_default();
        serde_json::json!({
            "enabled": config.code_index.roots.iter().any(|entry| entry == root),
            "embeddings_configured": config.semantic.embedding.enabled && !config.semantic.embedding.model.trim().is_empty(),
            "embedding_model": config.semantic.embedding.model,
            "estimate": estimate,
        })
    }

    pub(crate) async fn semantic_code_search(
        &self,
        root: &str,
        query: &str,
        limit: usize,
    ) -> Result<Vec<SemanticCodeHit>> {
        let config = self.get_config().await;
        if !config.code_index.roots.iter().any(|entry| entry == root) {
            anyhow::bail!(
                "Semantic search is off for this workspace. Turn it on in the Search panel first."
            );
        }
        let scores = self
            .semantic_document_scores(query, "code", limit.saturating_mul(4).max(limit), &config)
            .await?;
        let canonical = std::fs::canonicalize(root)
            .unwrap_or_else(|_| PathBuf::from(root))
            .to_string_lossy()
            .replace('\\', "/");
        let prefix = format!("{canonical}::");
        let mut ranked: Vec<(String, f64)> = scores
            .into_iter()
            .filter(|(source_id, _)| source_id.starts_with(&prefix))
            .collect();
        ranked.sort_by(|left, right| {
            right
                .1
                .partial_cmp(&left.1)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        ranked.truncate(limit);
        let ids: Vec<String> = ranked.iter().map(|(id, _)| id.clone()).collect();
        let documents = self.history.code_chunks_by_source_id(&ids).await?;
        Ok(ranked
            .into_iter()
            .filter_map(|(id, score)| {
                let (_, key, title, body) =
                    documents.iter().find(|document| document.0 == id)?.clone();
                Some(SemanticCodeHit {
                    path: path_from_key(&key),
                    line: line_from_title(&title),
                    title,
                    score,
                    snippet: body.chars().take(1200).collect(),
                })
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunks_follow_functions_so_editing_one_function_only_changes_its_chunk() {
        let before = "fn a() {\n    1;\n}\n\nfn b() {\n    2;\n}\n";
        let after = "// note\nfn a() {\n    1;\n}\n\nfn b() {\n    3;\n}\n";
        let first = chunks_for_file("src/lib.rs", before);
        let second = chunks_for_file("src/lib.rs", after);
        assert_eq!(
            first
                .iter()
                .map(|chunk| chunk.key.as_str())
                .collect::<Vec<_>>(),
            vec!["src/lib.rs#a", "src/lib.rs#b"]
        );
        assert_eq!(first[0].body, second[0].body);
        assert_ne!(first[1].body, second[1].body);
        assert_eq!(line_from_title(&second[0].title), 2);
    }

    #[test]
    fn methods_get_their_own_chunks_and_symbol_less_files_become_one_chunk() {
        let chunks = chunks_for_file(
            "src/store.ts",
            "export class Store {\n  load() {\n    return 1;\n  }\n}\n",
        );
        assert_eq!(
            chunks
                .iter()
                .map(|chunk| chunk.key.as_str())
                .collect::<Vec<_>>(),
            vec!["src/store.ts#Store", "src/store.ts#Store::load"]
        );
        assert_eq!(chunks_for_file("config.json", "{\n  \"a\": 1\n}").len(), 1);
        assert_eq!(path_from_key("src/store.ts#Store::load"), "src/store.ts");
    }

    #[test]
    fn collection_skips_ignored_files_and_estimates_the_embedding_size() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join(".gitignore"), "gen/\n").unwrap();
        std::fs::create_dir_all(root.path().join("gen")).unwrap();
        std::fs::write(root.path().join("gen/out.rs"), "fn generated() {\n}\n").unwrap();
        std::fs::write(root.path().join("main.rs"), "fn main() {\n    run();\n}\n").unwrap();
        std::process::Command::new("git")
            .arg("init")
            .arg("-q")
            .current_dir(root.path())
            .status()
            .ok();
        let chunks = collect_code_chunks(root.path());
        assert_eq!(
            chunks
                .iter()
                .map(|chunk| chunk.key.as_str())
                .collect::<Vec<_>>(),
            vec!["main.rs#main"]
        );
        let summary = estimate(&chunks);
        assert_eq!((summary.files, summary.chunks), (1, 1));
        assert_eq!(summary.approx_tokens, summary.characters / 4);
    }
}
