use std::fs::OpenOptions;
use std::path::Path;

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::error::{Result, StoreError};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LockOwner {
    pub pid: u32,
    pub hostname: String,
    pub operation: String,
    pub started_at: String,
    pub process_start_marker: String,
}

pub struct WorkspaceLock {
    _file: std::fs::File,
}

impl WorkspaceLock {
    pub fn acquire(root: &Path, operation: &str) -> Result<Self> {
        let path = root.join(".wiki-db/workspace.lock");
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(&path)?;
        file.try_lock().map_err(|_| StoreError::Locked {
            owner: read_owner(&path).unwrap_or_else(|| "unknown process".to_owned()),
        })?;

        let owner = LockOwner {
            pid: std::process::id(),
            hostname: hostname(),
            operation: operation.to_owned(),
            started_at: current_time(),
            process_start_marker: std::process::id().to_string(),
        };
        serde_json::to_writer_pretty(&file, &owner)?;
        file.sync_all()?;

        Ok(Self { _file: file })
    }
}

fn read_owner(path: &Path) -> Option<String> {
    let raw = std::fs::read_to_string(path).ok()?;
    let owner: LockOwner = serde_json::from_str(&raw).ok()?;
    Some(format!(
        "{} pid={} operation={} started_at={}",
        owner.hostname, owner.pid, owner.operation, owner.started_at
    ))
}

fn hostname() -> String {
    std::env::var("HOSTNAME")
        .or_else(|_| std::env::var("COMPUTERNAME"))
        .unwrap_or_else(|_| "localhost".to_owned())
}

fn current_time() -> String {
    OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| "unknown-time".to_owned())
}
