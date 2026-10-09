#![allow(dead_code)]

//! SQLite engine for the [`DbConn`]/[`DbTxn`] facade, backed by the existing
//! `tokio_rusqlite` connections and [`ReadPool`](super::super::ReadPool).
//!
//! Non-transactional statements run inside a single `tokio_rusqlite` `.call`.
//! Transactions can't span `.call` boundaries (`rusqlite::Transaction` borrows
//! the connection), so [`SqliteWriteConn::transaction`] spawns a dedicated
//! worker thread that owns a raw `rusqlite::Connection` for the transaction's
//! lifetime and applies statements as commands arrive.

use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::thread::JoinHandle;

use anyhow::{anyhow, Result};
use rusqlite::params_from_iter;

use super::{unsupported, DbConn, DbTxn, Params, Row, Value};
use crate::history::ReadPool;

fn map_row(row: &rusqlite::Row<'_>, column_count: usize) -> rusqlite::Result<Row> {
    let mut values = Vec::with_capacity(column_count);
    for idx in 0..column_count {
        values.push(Value::from(row.get::<_, rusqlite::types::Value>(idx)?));
    }
    Ok(Row::new(values))
}

impl From<rusqlite::types::Value> for Value {
    fn from(value: rusqlite::types::Value) -> Self {
        match value {
            rusqlite::types::Value::Null => Value::Null,
            rusqlite::types::Value::Integer(i) => Value::Integer(i),
            rusqlite::types::Value::Real(f) => Value::Real(f),
            rusqlite::types::Value::Text(s) => Value::Text(s),
            rusqlite::types::Value::Blob(b) => Value::Blob(b),
        }
    }
}

impl rusqlite::ToSql for Value {
    fn to_sql(&self) -> rusqlite::Result<rusqlite::types::ToSqlOutput<'_>> {
        use rusqlite::types::{ToSqlOutput, ValueRef};
        Ok(match self {
            Value::Null => ToSqlOutput::Borrowed(ValueRef::Null),
            Value::Integer(i) => ToSqlOutput::Owned(rusqlite::types::Value::Integer(*i)),
            Value::Real(f) => ToSqlOutput::Owned(rusqlite::types::Value::Real(*f)),
            Value::Text(s) => ToSqlOutput::Borrowed(ValueRef::Text(s.as_bytes())),
            Value::Blob(b) => ToSqlOutput::Borrowed(ValueRef::Blob(b)),
        })
    }
}

/// Runs the statements directly on a `rusqlite::Connection` (used for both the
/// async `.call` path and the transaction worker, since `Transaction` derefs to
/// `Connection`).
const SLOW_STATEMENT_WARN_MS: u128 = 250;

fn warn_if_slow(sql: &str, started: std::time::Instant, rows: usize) {
    let elapsed_ms = started.elapsed().as_millis();
    if elapsed_ms < SLOW_STATEMENT_WARN_MS {
        return;
    }
    let compact_sql = sql.split_whitespace().collect::<Vec<_>>().join(" ");
    let sql_head = compact_sql.chars().take(400).collect::<String>();
    tracing::warn!(elapsed_ms = elapsed_ms as u64, rows, sql = %sql_head, "slow sqlite statement");
}

fn run_execute(conn: &rusqlite::Connection, sql: &str, params: &Params) -> rusqlite::Result<u64> {
    let started = std::time::Instant::now();
    let affected = conn.execute(sql, params_from_iter(params.values().iter()))?;
    warn_if_slow(sql, started, affected);
    Ok(affected as u64)
}

fn run_query(
    conn: &rusqlite::Connection,
    sql: &str,
    params: &Params,
) -> rusqlite::Result<Vec<Row>> {
    let started = std::time::Instant::now();
    let mut stmt = conn.prepare(sql)?;
    let column_count = stmt.column_count();
    let rows = stmt
        .query_map(params_from_iter(params.values().iter()), |row| {
            map_row(row, column_count)
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    warn_if_slow(sql, started, rows.len());
    Ok(rows)
}

fn run_query_with_columns(
    conn: &rusqlite::Connection,
    sql: &str,
    params: &Params,
) -> rusqlite::Result<(Vec<String>, Vec<Row>)> {
    let started = std::time::Instant::now();
    let mut stmt = conn.prepare(sql)?;
    let column_count = stmt.column_count();
    let columns = (0..column_count)
        .map(|index| {
            stmt.column_name(index)
                .map(str::to_string)
                .unwrap_or_else(|_| format!("column_{}", index + 1))
        })
        .collect::<Vec<_>>();
    let rows = stmt
        .query_map(params_from_iter(params.values().iter()), |row| {
            map_row(row, column_count)
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    warn_if_slow(sql, started, rows.len());
    Ok((columns, rows))
}

/// Writer connection. Wraps a single `tokio_rusqlite` writer; supports
/// transactions via a dedicated worker thread that opens its own connection.
pub(crate) struct SqliteWriteConn {
    conn: tokio_rusqlite::Connection,
    db_path: PathBuf,
}

impl SqliteWriteConn {
    pub(crate) fn new(conn: tokio_rusqlite::Connection, db_path: PathBuf) -> Self {
        Self { conn, db_path }
    }
}

#[async_trait::async_trait]
impl DbConn for SqliteWriteConn {
    async fn execute(&self, sql: &str, params: Params) -> Result<u64> {
        let sql = sql.to_string();
        self.conn
            .call(move |conn| Ok(run_execute(conn, &sql, &params)?))
            .await
            .map_err(|e| anyhow!("{e}"))
    }

    async fn execute_batch(&self, sql: &str) -> Result<()> {
        let sql = sql.to_string();
        self.conn
            .call(move |conn| Ok(conn.execute_batch(&sql)?))
            .await
            .map_err(|e| anyhow!("{e}"))
    }

    async fn query(&self, sql: &str, params: Params) -> Result<Vec<Row>> {
        let sql = sql.to_string();
        self.conn
            .call(move |conn| Ok(run_query(conn, &sql, &params)?))
            .await
            .map_err(|e| anyhow!("{e}"))
    }

    async fn query_with_columns(
        &self,
        sql: &str,
        params: Params,
    ) -> Result<(Vec<String>, Vec<Row>)> {
        let sql = sql.to_string();
        self.conn
            .call(move |conn| Ok(run_query_with_columns(conn, &sql, &params)?))
            .await
            .map_err(|e| anyhow!("{e}"))
    }

    async fn total_changes(&self) -> Result<u64> {
        self.conn
            .call(move |conn| Ok(conn.total_changes() as u64))
            .await
            .map_err(|e| anyhow!("{e}"))
    }

    async fn last_insert_rowid(&self) -> Result<i64> {
        self.conn
            .call(move |conn| Ok(conn.last_insert_rowid()))
            .await
            .map_err(|e| anyhow!("{e}"))
    }

    async fn execute_returning_rowid(&self, sql: &str, params: Params) -> Result<i64> {
        let sql = sql.to_string();
        self.conn
            .call(move |conn| {
                run_execute(conn, &sql, &params)?;
                Ok(conn.last_insert_rowid())
            })
            .await
            .map_err(|e| anyhow!("{e}"))
    }

    async fn transaction(&self) -> Result<Box<dyn DbTxn>> {
        let txn = SqliteTxn::begin(self.db_path.clone()).await?;
        Ok(Box::new(txn))
    }
}

/// Read connection backed by the load-balanced [`ReadPool`]. Statements are
/// read-only in practice; transactions are unsupported (no call site begins a
/// transaction on a reader).
pub(crate) struct SqliteReadConn {
    pool: ReadPool,
}

impl SqliteReadConn {
    pub(crate) fn new(pool: ReadPool) -> Self {
        Self { pool }
    }
}

#[async_trait::async_trait]
impl DbConn for SqliteReadConn {
    async fn execute(&self, sql: &str, params: Params) -> Result<u64> {
        let sql = sql.to_string();
        self.pool
            .call(move |conn| Ok(run_execute(conn, &sql, &params)?))
            .await
            .map_err(|e| anyhow!("{e}"))
    }

    async fn execute_batch(&self, sql: &str) -> Result<()> {
        let sql = sql.to_string();
        self.pool
            .call(move |conn| Ok(conn.execute_batch(&sql)?))
            .await
            .map_err(|e| anyhow!("{e}"))
    }

    async fn query(&self, sql: &str, params: Params) -> Result<Vec<Row>> {
        let sql = sql.to_string();
        self.pool
            .call(move |conn| Ok(run_query(conn, &sql, &params)?))
            .await
            .map_err(|e| anyhow!("{e}"))
    }

    async fn query_with_columns(
        &self,
        sql: &str,
        params: Params,
    ) -> Result<(Vec<String>, Vec<Row>)> {
        let sql = sql.to_string();
        self.pool
            .call(move |conn| Ok(run_query_with_columns(conn, &sql, &params)?))
            .await
            .map_err(|e| anyhow!("{e}"))
    }

    async fn total_changes(&self) -> Result<u64> {
        self.pool
            .call(move |conn| Ok(conn.total_changes() as u64))
            .await
            .map_err(|e| anyhow!("{e}"))
    }

    async fn last_insert_rowid(&self) -> Result<i64> {
        self.pool
            .call(move |conn| Ok(conn.last_insert_rowid()))
            .await
            .map_err(|e| anyhow!("{e}"))
    }

    async fn transaction(&self) -> Result<Box<dyn DbTxn>> {
        Err(unsupported("transaction on a read pool"))
    }
}

/// Commands sent to the transaction worker thread.
enum TxnCmd {
    Execute {
        sql: String,
        params: Params,
        resp: tokio::sync::oneshot::Sender<Result<u64>>,
    },
    ExecuteBatch {
        sql: String,
        resp: tokio::sync::oneshot::Sender<Result<()>>,
    },
    Query {
        sql: String,
        params: Params,
        resp: tokio::sync::oneshot::Sender<Result<Vec<Row>>>,
    },
    LastInsertRowid {
        resp: tokio::sync::oneshot::Sender<Result<i64>>,
    },
    Commit {
        resp: tokio::sync::oneshot::Sender<Result<()>>,
    },
}

/// A transaction running on a dedicated worker thread that owns a raw
/// `rusqlite::Connection`. Dropping without [`DbTxn::commit`] closes the command
/// channel, which drops the `Transaction` on the worker and rolls back.
pub(crate) struct SqliteTxn {
    sender: Option<mpsc::Sender<TxnCmd>>,
    worker: Option<JoinHandle<()>>,
}

impl SqliteTxn {
    async fn begin(db_path: PathBuf) -> Result<Self> {
        let (sender, receiver) = mpsc::channel::<TxnCmd>();
        // `ready` reports whether opening the connection and issuing BEGIN
        // succeeded, so callers see open/begin failures immediately.
        let (ready_tx, ready_rx) = tokio::sync::oneshot::channel::<Result<()>>();
        let worker = std::thread::spawn(move || txn_worker(db_path, receiver, ready_tx));
        match ready_rx.await {
            Ok(Ok(())) => Ok(Self {
                sender: Some(sender),
                worker: Some(worker),
            }),
            Ok(Err(e)) => {
                let _ = worker.join();
                Err(e)
            }
            Err(_) => {
                let _ = worker.join();
                Err(anyhow!(
                    "transaction worker exited before signalling readiness"
                ))
            }
        }
    }

    async fn send<T>(
        &self,
        make: impl FnOnce(tokio::sync::oneshot::Sender<Result<T>>) -> TxnCmd,
    ) -> Result<T> {
        let (resp_tx, resp_rx) = tokio::sync::oneshot::channel::<Result<T>>();
        self.sender
            .as_ref()
            .ok_or_else(|| anyhow!("transaction already finished"))?
            .send(make(resp_tx))
            .map_err(|_| anyhow!("transaction worker is gone"))?;
        resp_rx
            .await
            .map_err(|_| anyhow!("transaction worker dropped the response"))?
    }
}

impl Drop for SqliteTxn {
    fn drop(&mut self) {
        // Closing the channel signals the worker to roll back; join so the
        // connection is closed before the temp dir (in tests) is removed.
        self.sender.take();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

#[async_trait::async_trait]
impl DbTxn for SqliteTxn {
    async fn execute(&mut self, sql: &str, params: Params) -> Result<u64> {
        let sql = sql.to_string();
        self.send(|resp| TxnCmd::Execute { sql, params, resp })
            .await
    }

    async fn execute_batch(&mut self, sql: &str) -> Result<()> {
        let sql = sql.to_string();
        self.send(|resp| TxnCmd::ExecuteBatch { sql, resp }).await
    }

    async fn query(&mut self, sql: &str, params: Params) -> Result<Vec<Row>> {
        let sql = sql.to_string();
        self.send(|resp| TxnCmd::Query { sql, params, resp }).await
    }

    async fn last_insert_rowid(&mut self) -> Result<i64> {
        self.send(|resp| TxnCmd::LastInsertRowid { resp }).await
    }

    async fn commit(mut self: Box<Self>) -> Result<()> {
        let result = self.send(|resp| TxnCmd::Commit { resp }).await;
        // Worker has returned after committing; drop closes the (now idle)
        // channel and joins the thread.
        result
    }
}

fn txn_worker(
    db_path: PathBuf,
    receiver: mpsc::Receiver<TxnCmd>,
    ready: tokio::sync::oneshot::Sender<Result<()>>,
) {
    let mut conn = match open_txn_connection(&db_path) {
        Ok(conn) => conn,
        Err(e) => {
            let _ = ready.send(Err(e));
            return;
        }
    };
    let txn = match conn.transaction() {
        Ok(txn) => txn,
        Err(e) => {
            let _ = ready.send(Err(anyhow!("failed to begin transaction: {e}")));
            return;
        }
    };
    if ready.send(Ok(())).is_err() {
        return; // caller gave up; `txn` drops -> rollback
    }

    while let Ok(cmd) = receiver.recv() {
        match cmd {
            TxnCmd::Execute { sql, params, resp } => {
                let _ = resp.send(run_execute(&txn, &sql, &params).map_err(|e| anyhow!("{e}")));
            }
            TxnCmd::ExecuteBatch { sql, resp } => {
                let _ = resp.send(txn.execute_batch(&sql).map_err(|e| anyhow!("{e}")));
            }
            TxnCmd::Query { sql, params, resp } => {
                let _ = resp.send(run_query(&txn, &sql, &params).map_err(|e| anyhow!("{e}")));
            }
            TxnCmd::LastInsertRowid { resp } => {
                let _ = resp.send(Ok(txn.last_insert_rowid()));
            }
            TxnCmd::Commit { resp } => {
                let result = txn.commit().map_err(|e| anyhow!("{e}"));
                let _ = resp.send(result);
                return;
            }
        }
    }
    // Channel closed without Commit: drop `txn` -> rollback.
}

fn open_txn_connection(db_path: &Path) -> Result<rusqlite::Connection> {
    let conn = rusqlite::Connection::open(db_path)
        .map_err(|e| anyhow!("failed to open transaction connection: {e}"))?;
    conn.pragma_update(None, "foreign_keys", "ON")
        .map_err(|e| anyhow!("{e}"))?;
    conn.pragma_update(None, "busy_timeout", "5000")
        .map_err(|e| anyhow!("{e}"))?;
    Ok(conn)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::history::db::db_params;

    async fn open_writer() -> (SqliteWriteConn, tempfile::TempDir) {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("facade-test.db");
        let conn = tokio_rusqlite::Connection::open(&path)
            .await
            .expect("open conn");
        let writer = SqliteWriteConn::new(conn, path);
        writer
            .execute_batch("CREATE TABLE t (id INTEGER PRIMARY KEY, name TEXT, score REAL)")
            .await
            .expect("create table");
        (writer, dir)
    }

    #[tokio::test]
    async fn execute_and_query_roundtrip() {
        let (db, _dir) = open_writer().await;
        let affected = db
            .execute(
                "INSERT INTO t (id, name, score) VALUES (?1, ?2, ?3)",
                db_params![1_i64, "alice", 9.5_f64],
            )
            .await
            .unwrap();
        assert_eq!(affected, 1);

        let rows = db
            .query("SELECT id, name, score FROM t ORDER BY id", Params::None)
            .await
            .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].get::<i64>(0).unwrap(), 1);
        assert_eq!(rows[0].get::<String>(1).unwrap(), "alice");
        assert!((rows[0].get::<f64>(2).unwrap() - 9.5).abs() < f64::EPSILON);

        let one = db
            .query_opt("SELECT name FROM t WHERE id = ?1", db_params![1_i64])
            .await
            .unwrap();
        assert_eq!(one.unwrap().get::<String>(0).unwrap(), "alice");
        let none = db
            .query_opt("SELECT name FROM t WHERE id = ?1", db_params![99_i64])
            .await
            .unwrap();
        assert!(none.is_none());
    }

    #[tokio::test]
    async fn null_maps_to_option_none() {
        let (db, _dir) = open_writer().await;
        db.execute(
            "INSERT INTO t (id, name, score) VALUES (?1, ?2, ?3)",
            db_params![1_i64, Option::<String>::None, Option::<f64>::None],
        )
        .await
        .unwrap();
        let rows = db
            .query("SELECT name, score FROM t", Params::None)
            .await
            .unwrap();
        assert!(rows[0].get::<Option<String>>(0).unwrap().is_none());
        assert!(rows[0].get::<Option<f64>>(1).unwrap().is_none());
    }

    #[tokio::test]
    async fn transaction_commit_persists() {
        let (db, _dir) = open_writer().await;
        let mut txn = db.transaction().await.unwrap();
        txn.execute(
            "INSERT INTO t (id, name) VALUES (?1, ?2)",
            db_params![1_i64, "a"],
        )
        .await
        .unwrap();
        txn.execute(
            "INSERT INTO t (id, name) VALUES (?1, ?2)",
            db_params![2_i64, "b"],
        )
        .await
        .unwrap();
        // Read-your-writes inside the transaction.
        let mid = txn
            .query("SELECT COUNT(*) FROM t", Params::None)
            .await
            .unwrap();
        assert_eq!(mid[0].get::<i64>(0).unwrap(), 2);
        txn.commit().await.unwrap();

        let rows = db
            .query("SELECT COUNT(*) FROM t", Params::None)
            .await
            .unwrap();
        assert_eq!(rows[0].get::<i64>(0).unwrap(), 2);
    }

    #[tokio::test]
    async fn transaction_drop_rolls_back() {
        let (db, _dir) = open_writer().await;
        {
            let mut txn = db.transaction().await.unwrap();
            txn.execute(
                "INSERT INTO t (id, name) VALUES (?1, ?2)",
                db_params![1_i64, "a"],
            )
            .await
            .unwrap();
            // Drop without commit.
        }
        let rows = db
            .query("SELECT COUNT(*) FROM t", Params::None)
            .await
            .unwrap();
        assert_eq!(rows[0].get::<i64>(0).unwrap(), 0);
    }
}
