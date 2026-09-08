use std::path::Path;

use rusqlite::Connection;

use crate::error::Result;

pub struct Metadata {
    connection: Connection,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PageRow {
    pub path: String,
    pub title: String,
    pub page_type: String,
    pub sha256: String,
    pub updated_at: String,
}

impl Metadata {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let connection = Connection::open(path)?;
        connection.pragma_update(None, "journal_mode", "WAL")?;
        connection.pragma_update(None, "foreign_keys", "ON")?;
        Self::migrate(&connection)?;
        Ok(Self { connection })
    }

    fn migrate(connection: &Connection) -> Result<()> {
        connection.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS workspace_meta (
              key TEXT PRIMARY KEY,
              value TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS sources (
              source_id TEXT PRIMARY KEY,
              source_version_id TEXT NOT NULL UNIQUE,
              sha256 TEXT NOT NULL UNIQUE,
              original_name TEXT NOT NULL,
              size INTEGER NOT NULL,
              imported_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS pages (
              path TEXT PRIMARY KEY,
              title TEXT NOT NULL,
              page_type TEXT NOT NULL,
              sha256 TEXT NOT NULL,
              updated_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS ingest_runs (
              run_id TEXT PRIMARY KEY,
              source_id TEXT NOT NULL,
              status TEXT NOT NULL,
              baseline_snapshot_id TEXT NOT NULL,
              baseline_manifest_id TEXT NOT NULL,
              touched_resources_json TEXT NOT NULL,
              current_backup_dir TEXT,
              created_at TEXT NOT NULL,
              finished_at TEXT
            );

            CREATE TABLE IF NOT EXISTS restore_runs (
              run_id TEXT PRIMARY KEY,
              target_snapshot_id TEXT NOT NULL,
              pre_restore_snapshot_id TEXT NOT NULL,
              phase TEXT NOT NULL,
              created_at TEXT NOT NULL,
              finished_at TEXT
            );
            "#,
        )?;
        Ok(())
    }

    pub fn set_meta(&self, key: &str, value: &str) -> Result<()> {
        self.connection.execute(
            "INSERT INTO workspace_meta(key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            rusqlite::params![key, value],
        )?;
        Ok(())
    }

    pub fn get_meta(&self, key: &str) -> Result<Option<String>> {
        let mut statement = self
            .connection
            .prepare("SELECT value FROM workspace_meta WHERE key = ?1")?;
        let value = statement.query_row(rusqlite::params![key], |row| row.get(0))?;
        Ok(value)
    }

    pub fn upsert_page(&self, page: &PageRow) -> Result<()> {
        self.connection.execute(
            "INSERT INTO pages(path, title, page_type, sha256, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(path) DO UPDATE SET
               title = excluded.title,
               page_type = excluded.page_type,
               sha256 = excluded.sha256,
               updated_at = excluded.updated_at",
            rusqlite::params![
                page.path,
                page.title,
                page.page_type,
                page.sha256,
                page.updated_at
            ],
        )?;
        Ok(())
    }

    pub fn page_paths(&self) -> Result<Vec<String>> {
        let mut statement = self.connection.prepare("SELECT path FROM pages")?;
        let paths = statement
            .query_map([], |row| row.get(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(paths)
    }

    pub fn remove_page(&self, path: &str) -> Result<()> {
        self.connection
            .execute("DELETE FROM pages WHERE path = ?1", rusqlite::params![path])?;
        Ok(())
    }

    pub fn pages(&self) -> Result<Vec<PageRow>> {
        let mut statement = self.connection.prepare(
            "SELECT path, title, page_type, sha256, updated_at FROM pages ORDER BY path",
        )?;
        let pages = statement
            .query_map([], |row| {
                Ok(PageRow {
                    path: row.get(0)?,
                    title: row.get(1)?,
                    page_type: row.get(2)?,
                    sha256: row.get(3)?,
                    updated_at: row.get(4)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(pages)
    }

    pub fn source_exists_by_sha(&self, sha256: &str) -> Result<bool> {
        let mut statement = self
            .connection
            .prepare("SELECT 1 FROM sources WHERE sha256 = ?1")?;
        let exists = statement.exists(rusqlite::params![sha256])?;
        Ok(exists)
    }

    pub fn insert_source(
        &self,
        source_id: &str,
        source_version_id: &str,
        sha256: &str,
        original_name: &str,
        size: u64,
        imported_at: &str,
    ) -> Result<()> {
        self.connection.execute(
            "INSERT INTO sources(source_id, source_version_id, sha256, original_name, size, imported_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![
                source_id,
                source_version_id,
                sha256,
                original_name,
                size,
                imported_at
            ],
        )?;
        Ok(())
    }

    pub fn has_pending_recovery(&self) -> Result<bool> {
        let mut statement = self
            .connection
            .prepare("SELECT 1 FROM ingest_runs WHERE status = 'applying' LIMIT 1")?;
        let pending = statement.exists([])?;
        Ok(pending)
    }
}
