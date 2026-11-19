use super::*;

const UI_COLOR_KINDS: [&str; 2] = ["thread", "root"];

pub(crate) fn valid_ui_color(kind: &str, key: &str, color: Option<&str>) -> bool {
    UI_COLOR_KINDS.contains(&kind)
        && !key.trim().is_empty()
        && key.len() <= 4096
        && color.is_none_or(|color| {
            !color.is_empty()
                && color.len() <= 32
                && color
                    .chars()
                    .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '#')
        })
}

impl HistoryStore {
    pub(crate) async fn set_ui_color(
        &self,
        kind: &str,
        key: &str,
        color: Option<&str>,
    ) -> Result<()> {
        if !valid_ui_color(kind, key, color) {
            anyhow::bail!("invalid ui color for {kind}");
        }
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|elapsed| elapsed.as_millis() as i64)
            .unwrap_or_default();
        match color {
            Some(color) => {
                self.conn_db
                    .execute(
                        "INSERT INTO ui_colors (kind, key, color, updated_at)
                             VALUES (?1, ?2, ?3, ?4)
                             ON CONFLICT(kind, key) DO UPDATE SET
                                color = excluded.color,
                                updated_at = excluded.updated_at",
                        db::db_params![kind, key, color, now_ms],
                    )
                    .await?;
            }
            None => {
                self.conn_db
                    .execute(
                        "DELETE FROM ui_colors WHERE kind = ?1 AND key = ?2",
                        db::db_params![kind, key],
                    )
                    .await?;
            }
        }
        Ok(())
    }

    pub(crate) async fn list_ui_colors(&self) -> Result<serde_json::Value> {
        let rows = self
            .interactive_read_db
            .query(
                "SELECT kind, key, color FROM ui_colors ORDER BY kind, key",
                db::db_params![],
            )
            .await?;
        let mut colors = serde_json::Map::new();
        for kind in UI_COLOR_KINDS {
            colors.insert(
                kind.to_string(),
                serde_json::Value::Object(serde_json::Map::new()),
            );
        }
        for row in &rows {
            let (Ok(kind), Ok(key), Ok(color)) = (
                row.get::<String>(0),
                row.get::<String>(1),
                row.get::<String>(2),
            ) else {
                continue;
            };
            if let Some(serde_json::Value::Object(entries)) = colors.get_mut(&kind) {
                entries.insert(key, serde_json::Value::String(color));
            }
        }
        Ok(serde_json::Value::Object(colors))
    }
}

#[cfg(test)]
mod tests {
    use super::valid_ui_color;

    #[test]
    fn ui_colors_accept_only_known_kinds_and_plain_color_names() {
        assert!(valid_ui_color("thread", "thread-1", Some("teal")));
        assert!(valid_ui_color("root", "/repo", None));
        assert!(!valid_ui_color("workspace", "x", Some("teal")));
        assert!(!valid_ui_color("thread", "  ", Some("teal")));
        assert!(!valid_ui_color("thread", "t", Some("red; drop table")));
        assert!(!valid_ui_color("thread", "t", Some("")));
    }
}
