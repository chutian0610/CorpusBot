use std::path::Path;

use crate::error::Result;
use rusqlite::{Connection, OptionalExtension};

pub struct Metadata {
    connection: Connection,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PageRow {
    pub path: String,
    pub title: String,
    pub page_type: String,
    pub sha256: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SourceRow {
    pub source_id: String,
    pub source_version_id: String,
    pub sha256: String,
    pub original_name: String,
    pub size: u64,
    pub imported_at: String,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TouchedResource {
    pub path: String,
    pub revision_kind: String,
    pub sha256: Option<String>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IngestRunRow {
    pub run_id: String,
    pub source_id: String,
    pub status: String,
    pub baseline_snapshot_id: String,
    pub baseline_manifest_id: String,
    pub touched_resources: Vec<TouchedResource>,
    pub created_at: String,
    pub finished_at: Option<String>,
    pub original_name: Option<String>,
    pub source_version_id: Option<String>,
    pub sha256: Option<String>,
    pub size: Option<u64>,
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
              request_json TEXT,
              current_backup_dir TEXT,
              created_at TEXT NOT NULL,
              finished_at TEXT
            );

            CREATE TABLE IF NOT EXISTS restore_runs (
              run_id TEXT PRIMARY KEY,
              target_snapshot_id TEXT NOT NULL,
              pre_restore_snapshot_id TEXT NOT NULL,
              expected_manifest_id TEXT NOT NULL,
              phase TEXT NOT NULL,
              created_at TEXT NOT NULL,
              finished_at TEXT
            );
            "#,
        )?;
        add_column_if_missing(connection, "ingest_runs", "request_json", "TEXT")?;
        add_column_if_missing(
            connection,
            "restore_runs",
            "expected_manifest_id",
            "TEXT NOT NULL DEFAULT ''",
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

    pub fn source_by_sha(&self, sha256: &str) -> Result<Option<SourceRow>> {
        let mut statement = self.connection.prepare(
            "SELECT source_id, source_version_id, sha256, original_name, size, imported_at
             FROM sources WHERE sha256 = ?1",
        )?;
        let source = statement
            .query_row(rusqlite::params![sha256], |row| {
                Ok(SourceRow {
                    source_id: row.get(0)?,
                    source_version_id: row.get(1)?,
                    sha256: row.get(2)?,
                    original_name: row.get(3)?,
                    size: row.get(4)?,
                    imported_at: row.get(5)?,
                })
            })
            .optional()?;
        Ok(source)
    }

    pub fn source_by_version_id(&self, source_version_id: &str) -> Result<Option<SourceRow>> {
        let mut statement = self.connection.prepare(
            "SELECT source_id, source_version_id, sha256, original_name, size, imported_at
             FROM sources WHERE source_version_id = ?1",
        )?;
        let source = statement
            .query_row(rusqlite::params![source_version_id], |row| {
                Ok(SourceRow {
                    source_id: row.get(0)?,
                    source_version_id: row.get(1)?,
                    sha256: row.get(2)?,
                    original_name: row.get(3)?,
                    size: row.get(4)?,
                    imported_at: row.get(5)?,
                })
            })
            .optional()?;
        Ok(source)
    }

    pub fn insert_source_tx(connection: &Connection, source: &SourceRow) -> Result<()> {
        connection.execute(
            "INSERT INTO sources(source_id, source_version_id, sha256, original_name, size, imported_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![
                source.source_id,
                source.source_version_id,
                source.sha256,
                source.original_name,
                source.size,
                source.imported_at
            ],
        )?;
        Ok(())
    }

    pub fn upsert_page_tx(connection: &Connection, page: &PageRow) -> Result<()> {
        Self::upsert_page_on(connection, page)
    }

    pub fn unchecked_transaction(&self) -> Result<rusqlite::Transaction<'_>> {
        Ok(self.connection.unchecked_transaction()?)
    }

    fn upsert_page_on(connection: &Connection, page: &PageRow) -> Result<()> {
        connection.execute(
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
        Ok(self.pending_ingest()?.is_some() || self.pending_restore()?.is_some())
    }

    pub fn pending_ingest(&self) -> Result<Option<PendingIngestRun>> {
        self.connection
            .query_row(
                "SELECT run_id, baseline_snapshot_id, baseline_manifest_id,
                        touched_resources_json, request_json, current_backup_dir
                 FROM ingest_runs
                 WHERE status = 'applying'
                 ORDER BY created_at
                 LIMIT 1",
                [],
                |row| {
                    Ok(PendingIngestRun {
                        run_id: row.get(0)?,
                        baseline_snapshot_id: row.get(1)?,
                        baseline_manifest_id: row.get(2)?,
                        touched_resources_json: row.get(3)?,
                        request_json: row.get(4)?,
                        current_backup_dir: row.get(5)?,
                    })
                },
            )
            .optional()
            .map_err(Into::into)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn insert_pending_ingest(
        &self,
        run_id: &str,
        source_id: &str,
        baseline_snapshot_id: &str,
        baseline_manifest_id: &str,
        touched_resources_json: &str,
        request_json: &str,
        created_at: &str,
        current_backup_dir: &str,
    ) -> Result<()> {
        self.connection.execute(
            "INSERT INTO ingest_runs(
                run_id, source_id, status, baseline_snapshot_id, baseline_manifest_id,
                touched_resources_json, request_json, current_backup_dir, created_at
             ) VALUES (?1, ?2, 'applying', ?3, ?4, ?5, ?6, ?7, ?8)",
            rusqlite::params![
                run_id,
                source_id,
                baseline_snapshot_id,
                baseline_manifest_id,
                touched_resources_json,
                request_json,
                current_backup_dir,
                created_at,
            ],
        )?;
        Ok(())
    }

    pub fn mark_ingest_finished(&self, run_id: &str, status: &str) -> Result<()> {
        self.connection.execute(
            "UPDATE ingest_runs
             SET status = ?2, finished_at = ?3
             WHERE run_id = ?1 AND status = 'applying'",
            rusqlite::params![
                run_id,
                status,
                &time::OffsetDateTime::now_utc()
                    .format(&time::format_description::well_known::Rfc3339)
                    .unwrap_or_else(|_| "unknown-time".to_owned()),
            ],
        )?;
        Ok(())
    }

    pub fn pending_restore(&self) -> Result<Option<PendingRestoreRun>> {
        self.connection
            .query_row(
                "SELECT run_id, target_snapshot_id, pre_restore_snapshot_id,
                        expected_manifest_id, phase
                 FROM restore_runs
                 WHERE phase IN ('prepared', 'switching')
                 ORDER BY created_at
                 LIMIT 1",
                [],
                |row| {
                    Ok(PendingRestoreRun {
                        run_id: row.get(0)?,
                        target_snapshot_id: row.get(1)?,
                        pre_restore_snapshot_id: row.get(2)?,
                        expected_manifest_id: row.get(3)?,
                        phase: row.get(4)?,
                    })
                },
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn insert_pending_restore(
        &self,
        run_id: &str,
        target_snapshot_id: &str,
        pre_restore_snapshot_id: &str,
        expected_manifest_id: &str,
        created_at: &str,
    ) -> Result<()> {
        self.connection.execute(
            "INSERT INTO restore_runs(
                run_id, target_snapshot_id, pre_restore_snapshot_id,
                expected_manifest_id, phase, created_at
             ) VALUES (?1, ?2, ?3, ?4, 'prepared', ?5)",
            rusqlite::params![
                run_id,
                target_snapshot_id,
                pre_restore_snapshot_id,
                expected_manifest_id,
                created_at,
            ],
        )?;
        Ok(())
    }

    pub fn update_restore_phase(&self, run_id: &str, phase: &str) -> Result<()> {
        self.connection.execute(
            "UPDATE restore_runs SET phase = ?2 WHERE run_id = ?1",
            rusqlite::params![run_id, phase],
        )?;
        Ok(())
    }

    pub fn mark_restore_finished(&self, run_id: &str, phase: &str) -> Result<()> {
        self.connection.execute(
            "UPDATE restore_runs
             SET phase = ?2, finished_at = ?3
             WHERE run_id = ?1 AND phase IN ('prepared', 'switching')",
            rusqlite::params![
                run_id,
                phase,
                time::OffsetDateTime::now_utc()
                    .format(&time::format_description::well_known::Rfc3339)
                    .unwrap_or_else(|_| "unknown-time".to_owned()),
            ],
        )?;
        Ok(())
    }

    pub fn ingest_runs(&self, limit: usize) -> Result<Vec<IngestRunRow>> {
        let mut statement = self.connection.prepare(
            "SELECT r.run_id, r.source_id, r.status, r.baseline_snapshot_id,
                    r.baseline_manifest_id, r.touched_resources_json, r.created_at, r.finished_at,
                    s.original_name, s.source_version_id, s.sha256, s.size
             FROM ingest_runs AS r
             LEFT JOIN sources AS s ON s.source_id = r.source_id
             ORDER BY r.created_at DESC
             LIMIT ?1",
        )?;
        let runs = statement
            .query_map(rusqlite::params![limit], |row| {
                Ok(IngestRunRow {
                    run_id: row.get(0)?,
                    source_id: row.get(1)?,
                    status: row.get(2)?,
                    baseline_snapshot_id: row.get(3)?,
                    baseline_manifest_id: row.get(4)?,
                    touched_resources: touched_resources(&row.get::<_, String>(5)?),
                    created_at: row.get(6)?,
                    finished_at: row.get(7)?,
                    original_name: row.get(8)?,
                    source_version_id: row.get(9)?,
                    sha256: row.get(10)?,
                    size: row.get(11)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(runs)
    }

    pub fn ingest_run(&self, run_id: &str) -> Result<Option<IngestRunRow>> {
        let mut statement = self.connection.prepare(
            "SELECT r.run_id, r.source_id, r.status, r.baseline_snapshot_id,
                    r.baseline_manifest_id, r.touched_resources_json, r.created_at, r.finished_at,
                    s.original_name, s.source_version_id, s.sha256, s.size
             FROM ingest_runs AS r
             LEFT JOIN sources AS s ON s.source_id = r.source_id
             WHERE r.run_id = ?1",
        )?;
        let run = statement
            .query_row(rusqlite::params![run_id], |row| {
                Ok(IngestRunRow {
                    run_id: row.get(0)?,
                    source_id: row.get(1)?,
                    status: row.get(2)?,
                    baseline_snapshot_id: row.get(3)?,
                    baseline_manifest_id: row.get(4)?,
                    touched_resources: touched_resources(&row.get::<_, String>(5)?),
                    created_at: row.get(6)?,
                    finished_at: row.get(7)?,
                    original_name: row.get(8)?,
                    source_version_id: row.get(9)?,
                    sha256: row.get(10)?,
                    size: row.get(11)?,
                })
            })
            .optional()?;
        Ok(run)
    }
}

#[derive(Clone, Debug)]
pub struct PendingIngestRun {
    pub run_id: String,
    pub baseline_snapshot_id: String,
    pub baseline_manifest_id: String,
    pub touched_resources_json: String,
    pub request_json: Option<String>,
    pub current_backup_dir: Option<String>,
}

#[derive(Clone, Debug)]
pub struct PendingRestoreRun {
    pub run_id: String,
    pub target_snapshot_id: String,
    pub pre_restore_snapshot_id: String,
    pub expected_manifest_id: String,
    pub phase: String,
}

fn add_column_if_missing(
    connection: &Connection,
    table: &str,
    column: &str,
    definition: &str,
) -> Result<()> {
    let exists = connection
        .prepare(&format!("PRAGMA table_info({table})"))?
        .query_map([], |row| row.get::<_, String>(1))?
        .filter_map(std::result::Result::ok)
        .any(|name| name == column);
    if !exists {
        connection.execute(
            &format!("ALTER TABLE {table} ADD COLUMN {column} {definition}"),
            [],
        )?;
    }
    Ok(())
}

fn touched_resources(raw: &str) -> Vec<TouchedResource> {
    serde_json::from_str::<Vec<corpusbot_core::ResourceRevision>>(raw)
        .map(|resources| {
            resources
                .into_iter()
                .map(|resource| {
                    let (revision_kind, sha256) = match resource.revision() {
                        corpusbot_core::Revision::Absent => ("absent".to_owned(), None),
                        corpusbot_core::Revision::Content { sha256 } => {
                            ("content".to_owned(), Some(sha256.clone()))
                        }
                        corpusbot_core::Revision::Generation { id, sequence } => {
                            ("generation".to_owned(), Some(format!("{id}:{sequence}")))
                        }
                    };
                    TouchedResource {
                        path: resource.resource().path().to_owned(),
                        revision_kind,
                        sha256,
                    }
                })
                .collect()
        })
        .unwrap_or_default()
}
