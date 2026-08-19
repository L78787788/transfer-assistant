use std::{path::Path, sync::Mutex};

use rusqlite::{Connection, OptionalExtension, params, types::Type};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use thiserror::Error;
use uuid::Uuid;

use crate::{
    manifest::EntryKind,
    model::{TransferDirection, TransferSnapshot, TransferState, TrustedPeer},
};

const SCHEMA_VERSION: i64 = 3;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HistoryFileRecord {
    pub id: String,
    pub transfer_id: String,
    pub file_name: String,
    pub relative_path: String,
    pub local_path: Option<String>,
    pub is_directory: bool,
    pub size: u64,
    pub peer_name: String,
    pub direction: TransferDirection,
    pub completed_unix_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredTransferItem {
    pub id: String,
    pub transfer_id: Uuid,
    pub relative_path: String,
    pub kind: EntryKind,
    pub size: u64,
    pub modified_unix_ms: i64,
    pub source_revision: Option<String>,
    pub temporary_ref: Option<String>,
    pub final_ref: Option<String>,
}

pub struct TransferRepository {
    connection: Mutex<Connection>,
}

impl TransferRepository {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, RepositoryError> {
        Self::from_connection(Connection::open(path)?)
    }

    pub fn in_memory() -> Result<Self, RepositoryError> {
        Self::from_connection(Connection::open_in_memory()?)
    }

    fn from_connection(connection: Connection) -> Result<Self, RepositoryError> {
        connection.execute_batch("PRAGMA foreign_keys = ON; PRAGMA journal_mode = WAL;")?;
        let existing_version: i64 =
            connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
        if existing_version > SCHEMA_VERSION {
            return Err(RepositoryError::UnsupportedSchema(existing_version));
        }
        if existing_version == 0 {
            connection.execute_batch(
                "CREATE TABLE IF NOT EXISTS trusted_peers (
               peer_id TEXT PRIMARY KEY,
               display_name TEXT NOT NULL,
               certificate_fingerprint BLOB NOT NULL CHECK(length(certificate_fingerprint) = 32),
               created_unix_ms INTEGER NOT NULL,
               last_seen_unix_ms INTEGER NOT NULL,
               auto_accept INTEGER NOT NULL DEFAULT 0
             );
             CREATE TABLE IF NOT EXISTS transfers (
               id TEXT PRIMARY KEY,
               peer_id TEXT NOT NULL,
               peer_name TEXT NOT NULL,
               direction TEXT NOT NULL,
               state TEXT NOT NULL,
               item_count INTEGER NOT NULL,
               total_bytes INTEGER NOT NULL,
               completed_bytes INTEGER NOT NULL,
               bytes_per_second INTEGER NOT NULL DEFAULT 0,
               created_unix_ms INTEGER NOT NULL,
               updated_unix_ms INTEGER NOT NULL,
               error TEXT
             );
             CREATE TABLE IF NOT EXISTS transfer_items (
               id TEXT PRIMARY KEY,
               transfer_id TEXT NOT NULL REFERENCES transfers(id) ON DELETE CASCADE,
               relative_path TEXT NOT NULL,
               entry_kind TEXT NOT NULL,
               size INTEGER NOT NULL,
               modified_unix_ms INTEGER NOT NULL,
               source_revision TEXT,
               temporary_ref TEXT,
               final_ref TEXT,
               UNIQUE(transfer_id, relative_path)
             );
             CREATE TABLE IF NOT EXISTS completed_chunks (
               transfer_id TEXT NOT NULL REFERENCES transfers(id) ON DELETE CASCADE,
               item_id TEXT NOT NULL REFERENCES transfer_items(id) ON DELETE CASCADE,
               chunk_index INTEGER NOT NULL,
               chunk_hash BLOB NOT NULL CHECK(length(chunk_hash) = 32),
               PRIMARY KEY(transfer_id, item_id, chunk_index)
             );
             CREATE TABLE IF NOT EXISTS settings (
               key TEXT PRIMARY KEY,
               value_json TEXT NOT NULL
             );
             CREATE INDEX IF NOT EXISTS idx_transfers_updated ON transfers(updated_unix_ms);
             CREATE INDEX IF NOT EXISTS idx_transfer_items_transfer_id ON transfer_items(transfer_id);
             CREATE INDEX IF NOT EXISTS idx_completed_chunks_transfer ON completed_chunks(transfer_id);",
            )?;
        }
        if existing_version > 0 && existing_version < SCHEMA_VERSION {
            if existing_version == 1 {
                connection.execute_batch(
                    "ALTER TABLE transfers
                     ADD COLUMN bytes_per_second INTEGER NOT NULL DEFAULT 0;",
                )?;
            }
            connection.execute_batch(
                "CREATE TABLE IF NOT EXISTS transfer_items (
                   id TEXT PRIMARY KEY,
                   transfer_id TEXT NOT NULL REFERENCES transfers(id) ON DELETE CASCADE,
                   relative_path TEXT NOT NULL,
                   entry_kind TEXT NOT NULL,
                   size INTEGER NOT NULL,
                   modified_unix_ms INTEGER NOT NULL,
                   source_revision TEXT,
                   temporary_ref TEXT,
                   final_ref TEXT,
                   UNIQUE(transfer_id, relative_path)
                 );
                 CREATE TABLE IF NOT EXISTS completed_chunks (
                   transfer_id TEXT NOT NULL REFERENCES transfers(id) ON DELETE CASCADE,
                   item_id TEXT NOT NULL REFERENCES transfer_items(id) ON DELETE CASCADE,
                   chunk_index INTEGER NOT NULL,
                   chunk_hash BLOB NOT NULL CHECK(length(chunk_hash) = 32),
                   PRIMARY KEY(transfer_id, item_id, chunk_index)
                 );
                 CREATE TABLE IF NOT EXISTS settings (
                   key TEXT PRIMARY KEY,
                   value_json TEXT NOT NULL
                 );
                 CREATE INDEX IF NOT EXISTS idx_transfers_updated ON transfers(updated_unix_ms);
                 CREATE INDEX IF NOT EXISTS idx_transfer_items_transfer_id ON transfer_items(transfer_id);
                 CREATE INDEX IF NOT EXISTS idx_completed_chunks_transfer ON completed_chunks(transfer_id);",
            )?;
        }
        if existing_version != SCHEMA_VERSION {
            connection.pragma_update(None, "user_version", SCHEMA_VERSION)?;
        }
        Ok(Self {
            connection: Mutex::new(connection),
        })
    }

    pub fn clear_history(&self) -> Result<usize, RepositoryError> {
        let connection = self.lock()?;
        let count = connection.execute(
            "DELETE FROM transfers WHERE state IN ('\"completed\"', '\"failed\"', '\"cancelled\"')",
            [],
        )?;
        let _ = connection.execute("PRAGMA incremental_vacuum;", []);
        Ok(count)
    }

    pub fn delete_history_transfer(&self, id: &str) -> Result<bool, RepositoryError> {
        let connection = self.lock()?;
        let count = connection.execute("DELETE FROM transfers WHERE id = ?1", params![id])?;
        Ok(count > 0)
    }

    pub fn schema_version(&self) -> Result<i64, RepositoryError> {
        let connection = self.lock()?;
        Ok(connection.pragma_query_value(None, "user_version", |row| row.get(0))?)
    }

    pub fn save_transfer(&self, transfer: &TransferSnapshot) -> Result<(), RepositoryError> {
        let direction = serde_json::to_string(&transfer.direction)?;
        let state = serde_json::to_string(&transfer.state)?;
        let connection = self.lock()?;
        connection.execute(
            "INSERT INTO transfers (
               id, peer_id, peer_name, direction, state, item_count, total_bytes,
               completed_bytes, bytes_per_second, created_unix_ms, updated_unix_ms, error
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
             ON CONFLICT(id) DO UPDATE SET
               peer_id = excluded.peer_id,
               peer_name = excluded.peer_name,
               direction = excluded.direction,
               state = excluded.state,
               item_count = excluded.item_count,
               total_bytes = excluded.total_bytes,
               completed_bytes = excluded.completed_bytes,
               bytes_per_second = excluded.bytes_per_second,
               updated_unix_ms = excluded.updated_unix_ms,
               error = excluded.error",
            params![
                transfer.id.to_string(),
                transfer.peer_id,
                transfer.peer_name,
                direction,
                state,
                transfer.item_count,
                transfer.total_bytes,
                transfer.completed_bytes,
                transfer.bytes_per_second,
                transfer.created_unix_ms,
                transfer.updated_unix_ms,
                transfer.error,
            ],
        )?;
        Ok(())
    }

    pub fn list_transfers(&self) -> Result<Vec<TransferSnapshot>, RepositoryError> {
        let connection = self.lock()?;
        let mut statement = connection.prepare(
            "SELECT id, peer_id, peer_name, direction, state, item_count, total_bytes,
                    completed_bytes, bytes_per_second, created_unix_ms, updated_unix_ms, error
             FROM transfers ORDER BY created_unix_ms DESC, id DESC",
        )?;
        let rows = statement.query_map([], decode_transfer)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn trust_peer(&self, peer: &TrustedPeer) -> Result<(), RepositoryError> {
        let connection = self.lock()?;
        connection.execute(
            "INSERT INTO trusted_peers (
               peer_id, display_name, certificate_fingerprint, created_unix_ms,
               last_seen_unix_ms, auto_accept
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(peer_id) DO UPDATE SET
               display_name = excluded.display_name,
               certificate_fingerprint = excluded.certificate_fingerprint,
               last_seen_unix_ms = excluded.last_seen_unix_ms,
               auto_accept = excluded.auto_accept",
            params![
                peer.peer_id,
                peer.display_name,
                peer.certificate_fingerprint.as_slice(),
                peer.created_unix_ms,
                peer.last_seen_unix_ms,
                peer.auto_accept,
            ],
        )?;
        Ok(())
    }

    pub fn trusted_peer(&self, peer_id: &str) -> Result<Option<TrustedPeer>, RepositoryError> {
        let connection = self.lock()?;
        connection
            .query_row(
                "SELECT peer_id, display_name, certificate_fingerprint, created_unix_ms,
                        last_seen_unix_ms, auto_accept
                 FROM trusted_peers WHERE peer_id = ?1",
                [peer_id],
                |row| {
                    let fingerprint: Vec<u8> = row.get(2)?;
                    let certificate_fingerprint =
                        fingerprint.try_into().map_err(|value: Vec<u8>| {
                            rusqlite::Error::FromSqlConversionFailure(
                                2,
                                Type::Blob,
                                format!("invalid fingerprint length: {}", value.len()).into(),
                            )
                        })?;
                    Ok(TrustedPeer {
                        peer_id: row.get(0)?,
                        display_name: row.get(1)?,
                        certificate_fingerprint,
                        created_unix_ms: row.get(3)?,
                        last_seen_unix_ms: row.get(4)?,
                        auto_accept: row.get(5)?,
                    })
                },
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn save_item(&self, item: &StoredTransferItem) -> Result<(), RepositoryError> {
        let kind = serde_json::to_string(&item.kind)?;
        let connection = self.lock()?;
        connection.execute(
            "INSERT INTO transfer_items (
               id, transfer_id, relative_path, entry_kind, size, modified_unix_ms,
               source_revision, temporary_ref, final_ref
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
             ON CONFLICT(id) DO UPDATE SET
               relative_path = excluded.relative_path,
               entry_kind = excluded.entry_kind,
               size = excluded.size,
               modified_unix_ms = excluded.modified_unix_ms,
               source_revision = excluded.source_revision,
               temporary_ref = excluded.temporary_ref,
               final_ref = excluded.final_ref",
            params![
                item.id,
                item.transfer_id.to_string(),
                item.relative_path,
                kind,
                item.size,
                item.modified_unix_ms,
                item.source_revision,
                item.temporary_ref,
                item.final_ref,
            ],
        )?;
        Ok(())
    }

    pub fn list_items(
        &self,
        transfer_id: Uuid,
    ) -> Result<Vec<StoredTransferItem>, RepositoryError> {
        let connection = self.lock()?;
        let mut statement = connection.prepare(
            "SELECT id, transfer_id, relative_path, entry_kind, size, modified_unix_ms,
                    source_revision, temporary_ref, final_ref
             FROM transfer_items WHERE transfer_id = ?1 ORDER BY relative_path, id",
        )?;
        let rows = statement.query_map([transfer_id.to_string()], |row| {
            let raw_transfer_id: String = row.get(1)?;
            let raw_kind: String = row.get(3)?;
            Ok(StoredTransferItem {
                id: row.get(0)?,
                transfer_id: Uuid::parse_str(&raw_transfer_id).map_err(|error| {
                    rusqlite::Error::FromSqlConversionFailure(1, Type::Text, Box::new(error))
                })?,
                relative_path: row.get(2)?,
                kind: serde_json::from_str(&raw_kind).map_err(|error| {
                    rusqlite::Error::FromSqlConversionFailure(3, Type::Text, Box::new(error))
                })?,
                size: row.get(4)?,
                modified_unix_ms: row.get(5)?,
                source_revision: row.get(6)?,
                temporary_ref: row.get(7)?,
                final_ref: row.get(8)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn list_history_files(&self) -> Result<Vec<HistoryFileRecord>, RepositoryError> {
        let connection = self.lock()?;
        let mut statement = connection.prepare(
            "SELECT i.id, i.transfer_id, i.relative_path, i.entry_kind, i.size,
                    i.temporary_ref, i.final_ref, t.peer_name, t.direction, t.updated_unix_ms
             FROM transfer_items i
             INNER JOIN transfers t ON i.transfer_id = t.id
             WHERE t.state = '\"completed\"' OR t.state = 'completed'
             ORDER BY t.updated_unix_ms DESC, i.relative_path ASC",
        )?;
        let rows = statement.query_map([], |row| {
            let relative_path: String = row.get(2)?;
            let raw_kind: String = row.get(3)?;
            let kind: EntryKind = serde_json::from_str(&raw_kind).unwrap_or(EntryKind::File);
            let temporary_ref: Option<String> = row.get(5)?;
            let final_ref: Option<String> = row.get(6)?;
            let raw_direction: String = row.get(8)?;
            let direction: TransferDirection =
                serde_json::from_str(&raw_direction).unwrap_or(TransferDirection::Incoming);

            let local_path = match direction {
                TransferDirection::Incoming => final_ref.or(temporary_ref),
                TransferDirection::Outgoing => temporary_ref.or(final_ref),
            };

            let file_name = std::path::Path::new(&relative_path)
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| relative_path.clone());

            Ok(HistoryFileRecord {
                id: row.get(0)?,
                transfer_id: row.get(1)?,
                file_name,
                relative_path,
                local_path,
                is_directory: kind == EntryKind::Directory,
                size: row.get(4)?,
                peer_name: row.get(7)?,
                direction,
                completed_unix_ms: row.get(9)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn mark_chunk_complete(
        &self,
        transfer_id: Uuid,
        item_id: &str,
        chunk_index: u32,
        chunk_hash: &[u8; 32],
    ) -> Result<bool, RepositoryError> {
        let connection = self.lock()?;
        let inserted = connection.execute(
            "INSERT OR IGNORE INTO completed_chunks (
               transfer_id, item_id, chunk_index, chunk_hash
             ) VALUES (?1, ?2, ?3, ?4)",
            params![
                transfer_id.to_string(),
                item_id,
                chunk_index,
                chunk_hash.as_slice(),
            ],
        )?;
        Ok(inserted != 0)
    }

    pub fn completed_chunks(
        &self,
        transfer_id: Uuid,
        item_id: &str,
    ) -> Result<Vec<u32>, RepositoryError> {
        let connection = self.lock()?;
        let mut statement = connection.prepare(
            "SELECT chunk_index FROM completed_chunks
             WHERE transfer_id = ?1 AND item_id = ?2 ORDER BY chunk_index",
        )?;
        let rows =
            statement.query_map(params![transfer_id.to_string(), item_id], |row| row.get(0))?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn clear_completed_chunks(&self, transfer_id: Uuid) -> Result<(), RepositoryError> {
        let connection = self.lock()?;
        connection.execute(
            "DELETE FROM completed_chunks WHERE transfer_id = ?1",
            [transfer_id.to_string()],
        )?;
        Ok(())
    }

    pub fn save_setting<T: Serialize>(&self, key: &str, value: &T) -> Result<(), RepositoryError> {
        let encoded = serde_json::to_string(value)?;
        let connection = self.lock()?;
        connection.execute(
            "INSERT INTO settings (key, value_json) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value_json = excluded.value_json",
            params![key, encoded],
        )?;
        Ok(())
    }

    pub fn setting<T: DeserializeOwned>(&self, key: &str) -> Result<Option<T>, RepositoryError> {
        let connection = self.lock()?;
        let encoded: Option<String> = connection
            .query_row(
                "SELECT value_json FROM settings WHERE key = ?1",
                [key],
                |row| row.get(0),
            )
            .optional()?;
        encoded
            .map(|value| serde_json::from_str(&value))
            .transpose()
            .map_err(Into::into)
    }

    pub fn remove_transfer(&self, id: Uuid) -> Result<(), RepositoryError> {
        let connection = self.lock()?;
        connection.execute("DELETE FROM transfers WHERE id = ?1", [id.to_string()])?;
        Ok(())
    }

    pub fn remove_trusted_peer(&self, peer_id: &str) -> Result<bool, RepositoryError> {
        let connection = self.lock()?;
        let deleted =
            connection.execute("DELETE FROM trusted_peers WHERE peer_id = ?1", [peer_id])?;
        Ok(deleted > 0)
    }

    fn lock(&self) -> Result<std::sync::MutexGuard<'_, Connection>, RepositoryError> {
        self.connection
            .lock()
            .map_err(|_| RepositoryError::LockPoisoned)
    }
}

fn decode_transfer(row: &rusqlite::Row<'_>) -> rusqlite::Result<TransferSnapshot> {
    let id: String = row.get(0)?;
    let direction: String = row.get(3)?;
    let state: String = row.get(4)?;
    Ok(TransferSnapshot {
        id: Uuid::parse_str(&id).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(0, Type::Text, Box::new(error))
        })?,
        peer_id: row.get(1)?,
        peer_name: row.get(2)?,
        direction: serde_json::from_str::<TransferDirection>(&direction).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(3, Type::Text, Box::new(error))
        })?,
        state: serde_json::from_str::<TransferState>(&state).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(4, Type::Text, Box::new(error))
        })?,
        item_count: row.get(5)?,
        total_bytes: row.get(6)?,
        completed_bytes: row.get(7)?,
        bytes_per_second: row.get(8)?,
        created_unix_ms: row.get(9)?,
        updated_unix_ms: row.get(10)?,
        error: row.get(11)?,
    })
}

#[derive(Debug, Error)]
pub enum RepositoryError {
    #[error("数据库错误: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("数据编码错误: {0}")]
    Json(#[from] serde_json::Error),
    #[error("数据库锁已损坏")]
    LockPoisoned,
    #[error("数据库版本 {0} 高于当前程序支持的版本")]
    UnsupportedSchema(i64),
}

#[cfg(test)]
mod tests {
    use crate::{
        manifest::EntryKind,
        model::{TransferSnapshot, TransferState, TrustedPeer},
    };
    use rusqlite::Connection;
    use uuid::Uuid;

    use super::{StoredTransferItem, TransferRepository};

    #[test]
    fn repository_restores_transfers_and_pinned_peers() {
        let repository = TransferRepository::in_memory().expect("repository");
        let transfer = TransferSnapshot::new_outgoing("peer-7", "书房电脑", 3, 9_000, 123);
        repository
            .save_transfer(&transfer)
            .expect("save transfer snapshot");

        let trusted = TrustedPeer {
            peer_id: "peer-7".to_owned(),
            display_name: "书房电脑".to_owned(),
            certificate_fingerprint: [0x5a; 32],
            created_unix_ms: 100,
            last_seen_unix_ms: 123,
            auto_accept: false,
        };
        repository.trust_peer(&trusted).expect("pin peer");

        assert_eq!(
            repository.list_transfers().expect("history"),
            vec![transfer]
        );
        assert_eq!(
            repository.trusted_peer("peer-7").expect("lookup"),
            Some(trusted)
        );
    }

    #[test]
    fn version_one_database_migrates_without_losing_transfers() {
        let connection = Connection::open_in_memory().expect("legacy database");
        connection
            .execute_batch(
                "CREATE TABLE transfers (
                   id TEXT PRIMARY KEY,
                   peer_id TEXT NOT NULL,
                   peer_name TEXT NOT NULL,
                   direction TEXT NOT NULL,
                   state TEXT NOT NULL,
                   item_count INTEGER NOT NULL,
                   total_bytes INTEGER NOT NULL,
                   completed_bytes INTEGER NOT NULL,
                   created_unix_ms INTEGER NOT NULL,
                   updated_unix_ms INTEGER NOT NULL,
                   error TEXT
                 );
                 PRAGMA user_version = 1;",
            )
            .expect("legacy schema");
        let repository = TransferRepository::from_connection(connection).expect("migration");
        let transfer = TransferSnapshot::new_outgoing("peer", "旧设备", 1, 42, 7);
        repository
            .save_transfer(&transfer)
            .expect("save after migration");

        assert_eq!(
            repository.schema_version().expect("version"),
            super::SCHEMA_VERSION
        );
        assert_eq!(
            repository.list_transfers().expect("history"),
            vec![transfer]
        );
    }

    #[test]
    fn clear_history_removes_completed_and_cancelled_transfers() {
        let repository = TransferRepository::in_memory().expect("repository");
        let active = TransferSnapshot::new_outgoing("p1", "活跃设备", 1, 100, 10);
        let mut completed =
            TransferSnapshot::new_incoming(Uuid::new_v4(), "p2", "完成设备", 1, 100, 100);
        completed.state = TransferState::Completed;

        repository.save_transfer(&active).expect("save active");
        repository
            .save_transfer(&completed)
            .expect("save completed");

        assert_eq!(repository.list_transfers().expect("list").len(), 2);
        let deleted = repository.clear_history().expect("clear");
        assert_eq!(deleted, 1);
        let remaining = repository.list_transfers().expect("list");
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].id, active.id);
    }

    #[test]
    fn remove_trusted_peer_deletes_row_and_returns_true_only_once() {
        let repository = TransferRepository::in_memory().expect("repository");
        let trusted = TrustedPeer {
            peer_id: "peer-9".to_owned(),
            display_name: "客厅手机".to_owned(),
            certificate_fingerprint: [0x1f; 32],
            created_unix_ms: 3,
            last_seen_unix_ms: 99,
            auto_accept: false,
        };
        repository.trust_peer(&trusted).expect("insert");
        assert!(
            repository
                .remove_trusted_peer("peer-9")
                .expect("first removal")
        );
        assert!(repository.trusted_peer("peer-9").expect("lookup").is_none());
        assert!(
            !repository
                .remove_trusted_peer("peer-9")
                .expect("second removal")
        );
    }

    #[test]
    fn list_history_files_returns_completed_items() {
        let repository = TransferRepository::in_memory().expect("repository");
        let mut completed_transfer =
            TransferSnapshot::new_outgoing("peer-1", "办公电脑", 2, 1024, 100);
        completed_transfer.state = TransferState::Completed;
        completed_transfer.updated_unix_ms = 200;
        repository
            .save_transfer(&completed_transfer)
            .expect("save completed transfer");

        let item1 = StoredTransferItem {
            id: "item-1".to_owned(),
            transfer_id: completed_transfer.id,
            relative_path: "docs/report.pdf".to_owned(),
            kind: EntryKind::File,
            size: 512,
            modified_unix_ms: 100,
            source_revision: None,
            temporary_ref: Some("D:/docs/report.pdf".to_owned()),
            final_ref: None,
        };
        let item2 = StoredTransferItem {
            id: "item-2".to_owned(),
            transfer_id: completed_transfer.id,
            relative_path: "images/photo.png".to_owned(),
            kind: EntryKind::File,
            size: 512,
            modified_unix_ms: 100,
            source_revision: None,
            temporary_ref: Some("D:/images/photo.png".to_owned()),
            final_ref: None,
        };
        repository.save_item(&item1).expect("save item 1");
        repository.save_item(&item2).expect("save item 2");

        let history = repository.list_history_files().expect("history files");
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].file_name, "report.pdf");
        assert_eq!(history[0].local_path, Some("D:/docs/report.pdf".to_owned()));
        assert_eq!(history[0].peer_name, "办公电脑");
        assert_eq!(history[1].file_name, "photo.png");
    }
}
