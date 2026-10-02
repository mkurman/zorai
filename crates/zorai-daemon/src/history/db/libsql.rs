#![allow(dead_code)]

//! libSQL engine for the [`DbConn`]/[`DbTxn`] facade. Selected at runtime when
//! the configured database backend is libSQL (local file or, later, an embedded
//! remote replica). Unlike the SQLite engine, libSQL is natively async, so
//! transactions are held directly across `.await` points — no dedicated worker
//! thread is needed.

use std::path::Path;

use anyhow::{anyhow, Result};

use super::{DbConn, DbTxn, Params, Row, Value};

fn to_libsql_value(value: &Value) -> libsql::Value {
    match value {
        Value::Null => libsql::Value::Null,
        Value::Integer(i) => libsql::Value::Integer(*i),
        Value::Real(f) => libsql::Value::Real(*f),
        Value::Text(s) => libsql::Value::Text(s.clone()),
        Value::Blob(b) => libsql::Value::Blob(b.clone()),
    }
}

fn from_libsql_value(value: libsql::Value) -> Value {
    match value {
        libsql::Value::Null => Value::Null,
        libsql::Value::Integer(i) => Value::Integer(i),
        libsql::Value::Real(f) => Value::Real(f),
        libsql::Value::Text(s) => Value::Text(s),
        libsql::Value::Blob(b) => Value::Blob(b),
    }
}

fn to_libsql_params(params: &Params) -> libsql::params::Params {
    libsql::params::Params::Positional(params.values().iter().map(to_libsql_value).collect())
}

async fn collect_rows(mut rows: libsql::Rows) -> Result<Vec<Row>> {
    let column_count = rows.column_count();
    let mut out = Vec::new();
    while let Some(row) = rows.next().await.map_err(|e| anyhow!("{e}"))? {
        let mut values = Vec::with_capacity(column_count as usize);
        for idx in 0..column_count {
            values.push(from_libsql_value(
                row.get_value(idx).map_err(|e| anyhow!("{e}"))?,
            ));
        }
        out.push(Row::new(values));
    }
    Ok(out)
}

/// A libSQL connection backing the facade. libSQL connections are cheap to
/// clone (internally reference-counted), so one of these is shared for the
/// writer and reader facade handles in local mode.
pub(crate) struct LibsqlConn {
    conn: libsql::Connection,
    // Kept for the connection's lifetime; also the handle `sync()` drives for
    // an embedded remote replica.
    db: std::sync::Arc<libsql::Database>,
    is_replica: bool,
}

impl LibsqlConn {
    /// Opens a local-file libSQL database and applies the connection pragmas
    /// the daemon relies on. Pragmas that return a row (e.g. `busy_timeout`)
    /// are issued via `query` because libSQL's `execute` rejects row-returning
    /// statements.
    pub(crate) async fn open_local(path: &Path) -> Result<Self> {
        let db = libsql::Builder::new_local(path)
            .build()
            .await
            .map_err(|e| anyhow!("failed to open libSQL database: {e}"))?;
        Self::from_database(db, false).await
    }

    /// Opens an embedded remote replica: a local file kept in sync with a
    /// libSQL/Turso server at `sync_url`. Reads/writes hit the local file;
    /// [`LibsqlConn::sync`] pulls/pushes changes against the server.
    pub(crate) async fn open_remote_replica(
        path: &Path,
        sync_url: String,
        auth_token: String,
    ) -> Result<Self> {
        let db = libsql::Builder::new_remote_replica(path, sync_url, auth_token)
            .build()
            .await
            .map_err(|e| anyhow!("failed to open libSQL remote replica: {e}"))?;
        Self::from_database(db, true).await
    }

    /// Opens a direct connection to a remote libSQL/Turso server (no local
    /// file, no replication). Used by the one-time `db push` seed to write the
    /// local database's contents up to the server.
    pub(crate) async fn open_remote(url: String, auth_token: String) -> Result<Self> {
        let db = libsql::Builder::new_remote(url, auth_token)
            .build()
            .await
            .map_err(|e| anyhow!("failed to open remote libSQL connection: {e}"))?;
        let conn = db.connect().map_err(|e| anyhow!("{e}"))?;
        Ok(Self {
            conn,
            db: std::sync::Arc::new(db),
            is_replica: false,
        })
    }

    async fn from_database(db: libsql::Database, is_replica: bool) -> Result<Self> {
        let conn = db.connect().map_err(|e| anyhow!("{e}"))?;
        let this = Self {
            conn,
            db: std::sync::Arc::new(db),
            is_replica,
        };
        this.apply_pragmas().await?;
        Ok(this)
    }

    async fn apply_pragmas(&self) -> Result<()> {
        // Value-setting pragmas run via execute; value-returning pragmas
        // (journal_mode, busy_timeout) must go through query.
        let mut set_stmts = vec!["PRAGMA foreign_keys = ON"];
        if !self.is_replica {
            set_stmts.push("PRAGMA synchronous = NORMAL");
        }
        set_stmts.push("PRAGMA temp_store = MEMORY");
        let cache_sql = format!("PRAGMA cache_size = {}", super::SQLITE_PAGE_CACHE_KIB);
        set_stmts.push(cache_sql.as_str());
        for stmt in set_stmts {
            if let Err(e) = self.conn.execute(stmt, ()).await {
                if self.is_replica {
                    tracing::debug!("skipping unsupported libSQL replica pragma `{stmt}`: {e}");
                } else {
                    return Err(anyhow!("failed to apply libSQL pragma `{stmt}`: {e}"));
                }
            }
        }

        let mut query_stmts = Vec::new();
        if !self.is_replica {
            query_stmts.push("PRAGMA journal_mode = WAL");
        }
        query_stmts.push("PRAGMA busy_timeout = 5000");
        for stmt in query_stmts {
            match self.conn.query(stmt, ()).await {
                Ok(mut rows) => {
                    let _ = rows.next().await;
                }
                Err(e) => {
                    if self.is_replica {
                        tracing::debug!("skipping unsupported libSQL replica pragma `{stmt}`: {e}");
                    } else {
                        return Err(anyhow!("failed to apply libSQL pragma `{stmt}`: {e}"));
                    }
                }
            }
        }
        Ok(())
    }

    pub(crate) fn connection(&self) -> libsql::Connection {
        self.conn.clone()
    }
}

#[async_trait::async_trait]
impl DbConn for LibsqlConn {
    async fn execute(&self, sql: &str, params: Params) -> Result<u64> {
        self.conn
            .execute(sql, to_libsql_params(&params))
            .await
            .map_err(|e| anyhow!("{e}"))
    }

    async fn execute_batch(&self, sql: &str) -> Result<()> {
        self.conn
            .execute_batch(sql)
            .await
            .map_err(|e| anyhow!("{e}"))?;
        Ok(())
    }

    async fn query(&self, sql: &str, params: Params) -> Result<Vec<Row>> {
        let rows = self
            .conn
            .query(sql, to_libsql_params(&params))
            .await
            .map_err(|e| anyhow!("{e}"))?;
        collect_rows(rows).await
    }

    async fn last_insert_rowid(&self) -> Result<i64> {
        Ok(self.conn.last_insert_rowid())
    }

    async fn sync(&self) -> Result<()> {
        if self.is_replica {
            self.db
                .sync()
                .await
                .map_err(|e| anyhow!("libSQL replica sync failed: {e}"))?;
        }
        Ok(())
    }

    async fn execute_returning_rowid(&self, sql: &str, params: Params) -> Result<i64> {
        self.conn
            .execute(sql, to_libsql_params(&params))
            .await
            .map_err(|e| anyhow!("{e}"))?;
        Ok(self.conn.last_insert_rowid())
    }

    async fn transaction(&self) -> Result<Box<dyn DbTxn>> {
        let txn = self.conn.transaction().await.map_err(|e| anyhow!("{e}"))?;
        Ok(Box::new(LibsqlTxn { txn: Some(txn) }))
    }
}

/// A libSQL transaction. Holds the `libsql::Transaction` directly across awaits;
/// dropping without [`DbTxn::commit`] rolls back.
pub(crate) struct LibsqlTxn {
    txn: Option<libsql::Transaction>,
}

impl LibsqlTxn {
    fn txn(&self) -> Result<&libsql::Transaction> {
        self.txn
            .as_ref()
            .ok_or_else(|| anyhow!("transaction already finished"))
    }
}

#[async_trait::async_trait]
impl DbTxn for LibsqlTxn {
    async fn execute(&mut self, sql: &str, params: Params) -> Result<u64> {
        self.txn()?
            .execute(sql, to_libsql_params(&params))
            .await
            .map_err(|e| anyhow!("{e}"))
    }

    async fn execute_batch(&mut self, sql: &str) -> Result<()> {
        self.txn()?
            .execute_batch(sql)
            .await
            .map_err(|e| anyhow!("{e}"))?;
        Ok(())
    }

    async fn query(&mut self, sql: &str, params: Params) -> Result<Vec<Row>> {
        let rows = self
            .txn()?
            .query(sql, to_libsql_params(&params))
            .await
            .map_err(|e| anyhow!("{e}"))?;
        collect_rows(rows).await
    }

    async fn last_insert_rowid(&mut self) -> Result<i64> {
        Ok(self.txn()?.last_insert_rowid())
    }

    async fn commit(mut self: Box<Self>) -> Result<()> {
        let txn = self
            .txn
            .take()
            .ok_or_else(|| anyhow!("transaction already finished"))?;
        txn.commit().await.map_err(|e| anyhow!("{e}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::history::db::db_params;

    async fn open() -> (LibsqlConn, tempfile::TempDir) {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("libsql-test.db");
        let conn = LibsqlConn::open_local(&path).await.expect("open libsql");
        conn.execute_batch("CREATE TABLE t (id INTEGER PRIMARY KEY, name TEXT, score REAL)")
            .await
            .expect("create table");
        (conn, dir)
    }

    #[tokio::test]
    async fn execute_and_query_roundtrip() {
        let (db, _dir) = open().await;
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
        let (db, _dir) = open().await;
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
    async fn transaction_commit_and_rollback() {
        let (db, _dir) = open().await;
        let mut txn = db.transaction().await.unwrap();
        txn.execute(
            "INSERT INTO t (id, name) VALUES (?1, ?2)",
            db_params![1_i64, "a"],
        )
        .await
        .unwrap();
        let mid = txn
            .query("SELECT COUNT(*) FROM t", Params::None)
            .await
            .unwrap();
        assert_eq!(mid[0].get::<i64>(0).unwrap(), 1);
        txn.commit().await.unwrap();
        let rows = db
            .query("SELECT COUNT(*) FROM t", Params::None)
            .await
            .unwrap();
        assert_eq!(rows[0].get::<i64>(0).unwrap(), 1);

        {
            let mut txn = db.transaction().await.unwrap();
            txn.execute(
                "INSERT INTO t (id, name) VALUES (?1, ?2)",
                db_params![2_i64, "b"],
            )
            .await
            .unwrap();
            // drop without commit -> rollback
        }
        let rows = db
            .query("SELECT COUNT(*) FROM t", Params::None)
            .await
            .unwrap();
        assert_eq!(rows[0].get::<i64>(0).unwrap(), 1);
    }

    #[tokio::test]
    async fn fts5_match_and_bm25() {
        let (db, _dir) = open().await;
        db.execute_batch("CREATE VIRTUAL TABLE history_fts USING fts5(id UNINDEXED, title, body);")
            .await
            .unwrap();
        db.execute(
            "INSERT INTO history_fts(id, title, body) VALUES (?1, ?2, ?3)",
            db_params!["e1", "deploy pipeline", "the quick brown fox"],
        )
        .await
        .unwrap();
        db.execute(
            "INSERT INTO history_fts(id, title, body) VALUES (?1, ?2, ?3)",
            db_params!["e2", "lazy dog", "a lazy dog sleeping"],
        )
        .await
        .unwrap();
        let rows = db
            .query(
                "SELECT id, bm25(history_fts) FROM history_fts WHERE history_fts MATCH ?1 ORDER BY bm25(history_fts)",
                db_params!["quick"],
            )
            .await
            .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].get::<String>(0).unwrap(), "e1");
    }

    #[tokio::test]
    async fn remote_write_then_replica_sync_roundtrip_against_live_server() {
        let Ok(url) = std::env::var("ZORAI_LIBSQL_TEST_URL") else {
            return;
        };
        let token = std::env::var("ZORAI_LIBSQL_TEST_TOKEN").unwrap_or_default();

        let remote = LibsqlConn::open_remote(url.clone(), token.clone())
            .await
            .expect("open_remote against live server");
        remote
            .execute_batch(
                "DROP TABLE IF EXISTS e2e; CREATE TABLE e2e (id INTEGER PRIMARY KEY, v TEXT)",
            )
            .await
            .expect("create remote table");
        remote
            .execute(
                "INSERT INTO e2e (id, v) VALUES (?1, ?2)",
                db_params![1_i64, "handoff"],
            )
            .await
            .expect("insert into remote");

        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("replica.db");
        let replica = LibsqlConn::open_remote_replica(&path, url, token)
            .await
            .expect("open_remote_replica");
        replica.sync().await.expect("replica sync");

        let rows = replica
            .query("SELECT v FROM e2e WHERE id = ?1", db_params![1_i64])
            .await
            .expect("read from synced replica");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].get::<String>(0).unwrap(), "handoff");
    }

    #[tokio::test]
    async fn full_daemon_schema_builds_on_live_server() {
        let Ok(url) = std::env::var("ZORAI_LIBSQL_TEST_URL") else {
            return;
        };
        let token = std::env::var("ZORAI_LIBSQL_TEST_TOKEN").unwrap_or_default();

        let remote = LibsqlConn::open_remote(url, token)
            .await
            .expect("open_remote against live server");
        let dir = tempfile::tempdir().expect("tempdir");
        crate::history::init_schema_on_connection(
            &mut crate::history::db::ConnExecutor(&remote),
            dir.path(),
        )
        .await
        .expect("init full daemon schema on live libSQL server");

        let rows = remote
            .query(
                "SELECT name FROM sqlite_master WHERE type IN ('table','view') AND name IN ('history_entries', 'episodes', 'agent_threads', 'agent_config_items')",
                Params::None,
            )
            .await
            .expect("read schema tables from live server");
        assert!(
            rows.len() >= 4,
            "expected core daemon tables to exist on the live server, found {}",
            rows.len()
        );

        let fts = remote
            .query(
                "SELECT name FROM sqlite_master WHERE name LIKE '%_fts'",
                Params::None,
            )
            .await
            .expect("query FTS tables on live server");
        assert!(
            !fts.is_empty(),
            "expected at least one FTS5 virtual table to build on the live server"
        );
    }
}
