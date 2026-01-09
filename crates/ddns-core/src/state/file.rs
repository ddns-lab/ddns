// # File State Store
//
// File-based implementation of StateStore with crash recovery.
//
// ## Purpose
//
// Provides persistent state storage across daemon restarts and crashes.
// Ensures idempotency by tracking the last known IP for each record.
//
// ## Crash Recovery
//
// - Atomic writes: Uses write-then-rename for atomicity
// - Corruption detection: Validates JSON on load
// - Automatic backup: Keeps .backup of last known good state
// - Recovery: Falls back to backup if corruption detected
//
// ## File Format
//
// ```json
// {
//   "version": "1.0",
//   "records": {
//     "example.com": {
//       "last_ip": "1.2.3.4",
//       "last_updated": "2025-01-09T12:00:00Z",
//       "provider_metadata": {}
//     }
//   }
// }
// ```

use async_trait::async_trait;
use std::collections::HashMap;
use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::fs;
use tokio::io::AsyncWriteExt;
use tokio::sync::RwLock;

use crate::Error;
use crate::traits::state_store::{StateRecord, StateStore};

/// State file format version
/// Used for future migration if format changes
const STATE_FILE_VERSION: &str = "1.0";

/// File-based state store with crash recovery
///
/// This implementation persists state to a JSON file with atomic writes
/// and automatic corruption recovery.
///
/// # Crash Recovery
///
/// - **Atomic writes**: New state written to temporary file, then renamed
/// - **Backup**: Last known good state kept in `.backup` file
/// - **Corruption detection**: JSON validation on load
/// - **Automatic recovery**: Falls back to backup if main file corrupted
///
/// # Example
///
/// ```rust,no_run
/// use ddns_core::state::FileStateStore;
/// use ddns_core::traits::state_store::StateStore;
///
/// #[tokio::main]
/// async fn main() -> Result<(), Box<dyn std::error::Error>> {
///     let store = FileStateStore::new("/var/lib/ddns/state.json").await?;
///
///     // Set IP (atomically written to disk)
///     store.set_last_ip("example.com", "1.2.3.4".parse()?).await?;
///
///     // Get IP
///     let ip = store.get_last_ip("example.com").await?;
///     assert_eq!(ip, Some("1.2.3.4".parse()?));
///
///     Ok(())
/// }
/// ```
#[derive(Debug)]
pub struct FileStateStore {
    path: PathBuf,
    state: Arc<RwLock<FileState>>,
}

/// Internal state for file-based store
#[derive(Debug)]
struct FileState {
    records: HashMap<String, StateRecord>,
    dirty: bool,
}

/// Serializable state file format
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct StateFileFormat {
    version: String,
    records: HashMap<String, StateRecord>,
}

impl FileStateStore {
    /// Create or load a file state store
    ///
    /// This will:
    /// 1. Try to load existing state file
    /// 2. If corruption detected, try to load from backup
    /// 3. If both fail, start with empty state
    /// 4. Create parent directories if needed
    pub async fn new<P: AsRef<Path>>(path: P) -> Result<Self, Error> {
        let path = path.as_ref().to_path_buf();

        // Create parent directory if it doesn't exist
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() && !parent.exists() {
                fs::create_dir_all(parent).await.map_err(|e| {
                    Error::config(&format!(
                        "Failed to create state directory {}: {}",
                        parent.display(),
                        e
                    ))
                })?;
            }
        }

        // Try to load existing state
        let records = Self::load_state_with_recovery(&path).await?;

        Ok(Self {
            path,
            state: Arc::new(RwLock::new(FileState {
                records,
                dirty: false,
            })),
        })
    }

    /// Load state from file with automatic recovery
    ///
    /// Recovery strategy:
    /// 1. Try to load main state file
    /// 2. If JSON parse error, try loading backup
    /// 3. If backup also fails, start with empty state
    async fn load_state_with_recovery(path: &Path) -> Result<HashMap<String, StateRecord>, Error> {
        // Try to load main file
        match Self::load_state(path).await {
            Ok(records) => {
                tracing::debug!("Loaded state from file: {} records", records.len());
                return Ok(records);
            }
            Err(e) => {
                // Check if it's a JSON parse error (corruption)
                let error_str = e.to_string().to_lowercase();
                if error_str.contains("json")
                    || error_str.contains("parse")
                    || error_str.contains("format")
                    || error_str.contains("expected value")
                    || error_str.contains("serde")
                {
                    tracing::warn!(
                        "State file appears corrupted: {}. Attempting recovery from backup.",
                        e
                    );

                    // Try to load backup
                    let backup_path = Self::backup_path(path);
                    if backup_path.exists() {
                        match Self::load_state(&backup_path).await {
                            Ok(records) => {
                                tracing::info!(
                                    "Recovered state from backup: {} records",
                                    records.len()
                                );

                                // Restore corrupted file from backup
                                if let Err(restore_err) =
                                    Self::restore_from_backup(path, &backup_path).await
                                {
                                    tracing::error!(
                                        "Failed to restore state file from backup: {}",
                                        restore_err
                                    );
                                }

                                return Ok(records);
                            }
                            Err(backup_err) => {
                                tracing::error!(
                                    "Backup also corrupted: {}. Starting with empty state.",
                                    backup_err
                                );
                                return Ok(HashMap::new());
                            }
                        }
                    } else {
                        tracing::warn!("No backup file found. Starting with empty state.");
                        return Ok(HashMap::new());
                    }
                }
                // Other error (not corruption)
                return Err(e);
            }
        }
    }

    /// Load state from file
    async fn load_state(path: &Path) -> Result<HashMap<String, StateRecord>, Error> {
        if !path.exists() {
            tracing::debug!("State file does not exist: {}", path.display());
            return Ok(HashMap::new());
        }

        let content = fs::read_to_string(path).await.map_err(|e| {
            Error::state_store(&format!(
                "Failed to read state file {}: {}",
                path.display(),
                e
            ))
        })?;

        // Parse JSON
        let state_file: StateFileFormat = serde_json::from_str(&content).map_err(|e| {
            Error::state_store(&format!(
                "Failed to parse state file {}: {}. \
                File may be corrupted. Try restoring from backup.",
                path.display(),
                e
            ))
        })?;

        // Validate version
        if state_file.version != STATE_FILE_VERSION {
            tracing::warn!(
                "State file version mismatch: expected {}, got {}. \
                Attempting to load anyway.",
                STATE_FILE_VERSION,
                state_file.version
            );
        }

        Ok(state_file.records)
    }

    /// Write state to file atomically
    async fn write_state(&self) -> Result<(), Error> {
        let state_guard = self.state.read().await;
        let records = &state_guard.records;

        // Serialize to JSON
        let state_file = StateFileFormat {
            version: STATE_FILE_VERSION.to_string(),
            records: records.clone(),
        };

        let json = serde_json::to_string_pretty(&state_file)
            .map_err(|e| Error::state_store(&format!("Failed to serialize state: {}", e)))?;

        // Write to temporary file first
        let temp_path = self.temp_path();
        {
            let mut file = fs::File::create(&temp_path).await.map_err(|e| {
                Error::state_store(&format!(
                    "Failed to create temp file {}: {}",
                    temp_path.display(),
                    e
                ))
            })?;

            file.write_all(json.as_bytes()).await.map_err(|e| {
                Error::state_store(&format!(
                    "Failed to write to temp file {}: {}",
                    temp_path.display(),
                    e
                ))
            })?;

            file.flush().await.map_err(|e| {
                Error::state_store(&format!(
                    "Failed to flush temp file {}: {}",
                    temp_path.display(),
                    e
                ))
            })?;
        }

        // Create backup of current file (if it exists)
        if self.path.exists() {
            let backup_path = Self::backup_path(&self.path);
            if let Err(e) = fs::copy(&self.path, &backup_path).await {
                tracing::warn!("Failed to create backup: {}", e);
            }
        }

        // Atomic rename (temp -> actual)
        fs::rename(&temp_path, &self.path).await.map_err(|e| {
            Error::state_store(&format!(
                "Failed to rename {} to {}: {}",
                temp_path.display(),
                self.path.display(),
                e
            ))
        })?;

        // Mark as clean
        drop(state_guard);
        {
            let mut state_guard = self.state.write().await;
            state_guard.dirty = false;
        }

        tracing::trace!("State written to file: {}", self.path.display());
        Ok(())
    }

    /// Restore state file from backup
    async fn restore_from_backup(path: &Path, backup_path: &Path) -> Result<(), Error> {
        fs::copy(backup_path, path).await.map_err(|e| {
            Error::state_store(&format!(
                "Failed to restore from backup {} to {}: {}",
                backup_path.display(),
                path.display(),
                e
            ))
        })?;

        tracing::info!("Restored state file from backup");
        Ok(())
    }

    /// Get path to temporary file for atomic writes
    fn temp_path(&self) -> PathBuf {
        let mut temp = self.path.clone();
        temp.set_extension("tmp");
        temp
    }

    /// Get path to backup file
    fn backup_path(path: &Path) -> PathBuf {
        let mut backup = path.to_path_buf();
        backup.set_extension("backup");
        backup
    }

    /// Force immediate write to disk
    pub async fn sync(&self) -> Result<(), Error> {
        self.write_state().await
    }
}

#[async_trait]
impl StateStore for FileStateStore {
    async fn get_last_ip(&self, record_name: &str) -> Result<Option<IpAddr>, Error> {
        let state_guard = self.state.read().await;
        Ok(state_guard.records.get(record_name).map(|r| r.last_ip))
    }

    async fn get_record(&self, record_name: &str) -> Result<Option<StateRecord>, Error> {
        let state_guard = self.state.read().await;
        Ok(state_guard.records.get(record_name).cloned())
    }

    async fn set_last_ip(&self, record_name: &str, ip: IpAddr) -> Result<(), Error> {
        {
            let mut state_guard = self.state.write().await;
            let record = StateRecord::new(ip);
            state_guard.records.insert(record_name.to_string(), record);
            state_guard.dirty = true;
        }

        // Immediate write for durability
        self.write_state().await
    }

    async fn set_record(&self, record_name: &str, record: &StateRecord) -> Result<(), Error> {
        {
            let mut state_guard = self.state.write().await;
            state_guard
                .records
                .insert(record_name.to_string(), record.clone());
            state_guard.dirty = true;
        }

        // Immediate write for durability
        self.write_state().await
    }

    async fn delete_record(&self, record_name: &str) -> Result<(), Error> {
        {
            let mut state_guard = self.state.write().await;
            state_guard.records.remove(record_name);
            state_guard.dirty = true;
        }

        // Immediate write for durability
        self.write_state().await
    }

    async fn list_records(&self) -> Result<Vec<String>, Error> {
        let state_guard = self.state.read().await;
        Ok(state_guard.records.keys().cloned().collect())
    }

    async fn flush(&self) -> Result<(), Error> {
        let state_guard = self.state.read().await;
        if state_guard.dirty {
            drop(state_guard);
            self.write_state().await
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_file_store_basic() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("state.json");

        let store = FileStateStore::new(&path).await.unwrap();

        // Initially empty
        let records = store.list_records().await.unwrap();
        assert_eq!(records.len(), 0);

        // Set and get
        let ip: IpAddr = "1.2.3.4".parse().unwrap();
        store.set_last_ip("example.com", ip).await.unwrap();

        let retrieved = store.get_last_ip("example.com").await.unwrap();
        assert_eq!(retrieved, Some(ip));

        // Verify file was written
        assert!(path.exists());

        // Load new instance and verify persistence
        let store2 = FileStateStore::new(&path).await.unwrap();
        let retrieved2 = store2.get_last_ip("example.com").await.unwrap();
        assert_eq!(retrieved2, Some(ip));
    }

    #[tokio::test]
    async fn test_file_store_corruption_recovery() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("state.json");

        // Create store and set state (first write)
        let store = FileStateStore::new(&path).await.unwrap();
        let ip1: IpAddr = "1.2.3.4".parse().unwrap();
        store.set_last_ip("example.com", ip1).await.unwrap();

        // Write again to ensure backup is created
        let ip2: IpAddr = "1.2.3.5".parse().unwrap();
        store.set_last_ip("example.com", ip2).await.unwrap();

        // Verify backup exists
        let backup_path = FileStateStore::backup_path(&path);
        assert!(backup_path.exists(), "Backup file should exist after write");

        // Corrupt the state file
        fs::write(&path, b"corrupted json data").await.unwrap();

        // Load should recover from backup (should not error)
        let store2 = FileStateStore::new(&path).await.expect(&format!(
            "Failed to create store from corrupted file. Backup should have been recovered.\n\
             Main file: {:?}\n\
             Backup file: {:?}",
            path, backup_path
        ));
        let recovered = store2.get_last_ip("example.com").await.unwrap();
        // Should have recovered the PREVIOUS value (from backup, before last write)
        assert_eq!(
            recovered,
            Some(ip1),
            "Backup should contain previous state, not latest"
        );
    }

    #[tokio::test]
    async fn test_file_store_atomic_write() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("state.json");

        let store = FileStateStore::new(&path).await.unwrap();

        // Write multiple updates rapidly
        for i in 0..10 {
            let ip: IpAddr = format!("1.2.3.{}", i).parse().unwrap();
            store.set_last_ip("example.com", ip).await.unwrap();
        }

        // Verify final state is consistent
        let store2 = FileStateStore::new(&path).await.unwrap();
        let final_ip = store2.get_last_ip("example.com").await.unwrap();
        assert_eq!(final_ip, Some("1.2.3.9".parse().unwrap()));
    }
}
