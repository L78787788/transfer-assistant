use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use thiserror::Error;

const MAX_PATH_BYTES: usize = 4_096;
const MAX_SEGMENT_BYTES: usize = 255;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EntryKind {
    File,
    Directory,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManifestEntry {
    pub relative_path: String,
    pub kind: EntryKind,
    pub size: u64,
    pub modified_unix_ms: i64,
}

impl ManifestEntry {
    pub fn file(relative_path: impl Into<String>, size: u64, modified_unix_ms: i64) -> Self {
        Self {
            relative_path: relative_path.into(),
            kind: EntryKind::File,
            size,
            modified_unix_ms,
        }
    }

    pub fn directory(relative_path: impl Into<String>, modified_unix_ms: i64) -> Self {
        Self {
            relative_path: relative_path.into(),
            kind: EntryKind::Directory,
            size: 0,
            modified_unix_ms,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransferManifest {
    pub entries: Vec<ManifestEntry>,
}

impl TransferManifest {
    pub fn new(entries: Vec<ManifestEntry>) -> Self {
        Self { entries }
    }

    pub fn validate(&self) -> Result<(), ManifestError> {
        let mut paths = HashSet::with_capacity(self.entries.len());

        for entry in &self.entries {
            validate_relative_path(&entry.relative_path)?;

            if entry.kind == EntryKind::Directory && entry.size != 0 {
                return Err(ManifestError::DirectoryHasSize(entry.relative_path.clone()));
            }
            if !paths.insert(entry.relative_path.as_str()) {
                return Err(ManifestError::DuplicatePath(entry.relative_path.clone()));
            }
        }

        Ok(())
    }
}

fn validate_relative_path(path: &str) -> Result<(), ManifestError> {
    if path.is_empty() {
        return Err(ManifestError::EmptyPath);
    }
    if path.len() > MAX_PATH_BYTES {
        return Err(ManifestError::PathTooLong(path.to_owned()));
    }
    if path.starts_with('/') || path.starts_with('\\') || has_windows_drive_prefix(path) {
        return Err(ManifestError::AbsolutePath(path.to_owned()));
    }
    if path.contains('\\') {
        return Err(ManifestError::Backslash(path.to_owned()));
    }

    for segment in path.split('/') {
        if segment == ".." {
            return Err(ManifestError::ParentTraversal(path.to_owned()));
        }
        if segment.is_empty() || segment == "." {
            return Err(ManifestError::InvalidSegment(path.to_owned()));
        }
        if segment.len() > MAX_SEGMENT_BYTES {
            return Err(ManifestError::SegmentTooLong(path.to_owned()));
        }
        if segment.contains('\0') {
            return Err(ManifestError::InvalidSegment(path.to_owned()));
        }
    }

    Ok(())
}

fn has_windows_drive_prefix(path: &str) -> bool {
    let bytes = path.as_bytes();
    bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':'
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ManifestError {
    #[error("清单路径不能为空")]
    EmptyPath,
    #[error("清单路径不能是绝对路径: {0}")]
    AbsolutePath(String),
    #[error("清单路径必须使用正斜杠: {0}")]
    Backslash(String),
    #[error("清单路径不能包含父目录跳转: {0}")]
    ParentTraversal(String),
    #[error("清单路径包含无效片段: {0}")]
    InvalidSegment(String),
    #[error("清单路径过长: {0}")]
    PathTooLong(String),
    #[error("清单路径片段过长: {0}")]
    SegmentTooLong(String),
    #[error("清单中包含重复路径: {0}")]
    DuplicatePath(String),
    #[error("目录条目的大小必须为零: {0}")]
    DirectoryHasSize(String),
}

#[cfg(test)]
mod tests {
    use super::{ManifestEntry, TransferManifest};

    #[test]
    fn manifest_rejects_parent_directory_traversal() {
        let manifest = TransferManifest::new(vec![ManifestEntry::file(
            "photos/../../private.txt",
            12,
            1_700_000_000,
        )]);

        let error = manifest.validate().expect_err("traversal must be rejected");

        assert_eq!(
            error.to_string(),
            "清单路径不能包含父目录跳转: photos/../../private.txt"
        );
    }
}
