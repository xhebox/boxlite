//! Migration v8 → v9: replace persisted `auto_remove` with `auto_delete`.

use std::path::Path;

use rusqlite::Connection;

use boxlite_shared::errors::BoxliteResult;

use super::{Migration, db_err};

pub(crate) struct ReplaceAutoRemove;

impl Migration for ReplaceAutoRemove {
    fn source_version(&self) -> i32 {
        8
    }

    fn target_version(&self) -> i32 {
        9
    }

    fn description(&self) -> &str {
        "Replace auto_remove with auto_delete in box configs"
    }

    fn run(&self, conn: &Connection, _home_dir: Option<&Path>) -> BoxliteResult<()> {
        let updated = db_err!(conn.execute(
            r#"UPDATE box_config
               SET json = json_remove(
                   json_set(
                       json,
                       '$.options.auto_delete',
                       COALESCE(
                           json_extract(json, '$.options.auto_delete'),
                           CASE json_type(json, '$.options.auto_remove')
                               WHEN 'false' THEN 0
                               ELSE 1
                           END
                       )
                   ),
                   '$.options.auto_remove'
               )
               WHERE json_type(json, '$.options.auto_remove') IS NOT NULL"#,
            [],
        ))?;
        tracing::info!("Migrated {updated} box configs: auto_remove → auto_delete");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn setup() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE box_config (id TEXT PRIMARY KEY, name TEXT, created_at INTEGER, json TEXT)",
        )
        .unwrap();
        conn
    }

    fn insert(conn: &Connection, id: &str, json: &str) {
        conn.execute(
            "INSERT INTO box_config VALUES (?1, NULL, 0, ?2)",
            rusqlite::params![id, json],
        )
        .unwrap();
    }

    fn load(conn: &Connection, id: &str) -> Value {
        let raw: String = conn
            .query_row("SELECT json FROM box_config WHERE id = ?1", [id], |row| {
                row.get(0)
            })
            .unwrap();
        serde_json::from_str(&raw).unwrap()
    }

    #[test]
    fn migrates_box_configs_in_one_pass_and_is_idempotent() {
        let conn = setup();
        insert(
            &conn,
            "true",
            r#"{"options":{"auto_remove":true,"auto_delete":null},"marker":1}"#,
        );
        insert(
            &conn,
            "false",
            r#"{"options":{"auto_remove":false},"marker":2}"#,
        );
        insert(
            &conn,
            "explicit",
            r#"{"options":{"auto_remove":false,"auto_delete":60},"marker":3}"#,
        );
        let untouched = r#"{"options":{"auto_delete":0},"marker":4}"#;
        insert(&conn, "untouched", untouched);

        let migration = ReplaceAutoRemove;
        migration.run(&conn, None).unwrap();
        migration.run(&conn, None).unwrap();

        for (id, expected) in [("true", 1), ("false", 0), ("explicit", 60)] {
            let value = load(&conn, id);
            assert_eq!(value["options"]["auto_delete"], expected);
            assert!(value["options"].get("auto_remove").is_none());
        }
        assert_eq!(load(&conn, "true")["marker"], 1);
        let raw: String = conn
            .query_row(
                "SELECT json FROM box_config WHERE id = 'untouched'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(raw, untouched);
    }

    #[test]
    fn maps_non_boolean_auto_remove_to_historical_default() {
        let conn = setup();
        for (id, legacy) in [
            ("string", r#""true""#),
            ("number", "0"),
            ("null", "null"),
            ("object", r#"{}"#),
        ] {
            insert(
                &conn,
                id,
                &format!(r#"{{"options":{{"auto_remove":{legacy}}}}}"#),
            );
        }

        ReplaceAutoRemove.run(&conn, None).unwrap();

        for id in ["string", "number", "null", "object"] {
            let value = load(&conn, id);
            assert_eq!(value["options"]["auto_delete"], 1);
            assert!(value["options"].get("auto_remove").is_none());
        }
    }
}
