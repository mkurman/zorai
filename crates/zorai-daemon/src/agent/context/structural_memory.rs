use std::path::{Component, Path, PathBuf};

use anyhow::Result;
use regex::Regex;
use serde::{Deserialize, Serialize};
use walkdir::WalkDir;

use crate::agent::semantic_env::scan_workspace_package_summaries_for_memory_graph;
use crate::agent::types::{AgentTask, TaskStatus};
use crate::agent::AgentEngine;

const SUPPORTED_TOOL_NAMES: &[&str] = &[
    zorai_protocol::tool_names::READ_FILE,
    zorai_protocol::tool_names::CREATE_FILE,
    zorai_protocol::tool_names::WRITE_FILE,
    zorai_protocol::tool_names::APPEND_TO_FILE,
    zorai_protocol::tool_names::REPLACE_IN_FILE,
    zorai_protocol::tool_names::APPLY_FILE_PATCH,
    zorai_protocol::tool_names::APPLY_PATCH,
];
const MANIFEST_FILE_NAMES: &[&str] = &[
    "Cargo.toml",
    "package.json",
    "tsconfig.json",
    "pyproject.toml",
    "requirements.txt",
];
const WALK_SKIP_DIRS: &[&str] = &[
    ".git",
    ".worktrees",
    "node_modules",
    "target",
    "dist",
    "release",
    "tmp",
];

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThreadStructuralMemory {
    #[serde(default, skip_serializing_if = "is_false")]
    pub workspace_seed_scan_complete: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub language_hints: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub workspace_seeds: Vec<WorkspaceSeed>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub observed_files: Vec<ObservedFileNode>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub edges: Vec<StructuralEdge>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceSeed {
    pub node_id: String,
    pub relative_path: String,
    pub kind: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObservedFileNode {
    pub node_id: String,
    pub relative_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StructuralEdge {
    pub from: String,
    pub to: String,
    pub kind: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StructuralGraphNeighbor {
    pub node_id: String,
    pub relation_kind: String,
    pub direction: String,
    pub summary: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StructuralContextEntry {
    pub node_id: String,
    pub summary: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MemoryGraphNodeUpsert {
    pub id: String,
    pub label: String,
    pub node_type: String,
    pub summary_text: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MemoryGraphEdgeUpsert {
    pub source_node_id: String,
    pub target_node_id: String,
    pub relation_type: String,
    pub weight: f64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct MemoryGraphUpdateBatch {
    pub nodes: Vec<MemoryGraphNodeUpsert>,
    pub edges: Vec<MemoryGraphEdgeUpsert>,
}

impl MemoryGraphUpdateBatch {
    fn push_node(&mut self, node: MemoryGraphNodeUpsert) {
        if !self.nodes.iter().any(|existing| existing.id == node.id) {
            self.nodes.push(node);
        }
    }

    fn push_edge(&mut self, edge: MemoryGraphEdgeUpsert) {
        if let Some(existing) = self.edges.iter_mut().find(|candidate| {
            candidate.source_node_id == edge.source_node_id
                && candidate.target_node_id == edge.target_node_id
                && candidate.relation_type == edge.relation_type
        }) {
            existing.weight += edge.weight;
            return;
        }
        self.edges.push(edge);
    }
}

impl ThreadStructuralMemory {
    pub fn is_empty(&self) -> bool {
        self.language_hints.is_empty()
            && self.workspace_seeds.is_empty()
            && self.observed_files.is_empty()
            && self.edges.is_empty()
    }

    pub fn has_structural_nodes(&self) -> bool {
        !self.workspace_seeds.is_empty()
            || !self.observed_files.is_empty()
            || !self.edges.is_empty()
    }

    pub fn concise_context_entries(
        &self,
        preferred_refs: &[String],
        limit: usize,
    ) -> Vec<StructuralContextEntry> {
        if limit == 0 || !self.has_structural_nodes() {
            return Vec::new();
        }

        let mut node_ids = Vec::new();
        for node_id in preferred_refs {
            self.push_context_node(&mut node_ids, node_id, limit);
        }
        for node in &self.observed_files {
            self.push_context_node(&mut node_ids, &node.node_id, limit);
        }
        for seed in &self.workspace_seeds {
            self.push_context_node(&mut node_ids, &seed.node_id, limit);
        }

        let seeded_ids = node_ids.clone();
        for node_id in seeded_ids {
            for edge in self
                .edges
                .iter()
                .filter(|edge| edge.from == node_id || edge.to == node_id)
            {
                self.push_context_node(&mut node_ids, &edge.from, limit);
                self.push_context_node(&mut node_ids, &edge.to, limit);
                if node_ids.len() >= limit {
                    break;
                }
            }
            if node_ids.len() >= limit {
                break;
            }
        }

        node_ids
            .into_iter()
            .map(|node_id| StructuralContextEntry {
                summary: self.describe_context_node(&node_id),
                node_id,
            })
            .collect()
    }

    pub fn graph_neighbors(&self, node_id: &str, limit: usize) -> Vec<StructuralGraphNeighbor> {
        if limit == 0 || !self.knows_node(node_id) {
            return Vec::new();
        }

        let mut neighbors = self
            .edges
            .iter()
            .filter_map(|edge| {
                if edge.from == node_id {
                    Some(StructuralGraphNeighbor {
                        node_id: edge.to.clone(),
                        relation_kind: edge.kind.clone(),
                        direction: "outgoing".to_string(),
                        summary: format!(
                            "{} -> {}",
                            edge.kind.replace('_', " "),
                            self.describe_context_node(&edge.to)
                        ),
                    })
                } else if edge.to == node_id {
                    Some(StructuralGraphNeighbor {
                        node_id: edge.from.clone(),
                        relation_kind: edge.kind.clone(),
                        direction: "incoming".to_string(),
                        summary: format!(
                            "{} <- {}",
                            edge.kind.replace('_', " "),
                            self.describe_context_node(&edge.from)
                        ),
                    })
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();

        neighbors.sort_by(|left, right| {
            left.relation_kind
                .cmp(&right.relation_kind)
                .then(left.direction.cmp(&right.direction))
                .then(left.node_id.cmp(&right.node_id))
        });
        neighbors.truncate(limit);
        neighbors
    }

    pub fn graph_lookup(
        &self,
        preferred_refs: &[String],
        limit: usize,
    ) -> Vec<StructuralGraphNeighbor> {
        if limit == 0 || !self.has_structural_nodes() {
            return Vec::new();
        }

        let anchor = preferred_refs
            .iter()
            .find(|node_id| self.knows_node(node_id))
            .cloned()
            .or_else(|| self.observed_files.first().map(|node| node.node_id.clone()))
            .or_else(|| {
                self.workspace_seeds
                    .first()
                    .map(|seed| seed.node_id.clone())
            });
        let Some(anchor) = anchor else {
            return Vec::new();
        };
        self.graph_neighbors(&anchor, limit)
    }

    pub fn merge(&mut self, other: ThreadStructuralMemory) {
        self.workspace_seed_scan_complete |= other.workspace_seed_scan_complete;
        for hint in other.language_hints {
            self.push_language_hint(hint);
        }
        for seed in other.workspace_seeds {
            self.push_workspace_seed(seed);
        }
        for node in other.observed_files {
            self.push_observed_file(node);
        }
        for edge in other.edges {
            self.push_edge(edge);
        }
    }

    fn push_language_hint(&mut self, hint: String) {
        if !self.language_hints.iter().any(|existing| existing == &hint) {
            self.language_hints.push(hint);
            self.language_hints.sort();
        }
    }

    fn push_workspace_seed(&mut self, seed: WorkspaceSeed) {
        if !self
            .workspace_seeds
            .iter()
            .any(|existing| existing == &seed)
        {
            self.workspace_seeds.push(seed);
            self.workspace_seeds.sort_by(|left, right| {
                left.node_id
                    .cmp(&right.node_id)
                    .then(left.kind.cmp(&right.kind))
            });
        }
    }

    fn push_observed_file(&mut self, node: ObservedFileNode) {
        if !self.observed_files.iter().any(|existing| existing == &node) {
            self.observed_files.push(node);
            self.observed_files
                .sort_by(|left, right| left.node_id.cmp(&right.node_id));
        }
    }

    fn push_edge(&mut self, edge: StructuralEdge) {
        if !self.edges.iter().any(|existing| existing == &edge) {
            self.edges.push(edge);
            self.edges.sort_by(|left, right| {
                left.from
                    .cmp(&right.from)
                    .then(left.kind.cmp(&right.kind))
                    .then(left.to.cmp(&right.to))
            });
        }
    }

    fn push_context_node(&self, node_ids: &mut Vec<String>, node_id: &str, limit: usize) {
        if node_ids.len() >= limit || !self.knows_node(node_id) {
            return;
        }
        if !node_ids.iter().any(|existing| existing == node_id) {
            node_ids.push(node_id.to_string());
        }
    }

    fn knows_node(&self, node_id: &str) -> bool {
        self.observed_files
            .iter()
            .any(|node| node.node_id == node_id)
            || self
                .workspace_seeds
                .iter()
                .any(|seed| seed.node_id == node_id)
            || self
                .edges
                .iter()
                .any(|edge| edge.from == node_id || edge.to == node_id)
    }

    fn describe_context_node(&self, node_id: &str) -> String {
        let mut parts = Vec::new();

        if let Some(node) = self
            .observed_files
            .iter()
            .find(|node| node.node_id == node_id)
        {
            parts.push(format!("observed file `{}`", node.relative_path));
        } else if let Some(seed) = self
            .workspace_seeds
            .iter()
            .find(|seed| seed.node_id == node_id)
        {
            parts.push(format!(
                "workspace seed `{}` ({})",
                seed.relative_path,
                seed.kind.replace('_', " ")
            ));
        } else if let Some(relative_path) = relative_path_from_node_id(node_id) {
            parts.push(format!("retained path `{relative_path}`"));
        }

        let outgoing = self
            .edges
            .iter()
            .filter(|edge| edge.from == node_id)
            .take(2)
            .map(|edge| format!("{} -> {}", edge.kind, edge.to))
            .collect::<Vec<_>>();
        if !outgoing.is_empty() {
            parts.push(format!("outgoing: {}", outgoing.join(", ")));
        }

        let incoming = self
            .edges
            .iter()
            .filter(|edge| edge.to == node_id)
            .take(2)
            .map(|edge| format!("{} <- {}", edge.kind, edge.from))
            .collect::<Vec<_>>();
        if !incoming.is_empty() {
            parts.push(format!("incoming: {}", incoming.join(", ")));
        }

        if parts.is_empty() {
            parts.push("structural node retained from thread memory".to_string());
        }

        parts.join("; ")
    }
}

fn is_false(value: &bool) -> bool {
    !*value
}

fn relative_path_from_node_id(node_id: &str) -> Option<&str> {
    node_id.strip_prefix("node:file:")
}

pub fn discover_workspace_seeds(repo_root: impl AsRef<Path>) -> Result<ThreadStructuralMemory> {
    let repo_root = repo_root.as_ref();
    let manifest_paths = collect_workspace_manifest_paths(repo_root);
    let mut memory = ThreadStructuralMemory::default();
    memory.workspace_seed_scan_complete = true;

    for manifest_path in &manifest_paths {
        merge_workspace_seed_for_path(&mut memory, repo_root, manifest_path);
    }

    memory.language_hints.clear();
    for hint in detect_language_hints(repo_root, &manifest_paths) {
        memory.push_language_hint(hint);
    }

    Ok(memory)
}

pub fn observe_successful_file_tool_result(
    memory: &mut ThreadStructuralMemory,
    repo_root: impl AsRef<Path>,
    tool_name: &str,
    tool_arguments: &str,
    tool_content: Option<&str>,
) -> Result<Vec<String>> {
    if !SUPPORTED_TOOL_NAMES
        .iter()
        .any(|candidate| *candidate == tool_name)
    {
        return Ok(Vec::new());
    }

    let repo_root = repo_root.as_ref();
    let candidate_paths: Vec<(PathBuf, String)> =
        extract_tool_file_paths(tool_name, tool_arguments)
            .into_iter()
            .filter_map(|raw_path| {
                let absolute_path = absolutize_tool_path(repo_root, &raw_path)?;
                let relative_path = normalized_relative_path(repo_root, &absolute_path)?;
                Some((absolute_path, relative_path))
            })
            .collect();

    if candidate_paths.is_empty() {
        return Ok(Vec::new());
    }

    if !memory.workspace_seed_scan_complete {
        memory.merge(discover_workspace_seeds(repo_root)?);
    }

    let mut structural_refs = Vec::new();
    for (absolute_path, relative_path) in candidate_paths {
        let node_id = node_id_for_relative_path(&relative_path);

        memory.push_observed_file(ObservedFileNode {
            node_id: node_id.clone(),
            relative_path: relative_path.clone(),
        });
        merge_workspace_seed_for_relative_path(memory, repo_root, &relative_path, &absolute_path);

        let file_content = if tool_name == zorai_protocol::tool_names::READ_FILE {
            tool_content.map(ToOwned::to_owned)
        } else {
            std::fs::read_to_string(&absolute_path)
                .ok()
                .or_else(|| tool_content.map(ToOwned::to_owned))
        };
        if let Some(file_content) = file_content.as_deref() {
            for imported_path in detect_imported_files(repo_root, &absolute_path, file_content) {
                let Some(imported_relative) = normalized_relative_path(repo_root, &imported_path)
                else {
                    continue;
                };
                memory.push_edge(StructuralEdge {
                    from: node_id.clone(),
                    to: node_id_for_relative_path(&imported_relative),
                    kind: "imported_file".to_string(),
                });
            }
        }

        if !structural_refs.iter().any(|existing| existing == &node_id) {
            structural_refs.push(node_id);
        }
    }

    Ok(structural_refs)
}

pub fn build_memory_graph_updates_for_file_tool(
    repo_root: impl AsRef<Path>,
    tool_name: &str,
    tool_arguments: &str,
    tool_content: Option<&str>,
) -> Result<MemoryGraphUpdateBatch> {
    if !SUPPORTED_TOOL_NAMES
        .iter()
        .any(|candidate| *candidate == tool_name)
    {
        return Ok(MemoryGraphUpdateBatch::default());
    }

    let repo_root = repo_root.as_ref();
    let mut batch = MemoryGraphUpdateBatch::default();
    let candidate_paths: Vec<(PathBuf, String)> =
        extract_tool_file_paths(tool_name, tool_arguments)
            .into_iter()
            .filter_map(|raw_path| {
                let absolute_path = absolutize_tool_path(repo_root, &raw_path)?;
                let relative_path = normalized_relative_path(repo_root, &absolute_path)?;
                Some((absolute_path, relative_path))
            })
            .collect();

    if candidate_paths.is_empty() {
        return Ok(batch);
    }

    let package_summaries =
        scan_workspace_package_summaries_for_memory_graph(repo_root).unwrap_or_default();
    for (absolute_path, relative_path) in candidate_paths {
        let file_node_id = node_id_for_relative_path(&relative_path);
        batch.push_node(MemoryGraphNodeUpsert {
            id: file_node_id.clone(),
            label: relative_path.clone(),
            node_type: "file".to_string(),
            summary_text: Some(format!("file observed via {tool_name}")),
        });

        let file_content = if tool_name == zorai_protocol::tool_names::READ_FILE {
            tool_content.map(ToOwned::to_owned)
        } else {
            std::fs::read_to_string(&absolute_path)
                .ok()
                .or_else(|| tool_content.map(ToOwned::to_owned))
        };
        if let Some(file_content) = file_content.as_deref() {
            for imported_path in detect_imported_files(repo_root, &absolute_path, file_content) {
                let Some(imported_relative) = normalized_relative_path(repo_root, &imported_path)
                else {
                    continue;
                };
                let imported_node_id = node_id_for_relative_path(&imported_relative);
                batch.push_node(MemoryGraphNodeUpsert {
                    id: imported_node_id.clone(),
                    label: imported_relative.clone(),
                    node_type: "file".to_string(),
                    summary_text: Some("file discovered through import relationship".to_string()),
                });
                batch.push_edge(MemoryGraphEdgeUpsert {
                    source_node_id: file_node_id.clone(),
                    target_node_id: imported_node_id,
                    relation_type: "imports_file".to_string(),
                    weight: 1.0,
                });
            }
        }

        for package in package_summaries.iter().filter(|package| {
            let manifest_path = Path::new(&package.manifest_path);
            let package_root = manifest_path.parent().unwrap_or(repo_root);
            absolute_path.starts_with(package_root)
        }) {
            let package_node_id = node_id_for_package(&package.ecosystem, &package.name);
            batch.push_node(MemoryGraphNodeUpsert {
                id: package_node_id.clone(),
                label: package.name.clone(),
                node_type: "package".to_string(),
                summary_text: Some(format!(
                    "{} package from {}",
                    package.ecosystem, package.manifest_path
                )),
            });
            batch.push_edge(MemoryGraphEdgeUpsert {
                source_node_id: file_node_id.clone(),
                target_node_id: package_node_id,
                relation_type: "file_in_package".to_string(),
                weight: 1.0,
            });
        }
    }

    Ok(batch)
}

pub fn build_memory_graph_updates_for_tool_failure(
    thread_id: &str,
    tool_name: &str,
    tool_arguments: &str,
    failure_description: &str,
) -> MemoryGraphUpdateBatch {
    let mut batch = MemoryGraphUpdateBatch::default();
    let error_label = failure_description
        .trim()
        .chars()
        .take(160)
        .collect::<String>();
    let error_node_id = node_id_for_error(tool_name, &error_label);
    batch.push_node(MemoryGraphNodeUpsert {
        id: error_node_id.clone(),
        label: if error_label.is_empty() {
            format!("{tool_name} failure")
        } else {
            error_label.clone()
        },
        node_type: "error".to_string(),
        summary_text: Some(format!("tool `{tool_name}` failed in thread `{thread_id}`")),
    });

    if let Ok(arguments) = crate::agent::tool_executor::parse_tool_args(tool_name, tool_arguments) {
        if let Some(path) = crate::agent::tool_executor::get_file_path_arg(&arguments)
            .or_else(|| crate::agent::tool_executor::get_string_arg(&arguments, &["filePath"]))
        {
            let file_node_id = if Path::new(path).is_absolute() {
                format!("node:file:{}", path)
            } else {
                node_id_for_relative_path(path)
            };
            batch.push_node(MemoryGraphNodeUpsert {
                id: file_node_id.clone(),
                label: path.to_string(),
                node_type: "file".to_string(),
                summary_text: Some("file referenced by failing tool invocation".to_string()),
            });
            batch.push_edge(MemoryGraphEdgeUpsert {
                source_node_id: file_node_id,
                target_node_id: error_node_id,
                relation_type: "file_hit_error".to_string(),
                weight: 1.0,
            });
        }
    }

    batch
}

pub fn build_memory_graph_updates_for_task(task: &AgentTask) -> MemoryGraphUpdateBatch {
    let mut batch = MemoryGraphUpdateBatch::default();
    let task_node_id = node_id_for_task(&task.id);
    batch.push_node(MemoryGraphNodeUpsert {
        id: task_node_id.clone(),
        label: task.title.clone(),
        node_type: "task".to_string(),
        summary_text: Some(format!("task status: {}", task_status_label(task.status))),
    });

    let haystack = format!("{} {}", task.title, task.description);
    for path_token in extract_file_like_tokens(&haystack) {
        let file_node_id = node_id_for_relative_path(&path_token);
        batch.push_node(MemoryGraphNodeUpsert {
            id: file_node_id.clone(),
            label: path_token.clone(),
            node_type: "file".to_string(),
            summary_text: Some("file inferred from task text".to_string()),
        });
        batch.push_edge(MemoryGraphEdgeUpsert {
            source_node_id: task_node_id.clone(),
            target_node_id: file_node_id,
            relation_type: "task_touches_file".to_string(),
            weight: 1.0,
        });
    }

    batch
}

pub fn build_memory_graph_updates_for_task_error(
    task: &AgentTask,
    failure_description: &str,
) -> MemoryGraphUpdateBatch {
    let mut batch = build_memory_graph_updates_for_task(task);
    let error_label = failure_description
        .trim()
        .chars()
        .take(160)
        .collect::<String>();
    if error_label.is_empty() {
        return batch;
    }
    let error_node_id = node_id_for_error(&task.id, &error_label);
    let task_node_id = node_id_for_task(&task.id);
    batch.push_node(MemoryGraphNodeUpsert {
        id: error_node_id.clone(),
        label: error_label,
        node_type: "error".to_string(),
        summary_text: Some(format!("error linked to task `{}`", task.title)),
    });
    batch.push_edge(MemoryGraphEdgeUpsert {
        source_node_id: task_node_id,
        target_node_id: error_node_id,
        relation_type: "task_hit_error".to_string(),
        weight: 1.0,
    });
    batch
}

impl AgentEngine {
    pub(crate) async fn apply_memory_graph_updates(&self, batch: MemoryGraphUpdateBatch) {
        if batch.nodes.is_empty() && batch.edges.is_empty() {
            return;
        }

        let now = crate::agent::now_millis();
        for node in batch.nodes {
            if let Err(error) = self
                .history
                .upsert_memory_node(
                    &node.id,
                    &node.label,
                    &node.node_type,
                    node.summary_text.as_deref(),
                    now,
                )
                .await
            {
                tracing::warn!(node_id = %node.id, %error, "failed to persist memory graph node");
            }
        }
        for edge in batch.edges {
            if let Err(error) = self
                .history
                .upsert_memory_edge(
                    &edge.source_node_id,
                    &edge.target_node_id,
                    &edge.relation_type,
                    edge.weight,
                    now,
                )
                .await
            {
                tracing::warn!(source = %edge.source_node_id, target = %edge.target_node_id, %error, "failed to persist memory graph edge");
            }
        }
    }

    pub(crate) async fn get_thread_structural_memory(
        &self,
        thread_id: &str,
    ) -> Option<ThreadStructuralMemory> {
        if let Some(state) = self
            .thread_structural_memories
            .read()
            .await
            .get(thread_id)
            .cloned()
        {
            return Some(state);
        }
        let loaded = match self
            .history
            .get_thread_structural_memory_state::<ThreadStructuralMemory>(thread_id)
            .await
        {
            Ok(state) => state?,
            Err(error) => {
                tracing::warn!(thread_id = %thread_id, %error, "failed to hydrate thread structural memory");
                return None;
            }
        };
        Some(
            self.thread_structural_memories
                .write()
                .await
                .entry(thread_id.to_string())
                .or_insert(loaded)
                .clone(),
        )
    }

    pub(crate) async fn clear_thread_structural_memory(&self, thread_id: &str) {
        self.thread_structural_memories
            .write()
            .await
            .remove(thread_id);
        if let Err(error) = self
            .history
            .delete_thread_structural_memory(thread_id)
            .await
        {
            tracing::warn!(thread_id = %thread_id, %error, "failed to delete thread structural memory");
        }
    }

    pub(crate) async fn enrich_thread_structural_memory_from_tool_result(
        &self,
        thread_id: &str,
        tool_name: &str,
        tool_arguments: &str,
        tool_content: Option<&str>,
    ) -> Vec<String> {
        if !SUPPORTED_TOOL_NAMES
            .iter()
            .any(|candidate| *candidate == tool_name)
        {
            return Vec::new();
        }

        let candidate_paths = extract_tool_file_paths(tool_name, tool_arguments);
        let mut repo_root = candidate_paths
            .iter()
            .find_map(|path| crate::git::find_git_root(&path.to_string_lossy()));
        if repo_root.is_none() {
            repo_root = self
                .resolve_thread_repo_root(thread_id)
                .await
                .map(|(root, _, _, _)| root);
        }
        if repo_root.is_none() {
            repo_root = self
                .workspace_root
                .as_ref()
                .map(|path| path.to_string_lossy().to_string());
        }

        let Some(repo_root) = repo_root else {
            return Vec::new();
        };
        let repo_root_path = PathBuf::from(repo_root);

        let graph_updates = match build_memory_graph_updates_for_file_tool(
            &repo_root_path,
            tool_name,
            tool_arguments,
            tool_content,
        ) {
            Ok(batch) => batch,
            Err(error) => {
                tracing::warn!(thread_id = %thread_id, tool_name = %tool_name, %error, "failed to build memory graph updates from tool result");
                MemoryGraphUpdateBatch::default()
            }
        };

        let mut state = self
            .get_thread_structural_memory(thread_id)
            .await
            .unwrap_or_default();
        let structural_refs = match observe_successful_file_tool_result(
            &mut state,
            &repo_root_path,
            tool_name,
            tool_arguments,
            tool_content,
        ) {
            Ok(structural_refs) => structural_refs,
            Err(error) => {
                tracing::warn!(thread_id = %thread_id, tool_name = %tool_name, %error, "failed to enrich thread structural memory");
                return Vec::new();
            }
        };

        let mut memories = self.thread_structural_memories.write().await;
        let became_empty = state.is_empty();
        if became_empty {
            memories.remove(thread_id);
        } else {
            memories.insert(thread_id.to_string(), state);
        }
        drop(memories);
        if became_empty {
            if let Err(error) = self
                .history
                .delete_thread_structural_memory(thread_id)
                .await
            {
                tracing::warn!(thread_id = %thread_id, %error, "failed to delete thread structural memory");
            }
        }
        self.apply_memory_graph_updates(graph_updates).await;
        if let Err(error) = self
            .refresh_memory_palace_from_thread(thread_id, None)
            .await
        {
            tracing::warn!(
                thread_id = %thread_id,
                tool_name = %tool_name,
                %error,
                "failed to refresh memory palace from tool result"
            );
        }
        structural_refs
    }

    pub(crate) async fn record_memory_graph_from_tool_failure(
        &self,
        thread_id: &str,
        tool_name: &str,
        tool_arguments: &str,
        failure_description: &str,
    ) {
        self.apply_memory_graph_updates(build_memory_graph_updates_for_tool_failure(
            thread_id,
            tool_name,
            tool_arguments,
            failure_description,
        ))
        .await;
        if let Err(error) = self
            .refresh_memory_palace_from_thread(thread_id, None)
            .await
        {
            tracing::warn!(
                thread_id = %thread_id,
                tool_name = %tool_name,
                %error,
                "failed to refresh memory palace from tool failure"
            );
        }
    }

    pub(crate) async fn record_memory_graph_from_task(&self, task: &AgentTask) {
        self.apply_memory_graph_updates(build_memory_graph_updates_for_task(task))
            .await;
        // Subagents carry a parent task; record the delegation edge so the shared
        // knowledge graph captures who delegated what across agents. Done after
        // the task's own node is upserted above so both edge endpoints exist.
        if let Some(parent_task_id) = task.parent_task_id.as_deref() {
            if let Err(error) = self
                .history
                .record_task_delegation_edge(
                    parent_task_id,
                    &task.id,
                    &task.title,
                    crate::agent::now_millis(),
                )
                .await
            {
                tracing::warn!(
                    parent_task_id = %parent_task_id,
                    child_task_id = %task.id,
                    %error,
                    "failed to record subagent delegation edge"
                );
            }
        }
        if let Some(error) = task.error.as_deref().or(task.last_error.as_deref()) {
            self.apply_memory_graph_updates(build_memory_graph_updates_for_task_error(task, error))
                .await;
        }
        if let Some(thread_id) = task.thread_id.as_deref() {
            if let Err(error) = self
                .refresh_memory_palace_from_thread(thread_id, Some(task.id.as_str()))
                .await
            {
                tracing::warn!(
                    thread_id = %thread_id,
                    task_id = %task.id,
                    %error,
                    "failed to refresh memory palace from task graph update"
                );
            }
        }
    }
}

fn collect_workspace_manifest_paths(repo_root: &Path) -> Vec<PathBuf> {
    let mut manifests = Vec::new();
    for entry in WalkDir::new(repo_root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|entry| should_walk_entry(entry.path()))
        .filter_map(std::result::Result::ok)
    {
        if !entry.file_type().is_file() {
            continue;
        }
        let file_name = entry.file_name().to_string_lossy();
        if MANIFEST_FILE_NAMES
            .iter()
            .any(|candidate| *candidate == file_name.as_ref())
        {
            manifests.push(entry.path().to_path_buf());
        }
    }
    manifests.sort();
    manifests
}

fn should_walk_entry(path: &Path) -> bool {
    !path.file_name().is_some_and(|name| {
        WALK_SKIP_DIRS
            .iter()
            .any(|candidate| name == std::ffi::OsStr::new(candidate))
    })
}

fn detect_language_hints(repo_root: &Path, manifest_paths: &[PathBuf]) -> Vec<String> {
    let has_cargo = manifest_paths
        .iter()
        .any(|path| path.file_name().is_some_and(|name| name == "Cargo.toml"));
    let has_tsconfig = manifest_paths
        .iter()
        .any(|path| path.file_name().is_some_and(|name| name == "tsconfig.json"));
    let has_package_json = manifest_paths
        .iter()
        .any(|path| path.file_name().is_some_and(|name| name == "package.json"));
    let has_python = manifest_paths.iter().any(|path| {
        path.file_name()
            .is_some_and(|name| name == "pyproject.toml" || name == "requirements.txt")
    });
    let has_shell = repo_root
        .join("scripts")
        .read_dir()
        .ok()
        .into_iter()
        .flatten()
        .filter_map(std::result::Result::ok)
        .any(|entry| {
            entry
                .path()
                .extension()
                .is_some_and(|extension| extension == "sh")
        });

    let mut hints = Vec::new();
    if has_cargo {
        hints.push("rust".to_string());
    }
    if has_tsconfig {
        hints.push("typescript".to_string());
    } else if has_package_json {
        hints.push("javascript".to_string());
    }
    if has_python {
        hints.push("python".to_string());
    }
    if has_shell {
        hints.push("shell".to_string());
    }
    hints.sort();
    hints
}

fn merge_workspace_seed_for_path(
    memory: &mut ThreadStructuralMemory,
    repo_root: &Path,
    absolute_path: &Path,
) {
    if let Some(relative_path) = normalized_relative_path(repo_root, absolute_path) {
        merge_workspace_seed_for_relative_path(memory, repo_root, &relative_path, absolute_path);
    }
}

fn merge_workspace_seed_for_relative_path(
    memory: &mut ThreadStructuralMemory,
    repo_root: &Path,
    relative_path: &str,
    absolute_path: &Path,
) {
    let file_name = Path::new(relative_path)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    match file_name {
        "Cargo.toml" => {
            memory.push_workspace_seed(WorkspaceSeed {
                node_id: node_id_for_relative_path(relative_path),
                relative_path: relative_path.to_string(),
                kind: "cargo_manifest".to_string(),
            });
            let crate_root =
                normalized_relative_path(repo_root, absolute_path.parent().unwrap_or(repo_root))
                    .unwrap_or_else(|| ".".to_string());
            memory.push_edge(StructuralEdge {
                from: node_id_for_relative_path(relative_path),
                to: node_id_for_relative_path(&crate_root),
                kind: "crate_path".to_string(),
            });
            memory.push_language_hint("rust".to_string());
        }
        "package.json" => {
            memory.push_workspace_seed(WorkspaceSeed {
                node_id: node_id_for_relative_path(relative_path),
                relative_path: relative_path.to_string(),
                kind: "manifest".to_string(),
            });
            let package_root =
                normalized_relative_path(repo_root, absolute_path.parent().unwrap_or(repo_root))
                    .unwrap_or_else(|| ".".to_string());
            memory.push_edge(StructuralEdge {
                from: node_id_for_relative_path(relative_path),
                to: node_id_for_relative_path(&package_root),
                kind: "package_root".to_string(),
            });
            memory.push_language_hint("javascript".to_string());
        }
        "pyproject.toml" | "requirements.txt" => {
            memory.push_workspace_seed(WorkspaceSeed {
                node_id: node_id_for_relative_path(relative_path),
                relative_path: relative_path.to_string(),
                kind: "manifest".to_string(),
            });
            let package_root =
                normalized_relative_path(repo_root, absolute_path.parent().unwrap_or(repo_root))
                    .unwrap_or_else(|| ".".to_string());
            memory.push_edge(StructuralEdge {
                from: node_id_for_relative_path(relative_path),
                to: node_id_for_relative_path(&package_root),
                kind: "package_root".to_string(),
            });
            memory.push_language_hint("python".to_string());
        }
        "tsconfig.json" => {
            memory.push_workspace_seed(WorkspaceSeed {
                node_id: node_id_for_relative_path(relative_path),
                relative_path: relative_path.to_string(),
                kind: "tsconfig".to_string(),
            });
            for source_root in derive_tsconfig_source_roots(repo_root, absolute_path) {
                if let Some(relative_source_root) =
                    normalized_relative_path(repo_root, &source_root)
                {
                    memory.push_edge(StructuralEdge {
                        from: node_id_for_relative_path(relative_path),
                        to: node_id_for_relative_path(&relative_source_root),
                        kind: "source_root".to_string(),
                    });
                }
            }
            memory.push_language_hint("typescript".to_string());
        }
        _ => {}
    }
}

fn derive_tsconfig_source_roots(repo_root: &Path, tsconfig_path: &Path) -> Vec<PathBuf> {
    let config_dir = tsconfig_path.parent().unwrap_or(repo_root);
    let mut roots = Vec::new();

    let root_dir = std::fs::read_to_string(tsconfig_path)
        .ok()
        .and_then(|content| serde_json::from_str::<serde_json::Value>(&content).ok())
        .and_then(|value| {
            value
                .get("compilerOptions")
                .and_then(|options| options.get("rootDir"))
                .and_then(|root_dir| root_dir.as_str())
                .map(|root_dir| config_dir.join(root_dir))
        });
    if let Some(root_dir) = root_dir {
        roots.push(root_dir);
    }

    let fallback_src = config_dir.join("src");
    if roots.is_empty() && fallback_src.exists() {
        roots.push(fallback_src);
    }
    if roots.is_empty() {
        roots.push(config_dir.to_path_buf());
    }

    roots.sort();
    roots.dedup();
    roots
}

fn extract_tool_file_paths(tool_name: &str, tool_arguments: &str) -> Vec<PathBuf> {
    if !SUPPORTED_TOOL_NAMES
        .iter()
        .any(|candidate| *candidate == tool_name)
    {
        return Vec::new();
    }

    let Ok(arguments) = crate::agent::tool_executor::parse_tool_args(tool_name, tool_arguments)
    else {
        return Vec::new();
    };

    let mut paths = Vec::new();
    let base_dir = match tool_name {
        zorai_protocol::tool_names::CREATE_FILE | zorai_protocol::tool_names::WRITE_FILE => {
            crate::agent::tool_executor::get_explicit_cwd_arg(&arguments).map(PathBuf::from)
        }
        _ => None,
    };

    if let Some(path) = crate::agent::tool_executor::get_file_path_arg(&arguments)
        .or_else(|| crate::agent::tool_executor::get_string_arg(&arguments, &["filePath"]))
    {
        paths.push(crate::agent::tool_executor::resolve_tool_path(
            path,
            base_dir.as_deref(),
        ));
    }

    if tool_name == zorai_protocol::tool_names::APPLY_PATCH {
        if let Some(patch_text) = crate::agent::tool_executor::get_apply_patch_text_arg(&arguments)
        {
            if let Ok(patch_paths) =
                crate::agent::tool_executor::extract_apply_patch_paths(patch_text)
            {
                paths.extend(patch_paths.into_iter().map(PathBuf::from));
            }
        }
    }

    paths.sort();
    paths.dedup();
    paths
}

fn absolutize_tool_path(repo_root: &Path, raw_path: &Path) -> Option<PathBuf> {
    if raw_path.as_os_str().is_empty() {
        return None;
    }

    let normalized_repo_root = normalize_lexical_path(repo_root);
    let candidate = if raw_path.is_absolute() {
        raw_path.to_path_buf()
    } else {
        normalized_repo_root.join(raw_path)
    };
    let normalized_candidate = normalize_lexical_path(&candidate);
    normalized_candidate
        .strip_prefix(&normalized_repo_root)
        .ok()?;

    Some(normalized_candidate)
}

fn normalize_lexical_path(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    let mut saw_root = false;

    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if !normalized.pop() && !saw_root {
                    normalized.push(component.as_os_str());
                }
            }
            Component::Normal(value) => normalized.push(value),
            Component::RootDir | Component::Prefix(_) => {
                normalized.push(component.as_os_str());
                saw_root = true;
            }
        }
    }

    normalized
}

fn normalized_relative_path(repo_root: &Path, absolute_path: &Path) -> Option<String> {
    let relative = if absolute_path == repo_root {
        PathBuf::new()
    } else {
        absolute_path.strip_prefix(repo_root).ok()?.to_path_buf()
    };
    Some(normalize_relative_path_components(&relative))
}

fn normalize_relative_path_components(path: &Path) -> String {
    let mut parts = Vec::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                parts.pop();
            }
            Component::Normal(value) => parts.push(value.to_string_lossy().to_string()),
            Component::RootDir | Component::Prefix(_) => {}
        }
    }

    if parts.is_empty() {
        ".".to_string()
    } else {
        parts.join("/")
    }
}

fn node_id_for_relative_path(relative_path: &str) -> String {
    format!("node:file:{relative_path}")
}

fn node_id_for_package(ecosystem: &str, package_name: &str) -> String {
    format!("node:package:{ecosystem}:{package_name}")
}

fn node_id_for_task(task_id: &str) -> String {
    format!("node:task:{task_id}")
}

fn node_id_for_error(scope: &str, label: &str) -> String {
    let mut normalized = String::new();
    for ch in label.chars() {
        if ch.is_ascii_alphanumeric() {
            normalized.push(ch.to_ascii_lowercase());
        } else if !normalized.ends_with('-') {
            normalized.push('-');
        }
        if normalized.len() >= 72 {
            break;
        }
    }
    let normalized = normalized.trim_matches('-');
    format!(
        "node:error:{scope}:{}",
        if normalized.is_empty() {
            "failure"
        } else {
            normalized
        }
    )
}

fn extract_file_like_tokens(text: &str) -> Vec<String> {
    let Ok(regex) =
        Regex::new(r"(?P<path>[A-Za-z0-9_./-]+\.(rs|toml|json|ts|tsx|js|jsx|py|md|sh))")
    else {
        return Vec::new();
    };
    let mut results = Vec::new();
    for capture in regex.captures_iter(text) {
        let Some(path) = capture.name("path") else {
            continue;
        };
        let normalized = path.as_str().trim_matches(|ch: char| {
            ch == '.' || ch == ',' || ch == ':' || ch == ';' || ch == ')' || ch == '('
        });
        if normalized.is_empty() {
            continue;
        }
        if !results.iter().any(|existing| existing == normalized) {
            results.push(normalized.to_string());
        }
    }
    results
}

fn task_status_label(status: TaskStatus) -> &'static str {
    match status {
        TaskStatus::Queued => "queued",
        TaskStatus::InProgress => "in_progress",
        TaskStatus::AwaitingApproval => "awaiting_approval",
        TaskStatus::Blocked => "blocked",
        TaskStatus::FailedAnalyzing => "failed_analyzing",
        TaskStatus::BudgetExceeded => "budget_exceeded",
        TaskStatus::Completed => "completed",
        TaskStatus::Failed => "failed",
        TaskStatus::Cancelled => "cancelled",
    }
}

fn detect_imported_files(repo_root: &Path, source_path: &Path, content: &str) -> Vec<PathBuf> {
    let mut imports = detect_relative_js_ts_imports(source_path, content);
    imports.extend(detect_rust_module_imports(source_path, content));
    imports.retain(|path| path.starts_with(repo_root));
    imports.sort();
    imports.dedup();
    imports
}

fn detect_relative_js_ts_imports(source_path: &Path, content: &str) -> Vec<PathBuf> {
    let stripped_content = strip_js_ts_comments(content);
    let import_re = Regex::new(
        r#"(?m)^\s*(?:import\s+[^\n]*?from\s+|import\s+|export\s+\*\s+from\s+|export\s*\{[^\n]*?\}\s*from\s+)[\"']([^\"']+)[\"']"#,
    )
    .expect("valid import regex");
    let require_re = Regex::new(
        r#"(?m)^\s*(?:(?:const|let|var)\s+[^=\n]+?\s*=\s*)?require\(\s*[\"']([^\"']+)[\"']\s*\)"#,
    )
    .expect("valid require regex");
    let mut imports = Vec::new();
    for captures in import_re.captures_iter(&stripped_content) {
        let Some(specifier) = captures.get(1).map(|entry| entry.as_str()) else {
            continue;
        };
        if !specifier.starts_with("./") && !specifier.starts_with("../") {
            continue;
        }
        if let Some(resolved) = resolve_module_specifier(source_path, specifier) {
            imports.push(resolved);
        }
    }
    for captures in require_re.captures_iter(&stripped_content) {
        let Some(specifier) = captures.get(1).map(|entry| entry.as_str()) else {
            continue;
        };
        if !specifier.starts_with("./") && !specifier.starts_with("../") {
            continue;
        }
        if let Some(resolved) = resolve_module_specifier(source_path, specifier) {
            imports.push(resolved);
        }
    }
    imports
}

fn resolve_module_specifier(source_path: &Path, specifier: &str) -> Option<PathBuf> {
    let base_dir = source_path.parent()?;
    let candidate = base_dir.join(specifier);
    if candidate.extension().is_some() {
        return candidate.exists().then_some(candidate);
    }

    for extension in ["ts", "tsx", "js", "jsx", "mjs", "cjs"] {
        let file_candidate = candidate.with_extension(extension);
        if file_candidate.exists() {
            return Some(file_candidate);
        }
        let index_candidate = candidate.join(format!("index.{extension}"));
        if index_candidate.exists() {
            return Some(index_candidate);
        }
    }

    None
}

fn strip_js_ts_comments(content: &str) -> String {
    #[derive(Clone, Copy, PartialEq, Eq)]
    enum State {
        Code,
        LineComment,
        BlockComment,
        SingleQuote,
        DoubleQuote,
        Template,
    }

    let mut stripped = String::with_capacity(content.len());
    let mut chars = content.chars().peekable();
    let mut state = State::Code;
    let mut escaped = false;

    while let Some(ch) = chars.next() {
        match state {
            State::Code => {
                if ch == '/' {
                    match chars.peek().copied() {
                        Some('/') => {
                            chars.next();
                            stripped.push(' ');
                            stripped.push(' ');
                            state = State::LineComment;
                            continue;
                        }
                        Some('*') => {
                            chars.next();
                            stripped.push(' ');
                            stripped.push(' ');
                            state = State::BlockComment;
                            continue;
                        }
                        _ => {}
                    }
                }

                match ch {
                    '\'' => {
                        stripped.push(ch);
                        state = State::SingleQuote;
                        escaped = false;
                    }
                    '"' => {
                        stripped.push(ch);
                        state = State::DoubleQuote;
                        escaped = false;
                    }
                    '`' => {
                        stripped.push(ch);
                        state = State::Template;
                        escaped = false;
                    }
                    _ => stripped.push(ch),
                }
            }
            State::LineComment => {
                if ch == '\n' {
                    stripped.push('\n');
                    state = State::Code;
                } else {
                    stripped.push(' ');
                }
            }
            State::BlockComment => {
                if ch == '*' && chars.peek().copied() == Some('/') {
                    chars.next();
                    stripped.push(' ');
                    stripped.push(' ');
                    state = State::Code;
                } else if ch == '\n' {
                    stripped.push('\n');
                } else {
                    stripped.push(' ');
                }
            }
            State::SingleQuote => {
                stripped.push(ch);
                if escaped {
                    escaped = false;
                } else if ch == '\\' {
                    escaped = true;
                } else if ch == '\'' {
                    state = State::Code;
                }
            }
            State::DoubleQuote => {
                stripped.push(ch);
                if escaped {
                    escaped = false;
                } else if ch == '\\' {
                    escaped = true;
                } else if ch == '"' {
                    state = State::Code;
                }
            }
            State::Template => {
                stripped.push(ch);
                if escaped {
                    escaped = false;
                } else if ch == '\\' {
                    escaped = true;
                } else if ch == '`' {
                    state = State::Code;
                }
            }
        }
    }

    stripped
}

fn detect_rust_module_imports(source_path: &Path, content: &str) -> Vec<PathBuf> {
    let parent = source_path.parent();
    let mut imports = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim();
        let Some(module_name) = trimmed
            .strip_prefix("mod ")
            .or_else(|| trimmed.strip_prefix("pub mod "))
        else {
            continue;
        };
        let module_name = module_name.trim_end_matches(';').trim();
        if module_name.is_empty() || module_name.contains(['{', ' ', ':']) {
            continue;
        }
        let Some(parent) = parent else {
            continue;
        };
        let file_candidate = parent.join(format!("{module_name}.rs"));
        if file_candidate.exists() {
            imports.push(file_candidate);
            continue;
        }
        let mod_candidate = parent.join(module_name).join("mod.rs");
        if mod_candidate.exists() {
            imports.push(mod_candidate);
        }
    }
    imports
}
