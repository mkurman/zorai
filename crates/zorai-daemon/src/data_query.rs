use anyhow::{bail, Context, Result};
use duckdb::types::Value;
use duckdb::Connection;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::Duration;

const MAX_PAGE_ROWS: u64 = 1_000;
const DEFAULT_PAGE_ROWS: u64 = 200;
const MAX_SPREADSHEET_ROWS: usize = 1_000_000;
const QUERY_TIMEOUT: Duration = Duration::from_secs(30);
const MEMORY_LIMIT: &str = "1GB";

#[derive(Debug, Clone, Deserialize)]
pub struct DataQueryRequest {
    #[serde(default)]
    pub request_id: Option<String>,
    #[serde(default = "default_op")]
    pub op: String,
    pub path: String,
    #[serde(default)]
    pub sheet: Option<String>,
    #[serde(default)]
    pub offset: Option<u64>,
    #[serde(default)]
    pub limit: Option<u64>,
    #[serde(default)]
    pub sort_column: Option<String>,
    #[serde(default)]
    pub sort_direction: Option<String>,
    #[serde(default)]
    pub search: Option<String>,
}

fn default_op() -> String {
    "page".to_string()
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct DataColumn {
    pub name: String,
    pub data_type: String,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct DataQueryResponse {
    pub request_id: Option<String>,
    pub ok: bool,
    pub error: Option<String>,
    pub path: String,
    pub op: String,
    pub format: String,
    pub sheets: Vec<String>,
    pub sheet: Option<String>,
    pub columns: Vec<DataColumn>,
    pub total_rows: u64,
    pub offset: u64,
    pub rows: Vec<Vec<Option<String>>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataFormat {
    Csv,
    Tsv,
    Jsonl,
    Json,
    Parquet,
    Spreadsheet,
}

impl DataFormat {
    pub fn for_path(path: &Path) -> Option<Self> {
        let extension = path.extension()?.to_str()?.to_ascii_lowercase();
        Some(match extension.as_str() {
            "csv" => Self::Csv,
            "tsv" | "tab" => Self::Tsv,
            "jsonl" | "ndjson" => Self::Jsonl,
            "json" => Self::Json,
            "parquet" | "pq" => Self::Parquet,
            "xlsx" | "xlsm" | "xlsb" | "xls" | "ods" => Self::Spreadsheet,
            _ => return None,
        })
    }

    fn label(self) -> &'static str {
        match self {
            Self::Csv => "csv",
            Self::Tsv => "tsv",
            Self::Jsonl => "jsonl",
            Self::Json => "json",
            Self::Parquet => "parquet",
            Self::Spreadsheet => "spreadsheet",
        }
    }
}

pub async fn run_data_query(request: DataQueryRequest) -> DataQueryResponse {
    let request_id = request.request_id.clone();
    let path = request.path.clone();
    let op = request.op.clone();
    let (connection_tx, connection_rx) = std::sync::mpsc::channel();
    let task = tokio::task::spawn_blocking(move || execute(request, connection_tx));
    let outcome = tokio::time::timeout(QUERY_TIMEOUT, task).await;
    let result = match outcome {
        Ok(Ok(result)) => result,
        Ok(Err(join_error)) => Err(anyhow::anyhow!("data query task failed: {join_error}")),
        Err(_) => {
            if let Ok(handle) = connection_rx.try_recv() {
                let handle: std::sync::Arc<duckdb::InterruptHandle> = handle;
                handle.interrupt();
            }
            Err(anyhow::anyhow!(
                "data query timed out after {}s",
                QUERY_TIMEOUT.as_secs()
            ))
        }
    };
    match result {
        Ok(mut response) => {
            response.request_id = request_id;
            response
        }
        Err(error) => {
            tracing::warn!(path = %path, op = %op, error = %error, "data query failed");
            DataQueryResponse {
                request_id,
                ok: false,
                error: Some(format!("{error:#}")),
                path,
                op,
                ..DataQueryResponse::default()
            }
        }
    }
}

fn execute(
    request: DataQueryRequest,
    connection_tx: std::sync::mpsc::Sender<std::sync::Arc<duckdb::InterruptHandle>>,
) -> Result<DataQueryResponse> {
    let path = resolve_local_file(&request.path)?;
    let format = DataFormat::for_path(&path)
        .with_context(|| format!("unsupported data file type: {}", path.display()))?;
    let connection = open_connection()?;
    let _ = connection_tx.send(connection.interrupt_handle());

    let mut sheets = Vec::new();
    let mut sheet = None;
    let source = if format == DataFormat::Spreadsheet {
        let loaded = load_spreadsheet(&connection, &path, request.sheet.as_deref())?;
        sheets = loaded.0;
        sheet = Some(loaded.1);
        "src_sheet".to_string()
    } else {
        source_relation(format, &path)
    };

    let columns = describe(&connection, &source)?;
    let (columns, total_rows, offset, rows) = match request.op.as_str() {
        "page" => {
            let filter = search_filter(&columns, request.search.as_deref());
            let order = order_clause(&columns, &request)?;
            let total_rows = count_rows(&connection, &source, &filter)?;
            let offset = request.offset.unwrap_or(0);
            let limit = request
                .limit
                .unwrap_or(DEFAULT_PAGE_ROWS)
                .clamp(1, MAX_PAGE_ROWS);
            let sql = format!(
                "SELECT COLUMNS(*)::VARCHAR FROM {source} AS src_rel{} {order} LIMIT {limit} OFFSET {offset}",
                filter.sql
            );
            let rows = fetch_rows(&connection, &sql, &filter.params, columns.len())?;
            (columns, total_rows, offset, rows)
        }
        "summarize" => {
            let sql = format!("SELECT COLUMNS(*)::VARCHAR FROM (SUMMARIZE SELECT * FROM {source})");
            let summary_columns = describe(
                &connection,
                &format!("(SUMMARIZE SELECT * FROM {source})"),
            )?;
            let rows = fetch_rows(&connection, &sql, &[], summary_columns.len())?;
            let total_rows = rows.len() as u64;
            (summary_columns, total_rows, 0, rows)
        }
        other => bail!("unsupported data query op '{other}' (use page or summarize)"),
    };

    Ok(DataQueryResponse {
        request_id: None,
        ok: true,
        error: None,
        path: path.display().to_string(),
        op: request.op,
        format: format.label().to_string(),
        sheets,
        sheet,
        columns,
        total_rows,
        offset,
        rows,
    })
}

fn resolve_local_file(raw: &str) -> Result<PathBuf> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        bail!("data query requires a file path");
    }
    if trimmed.contains("://") {
        bail!("data queries only read local files");
    }
    let path = PathBuf::from(trimmed);
    let canonical = std::fs::canonicalize(&path)
        .with_context(|| format!("cannot open {}", path.display()))?;
    if !canonical.is_file() {
        bail!("{} is not a file", canonical.display());
    }
    Ok(canonical)
}

fn open_connection() -> Result<Connection> {
    let connection = Connection::open_in_memory().context("failed to start DuckDB")?;
    connection
        .execute_batch(&format!(
            "SET memory_limit='{MEMORY_LIMIT}';
             SET threads=2;
             SET autoinstall_known_extensions=false;
             SET autoload_known_extensions=false;
             SET lock_configuration=true;"
        ))
        .context("failed to configure DuckDB")?;
    Ok(connection)
}

fn sql_string(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

fn sql_identifier(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\"\""))
}

fn source_relation(format: DataFormat, path: &Path) -> String {
    let path = sql_string(&path.display().to_string());
    match format {
        DataFormat::Csv => format!("read_csv_auto({path})"),
        DataFormat::Tsv => format!("read_csv_auto({path}, delim='\\t')"),
        DataFormat::Jsonl => format!("read_json_auto({path}, format='newline_delimited')"),
        DataFormat::Json => format!("read_json_auto({path})"),
        DataFormat::Parquet => format!("read_parquet({path})"),
        DataFormat::Spreadsheet => unreachable!("spreadsheets are loaded into a table"),
    }
}

fn describe(connection: &Connection, source: &str) -> Result<Vec<DataColumn>> {
    let mut statement = connection.prepare(&format!("DESCRIBE SELECT * FROM {source}"))?;
    let columns = statement
        .query_map([], |row| {
            Ok(DataColumn {
                name: row.get(0)?,
                data_type: row.get(1)?,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(columns)
}

struct Filter {
    sql: String,
    params: Vec<String>,
}

fn search_filter(columns: &[DataColumn], search: Option<&str>) -> Filter {
    let Some(search) = search.map(str::trim).filter(|value| !value.is_empty()) else {
        return Filter {
            sql: String::new(),
            params: Vec::new(),
        };
    };
    let pattern = format!(
        "%{}%",
        search
            .replace('\\', "\\\\")
            .replace('%', "\\%")
            .replace('_', "\\_")
    );
    let clauses = columns
        .iter()
        .map(|column| {
            format!(
                "CAST({} AS VARCHAR) ILIKE ? ESCAPE '\\'",
                sql_identifier(&column.name)
            )
        })
        .collect::<Vec<_>>();
    Filter {
        sql: format!(" WHERE {}", clauses.join(" OR ")),
        params: vec![pattern; columns.len()],
    }
}

fn order_clause(columns: &[DataColumn], request: &DataQueryRequest) -> Result<String> {
    let Some(column) = request
        .sort_column
        .as_deref()
        .filter(|value| !value.is_empty())
    else {
        return Ok(String::new());
    };
    if !columns.iter().any(|candidate| candidate.name == column) {
        bail!("unknown sort column '{column}'");
    }
    let direction = match request.sort_direction.as_deref() {
        Some("desc") => "DESC",
        _ => "ASC",
    };
    Ok(format!(
        "ORDER BY src_rel.{} {direction} NULLS LAST",
        sql_identifier(column)
    ))
}

fn count_rows(connection: &Connection, source: &str, filter: &Filter) -> Result<u64> {
    let sql = format!("SELECT count(*) FROM {source}{}", filter.sql);
    let count: i64 = connection.query_row(
        &sql,
        duckdb::params_from_iter(filter.params.iter()),
        |row| row.get(0),
    )?;
    Ok(count.max(0) as u64)
}

fn fetch_rows(
    connection: &Connection,
    sql: &str,
    params: &[String],
    column_count: usize,
) -> Result<Vec<Vec<Option<String>>>> {
    let mut statement = connection.prepare(sql)?;
    let rows = statement
        .query_map(duckdb::params_from_iter(params.iter()), |row| {
            (0..column_count)
                .map(|index| row.get::<_, Option<String>>(index))
                .collect::<std::result::Result<Vec<_>, _>>()
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(rows)
}

fn load_spreadsheet(
    connection: &Connection,
    path: &Path,
    requested_sheet: Option<&str>,
) -> Result<(Vec<String>, String)> {
    use calamine::{open_workbook_auto, Data, Reader};

    let mut workbook = open_workbook_auto(path)
        .with_context(|| format!("cannot open spreadsheet {}", path.display()))?;
    let sheets = workbook.sheet_names();
    let sheet = match requested_sheet.filter(|value| !value.is_empty()) {
        Some(name) if sheets.iter().any(|candidate| candidate == name) => name.to_string(),
        Some(name) => bail!("sheet '{name}' not found"),
        None => sheets
            .first()
            .cloned()
            .context("spreadsheet has no sheets")?,
    };
    let range = workbook
        .worksheet_range(&sheet)
        .with_context(|| format!("cannot read sheet '{sheet}'"))?;
    let mut rows = range.rows();
    let header = rows.next().map(|row| row.to_vec()).unwrap_or_default();
    let body = rows.take(MAX_SPREADSHEET_ROWS).collect::<Vec<_>>();
    let width = body
        .iter()
        .map(|row| row.len())
        .chain(std::iter::once(header.len()))
        .max()
        .unwrap_or(0);
    if width == 0 {
        connection.execute_batch("CREATE TABLE src_sheet (empty VARCHAR)")?;
        return Ok((sheets, sheet));
    }

    let mut names = Vec::with_capacity(width);
    for index in 0..width {
        let base = header
            .get(index)
            .map(|cell| cell.to_string().trim().to_string())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| format!("column_{}", index + 1));
        let mut name = base.clone();
        let mut suffix = 2;
        while names.contains(&name) {
            name = format!("{base}_{suffix}");
            suffix += 1;
        }
        names.push(name);
    }
    let numeric = (0..width)
        .map(|index| {
            let mut saw_value = false;
            let all_numeric = body.iter().all(|row| match row.get(index) {
                Some(Data::Int(_)) | Some(Data::Float(_)) => {
                    saw_value = true;
                    true
                }
                Some(Data::Empty) | None => true,
                _ => false,
            });
            all_numeric && saw_value
        })
        .collect::<Vec<_>>();
    let definition = names
        .iter()
        .zip(&numeric)
        .map(|(name, numeric)| {
            format!(
                "{} {}",
                sql_identifier(name),
                if *numeric { "DOUBLE" } else { "VARCHAR" }
            )
        })
        .collect::<Vec<_>>()
        .join(", ");
    connection.execute_batch(&format!("CREATE TABLE src_sheet ({definition})"))?;

    let mut appender = connection.appender("src_sheet")?;
    for row in &body {
        let values = (0..width)
            .map(|index| match row.get(index) {
                None | Some(Data::Empty) => Value::Null,
                Some(Data::Int(value)) if numeric[index] => Value::Double(*value as f64),
                Some(Data::Float(value)) if numeric[index] => Value::Double(*value),
                Some(cell) => Value::Text(cell.to_string()),
            })
            .collect::<Vec<_>>();
        appender.append_row(duckdb::appender_params_from_iter(values))?;
    }
    appender.flush()?;
    Ok((sheets, sheet))
}

#[cfg(test)]
#[path = "data_query_tests.rs"]
mod tests;
