use super::*;
use base64::Engine;

const SKILL_LIST_CURSOR_PREFIX: &str = "skill-list:";

impl HistoryStore {
    pub async fn list_skill_variants_page(
        &self,
        query: Option<&str>,
        cursor: Option<&str>,
        limit: usize,
    ) -> Result<SkillVariantPage> {
        let variants = self.load_skill_variants(query).await?;
        page_skill_variants(variants, cursor, limit)
    }

    pub async fn inspect_skill_variants(
        &self,
        skill: &str,
        context_tags: &[String],
    ) -> Result<Vec<SkillVariantInspection>> {
        let normalized = normalize_skill_lookup(skill);
        if normalized.is_empty() {
            return Ok(Vec::new());
        }
        let context_tags = context_tags.to_vec();
        let rows = self
            .conn_db
            .query(
                "SELECT variant_id, skill_name, variant_name, relative_path, parent_variant_id, version, context_tags_json, use_count, success_count, failure_count, fitness_score, status, last_used_at, created_at, updated_at \
                 FROM skill_variants",
                db::Params::None,
            )
            .await?;
        let mut variants = rows
            .iter()
            .filter_map(|row| map_skill_variant_row_db(row).ok())
            .filter(|record| skill_variant_matches(record, &normalized))
            .collect::<Vec<_>>();
        let trend_by_variant =
            load_skill_variant_trends_db(&mut db::ConnExecutor(&*self.conn_db), &variants, 8)
                .await?;
        variants.sort_by(|left, right| {
            compare_skill_variants(left, right, &context_tags, &trend_by_variant)
        });
        let selected_id = variants.first().map(|record| record.variant_id.clone());
        let now = now_ts();
        let mut inspections = Vec::with_capacity(variants.len());
        for record in variants {
            let history_rows = self
                .conn_db
                .query(
                    "SELECT id, variant_id, recorded_at, outcome, fitness_score \
                     FROM skill_variant_history WHERE variant_id = ?1 \
                     ORDER BY recorded_at ASC, rowid ASC",
                    db::db_params![record.variant_id.as_str()],
                )
                .await?;
            let fitness_history = history_rows
                .iter()
                .map(|row| {
                    Ok(SkillVariantFitnessHistoryRow {
                        id: row.get(0)?,
                        variant_id: row.get(1)?,
                        recorded_at: row.get(2)?,
                        outcome: row.get(3)?,
                        fitness_score: row.get(4)?,
                    })
                })
                .collect::<anyhow::Result<Vec<_>>>()?;
            inspections.push(SkillVariantInspection {
                lifecycle_summary: describe_skill_variant_lifecycle(&record, now),
                selection_summary: describe_skill_variant_selection(&record, &context_tags),
                selected_for_context: selected_id.as_deref() == Some(record.variant_id.as_str()),
                fitness_score: record.fitness_score,
                fitness_snapshot: SkillVariantFitnessSnapshot {
                    recorded_at: record.updated_at,
                    fitness_score: record.fitness_score,
                    use_count: record.use_count,
                    success_rate: record.success_rate(),
                },
                fitness_history,
                record,
            });
        }
        Ok(inspections)
    }

    pub async fn register_skill_document(&self, path: &Path) -> Result<SkillVariantRecord> {
        let skills_root = self.skills_root();
        let canonical = std::fs::canonicalize(path)
            .with_context(|| format!("failed to access skill document {}", path.display()))?;
        if !canonical.starts_with(&skills_root) {
            anyhow::bail!(
                "skill document {} must stay inside {}",
                canonical.display(),
                skills_root.display()
            );
        }

        let relative_path = canonical
            .strip_prefix(&skills_root)
            .unwrap_or(canonical.as_path())
            .to_string_lossy()
            .replace('\\', "/");

        let existing: Option<SkillVariantRecord> = self
            .conn_db
            .query_opt(
                "SELECT variant_id, skill_name, variant_name, relative_path, parent_variant_id, version, context_tags_json, use_count, success_count, failure_count, fitness_score, status, last_used_at, created_at, updated_at \
                 FROM skill_variants WHERE relative_path = ?1",
                db::db_params![relative_path.clone()],
            )
            .await?
            .map(|row| map_skill_variant_row_db(&row))
            .transpose()?;

        let file_mtime = std::fs::metadata(&canonical)
            .ok()
            .and_then(|meta| meta.modified().ok())
            .and_then(|modified| modified.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|duration| duration.as_secs())
            .unwrap_or(0);
        if let Some(record) = existing.as_ref() {
            if file_mtime <= record.updated_at {
                return Ok(record.clone());
            }
        }

        let content = std::fs::read_to_string(&canonical)
            .with_context(|| format!("failed to read skill document {}", canonical.display()))?;
        let metadata = derive_skill_metadata(&relative_path, &content);
        let now = now_ts();
        let context_tags_json = serde_json::to_string(&metadata.context_tags)?;
        let skill_name = metadata.skill_name.clone();
        let variant_name = metadata.variant_name.clone();

        let _path = path;
        let mut txn = self.conn_db.transaction().await?;

        let variant_id = existing
            .as_ref()
            .map(|record| record.variant_id.clone())
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        let version = match existing.as_ref().map(|record| record.version.clone()) {
            Some(version) => version,
            None => {
                let version_num: i64 = txn
                    .query_opt(
                        "SELECT COUNT(*) FROM skill_variants WHERE skill_name = ?1",
                        db::db_params![skill_name.as_str()],
                    )
                    .await?
                    .map(|row| row.get::<i64>(0))
                    .transpose()?
                    .unwrap_or(0);
                format!("v{}.0", version_num + 1)
            }
        };
        let parent_variant_id: Option<String> = if variant_name == "canonical" {
            None
        } else {
            txn.query_opt(
                "SELECT variant_id FROM skill_variants WHERE skill_name = ?1 AND variant_name = 'canonical' LIMIT 1",
                db::db_params![skill_name.as_str()],
            )
            .await?
            .map(|row| row.get::<String>(0))
            .transpose()?
        };
        let created_at = existing
            .as_ref()
            .map(|record| record.created_at)
            .unwrap_or(now);
        let last_used_at = existing.as_ref().and_then(|record| record.last_used_at);
        let use_count = existing
            .as_ref()
            .map(|record| record.use_count)
            .unwrap_or(0);
        let success_count = existing
            .as_ref()
            .map(|record| record.success_count)
            .unwrap_or(0);
        let failure_count = existing
            .as_ref()
            .map(|record| record.failure_count)
            .unwrap_or(0);
        let status = existing
            .as_ref()
            .map(|record| record.status.clone())
            .unwrap_or_else(|| "active".to_string());

        let fitness_score = existing
            .as_ref()
            .map(|record| record.fitness_score)
            .unwrap_or_else(|| f64::from(success_count) - f64::from(failure_count));

        txn.execute(
            "INSERT OR REPLACE INTO skill_variants \
             (variant_id, skill_name, variant_name, relative_path, parent_variant_id, version, context_tags_json, use_count, success_count, failure_count, fitness_score, status, last_used_at, created_at, updated_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
            db::db_params![
                variant_id.clone(),
                skill_name,
                variant_name,
                relative_path,
                parent_variant_id,
                version,
                context_tags_json,
                use_count as i64,
                success_count as i64,
                failure_count as i64,
                fitness_score,
                status,
                last_used_at.map(|value| value as i64),
                created_at as i64,
                now as i64,
            ],
        )
        .await?;
        txn.commit().await?;

        self.rebalance_skill_variants(&metadata.skill_name).await?;

        self.conn_db
            .query_opt(
                "SELECT variant_id, skill_name, variant_name, relative_path, parent_variant_id, version, context_tags_json, use_count, success_count, failure_count, fitness_score, status, last_used_at, created_at, updated_at \
                 FROM skill_variants WHERE variant_id = ?1",
                db::db_params![variant_id],
            )
            .await?
            .map(|row| map_skill_variant_row_db(&row))
            .transpose()?
            .context("failed to load rebalanced skill variant")
    }

    pub async fn list_skill_variants(
        &self,
        query: Option<&str>,
        limit: usize,
    ) -> Result<Vec<SkillVariantRecord>> {
        let page = self.list_skill_variants_page(query, None, limit).await?;
        Ok(page.variants)
    }

    pub async fn list_discoverable_skill_variants(
        &self,
        limit: usize,
    ) -> Result<Vec<SkillVariantRecord>> {
        let limit = limit.clamp(1, 2000) as i64;
        let rows = self
            .conn_db
            .query(
                "SELECT variant_id, skill_name, variant_name, relative_path, parent_variant_id, version, context_tags_json, use_count, success_count, failure_count, fitness_score, status, last_used_at, created_at, updated_at \
                 FROM skill_variants \
                 WHERE status NOT IN ('archived', 'merged', 'draft') \
                   AND relative_path LIKE '%.md' \
                 ORDER BY CASE status \
                     WHEN 'promoted-to-canonical' THEN 4 \
                     WHEN 'active' THEN 3 \
                     WHEN 'deprecated' THEN 2 \
                     ELSE 0 \
                   END DESC, \
                   fitness_score DESC, \
                   CASE WHEN use_count > 0 THEN CAST(success_count AS REAL) / CAST(use_count AS REAL) ELSE 0.0 END DESC, \
                   use_count DESC, \
                   CASE WHEN variant_name = 'canonical' THEN 1 ELSE 0 END DESC, \
                   updated_at DESC, \
                   relative_path ASC \
                 LIMIT ?1",
                db::db_params![limit],
            )
            .await?;
        let mut variants = rows
            .iter()
            .map(map_skill_variant_row_db)
            .collect::<anyhow::Result<Vec<_>>>()?;
        variants.retain(|record| {
            !crate::agent::skill_library::is_skill_relative_path_disabled(&record.relative_path)
        });
        let trend_by_variant =
            load_skill_variant_trends_db(&mut db::ConnExecutor(&*self.conn_db), &variants, 8)
                .await?;
        variants.sort_by(|left, right| compare_skill_variants(left, right, &[], &trend_by_variant));
        Ok(variants)
    }

    /// Update the maturity status of a skill variant and bump `updated_at`.
    pub async fn update_skill_variant_status(
        &self,
        variant_id: &str,
        new_status: &str,
    ) -> Result<()> {
        self.caches
            .skill_variant
            .invalidate(&variant_id.to_string());
        self.conn_db
            .execute(
                "UPDATE skill_variants SET status = ?2, updated_at = ?3 WHERE variant_id = ?1",
                db::db_params![variant_id, new_status, now_ts() as i64],
            )
            .await?;
        Ok(())
    }

    /// Retrieve a single skill variant by its `variant_id`.
    pub async fn get_skill_variant(&self, variant_id: &str) -> Result<Option<SkillVariantRecord>> {
        let variant_id_owned = variant_id.to_string();
        if let Some(cached) = self.caches.skill_variant.get(&variant_id_owned) {
            return Ok(cached);
        }
        let value: Option<SkillVariantRecord> = self
            .conn_db
            .query_opt(
                "SELECT variant_id, skill_name, variant_name, relative_path, parent_variant_id, version, context_tags_json, use_count, success_count, failure_count, fitness_score, status, last_used_at, created_at, updated_at \
                 FROM skill_variants WHERE variant_id = ?1",
                db::db_params![variant_id_owned.as_str()],
            )
            .await?
            .map(|row| map_skill_variant_row_db(&row))
            .transpose()?;
        self.caches
            .skill_variant
            .insert(variant_id_owned, value.clone());
        Ok(value)
    }

    pub async fn resolve_skill_variant(
        &self,
        skill: &str,
        context_tags: &[String],
    ) -> Result<Option<SkillVariantRecord>> {
        let normalized = normalize_skill_lookup(skill);
        if normalized.is_empty() {
            return Ok(None);
        }
        let context_tags = context_tags.to_vec();
        let skills_root = self.skills_root();

        let rows = self
            .conn_db
            .query(
                "SELECT variant_id, skill_name, variant_name, relative_path, parent_variant_id, version, context_tags_json, use_count, success_count, failure_count, fitness_score, status, last_used_at, created_at, updated_at \
                 FROM skill_variants",
                db::Params::None,
            )
            .await?;
        let mut candidates = rows
            .iter()
            .filter_map(|row| map_skill_variant_row_db(row).ok())
            .filter(|record| skill_variant_matches(record, &normalized))
            .filter(|record| record.status != "archived" && record.status != "merged")
            .filter(|record| {
                !crate::agent::skill_library::is_skill_relative_path_disabled(&record.relative_path)
            })
            .collect::<Vec<_>>();
        if candidates.is_empty() {
            return Ok(None);
        }
        let live_candidates = candidates
            .iter()
            .filter(|record| skill_variant_document_exists(&skills_root, record))
            .cloned()
            .collect::<Vec<_>>();
        if !live_candidates.is_empty() {
            candidates = live_candidates;
        }
        let trend_by_variant =
            load_skill_variant_trends_db(&mut db::ConnExecutor(&*self.conn_db), &candidates, 8)
                .await?;
        candidates.sort_by(|left, right| {
            let left_current_path = skill_variant_matches_current_relative_path(&skills_root, left);
            let right_current_path =
                skill_variant_matches_current_relative_path(&skills_root, right);
            right_current_path
                .cmp(&left_current_path)
                .then_with(|| compare_skill_variants(left, right, &context_tags, &trend_by_variant))
        });
        Ok(candidates.into_iter().next())
    }

    pub async fn record_skill_variant_use(
        &self,
        variant_id: &str,
        outcome: Option<bool>,
    ) -> Result<()> {
        let now = now_ts() as i64;
        match outcome {
            Some(true) => {
                self.conn_db
                    .execute(
                        "UPDATE skill_variants \
                         SET use_count = use_count + 1, success_count = success_count + 1, fitness_score = fitness_score + 1.0, last_used_at = ?2, updated_at = ?2 \
                         WHERE variant_id = ?1",
                        db::db_params![variant_id, now],
                    )
                    .await?;
            }
            Some(false) => {
                self.conn_db
                    .execute(
                        "UPDATE skill_variants \
                         SET use_count = use_count + 1, failure_count = failure_count + 1, fitness_score = fitness_score - 1.0, last_used_at = ?2, updated_at = ?2 \
                         WHERE variant_id = ?1",
                        db::db_params![variant_id, now],
                    )
                    .await?;
            }
            None => {
                self.conn_db
                    .execute(
                        "UPDATE skill_variants \
                         SET use_count = use_count + 1, last_used_at = ?2, updated_at = ?2 \
                         WHERE variant_id = ?1",
                        db::db_params![variant_id, now],
                    )
                    .await?;
            }
        }
        Ok(())
    }

    pub async fn record_skill_variant_consultation(
        &self,
        record: &SkillVariantConsultationRecord<'_>,
    ) -> Result<()> {
        let now = record.consulted_at as i64;
        let context_tags_json = serde_json::to_string(record.context_tags)?;
        let usage_id = record.usage_id.to_string();
        let variant_id = record.variant_id.to_string();
        let thread_id = record.thread_id.map(str::to_string);
        let task_id = record.task_id.map(str::to_string);
        let goal_run_id = record.goal_run_id.map(str::to_string);
        let context_tags_owned: Vec<String> = record.context_tags.to_vec();

        self.conn_db
            .execute(
                "INSERT OR REPLACE INTO skill_variant_usage \
                 (usage_id, variant_id, thread_id, task_id, goal_run_id, context_tags_json, consulted_at, resolved_at, outcome) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, NULL, NULL)",
                db::db_params![
                    usage_id,
                    variant_id,
                    thread_id,
                    task_id,
                    goal_run_id,
                    context_tags_json,
                    now,
                ],
            )
            .await?;
        self.append_telemetry(
            "cognitive",
            json!({
                "timestamp": now,
                "kind": "skill_variant_consulted",
                "variant_id": record.variant_id,
                "thread_id": record.thread_id,
                "task_id": record.task_id,
                "goal_run_id": record.goal_run_id,
                "context_tags": context_tags_owned,
            }),
        )
        .await?;
        Ok(())
    }
}

fn skill_variant_document_exists(skills_root: &Path, record: &SkillVariantRecord) -> bool {
    let root_canonical =
        std::fs::canonicalize(skills_root).unwrap_or_else(|_| skills_root.to_path_buf());
    let candidate = resolve_skill_variant_document_path(skills_root, &record.relative_path);
    let Ok(canonical) = std::fs::canonicalize(candidate) else {
        return false;
    };
    canonical.starts_with(root_canonical)
}

fn skill_variant_matches_current_relative_path(
    skills_root: &Path,
    record: &SkillVariantRecord,
) -> bool {
    let root_canonical =
        std::fs::canonicalize(skills_root).unwrap_or_else(|_| skills_root.to_path_buf());
    let candidate = resolve_skill_variant_document_path(skills_root, &record.relative_path);
    let Ok(canonical) = std::fs::canonicalize(candidate) else {
        return false;
    };
    let Ok(relative) = canonical.strip_prefix(root_canonical) else {
        return false;
    };
    normalize_relative_path(&record.relative_path)
        == normalize_relative_path(&relative.to_string_lossy())
}

fn normalize_relative_path(path: &str) -> String {
    path.replace('\\', "/").trim_matches('/').to_string()
}

fn resolve_skill_variant_document_path(skills_root: &Path, relative_path: &str) -> PathBuf {
    let normalized = relative_path.replace('\\', "/");
    let candidate = skills_root.join(&normalized);
    if candidate.exists() {
        return candidate;
    }

    if let Some(stripped) = normalized.strip_prefix("builtin/") {
        let migrated = skills_root.join(stripped);
        if migrated.exists() {
            return migrated;
        }
    }

    resolve_skill_variant_document_by_suffix(skills_root, &normalized).unwrap_or(candidate)
}

fn resolve_skill_variant_document_by_suffix(
    skills_root: &Path,
    relative_path: &str,
) -> Option<PathBuf> {
    let mut files = Vec::new();
    collect_skill_documents(skills_root, &mut files).ok()?;

    for suffix in relative_path_suffixes(relative_path) {
        let matches = files
            .iter()
            .filter_map(|path| {
                let relative = path
                    .strip_prefix(skills_root)
                    .ok()?
                    .to_string_lossy()
                    .replace('\\', "/");
                if relative == suffix || relative.ends_with(&format!("/{suffix}")) {
                    Some(path.clone())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        if matches.len() == 1 {
            return matches.into_iter().next();
        }
    }

    None
}

fn relative_path_suffixes(relative_path: &str) -> Vec<String> {
    let mut suffixes = Vec::new();
    let mut current = relative_path.trim_matches('/').to_string();
    while !current.is_empty() {
        suffixes.push(current.clone());
        let Some((_, tail)) = current.split_once('/') else {
            break;
        };
        if !tail.contains('/') && tail.eq_ignore_ascii_case("SKILL.md") {
            break;
        }
        current = tail.to_string();
    }
    suffixes
}

fn collect_skill_documents(dir: &Path, out: &mut Vec<PathBuf>) -> std::io::Result<()> {
    if !dir.exists() {
        return Ok(());
    }

    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            collect_skill_documents(&path, out)?;
            continue;
        }
        if !file_type.is_file() {
            continue;
        }

        let is_md = path
            .extension()
            .and_then(|value| value.to_str())
            .is_some_and(|value| value.eq_ignore_ascii_case("md"));
        if is_md {
            out.push(path);
        }
    }

    Ok(())
}

impl HistoryStore {
    async fn load_skill_variants(&self, query: Option<&str>) -> Result<Vec<SkillVariantRecord>> {
        let normalized_query = query
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| value.to_ascii_lowercase());

        let rows = if let Some(query) = normalized_query.as_deref() {
            let like = format!("%{query}%");
            self.conn_db
                .query(
                    "SELECT variant_id, skill_name, variant_name, relative_path, parent_variant_id, version, context_tags_json, use_count, success_count, failure_count, fitness_score, status, last_used_at, created_at, updated_at \
                     FROM skill_variants \
                     WHERE skill_name LIKE ?1 OR variant_name LIKE ?1 OR relative_path LIKE ?1 OR context_tags_json LIKE ?1",
                    db::db_params![like],
                )
                .await?
        } else {
            self.conn_db
                .query(
                    "SELECT variant_id, skill_name, variant_name, relative_path, parent_variant_id, version, context_tags_json, use_count, success_count, failure_count, fitness_score, status, last_used_at, created_at, updated_at \
                     FROM skill_variants",
                    db::Params::None,
                )
                .await?
        };
        let mut variants = rows
            .iter()
            .filter_map(|row| map_skill_variant_row_db(row).ok())
            .filter(|record| {
                !crate::agent::skill_library::is_skill_relative_path_disabled(&record.relative_path)
            })
            .collect::<Vec<_>>();

        let trend_by_variant =
            load_skill_variant_trends_db(&mut db::ConnExecutor(&*self.conn_db), &variants, 8)
                .await?;
        variants.sort_by(|left, right| compare_skill_variants(left, right, &[], &trend_by_variant));
        Ok(variants)
    }
}

pub(crate) fn page_skill_variants(
    variants: Vec<SkillVariantRecord>,
    cursor: Option<&str>,
    limit: usize,
) -> Result<SkillVariantPage> {
    let limit = limit.clamp(1, 200);
    let start_index = decode_skill_list_cursor(cursor)?
        .as_deref()
        .and_then(|variant_id| {
            variants
                .iter()
                .position(|variant| variant.variant_id == variant_id)
                .map(|index| index + 1)
        })
        .unwrap_or(0);
    let page_variants = variants
        .iter()
        .skip(start_index)
        .take(limit)
        .cloned()
        .collect::<Vec<_>>();
    let next_cursor = if start_index + page_variants.len() < variants.len() {
        page_variants
            .last()
            .map(|variant| encode_skill_list_cursor(&variant.variant_id))
    } else {
        None
    };
    Ok(SkillVariantPage {
        variants: page_variants,
        next_cursor,
    })
}

fn encode_skill_list_cursor(variant_id: &str) -> String {
    let encoded = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(variant_id.as_bytes());
    format!("{SKILL_LIST_CURSOR_PREFIX}{encoded}")
}

fn decode_skill_list_cursor(cursor: Option<&str>) -> Result<Option<String>> {
    let Some(cursor) = cursor.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    let payload = cursor
        .strip_prefix(SKILL_LIST_CURSOR_PREFIX)
        .ok_or_else(|| anyhow::anyhow!("invalid skill list cursor"))?;
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload)
        .map_err(|error| anyhow::anyhow!("invalid skill list cursor: {error}"))?;
    let value = String::from_utf8(bytes)
        .map_err(|error| anyhow::anyhow!("invalid skill list cursor: {error}"))?;
    Ok(Some(value))
}

fn compute_skill_variant_fitness(record: &SkillVariantRecord) -> f64 {
    f64::from(record.success_count) - f64::from(record.failure_count)
}
