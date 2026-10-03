use super::*;
use chrono::{Datelike, Duration, Local, TimeZone};
use zorai_protocol::{DailyStatisticsRow, SessionStatisticsRow};

#[derive(Debug)]
struct StatisticsTotalsRow {
    input_tokens: i64,
    output_tokens: i64,
    total_tokens: i64,
    cost_usd: f64,
    provider_count: i64,
    model_count: i64,
    missing_cost_rows: i64,
}

fn map_provider_statistics_row(row: &db::Row) -> anyhow::Result<ProviderStatisticsRow> {
    Ok(ProviderStatisticsRow {
        provider: row.get(0)?,
        input_tokens: row.get::<i64>(1)?.max(0) as u64,
        output_tokens: row.get::<i64>(2)?.max(0) as u64,
        total_tokens: row.get::<i64>(3)?.max(0) as u64,
        cost_usd: row.get(4)?,
    })
}

fn map_model_statistics_row(row: &db::Row) -> anyhow::Result<ModelStatisticsRow> {
    Ok(ModelStatisticsRow {
        provider: row.get(0)?,
        model: row.get(1)?,
        input_tokens: row.get::<i64>(2)?.max(0) as u64,
        output_tokens: row.get::<i64>(3)?.max(0) as u64,
        total_tokens: row.get::<i64>(4)?.max(0) as u64,
        cost_usd: row.get(5)?,
    })
}

impl HistoryStore {
    pub async fn get_agent_statistics(
        &self,
        window: AgentStatisticsWindow,
        session_limit: Option<usize>,
        session_offset: Option<usize>,
    ) -> Result<AgentStatisticsSnapshot> {
        let session_limit = session_limit.unwrap_or(25).clamp(1, 100);
        let session_offset = session_offset.unwrap_or(0);
        let cutoff_ms = window_cutoff_ms(window);
        let totals_db_row = self
            .read_db
            .query_opt(
                "SELECT
                        COALESCE(SUM(COALESCE(input_tokens, 0)), 0) AS input_tokens,
                        COALESCE(SUM(COALESCE(output_tokens, 0)), 0) AS output_tokens,
                        COALESCE(SUM(COALESCE(total_tokens, COALESCE(input_tokens, 0) + COALESCE(output_tokens, 0))), 0) AS total_tokens,
                        COALESCE(SUM(COALESCE(cost_usd, 0)), 0.0) AS cost_usd,
                        COUNT(DISTINCT CASE WHEN provider IS NOT NULL AND TRIM(provider) <> '' THEN provider END) AS provider_count,
                        COUNT(DISTINCT CASE WHEN model IS NOT NULL AND TRIM(model) <> '' THEN model END) AS model_count,
                        COALESCE(SUM(CASE WHEN cost_usd IS NULL THEN 1 ELSE 0 END), 0) AS missing_cost_rows
                     FROM agent_messages
                     WHERE role = 'assistant'
                       AND deleted_at IS NULL
                       AND (?1 IS NULL OR created_at >= ?1)",
                db::db_params![cutoff_ms],
            )
            .await?
            .ok_or_else(|| anyhow::anyhow!("statistics totals query returned no row"))?;
        let totals_row = StatisticsTotalsRow {
            input_tokens: totals_db_row.get(0)?,
            output_tokens: totals_db_row.get(1)?,
            total_tokens: totals_db_row.get(2)?,
            cost_usd: totals_db_row.get(3)?,
            provider_count: totals_db_row.get(4)?,
            model_count: totals_db_row.get(5)?,
            missing_cost_rows: totals_db_row.get(6)?,
        };

        let provider_rows = self
            .read_db
            .query(
                "SELECT
                        CASE WHEN provider IS NULL OR TRIM(provider) = '' THEN 'unknown' ELSE provider END AS provider_key,
                        COALESCE(SUM(COALESCE(input_tokens, 0)), 0) AS input_tokens,
                        COALESCE(SUM(COALESCE(output_tokens, 0)), 0) AS output_tokens,
                        COALESCE(SUM(COALESCE(total_tokens, COALESCE(input_tokens, 0) + COALESCE(output_tokens, 0))), 0) AS total_tokens,
                        COALESCE(SUM(COALESCE(cost_usd, 0)), 0.0) AS cost_usd
                     FROM agent_messages
                     WHERE role = 'assistant'
                       AND deleted_at IS NULL
                       AND (?1 IS NULL OR created_at >= ?1)
                     GROUP BY provider_key",
                db::db_params![cutoff_ms],
            )
            .await?;
        let mut providers = provider_rows
            .iter()
            .map(map_provider_statistics_row)
            .collect::<Result<Vec<_>>>()?;

        providers.sort_by(|left, right| {
            right
                .total_tokens
                .cmp(&left.total_tokens)
                .then_with(|| right.cost_usd.total_cmp(&left.cost_usd))
                .then_with(|| left.provider.cmp(&right.provider))
        });

        let model_rows = self
            .read_db
            .query(
                "SELECT
                        CASE WHEN provider IS NULL OR TRIM(provider) = '' THEN 'unknown' ELSE provider END AS provider_key,
                        CASE WHEN model IS NULL OR TRIM(model) = '' THEN 'unknown' ELSE model END AS model_key,
                        COALESCE(SUM(COALESCE(input_tokens, 0)), 0) AS input_tokens,
                        COALESCE(SUM(COALESCE(output_tokens, 0)), 0) AS output_tokens,
                        COALESCE(SUM(COALESCE(total_tokens, COALESCE(input_tokens, 0) + COALESCE(output_tokens, 0))), 0) AS total_tokens,
                        COALESCE(SUM(COALESCE(cost_usd, 0)), 0.0) AS cost_usd
                     FROM agent_messages
                     WHERE role = 'assistant'
                       AND deleted_at IS NULL
                       AND (?1 IS NULL OR created_at >= ?1)
                       AND NOT (
                           (provider IS NULL OR TRIM(provider) = '')
                           AND (model IS NULL OR TRIM(model) = '')
                       )
                     GROUP BY provider_key, model_key",
                db::db_params![cutoff_ms],
            )
            .await?;
        let models = model_rows
            .iter()
            .map(map_model_statistics_row)
            .collect::<Result<Vec<_>>>()?;

        let mut sorted_models = models.clone();
        sorted_models.sort_by(|left, right| {
            right
                .total_tokens
                .cmp(&left.total_tokens)
                .then_with(|| right.cost_usd.total_cmp(&left.cost_usd))
                .then_with(|| left.provider.cmp(&right.provider))
                .then_with(|| left.model.cmp(&right.model))
        });

        let mut top_models_by_cost = models.clone();
        top_models_by_cost.sort_by(|left, right| {
            right
                .cost_usd
                .total_cmp(&left.cost_usd)
                .then_with(|| right.total_tokens.cmp(&left.total_tokens))
                .then_with(|| left.provider.cmp(&right.provider))
                .then_with(|| left.model.cmp(&right.model))
        });
        top_models_by_cost.truncate(5);

        let mut top_models_by_tokens = sorted_models.clone();
        top_models_by_tokens.truncate(5);

        let daily_rows = self
            .read_db
            .query(
                "SELECT
                        strftime('%Y-%m-%d', created_at / 1000, 'unixepoch', 'localtime') AS day_key,
                        COALESCE(SUM(COALESCE(input_tokens, 0)), 0),
                        COALESCE(SUM(COALESCE(output_tokens, 0)), 0),
                        COALESCE(SUM(COALESCE(total_tokens, COALESCE(input_tokens, 0) + COALESCE(output_tokens, 0))), 0),
                        COALESCE(SUM(COALESCE(cost_usd, 0)), 0.0),
                        COUNT(*)
                     FROM agent_messages
                     WHERE role = 'assistant'
                       AND deleted_at IS NULL
                       AND created_at > 0
                       AND (?1 IS NULL OR created_at >= ?1)
                     GROUP BY day_key
                     ORDER BY day_key ASC",
                db::db_params![cutoff_ms],
            )
            .await?;
        let daily = daily_rows
            .iter()
            .map(|row| -> Result<DailyStatisticsRow> {
                let day_key = row.get::<String>(0)?;
                let day_start = local_day_start_ms(&day_key).ok_or_else(|| {
                    anyhow::anyhow!("statistics query returned invalid local day key `{day_key}`")
                })?;
                Ok(DailyStatisticsRow {
                    day_start,
                    day_key,
                    input_tokens: row.get::<i64>(1)?.max(0) as u64,
                    output_tokens: row.get::<i64>(2)?.max(0) as u64,
                    total_tokens: row.get::<i64>(3)?.max(0) as u64,
                    cost_usd: row.get(4)?,
                    request_count: row.get::<i64>(5)?.max(0) as u64,
                })
            })
            .collect::<Result<Vec<_>>>()?;

        let (sessions, session_total) = self
            .load_session_statistics(window, session_limit, session_offset)
            .await?;

        Ok(AgentStatisticsSnapshot {
            window,
            generated_at: current_time_ms(),
            has_incomplete_cost_history: totals_row.missing_cost_rows > 0,
            totals: AgentStatisticsTotals {
                input_tokens: totals_row.input_tokens.max(0) as u64,
                output_tokens: totals_row.output_tokens.max(0) as u64,
                total_tokens: totals_row.total_tokens.max(0) as u64,
                cost_usd: totals_row.cost_usd,
                provider_count: totals_row.provider_count.max(0) as u64,
                model_count: totals_row.model_count.max(0) as u64,
            },
            providers,
            models: sorted_models,
            top_models_by_tokens,
            top_models_by_cost,
            daily,
            sessions,
            session_total,
            session_limit: session_limit as u64,
            session_offset: session_offset as u64,
        })
    }

    pub async fn get_agent_session_page(
        &self,
        window: AgentStatisticsWindow,
        session_limit: Option<usize>,
        session_offset: Option<usize>,
    ) -> Result<AgentStatisticsSnapshot> {
        let session_limit = session_limit.unwrap_or(25).clamp(1, 100);
        let session_offset = session_offset.unwrap_or(0);
        let (sessions, session_total) = self
            .load_session_statistics(window, session_limit, session_offset)
            .await?;
        Ok(AgentStatisticsSnapshot {
            window,
            generated_at: current_time_ms(),
            has_incomplete_cost_history: false,
            totals: AgentStatisticsTotals {
                input_tokens: 0,
                output_tokens: 0,
                total_tokens: 0,
                cost_usd: 0.0,
                provider_count: 0,
                model_count: 0,
            },
            providers: Vec::new(),
            models: Vec::new(),
            top_models_by_tokens: Vec::new(),
            top_models_by_cost: Vec::new(),
            daily: Vec::new(),
            sessions,
            session_total,
            session_limit: session_limit as u64,
            session_offset: session_offset as u64,
        })
    }

    async fn load_session_statistics(
        &self,
        window: AgentStatisticsWindow,
        session_limit: usize,
        session_offset: usize,
    ) -> Result<(Vec<SessionStatisticsRow>, u64)> {
        let cutoff_ms = window_cutoff_ms(window);
        let session_total_row = self
            .read_db
            .query_opt(
                "SELECT COUNT(*) FROM (
                     SELECT m.thread_id
                     FROM agent_messages m
                     LEFT JOIN agent_threads t ON t.id = m.thread_id
                     WHERE m.role = 'assistant'
                       AND m.deleted_at IS NULL
                       AND t.deleted_at IS NULL
                       AND m.created_at > 0
                       AND (?1 IS NULL OR m.created_at >= ?1)
                     GROUP BY m.thread_id
                 )",
                db::db_params![cutoff_ms],
            )
            .await?;
        let session_total = session_total_row
            .as_ref()
            .map(|row| row.get::<i64>(0))
            .transpose()?
            .unwrap_or_default()
            .max(0) as u64;

        let session_rows = self
            .read_db
            .query(
                "SELECT
                        m.thread_id,
                        COALESCE(NULLIF(TRIM(t.title), ''), m.thread_id) AS title,
                        MAX(m.created_at) AS updated_at,
                        COALESCE(GROUP_CONCAT(DISTINCT
                            (CASE WHEN m.provider IS NULL OR TRIM(m.provider) = '' THEN 'unknown' ELSE m.provider END)
                            || '/' ||
                            (CASE WHEN m.model IS NULL OR TRIM(m.model) = '' THEN 'unknown' ELSE m.model END)
                        ), ''),
                        COUNT(*),
                        COALESCE(SUM(COALESCE(m.input_tokens, 0)), 0),
                        COALESCE(SUM(COALESCE(m.output_tokens, 0)), 0),
                        COALESCE(SUM(COALESCE(m.total_tokens, COALESCE(m.input_tokens, 0) + COALESCE(m.output_tokens, 0))), 0),
                        COALESCE(SUM(COALESCE(m.cost_usd, 0)), 0.0)
                     FROM agent_messages m
                     LEFT JOIN agent_threads t ON t.id = m.thread_id
                     WHERE m.role = 'assistant'
                       AND m.deleted_at IS NULL
                       AND t.deleted_at IS NULL
                       AND m.created_at > 0
                       AND (?1 IS NULL OR m.created_at >= ?1)
                     GROUP BY m.thread_id, title
                     ORDER BY updated_at DESC, m.thread_id ASC
                     LIMIT ?2 OFFSET ?3",
                db::db_params![cutoff_ms, session_limit as i64, session_offset as i64],
            )
            .await?;
        let sessions = session_rows
            .iter()
            .map(|row| -> Result<SessionStatisticsRow> {
                let provider_models = row
                    .get::<String>(3)?
                    .split(',')
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(str::to_string)
                    .collect();
                Ok(SessionStatisticsRow {
                    thread_id: row.get(0)?,
                    title: row.get(1)?,
                    updated_at: row.get::<i64>(2)?.max(0) as u64,
                    provider_models,
                    request_count: row.get::<i64>(4)?.max(0) as u64,
                    input_tokens: row.get::<i64>(5)?.max(0) as u64,
                    output_tokens: row.get::<i64>(6)?.max(0) as u64,
                    total_tokens: row.get::<i64>(7)?.max(0) as u64,
                    cost_usd: row.get(8)?,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        Ok((sessions, session_total))
    }
}

fn local_day_start_ms(day_key: &str) -> Option<u64> {
    let date = chrono::NaiveDate::parse_from_str(day_key, "%Y-%m-%d").ok()?;
    let local = Local
        .with_ymd_and_hms(date.year(), date.month(), date.day(), 0, 0, 0)
        .single()?;
    Some(local.timestamp_millis().max(0) as u64)
}

fn current_time_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn window_cutoff_ms(window: AgentStatisticsWindow) -> Option<i64> {
    match window {
        AgentStatisticsWindow::Today => {
            let now = Local::now();
            let start_of_day = Local
                .with_ymd_and_hms(now.year(), now.month(), now.day(), 0, 0, 0)
                .single()
                .unwrap_or(now);
            Some(start_of_day.timestamp_millis())
        }
        AgentStatisticsWindow::Last7Days => {
            Some((Local::now() - Duration::days(7)).timestamp_millis())
        }
        AgentStatisticsWindow::Last30Days => {
            Some((Local::now() - Duration::days(30)).timestamp_millis())
        }
        AgentStatisticsWindow::All => None,
    }
}
