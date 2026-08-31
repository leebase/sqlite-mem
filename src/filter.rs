//! Resolves `ask --where` terms plus status flags into an allowed-memory-id
//! set in one indexed SQL pass, shared by both retrieval legs (rag-ferrite
//! pattern, architecture.md §13: "filters constrain retrieval instead of
//! starving post-fusion results").
//!
//! Grammar (architecture.md §12): repeated `--where` terms are ANDed;
//! `KEY=VALUE` equality, `KEY!=VALUE` exclusion ("no meta row with that
//! key=value" -- a memory either lacking the key entirely or holding a
//! different value both pass), and `KEY=*` key existence (any value).

use crate::error::AppError;
use rusqlite::{params_from_iter, types::Value as SqlValue, Connection};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WhereOp {
    Eq,
    Ne,
    Exists,
}

#[derive(Debug, Clone)]
pub struct WhereTerm {
    pub key: String,
    pub op: WhereOp,
    pub value: Option<String>, // None for `Exists`
}

/// Same charset as `save`'s metadata keys (architecture.md §11: metadata
/// keys are data, never interpolated into SQL -- validated once here, then
/// only ever bound as query parameters).
fn validate_key(key: &str) -> Result<(), AppError> {
    let ok = !key.is_empty()
        && key
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'.' || b == b'-');
    if !ok {
        return Err(AppError::usage(format!(
            "invalid --where key '{key}': metadata keys must match [A-Za-z0-9_.-]+ and be non-empty"
        )));
    }
    Ok(())
}

/// Parses raw `--where` strings into `WhereTerm`s. A term matching neither
/// `KEY!=VALUE` nor `KEY=VALUE`/`KEY=*` is a usage error (architecture.md
/// §12 CLI contract: "Unknown flags/values: exit 2"), not a validation
/// failure -- it is a malformed flag, not oversized/invalid *content*.
pub fn parse_where_terms(raw: &[String]) -> Result<Vec<WhereTerm>, AppError> {
    let mut out = Vec::with_capacity(raw.len());
    for entry in raw {
        if let Some(idx) = entry.find("!=") {
            let key = &entry[..idx];
            let value = &entry[idx + 2..];
            validate_key(key)?;
            out.push(WhereTerm {
                key: key.to_string(),
                op: WhereOp::Ne,
                value: Some(value.to_string()),
            });
            continue;
        }
        let idx = entry.find('=').ok_or_else(|| {
            AppError::usage(format!(
                "--where expects KEY=VALUE, KEY!=VALUE, or KEY=*, got '{entry}'"
            ))
        })?;
        let key = &entry[..idx];
        let value = &entry[idx + 1..];
        validate_key(key)?;
        if value == "*" {
            out.push(WhereTerm {
                key: key.to_string(),
                op: WhereOp::Exists,
                value: None,
            });
        } else {
            out.push(WhereTerm {
                key: key.to_string(),
                op: WhereOp::Eq,
                value: Some(value.to_string()),
            });
        }
    }
    Ok(out)
}

/// Builds `temp.ask_allowed(id)`, containing every memory id that passes
/// the status filter and every ANDed `--where` term, then returns its row
/// count. Both retrieval legs subsequently `JOIN` against this one table
/// instead of re-resolving filters per leg. Safe to call more than once on
/// the same connection (drops any prior `ask_allowed` first).
pub fn resolve_allowed_ids(
    conn: &Connection,
    terms: &[WhereTerm],
    include_superseded: bool,
    include_forgotten: bool,
) -> Result<i64, AppError> {
    conn.execute_batch("DROP TABLE IF EXISTS ask_allowed;")?;

    let mut statuses = vec!["'active'"];
    if include_superseded {
        statuses.push("'superseded'");
    }
    if include_forgotten {
        statuses.push("'forgotten'");
    }

    let mut sql = format!(
        "CREATE TEMP TABLE ask_allowed AS SELECT m.id AS id FROM memories m WHERE m.status IN ({})",
        statuses.join(",")
    );
    let mut params: Vec<SqlValue> = Vec::new();
    for term in terms {
        match term.op {
            WhereOp::Eq => {
                sql.push_str(
                    " AND EXISTS (SELECT 1 FROM memory_meta mm WHERE mm.memory_id = m.id AND mm.key = ? AND mm.value = ?)",
                );
                params.push(SqlValue::Text(term.key.clone()));
                params.push(SqlValue::Text(
                    term.value.clone().expect("Eq term always carries a value"),
                ));
            }
            WhereOp::Ne => {
                sql.push_str(
                    " AND NOT EXISTS (SELECT 1 FROM memory_meta mm WHERE mm.memory_id = m.id AND mm.key = ? AND mm.value = ?)",
                );
                params.push(SqlValue::Text(term.key.clone()));
                params.push(SqlValue::Text(
                    term.value.clone().expect("Ne term always carries a value"),
                ));
            }
            WhereOp::Exists => {
                sql.push_str(
                    " AND EXISTS (SELECT 1 FROM memory_meta mm WHERE mm.memory_id = m.id AND mm.key = ?)",
                );
                params.push(SqlValue::Text(term.key.clone()));
            }
        }
    }

    conn.execute(&sql, params_from_iter(params))?;
    conn.execute_batch("CREATE UNIQUE INDEX idx_ask_allowed_id ON ask_allowed(id);")?;

    let count: i64 = conn.query_row("SELECT COUNT(*) FROM ask_allowed", [], |r| r.get(0))?;
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;
    use rusqlite::params;
    use tempfile::tempdir;

    fn seed(conn: &Connection, id: &str, status: &str, meta: &[(&str, &str)]) {
        conn.execute(
            "INSERT INTO memories (id, content, content_hash, source, created_at, status, superseded_by, forgotten_at)
             VALUES (?1, 'c', 'h', NULL, '2026-01-01T00:00:00Z', ?2, NULL, NULL)",
            params![id, status],
        )
        .unwrap();
        for (k, v) in meta {
            conn.execute(
                "INSERT INTO memory_meta (memory_id, key, value) VALUES (?1, ?2, ?3)",
                params![id, k, v],
            )
            .unwrap();
        }
    }

    fn allowed_ids(conn: &Connection) -> Vec<String> {
        let mut stmt = conn
            .prepare("SELECT id FROM ask_allowed ORDER BY id")
            .unwrap();
        stmt.query_map([], |r| r.get(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap()
    }

    #[test]
    fn equality_filter_matches_only_matching_memories() {
        let dir = tempdir().unwrap();
        let db = Db::open(&dir.path().join("m.db")).unwrap();
        seed(&db.conn, "m1", "active", &[("kind", "decision")]);
        seed(&db.conn, "m2", "active", &[("kind", "note")]);

        let terms = parse_where_terms(&["kind=decision".to_string()]).unwrap();
        resolve_allowed_ids(&db.conn, &terms, false, false).unwrap();
        assert_eq!(allowed_ids(&db.conn), vec!["m1".to_string()]);
    }

    #[test]
    fn not_equal_filter_excludes_matching_value_but_includes_missing_key() {
        let dir = tempdir().unwrap();
        let db = Db::open(&dir.path().join("m.db")).unwrap();
        seed(&db.conn, "m1", "active", &[("kind", "decision")]);
        seed(&db.conn, "m2", "active", &[("kind", "note")]);
        seed(&db.conn, "m3", "active", &[]); // no `kind` key at all

        let terms = parse_where_terms(&["kind!=decision".to_string()]).unwrap();
        resolve_allowed_ids(&db.conn, &terms, false, false).unwrap();
        assert_eq!(
            allowed_ids(&db.conn),
            vec!["m2".to_string(), "m3".to_string()]
        );
    }

    #[test]
    fn existence_filter_matches_any_value_but_not_a_missing_key() {
        let dir = tempdir().unwrap();
        let db = Db::open(&dir.path().join("m.db")).unwrap();
        seed(&db.conn, "m1", "active", &[("kind", "decision")]);
        seed(&db.conn, "m2", "active", &[("kind", "note")]);
        seed(&db.conn, "m3", "active", &[]);

        let terms = parse_where_terms(&["kind=*".to_string()]).unwrap();
        resolve_allowed_ids(&db.conn, &terms, false, false).unwrap();
        assert_eq!(
            allowed_ids(&db.conn),
            vec!["m1".to_string(), "m2".to_string()]
        );
    }

    #[test]
    fn multiple_where_terms_are_anded() {
        let dir = tempdir().unwrap();
        let db = Db::open(&dir.path().join("m.db")).unwrap();
        seed(
            &db.conn,
            "m1",
            "active",
            &[("kind", "decision"), ("project", "factory")],
        );
        seed(
            &db.conn,
            "m2",
            "active",
            &[("kind", "decision"), ("project", "other")],
        );

        let terms =
            parse_where_terms(&["kind=decision".to_string(), "project=factory".to_string()])
                .unwrap();
        resolve_allowed_ids(&db.conn, &terms, false, false).unwrap();
        assert_eq!(allowed_ids(&db.conn), vec!["m1".to_string()]);
    }

    #[test]
    fn status_defaults_to_active_only() {
        let dir = tempdir().unwrap();
        let db = Db::open(&dir.path().join("m.db")).unwrap();
        seed(&db.conn, "m1", "active", &[]);
        seed(&db.conn, "m2", "superseded", &[]);
        seed(&db.conn, "m3", "forgotten", &[]);

        resolve_allowed_ids(&db.conn, &[], false, false).unwrap();
        assert_eq!(allowed_ids(&db.conn), vec!["m1".to_string()]);
    }

    #[test]
    fn include_superseded_and_forgotten_flags_are_additive() {
        let dir = tempdir().unwrap();
        let db = Db::open(&dir.path().join("m.db")).unwrap();
        seed(&db.conn, "m1", "active", &[]);
        seed(&db.conn, "m2", "superseded", &[]);
        seed(&db.conn, "m3", "forgotten", &[]);

        resolve_allowed_ids(&db.conn, &[], true, false).unwrap();
        assert_eq!(
            allowed_ids(&db.conn),
            vec!["m1".to_string(), "m2".to_string()]
        );

        resolve_allowed_ids(&db.conn, &[], true, true).unwrap();
        assert_eq!(
            allowed_ids(&db.conn),
            vec!["m1".to_string(), "m2".to_string(), "m3".to_string()]
        );
    }

    #[test]
    fn resolve_is_safe_to_call_twice_on_the_same_connection() {
        let dir = tempdir().unwrap();
        let db = Db::open(&dir.path().join("m.db")).unwrap();
        seed(&db.conn, "m1", "active", &[]);
        resolve_allowed_ids(&db.conn, &[], false, false).unwrap();
        resolve_allowed_ids(&db.conn, &[], false, false).unwrap();
        assert_eq!(allowed_ids(&db.conn), vec!["m1".to_string()]);
    }

    #[test]
    fn parse_where_rejects_a_term_with_no_equals_sign() {
        let err = parse_where_terms(&["noequals".to_string()]).unwrap_err();
        assert_eq!(err.exit, crate::error::ExitCode::Usage);
    }

    #[test]
    fn parse_where_rejects_a_bad_key_charset() {
        let err = parse_where_terms(&["bad key=v".to_string()]).unwrap_err();
        assert_eq!(err.exit, crate::error::ExitCode::Usage);
    }

    #[test]
    fn parse_where_recognizes_all_three_forms() {
        let terms = parse_where_terms(&[
            "kind=decision".to_string(),
            "kind!=note".to_string(),
            "kind=*".to_string(),
        ])
        .unwrap();
        assert_eq!(terms[0].op, WhereOp::Eq);
        assert_eq!(terms[0].value.as_deref(), Some("decision"));
        assert_eq!(terms[1].op, WhereOp::Ne);
        assert_eq!(terms[1].value.as_deref(), Some("note"));
        assert_eq!(terms[2].op, WhereOp::Exists);
        assert_eq!(terms[2].value, None);
    }
}
