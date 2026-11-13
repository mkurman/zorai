import datetime
import decimal
import math
import re

DEFAULT_ROW_LIMIT = 200
MAX_ROW_LIMIT = 5000
READ_ONLY = re.compile(r"^\s*(select|with|from|values|describe|summarize|show|pivot|unpivot)\b", re.IGNORECASE)


def run_query(sql: str, limit=None, folder=None) -> dict:
    import duckdb

    query = (sql or "").strip().rstrip(";").strip()
    if not query:
        raise ValueError("Write a query, for example: SELECT * FROM 'data/*.parquet' LIMIT 10")
    if not READ_ONLY.match(query) or ";" in query:
        raise ValueError("Only one read-only query is allowed (SELECT, WITH, FROM, DESCRIBE, SUMMARIZE, SHOW).")
    limit = max(1, min(int(limit or DEFAULT_ROW_LIMIT), MAX_ROW_LIMIT))
    connection = duckdb.connect()
    try:
        if folder:
            connection.execute("SET file_search_path = ?", [str(folder)])
        if re.match(r"^\s*(describe|summarize|show|pivot|unpivot)\b", query, re.IGNORECASE):
            result = connection.execute(query)
        else:
            result = connection.execute(f"SELECT * FROM ({query}) AS zorai_query LIMIT {limit + 1}")
        columns = [column[0] for column in result.description]
        rows = result.fetchall()
    finally:
        connection.close()
    truncated = len(rows) > limit
    return {"columns": columns, "rows": [[_plain(value) for value in row] for row in rows[:limit]], "truncated": truncated, "limit": limit}


def _plain(value):
    if value is None or isinstance(value, (bool, int, str)):
        return value
    if isinstance(value, float):
        return value if math.isfinite(value) else str(value)
    if isinstance(value, decimal.Decimal):
        return float(value)
    if isinstance(value, (datetime.date, datetime.datetime, datetime.time)):
        return value.isoformat()
    if isinstance(value, (list, tuple)):
        return [_plain(item) for item in value]
    if isinstance(value, dict):
        return {str(key): _plain(item) for key, item in value.items()}
    if isinstance(value, bytes):
        return f"<{len(value)} bytes>"
    return str(value)
