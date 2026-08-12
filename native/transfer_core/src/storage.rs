use std::{
    fs::File,
    io::{Read, Write},
};

use thiserror::Error;

use crate::chunk::ChunkSpec;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedChunk {
    pub spec: ChunkSpec,
    pub blake3_hash: [u8; 32],
    pub bytes: Vec<u8>,
}

pub fn read_chunk(file: &File, spec: ChunkSpec) -> Result<VerifiedChunk, StorageError> {
    let mut bytes = vec![0_u8; spec.length as usize];
    read_exact_at(file, &mut bytes, spec.offset)?;
    let blake3_hash = *blake3::hash(&bytes).as_bytes();
    Ok(VerifiedChunk {
        spec,
        blake3_hash,
        bytes,
    })
}

pub fn read_sequential_chunk(file: &File, spec: ChunkSpec) -> Result<VerifiedChunk, StorageError> {
    let mut bytes = vec![0_u8; spec.length as usize];
    let mut reader = file;
    reader.read_exact(&mut bytes)?;
    Ok(VerifiedChunk {
        spec,
        blake3_hash: *blake3::hash(&bytes).as_bytes(),
        bytes,
    })
}

pub fn write_verified_chunk(file: &File, chunk: &VerifiedChunk) -> Result<(), StorageError> {
    verify_chunk(chunk)?;
    write_all_at(file, &chunk.bytes, chunk.spec.offset)?;
    Ok(())
}

pub fn write_sequential_verified_chunk(
    file: &File,
    chunk: &VerifiedChunk,
) -> Result<(), StorageError> {
    verify_chunk(chunk)?;
    let mut writer = file;
    writer.write_all(&chunk.bytes)?;
    Ok(())
}

fn verify_chunk(chunk: &VerifiedChunk) -> Result<(), StorageError> {
    if chunk.bytes.len() != chunk.spec.length as usize {
        return Err(StorageError::LengthMismatch {
            index: chunk.spec.index,
            declared: chunk.spec.length,
            actual: chunk.bytes.len(),
        });
    }

    let actual_hash = *blake3::hash(&chunk.bytes).as_bytes();
    if actual_hash != chunk.blake3_hash {
        return Err(StorageError::HashMismatch {
            index: chunk.spec.index,
        });
    }

    Ok(())
}

fn read_exact_at(file: &File, mut buffer: &mut [u8], mut offset: u64) -> Result<(), StorageError> {
    while !buffer.is_empty() {
        let read = positioned_read(file, buffer, offset)?;
        if read == 0 {
            return Err(StorageError::UnexpectedEof { offset });
        }
        offset += read as u64;
        buffer = &mut buffer[read..];
    }
    Ok(())
}

fn write_all_at(file: &File, mut buffer: &[u8], mut offset: u64) -> Result<(), StorageError> {
    while !buffer.is_empty() {
        let written = positioned_write(file, buffer, offset)?;
        if written == 0 {
            return Err(StorageError::WriteZero { offset });
        }
        offset += written as u64;
        buffer = &buffer[written..];
    }
    Ok(())
}

#[cfg(windows)]
fn positioned_read(file: &File, buffer: &mut [u8], offset: u64) -> std::io::Result<usize> {
    use std::os::windows::fs::FileExt;
    file.seek_read(buffer, offset)
}

#[cfg(windows)]
fn positioned_write(file: &File, buffer: &[u8], offset: u64) -> std::io::Result<usize> {
    use std::os::windows::fs::FileExt;
    file.seek_write(buffer, offset)
}

#[cfg(unix)]
fn positioned_read(file: &File, buffer: &mut [u8], offset: u64) -> std::io::Result<usize> {
    use std::os::unix::fs::FileExt;
    file.read_at(buffer, offset)
}

#[cfg(unix)]
fn positioned_write(file: &File, buffer: &[u8], offset: u64) -> std::io::Result<usize> {
    use std::os::unix::fs::FileExt;
    file.write_at(buffer, offset)
}

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("文件读写失败: {0}")]
    Io(#[from] std::io::Error),
    #[error("读取到文件末尾，位置 {offset}")]
    UnexpectedEof { offset: u64 },
    #[error("写入未产生数据，位置 {offset}")]
    WriteZero { offset: u64 },
    #[error("数据块 {index} 长度不匹配，声明 {declared}，实际 {actual}")]
    LengthMismatch {
        index: u32,
        declared: u32,
        actual: usize,
    },
    #[error("数据块 {index} 的 BLAKE3 校验失败")]
    HashMismatch { index: u32 },
}

#[cfg(test)]
mod tests {
    use std::{fs::File, io::Write};

    use crate::chunk::ChunkSpec;

    use super::{StorageError, read_chunk, write_verified_chunk};

    #[test]
    fn positioned_io_verifies_blake3_before_writing() {
        let source_dir = tempfile::tempdir().expect("source directory");
        let source_path = source_dir.path().join("source.bin");
        File::create(&source_path)
            .and_then(|mut file| file.write_all(b"abc"))
            .expect("source fixture");
        let source = File::open(&source_path).expect("source file");
        let chunk = read_chunk(
            &source,
            ChunkSpec {
                index: 0,
                offset: 0,
                length: 3,
            },
        )
        .expect("read verified chunk");
        assert_eq!(
            blake3::Hash::from_bytes(chunk.blake3_hash)
                .to_hex()
                .as_str(),
            "6437b3ac38465133ffb63b75273a8db548c558465d79db03fd359c6cd5bd9d85"
        );

        let target_path = source_dir.path().join("target.part");
        let target = File::create(&target_path).expect("target file");
        let mut corrupted = chunk.clone();
        corrupted.bytes[1] ^= 0xff;
        assert!(matches!(
            write_verified_chunk(&target, &corrupted),
            Err(StorageError::HashMismatch { .. })
        ));
        assert_eq!(target.metadata().expect("target metadata").len(), 0);

        write_verified_chunk(&target, &chunk).expect("valid chunk writes");
        assert_eq!(std::fs::read(target_path).expect("target bytes"), b"abc");
    }
}
