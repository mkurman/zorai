#![allow(dead_code)]

//! Engine-neutral async database facade.
//!
//! The history layer historically called `rusqlite` synchronously inside
//! `tokio_rusqlite` `.call(|conn| ...)` closures. To support an alternative
//! engine (libSQL/Turso, whose Rust API is async and incompatible with
//! `rusqlite`'s synchronous closures) the storage layer is being moved behind
//! the [`DbConn`]/[`DbTxn`] traits defined here. Signatures use only owned,
//! engine-neutral types ([`Value`], [`Params`], [`Row`]) so neither
//! `rusqlite::Row` nor `libsql::Rows` leaks into call sites.

use anyhow::{anyhow, Result};

/// Per-connection page cache, as a negative KiB `cache_size` (4 MiB).
/// The history store opens 14 connections. 64 MiB each reserved about 900 MiB
/// before any query ran, and those pages stayed resident while idle.
pub(crate) const SQLITE_PAGE_CACHE_KIB: &str = "-4096";
/// Per-connection mmap window. 256 MiB times the reader pool mapped far more
/// of the database than an idle daemon needs.
pub(crate) const SQLITE_MMAP_BYTES: &str = "16777216";

pub(crate) mod libsql;
pub(crate) mod sqlite;

/// A single SQL value, mirroring SQLite's storage classes.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Value {
    Null,
    Integer(i64),
    Real(f64),
    Text(String),
    Blob(Vec<u8>),
}

/// Bound statement parameters. Positional only — every current call site uses
/// positional `?N` placeholders.
#[derive(Debug, Clone)]
pub(crate) enum Params {
    None,
    Positional(Vec<Value>),
}

impl Params {
    fn values(&self) -> &[Value] {
        match self {
            Params::None => &[],
            Params::Positional(values) => values,
        }
    }
}

/// Converts a Rust value into a [`Value`] for parameter binding.
pub(crate) trait IntoValue {
    fn into_value(self) -> Value;
}

impl IntoValue for Value {
    fn into_value(self) -> Value {
        self
    }
}
impl IntoValue for i64 {
    fn into_value(self) -> Value {
        Value::Integer(self)
    }
}
impl IntoValue for i32 {
    fn into_value(self) -> Value {
        Value::Integer(self as i64)
    }
}
impl IntoValue for u32 {
    fn into_value(self) -> Value {
        Value::Integer(self as i64)
    }
}
impl IntoValue for u64 {
    fn into_value(self) -> Value {
        Value::Integer(self as i64)
    }
}
impl IntoValue for usize {
    fn into_value(self) -> Value {
        Value::Integer(self as i64)
    }
}
impl IntoValue for bool {
    fn into_value(self) -> Value {
        Value::Integer(self as i64)
    }
}
impl IntoValue for f64 {
    fn into_value(self) -> Value {
        Value::Real(self)
    }
}
impl IntoValue for String {
    fn into_value(self) -> Value {
        Value::Text(self)
    }
}
impl IntoValue for &str {
    fn into_value(self) -> Value {
        Value::Text(self.to_string())
    }
}
impl IntoValue for &String {
    fn into_value(self) -> Value {
        Value::Text(self.clone())
    }
}
impl IntoValue for Vec<u8> {
    fn into_value(self) -> Value {
        Value::Blob(self)
    }
}
impl<T: IntoValue> IntoValue for Option<T> {
    fn into_value(self) -> Value {
        match self {
            Some(inner) => inner.into_value(),
            None => Value::Null,
        }
    }
}

/// Builds [`Params`], mirroring `rusqlite::params!`.
macro_rules! db_params {
    () => {
        $crate::history::db::Params::None
    };
    ($($value:expr),+ $(,)?) => {
        $crate::history::db::Params::Positional(vec![
            $($crate::history::db::IntoValue::into_value($value)),+
        ])
    };
}
pub(crate) use db_params;

/// One materialized result row. Columns are read by zero-based index via
/// [`Row::get`].
#[derive(Debug, Clone)]
pub(crate) struct Row {
    values: Vec<Value>,
}

impl Row {
    pub(crate) fn new(values: Vec<Value>) -> Self {
        Self { values }
    }

    pub(crate) fn len(&self) -> usize {
        self.values.len()
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    /// Borrows the raw stored values, for callers that need dynamic per-cell
    /// access without a known target type (e.g. the database viewer rendering
    /// arbitrary query results).
    pub(crate) fn values(&self) -> &[Value] {
        &self.values
    }

    /// Reads column `idx`, converting to `T`. Errors if the index is out of
    /// range or the stored value is not convertible to `T`.
    pub(crate) fn get<T: FromValue>(&self, idx: usize) -> Result<T> {
        let value = self.values.get(idx).ok_or_else(|| {
            anyhow!(
                "column index {idx} out of range (row has {} columns)",
                self.values.len()
            )
        })?;
        T::from_value(value)
    }
}

/// Converts a stored [`Value`] into a Rust type. `Option<T>` maps SQL NULL to
/// `None`; non-optional getters error on NULL, matching `rusqlite`'s behavior.
pub(crate) trait FromValue: Sized {
    fn from_value(value: &Value) -> Result<Self>;
}

fn type_err(expected: &str, value: &Value) -> anyhow::Error {
    anyhow!("expected {expected}, found {value:?}")
}

impl FromValue for i64 {
    fn from_value(value: &Value) -> Result<Self> {
        match value {
            Value::Integer(i) => Ok(*i),
            other => Err(type_err("integer", other)),
        }
    }
}
impl FromValue for i32 {
    fn from_value(value: &Value) -> Result<Self> {
        Ok(i64::from_value(value)? as i32)
    }
}
impl FromValue for u32 {
    fn from_value(value: &Value) -> Result<Self> {
        Ok(i64::from_value(value)? as u32)
    }
}
impl FromValue for u64 {
    fn from_value(value: &Value) -> Result<Self> {
        Ok(i64::from_value(value)? as u64)
    }
}
impl FromValue for usize {
    fn from_value(value: &Value) -> Result<Self> {
        Ok(i64::from_value(value)? as usize)
    }
}
impl FromValue for bool {
    fn from_value(value: &Value) -> Result<Self> {
        Ok(i64::from_value(value)? != 0)
    }
}
impl FromValue for f64 {
    fn from_value(value: &Value) -> Result<Self> {
        match value {
            Value::Real(f) => Ok(*f),
            Value::Integer(i) => Ok(*i as f64),
            other => Err(type_err("real", other)),
        }
    }
}
impl FromValue for String {
    fn from_value(value: &Value) -> Result<Self> {
        match value {
            Value::Text(s) => Ok(s.clone()),
            other => Err(type_err("text", other)),
        }
    }
}
impl FromValue for Vec<u8> {
    fn from_value(value: &Value) -> Result<Self> {
        match value {
            Value::Blob(b) => Ok(b.clone()),
            Value::Text(s) => Ok(s.as_bytes().to_vec()),
            other => Err(type_err("blob", other)),
        }
    }
}
impl<T: FromValue> FromValue for Option<T> {
    fn from_value(value: &Value) -> Result<Self> {
        match value {
            Value::Null => Ok(None),
            other => Ok(Some(T::from_value(other)?)),
        }
    }
}

/// An async database connection. Implemented per engine (SQLite today, libSQL
/// next). All query results are materialized into `Vec<Row>` — no call site
/// streams lazily across an open statement.
#[async_trait::async_trait]
pub(crate) trait DbConn: Send + Sync {
    async fn execute(&self, sql: &str, params: Params) -> Result<u64>;
    async fn execute_batch(&self, sql: &str) -> Result<()>;
    async fn query(&self, sql: &str, params: Params) -> Result<Vec<Row>>;
    /// Like [`DbConn::query`] but also returns the result column names, for
    /// callers rendering arbitrary SQL (the database viewer). The default
    /// returns empty column names alongside the rows; the SQLite engine
    /// overrides it to report the real names.
    async fn query_with_columns(
        &self,
        sql: &str,
        params: Params,
    ) -> Result<(Vec<String>, Vec<Row>)> {
        Ok((Vec::new(), self.query(sql, params).await?))
    }
    /// Returns the connection's cumulative changed-row counter (SQLite's
    /// `total_changes`). Used by the database viewer to report rows affected by
    /// arbitrary multi-statement SQL.
    async fn total_changes(&self) -> Result<u64> {
        Err(anyhow!("total_changes is not supported on this connection"))
    }
    /// Syncs an embedded remote replica against its server. Default no-op for
    /// engines/connections without replication (SQLite, local libSQL, readers).
    async fn sync(&self) -> Result<()> {
        Ok(())
    }
    async fn query_opt(&self, sql: &str, params: Params) -> Result<Option<Row>> {
        let mut rows = self.query(sql, params).await?;
        Ok(if rows.is_empty() {
            None
        } else {
            Some(rows.swap_remove(0))
        })
    }
    async fn last_insert_rowid(&self) -> Result<i64>;
    /// Runs an INSERT and returns the generated rowid atomically. The default
    /// is `execute` followed by `last_insert_rowid`, which is only safe when no
    /// other write can interleave on the same connection; engines override this
    /// to run both as a single unit (the SQLite engine does the insert and
    /// rowid read inside one connection job).
    async fn execute_returning_rowid(&self, sql: &str, params: Params) -> Result<i64> {
        self.execute(sql, params).await?;
        self.last_insert_rowid().await
    }
    /// Begins a transaction. The returned guard runs statements atomically;
    /// dropping it without [`DbTxn::commit`] rolls back.
    async fn transaction(&self) -> Result<Box<dyn DbTxn>>;
}

/// An open transaction. Mirrors [`DbConn`] but commits explicitly; drop = rollback.
#[async_trait::async_trait]
pub(crate) trait DbTxn: Send {
    async fn execute(&mut self, sql: &str, params: Params) -> Result<u64>;
    async fn execute_batch(&mut self, sql: &str) -> Result<()>;
    async fn query(&mut self, sql: &str, params: Params) -> Result<Vec<Row>>;
    async fn query_opt(&mut self, sql: &str, params: Params) -> Result<Option<Row>> {
        let mut rows = self.query(sql, params).await?;
        Ok(if rows.is_empty() {
            None
        } else {
            Some(rows.swap_remove(0))
        })
    }
    async fn last_insert_rowid(&mut self) -> Result<i64>;
    /// Runs an INSERT and returns the generated rowid. Atomic because a
    /// transaction's statements are serialized on its own worker.
    async fn execute_returning_rowid(&mut self, sql: &str, params: Params) -> Result<i64> {
        self.execute(sql, params).await?;
        self.last_insert_rowid().await
    }
    async fn commit(self: Box<Self>) -> Result<()>;
}

/// Unifies [`DbConn`] (standalone) and [`DbTxn`] (in-transaction) so shared
/// helpers can run against either. `rusqlite` got this for free via
/// `Transaction: Deref<Target = Connection>`; the facade has no such deref, so
/// helpers take `&mut impl DbExecutor` instead of `&rusqlite::Connection`.
#[async_trait::async_trait]
pub(crate) trait DbExecutor: Send {
    async fn execute(&mut self, sql: &str, params: Params) -> Result<u64>;
    async fn execute_batch(&mut self, sql: &str) -> Result<()>;
    async fn query(&mut self, sql: &str, params: Params) -> Result<Vec<Row>>;
    async fn query_opt(&mut self, sql: &str, params: Params) -> Result<Option<Row>>;
    async fn execute_returning_rowid(&mut self, sql: &str, params: Params) -> Result<i64>;
    async fn last_insert_rowid(&mut self) -> Result<i64>;
}

#[async_trait::async_trait]
impl DbExecutor for dyn DbTxn + '_ {
    async fn execute(&mut self, sql: &str, params: Params) -> Result<u64> {
        DbTxn::execute(self, sql, params).await
    }
    async fn execute_batch(&mut self, sql: &str) -> Result<()> {
        DbTxn::execute_batch(self, sql).await
    }
    async fn query(&mut self, sql: &str, params: Params) -> Result<Vec<Row>> {
        DbTxn::query(self, sql, params).await
    }
    async fn query_opt(&mut self, sql: &str, params: Params) -> Result<Option<Row>> {
        DbTxn::query_opt(self, sql, params).await
    }
    async fn execute_returning_rowid(&mut self, sql: &str, params: Params) -> Result<i64> {
        DbTxn::execute_returning_rowid(self, sql, params).await
    }
    async fn last_insert_rowid(&mut self) -> Result<i64> {
        DbTxn::last_insert_rowid(self).await
    }
}

/// Adapts a shared `&dyn DbConn` into a [`DbExecutor`] for standalone
/// (non-transactional) shared-helper calls.
pub(crate) struct ConnExecutor<'a>(pub(crate) &'a dyn DbConn);

#[async_trait::async_trait]
impl DbExecutor for ConnExecutor<'_> {
    async fn execute(&mut self, sql: &str, params: Params) -> Result<u64> {
        self.0.execute(sql, params).await
    }
    async fn execute_batch(&mut self, sql: &str) -> Result<()> {
        self.0.execute_batch(sql).await
    }
    async fn query(&mut self, sql: &str, params: Params) -> Result<Vec<Row>> {
        self.0.query(sql, params).await
    }
    async fn query_opt(&mut self, sql: &str, params: Params) -> Result<Option<Row>> {
        self.0.query_opt(sql, params).await
    }
    async fn execute_returning_rowid(&mut self, sql: &str, params: Params) -> Result<i64> {
        self.0.execute_returning_rowid(sql, params).await
    }
    async fn last_insert_rowid(&mut self) -> Result<i64> {
        self.0.last_insert_rowid().await
    }
}

/// Error for methods an engine does not support (e.g. read pools do not begin
/// transactions).
pub(crate) fn unsupported(what: &str) -> anyhow::Error {
    anyhow!("{what} is not supported on this connection")
}
