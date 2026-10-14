use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

const DATA_PREVIEW_EXTENSIONS: &[&str] = &[
    "csv", "tsv", "tab", "jsonl", "ndjson", "parquet", "pq", "xlsx", "xlsm", "xlsb", "xls", "ods",
];
const MAX_CELL_WIDTH: usize = 32;
pub(crate) const DATA_PREVIEW_ROWS: u64 = 200;

pub(crate) fn is_data_preview_path(path: &str) -> bool {
    std::path::Path::new(path)
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            DATA_PREVIEW_EXTENSIONS.contains(&extension.to_ascii_lowercase().as_str())
        })
}

fn clip(value: &str) -> String {
    let single_line = value.replace(['\n', '\r', '\t'], " ");
    if single_line.width() <= MAX_CELL_WIDTH {
        return single_line;
    }
    let mut out = String::new();
    let mut width = 0;
    for ch in single_line.chars() {
        let ch_width = ch.width().unwrap_or(0);
        if width + ch_width > MAX_CELL_WIDTH - 1 {
            break;
        }
        out.push(ch);
        width += ch_width;
    }
    out.push('…');
    out
}

fn pad(value: &str, width: usize) -> String {
    let padding = width.saturating_sub(value.width());
    format!("{value}{}", " ".repeat(padding))
}

pub(crate) fn render_data_query_preview(result_json: &str) -> (String, String) {
    let value: serde_json::Value = serde_json::from_str(result_json).unwrap_or_default();
    let path = value["path"].as_str().unwrap_or_default().to_string();
    if value["ok"].as_bool() != Some(true) {
        let error = value["error"].as_str().unwrap_or("data query failed");
        return (path, format!("Data preview failed: {error}"));
    }
    let columns = value["columns"]
        .as_array()
        .map(|columns| {
            columns
                .iter()
                .map(|column| {
                    (
                        column["name"].as_str().unwrap_or_default().to_string(),
                        column["data_type"].as_str().unwrap_or_default().to_string(),
                    )
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let rows = value["rows"]
        .as_array()
        .map(|rows| {
            rows.iter()
                .map(|row| {
                    row.as_array()
                        .map(|cells| {
                            cells
                                .iter()
                                .map(|cell| clip(cell.as_str().unwrap_or("null")))
                                .collect::<Vec<_>>()
                        })
                        .unwrap_or_default()
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let total_rows = value["total_rows"].as_u64().unwrap_or(rows.len() as u64);

    let headers = columns
        .iter()
        .map(|(name, _)| clip(name))
        .collect::<Vec<_>>();
    let types = columns
        .iter()
        .map(|(_, data_type)| clip(data_type))
        .collect::<Vec<_>>();
    let widths = (0..columns.len())
        .map(|index| {
            rows.iter()
                .filter_map(|row| row.get(index))
                .map(|cell| cell.width())
                .chain([headers[index].width(), types[index].width()])
                .max()
                .unwrap_or(0)
        })
        .collect::<Vec<_>>();
    let line = |cells: &[String]| {
        cells
            .iter()
            .zip(&widths)
            .map(|(cell, width)| pad(cell, *width))
            .collect::<Vec<_>>()
            .join(" │ ")
            .trim_end()
            .to_string()
    };

    let mut out = vec![format!(
        "{} · {} rows × {} columns{}",
        value["format"].as_str().unwrap_or("data"),
        total_rows,
        columns.len(),
        if total_rows > rows.len() as u64 {
            format!(" (showing first {})", rows.len())
        } else {
            String::new()
        }
    )];
    if let Some(sheets) = value["sheets"].as_array().filter(|sheets| sheets.len() > 1) {
        let names = sheets
            .iter()
            .filter_map(|sheet| sheet.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        out.push(format!(
            "sheet: {} (sheets: {names})",
            value["sheet"].as_str().unwrap_or_default()
        ));
    }
    out.push(String::new());
    out.push(line(&headers));
    out.push(line(&types));
    out.push(
        widths
            .iter()
            .map(|width| "─".repeat(*width))
            .collect::<Vec<_>>()
            .join("─┼─"),
    );
    out.extend(rows.iter().map(|row| line(row)));
    (path, out.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_files_render_as_aligned_tables_with_counts() {
        let json = serde_json::json!({
            "ok": true,
            "path": "/d/cases.parquet",
            "format": "parquet",
            "columns": [{"name": "id", "data_type": "BIGINT"}, {"name": "label", "data_type": "VARCHAR"}],
            "rows": [["1", "short"], ["22", null]],
            "total_rows": 500,
            "sheets": [],
        })
        .to_string();
        let (path, content) = render_data_query_preview(&json);
        assert_eq!(path, "/d/cases.parquet");
        let lines = content.lines().collect::<Vec<_>>();
        assert_eq!(lines[0], "parquet · 500 rows × 2 columns (showing first 2)");
        assert_eq!(lines[2], "id     │ label");
        assert_eq!(lines[3], "BIGINT │ VARCHAR");
        assert_eq!(lines[5], "1      │ short");
        assert_eq!(lines[6], "22     │ null", "nulls must be visible, not blank");
    }

    #[test]
    fn failures_are_shown_in_the_preview_instead_of_an_empty_pane() {
        let (_, content) =
            render_data_query_preview(r#"{"ok":false,"path":"/x.parquet","error":"cannot open"}"#);
        assert_eq!(content, "Data preview failed: cannot open");
    }

    #[test]
    fn only_known_data_extensions_use_the_engine() {
        assert!(is_data_preview_path("/d/Report.XLSX"));
        assert!(!is_data_preview_path("/d/notes.md"));
        assert!(!is_data_preview_path("/d/config.json"));
    }
}
