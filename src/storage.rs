use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result};
use rusqlite::{Connection, OptionalExtension, params};
use sha2::{Digest, Sha256};

use crate::paths;

const HISTORY_RETENTION_LIMIT: i64 = 500;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ClipboardEntryKind {
    Text,
    Image,
}

#[derive(Clone, Debug)]
pub struct ClipboardEntry {
    pub id: i64,
    pub kind: ClipboardEntryKind,
    pub content: String,
    pub content_type: String,
    pub binary_content: Option<Vec<u8>>,
    pub created_at: String,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HistorySignature {
    pub count: i64,
    pub max_id: i64,
}

pub struct Storage {
    conn: Connection,
    path: PathBuf,
}

impl Storage {
    pub fn open() -> Result<Self> {
        let path = paths::database_path()?;
        let conn = Connection::open(&path)
            .with_context(|| format!("failed to open database at {}", path.display()))?;

        conn.busy_timeout(Duration::from_secs(3))
            .context("failed to configure sqlite busy timeout")?;

        initialize_schema(&conn)?;

        Ok(Self { conn, path })
    }

    pub fn database_path(&self) -> &Path {
        &self.path
    }

    pub fn recent_entries(&self, limit: usize) -> Result<Vec<ClipboardEntry>> {
        let mut statement = self.conn.prepare(
            "SELECT id, kind, content, content_type, binary_content, created_at
             FROM clipboard_entries
             ORDER BY id DESC
             LIMIT ?1",
        )?;

        let rows = statement.query_map([limit as i64], |row| {
            Ok(ClipboardEntry {
                id: row.get(0)?,
                kind: parse_kind(row.get::<_, String>(1)?),
                content: row.get(2)?,
                content_type: row.get(3)?,
                binary_content: row.get(4)?,
                created_at: row.get(5)?,
            })
        })?;

        rows.collect::<rusqlite::Result<Vec<_>>>()
            .context("failed to load clipboard history")
    }

    pub fn entry_by_id(&self, id: i64) -> Result<Option<ClipboardEntry>> {
        self.conn
            .query_row(
                "SELECT id, kind, content, content_type, binary_content, created_at
                 FROM clipboard_entries
                 WHERE id = ?1",
                [id],
                |row| {
                    Ok(ClipboardEntry {
                        id: row.get(0)?,
                        kind: parse_kind(row.get::<_, String>(1)?),
                        content: row.get(2)?,
                        content_type: row.get(3)?,
                        binary_content: row.get(4)?,
                        created_at: row.get(5)?,
                    })
                },
            )
            .optional()
            .context("failed to load clipboard entry")
    }

    pub fn history_signature(&self) -> Result<HistorySignature> {
        self.conn
            .query_row(
                "SELECT COUNT(*), COALESCE(MAX(id), 0)
                 FROM clipboard_entries",
                [],
                |row| {
                    Ok(HistorySignature {
                        count: row.get(0)?,
                        max_id: row.get(1)?,
                    })
                },
            )
            .context("failed to read clipboard history signature")
    }

    pub fn upsert_text(&mut self, content: &str) -> Result<bool> {
        if content.is_empty() {
            return Ok(false);
        }

        let hash = hash_bytes(content.as_bytes());
        let transaction = self.conn.transaction()?;

        transaction.execute(
            "DELETE FROM clipboard_entries
             WHERE kind = 'text' AND (content_hash = ?1 OR content = ?2)",
            params![hash, content],
        )?;

        transaction.execute(
            "INSERT INTO clipboard_entries (
                 kind, content, content_type, binary_content, content_hash, created_at
             )
             VALUES ('text', ?1, 'text/plain', NULL, ?2, datetime('now', 'localtime'))",
            params![content, hash],
        )?;

        prune_history(&transaction)?;
        transaction.commit()?;

        Ok(true)
    }

    pub fn upsert_image(&mut self, content_type: &str, bytes: &[u8]) -> Result<bool> {
        if bytes.is_empty() {
            return Ok(false);
        }

        let hash = hash_bytes(bytes);
        let summary = image_summary(content_type, bytes.len());
        let transaction = self.conn.transaction()?;

        transaction.execute(
            "DELETE FROM clipboard_entries
             WHERE kind = 'image' AND content_hash = ?1",
            params![hash],
        )?;

        transaction.execute(
            "INSERT INTO clipboard_entries (
                 kind, content, content_type, binary_content, content_hash, created_at
             )
             VALUES ('image', ?1, ?2, ?3, ?4, datetime('now', 'localtime'))",
            params![summary, content_type, bytes, hash],
        )?;

        prune_history(&transaction)?;
        transaction.commit()?;

        Ok(true)
    }

    pub fn delete_entry(&self, id: i64) -> Result<()> {
        self.conn
            .execute("DELETE FROM clipboard_entries WHERE id = ?1", [id])
            .with_context(|| format!("failed to delete clipboard entry {id}"))?;

        Ok(())
    }

    pub fn clear(&self) -> Result<()> {
        self.conn
            .execute("DELETE FROM clipboard_entries", [])
            .context("failed to clear clipboard history")?;

        Ok(())
    }
}

fn initialize_schema(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "PRAGMA journal_mode = WAL;
         PRAGMA synchronous = NORMAL;

         CREATE TABLE IF NOT EXISTS clipboard_entries (
             id INTEGER PRIMARY KEY AUTOINCREMENT,
             content TEXT NOT NULL,
             content_type TEXT NOT NULL,
             created_at TEXT NOT NULL
         );",
    )
    .context("failed to initialize base database schema")?;

    ensure_column(
        conn,
        "clipboard_entries",
        "kind",
        "TEXT NOT NULL DEFAULT 'text'",
    )?;
    ensure_column(conn, "clipboard_entries", "binary_content", "BLOB")?;
    ensure_column(conn, "clipboard_entries", "content_hash", "TEXT")?;

    conn.execute_batch(
        "CREATE INDEX IF NOT EXISTS idx_clipboard_entries_created_at
         ON clipboard_entries (created_at DESC);

         CREATE INDEX IF NOT EXISTS idx_clipboard_entries_kind_hash
         ON clipboard_entries (kind, content_hash);",
    )
    .context("failed to initialize clipboard history indexes")
}

fn ensure_column(conn: &Connection, table: &str, column: &str, definition: &str) -> Result<()> {
    let pragma = format!("PRAGMA table_info({table})");
    let mut statement = conn
        .prepare(&pragma)
        .with_context(|| format!("failed to inspect table {table}"))?;
    let mut rows = statement.query([])?;

    while let Some(row) = rows.next()? {
        let existing_name: String = row.get(1)?;
        if existing_name == column {
            return Ok(());
        }
    }

    conn.execute(
        &format!("ALTER TABLE {table} ADD COLUMN {column} {definition}"),
        [],
    )
    .with_context(|| format!("failed to add column {column} to {table}"))?;

    Ok(())
}

fn prune_history(transaction: &rusqlite::Transaction<'_>) -> Result<()> {
    transaction.execute(
        "DELETE FROM clipboard_entries
         WHERE id NOT IN (
             SELECT id
             FROM clipboard_entries
             ORDER BY id DESC
             LIMIT ?1
         )",
        [HISTORY_RETENTION_LIMIT],
    )?;

    Ok(())
}

fn parse_kind(kind: String) -> ClipboardEntryKind {
    match kind.as_str() {
        "image" => ClipboardEntryKind::Image,
        _ => ClipboardEntryKind::Text,
    }
}

fn hash_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let digest = hasher.finalize();

    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn image_summary(content_type: &str, byte_len: usize) -> String {
    let kind = content_type
        .strip_prefix("image/")
        .unwrap_or(content_type)
        .to_ascii_uppercase();

    format!("{kind} image | {}", human_size(byte_len as u64))
}

pub fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["B", "KB", "MB", "GB"];

    let mut value = bytes as f64;
    let mut unit = 0usize;

    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }

    if unit == 0 {
        format!("{} {}", bytes, UNITS[unit])
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}
