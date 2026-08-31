//! Forward-only schema migrations, keyed by target `PRAGMA user_version`.
//!
//! Schema v1 DDL exactly per architecture.md §10, including the FTS5
//! external-content sync triggers (AI/AD/AU) -- pattern lifted from
//! Satchel's `src/rag/mod.rs` FTS5 schema (MIT, virgilvox/satchel; see
//! THIRD-PARTY.md). Each entry here is one forward step; `db::open` runs
//! every entry whose version is greater than the database's current
//! `user_version`, inside a single transaction, preceded by a timestamped
//! `.bak` copy when the database file already existed on disk.

/// (target user_version, DDL to reach it from the previous version).
pub const MIGRATIONS: &[(i64, &str)] = &[(1, V001_INIT)];

const V001_INIT: &str = r#"
CREATE TABLE db_info (
  key   TEXT PRIMARY KEY,
  value TEXT NOT NULL
);

CREATE TABLE memories (
  id            TEXT PRIMARY KEY,
  content       TEXT NOT NULL,
  content_hash  TEXT NOT NULL,
  source        TEXT,
  created_at    TEXT NOT NULL,
  status        TEXT NOT NULL DEFAULT 'active',
  superseded_by TEXT REFERENCES memories(id),
  forgotten_at  TEXT
);
CREATE INDEX idx_memories_status ON memories(status);
CREATE INDEX idx_memories_hash   ON memories(content_hash);

CREATE TABLE memory_meta (
  memory_id TEXT NOT NULL REFERENCES memories(id) ON DELETE CASCADE,
  key       TEXT NOT NULL,
  value     TEXT NOT NULL,
  PRIMARY KEY (memory_id, key)
);
CREATE INDEX idx_meta_kv ON memory_meta(key, value);

CREATE TABLE chunks (
  id        TEXT PRIMARY KEY,
  memory_id TEXT NOT NULL REFERENCES memories(id) ON DELETE CASCADE,
  idx       INTEGER NOT NULL,
  text      TEXT NOT NULL,
  embedding BLOB NOT NULL
);
CREATE INDEX idx_chunks_memory ON chunks(memory_id);

CREATE VIRTUAL TABLE chunks_fts USING fts5(
  text, content='chunks', content_rowid='rowid',
  tokenize='porter unicode61 remove_diacritics 2'
);

CREATE TRIGGER chunks_ai AFTER INSERT ON chunks BEGIN
  INSERT INTO chunks_fts(rowid, text) VALUES (new.rowid, new.text);
END;

CREATE TRIGGER chunks_ad AFTER DELETE ON chunks BEGIN
  INSERT INTO chunks_fts(chunks_fts, rowid, text) VALUES('delete', old.rowid, old.text);
END;

CREATE TRIGGER chunks_au AFTER UPDATE ON chunks BEGIN
  INSERT INTO chunks_fts(chunks_fts, rowid, text) VALUES('delete', old.rowid, old.text);
  INSERT INTO chunks_fts(rowid, text) VALUES (new.rowid, new.text);
END;
"#;
