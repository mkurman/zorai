use super::*;

fn request(path: &Path, op: &str) -> DataQueryRequest {
    DataQueryRequest {
        request_id: Some("req-1".to_string()),
        op: op.to_string(),
        path: path.display().to_string(),
        sheet: None,
        offset: None,
        limit: None,
        sort_column: None,
        sort_direction: None,
        search: None,
    }
}

fn write_parquet(dir: &Path) -> PathBuf {
    let path = dir.join("cases.parquet");
    let connection = Connection::open_in_memory().unwrap();
    connection
        .execute_batch(&format!(
            "COPY (SELECT range AS id, 'case-' || range AS label, range * 1.5 AS score FROM range(500)) TO {} (FORMAT parquet)",
            sql_string(&path.display().to_string())
        ))
        .unwrap();
    path
}

#[tokio::test]
async fn parquet_pages_are_sorted_and_counted_by_the_engine_not_the_ui() {
    let dir = tempfile::tempdir().unwrap();
    let path = write_parquet(dir.path());
    let mut req = request(&path, "page");
    req.sort_column = Some("score".to_string());
    req.sort_direction = Some("desc".to_string());
    req.offset = Some(10);
    req.limit = Some(5);

    let response = run_data_query(req).await;

    assert!(response.ok, "{:?}", response.error);
    assert_eq!(response.request_id.as_deref(), Some("req-1"));
    assert_eq!(response.format, "parquet");
    assert_eq!(response.total_rows, 500, "the UI needs the full row count to page");
    assert_eq!(response.rows.len(), 5);
    assert_eq!(response.rows[0][0].as_deref(), Some("489"));
    assert_eq!(
        response.columns.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(),
        vec!["id", "label", "score"]
    );
}

#[tokio::test]
async fn search_filters_rows_and_treats_wildcards_literally() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("drugs.csv");
    std::fs::write(&path, "name,dose\nAspirin,100%\nIbuprofen,200\nasp_like,5\n").unwrap();
    let mut req = request(&path, "page");
    req.search = Some("asp".to_string());
    let response = run_data_query(req).await;
    assert!(response.ok, "{:?}", response.error);
    assert_eq!(response.total_rows, 2, "search must be case-insensitive across columns");

    let mut req = request(&path, "page");
    req.search = Some("%".to_string());
    let response = run_data_query(req).await;
    assert_eq!(
        response.total_rows, 1,
        "a literal % must not match every row"
    );
}

#[tokio::test]
async fn jsonl_is_read_as_newline_delimited_records() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("events.jsonl");
    std::fs::write(&path, "{\"a\":1,\"b\":\"x\"}\n{\"a\":2,\"b\":\"y\"}\n").unwrap();
    let response = run_data_query(request(&path, "page")).await;
    assert!(response.ok, "{:?}", response.error);
    assert_eq!(response.total_rows, 2);
    assert_eq!(response.rows[1], vec![Some("2".to_string()), Some("y".to_string())]);
}

#[tokio::test]
async fn spreadsheets_expose_sheets_and_keep_numeric_columns_sortable() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("book.xlsx");
    let mut workbook = rust_xlsxwriter::Workbook::new();
    let first = workbook.add_worksheet().set_name("Summary").unwrap();
    first.write(0, 0, "note").unwrap();
    first.write(1, 0, "hello").unwrap();
    let second = workbook.add_worksheet().set_name("Scores").unwrap();
    second.write(0, 0, "patient").unwrap();
    second.write(0, 1, "score").unwrap();
    for (row, (name, score)) in [("A", 9.0), ("B", 10.0), ("C", 100.0)].iter().enumerate() {
        second.write(row as u32 + 1, 0, *name).unwrap();
        second.write(row as u32 + 1, 1, *score).unwrap();
    }
    workbook.save(&path).unwrap();

    let mut req = request(&path, "page");
    req.sheet = Some("Scores".to_string());
    req.sort_column = Some("score".to_string());
    req.sort_direction = Some("desc".to_string());
    let response = run_data_query(req).await;

    assert!(response.ok, "{:?}", response.error);
    assert_eq!(response.sheets, vec!["Summary", "Scores"]);
    assert_eq!(response.sheet.as_deref(), Some("Scores"));
    assert_eq!(response.columns[1].data_type, "DOUBLE");
    assert_eq!(
        response.rows.iter().map(|row| row[0].clone().unwrap()).collect::<Vec<_>>(),
        vec!["C", "B", "A"],
        "numbers from Excel must sort numerically, not as text"
    );
}

#[tokio::test]
async fn summarize_returns_per_column_statistics() {
    let dir = tempfile::tempdir().unwrap();
    let path = write_parquet(dir.path());
    let response = run_data_query(request(&path, "summarize")).await;
    assert!(response.ok, "{:?}", response.error);
    assert_eq!(response.rows.len(), 3, "one stats row per source column");
    assert_eq!(response.columns[0].name, "column_name");
}

#[tokio::test]
async fn failures_are_reported_with_the_request_id_instead_of_dropped() {
    let response = run_data_query(DataQueryRequest {
        path: "/definitely/missing.parquet".to_string(),
        ..request(Path::new("x"), "page")
    })
    .await;
    assert!(!response.ok);
    assert_eq!(response.request_id.as_deref(), Some("req-1"));
    assert!(response.error.unwrap().contains("cannot open"));

    let response = run_data_query(request(Path::new("https://example.com/a.csv"), "page")).await;
    assert!(!response.ok, "remote URLs must not be fetched by the daemon");

    let dir = tempfile::tempdir().unwrap();
    let path = write_parquet(dir.path());
    let mut req = request(&path, "page");
    req.sort_column = Some("score; DROP TABLE x".to_string());
    let response = run_data_query(req).await;
    assert!(!response.ok, "sort columns must be validated against the schema");
}
