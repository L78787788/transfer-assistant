use std::fs::File;

use prost::Message;
use thiserror::Error;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use crate::{
    chunk::{ChunkPlan, DEFAULT_CHUNK_SIZE, ResumeMap},
    protocol::wire::ChunkHeader,
    storage::{StorageError, VerifiedChunk, read_chunk, write_verified_chunk},
};

const MAX_CHUNK_HEADER_BYTES: usize = 64 * 1024;

pub async fn send_file_chunks<W>(
    mut writer: W,
    source: File,
    transfer_id: &str,
    item_id: &str,
    plan: ChunkPlan,
    resume: ResumeMap,
) -> Result<u32, TransferIoError>
where
    W: AsyncWrite + Unpin,
{
    let missing = plan.missing_chunks(&resume)?;
    let mut sent = 0_u32;

    for spec in missing {
        let source = source.try_clone()?;
        let chunk = tokio::task::spawn_blocking(move || read_chunk(&source, spec)).await??;
        let header = ChunkHeader {
            transfer_id: transfer_id.to_owned(),
            item_id: item_id.to_owned(),
            chunk_index: chunk.spec.index,
            offset: chunk.spec.offset,
            length: chunk.spec.length,
            blake3_hash: chunk.blake3_hash.to_vec(),
        };
        write_header(&mut writer, &header).await?;
        writer.write_all(&chunk.bytes).await?;
        sent += 1;
    }
    writer.shutdown().await?;
    Ok(sent)
}

pub async fn receive_file_chunks<R>(
    mut reader: R,
    target: File,
    transfer_id: &str,
    item_id: &str,
    file_size: u64,
    expected_chunks: u32,
) -> Result<u32, TransferIoError>
where
    R: AsyncRead + Unpin,
{
    for received in 0..expected_chunks {
        let header = read_header(&mut reader).await?;
        validate_header(&header, transfer_id, item_id, file_size)?;
        let mut bytes = vec![0_u8; header.length as usize];
        reader.read_exact(&mut bytes).await?;
        let blake3_hash: [u8; 32] = header
            .blake3_hash
            .try_into()
            .map_err(|hash: Vec<u8>| TransferIoError::InvalidHashLength { actual: hash.len() })?;
        let chunk = VerifiedChunk {
            spec: crate::chunk::ChunkSpec {
                index: header.chunk_index,
                offset: header.offset,
                length: header.length,
            },
            blake3_hash,
            bytes,
        };
        let target = target.try_clone()?;
        tokio::task::spawn_blocking(move || write_verified_chunk(&target, &chunk)).await??;

        if received + 1 == expected_chunks {
            return Ok(expected_chunks);
        }
    }
    Ok(0)
}

pub(crate) async fn write_header<W>(
    writer: &mut W,
    header: &ChunkHeader,
) -> Result<(), TransferIoError>
where
    W: AsyncWrite + Unpin,
{
    let encoded = header.encode_to_vec();
    if encoded.len() > MAX_CHUNK_HEADER_BYTES {
        return Err(TransferIoError::HeaderTooLarge(encoded.len()));
    }
    let length = u32::try_from(encoded.len()).expect("header limit fits u32");
    writer.write_all(&length.to_be_bytes()).await?;
    writer.write_all(&encoded).await?;
    Ok(())
}

pub(crate) async fn read_header<R>(reader: &mut R) -> Result<ChunkHeader, TransferIoError>
where
    R: AsyncRead + Unpin,
{
    let mut length = [0_u8; 4];
    reader.read_exact(&mut length).await?;
    let length = u32::from_be_bytes(length) as usize;
    if length > MAX_CHUNK_HEADER_BYTES {
        return Err(TransferIoError::HeaderTooLarge(length));
    }
    let mut encoded = vec![0_u8; length];
    reader.read_exact(&mut encoded).await?;
    Ok(ChunkHeader::decode(encoded.as_slice())?)
}

pub(crate) fn validate_header(
    header: &ChunkHeader,
    transfer_id: &str,
    item_id: &str,
    file_size: u64,
) -> Result<(), TransferIoError> {
    if header.transfer_id != transfer_id || header.item_id != item_id {
        return Err(TransferIoError::UnexpectedChunk {
            transfer_id: header.transfer_id.clone(),
            item_id: header.item_id.clone(),
        });
    }
    if header.length == 0 || header.length > DEFAULT_CHUNK_SIZE {
        return Err(TransferIoError::InvalidChunkLength(header.length));
    }
    if header.offset.saturating_add(u64::from(header.length)) > file_size {
        return Err(TransferIoError::ChunkOutsideFile {
            offset: header.offset,
            length: header.length,
            file_size,
        });
    }
    if header.blake3_hash.len() != 32 {
        return Err(TransferIoError::InvalidHashLength {
            actual: header.blake3_hash.len(),
        });
    }
    Ok(())
}

#[derive(Debug, Error)]
pub enum TransferIoError {
    #[error("网络读写失败: {0}")]
    Io(#[from] std::io::Error),
    #[error("文件读写失败: {0}")]
    Storage(#[from] StorageError),
    #[error("数据块计划无效: {0}")]
    Chunk(#[from] crate::chunk::ChunkError),
    #[error("后台文件任务失败: {0}")]
    Join(#[from] tokio::task::JoinError),
    #[error("数据块头无法解析: {0}")]
    Decode(#[from] prost::DecodeError),
    #[error("数据块头长度 {0} 超过限制")]
    HeaderTooLarge(usize),
    #[error("收到不属于当前任务的数据块: {transfer_id}/{item_id}")]
    UnexpectedChunk {
        transfer_id: String,
        item_id: String,
    },
    #[error("数据块长度无效: {0}")]
    InvalidChunkLength(u32),
    #[error("数据块超出文件范围: offset={offset}, length={length}, file_size={file_size}")]
    ChunkOutsideFile {
        offset: u64,
        length: u32,
        file_size: u64,
    },
    #[error("BLAKE3 长度必须为 32 字节，实际 {actual}")]
    InvalidHashLength { actual: usize },
}

#[cfg(test)]
mod tests {
    use std::{fs::File, io::Write};

    use prost::Message;

    use crate::{
        chunk::{ChunkPlan, ResumeMap},
        storage::StorageError,
    };

    use super::{TransferIoError, receive_file_chunks, send_file_chunks};

    #[tokio::test]
    async fn file_pipeline_transfers_missing_chunks_with_bounded_io() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let source_path = directory.path().join("source.bin");
        let target_path = directory.path().join("target.part");
        let bytes: Vec<u8> = (0..(9 * 1024 * 1024 + 17))
            .map(|index| (index % 251) as u8)
            .collect();
        File::create(&source_path)
            .and_then(|mut file| file.write_all(&bytes))
            .expect("source fixture");
        let target = File::create(&target_path).expect("target file");
        target.set_len(bytes.len() as u64).expect("target length");

        let plan = ChunkPlan::new(bytes.len() as u64, 4 * 1024 * 1024).expect("chunk plan");
        let mut resume = ResumeMap::new(plan.chunk_count());
        resume.mark_complete(1).expect("resume middle chunk");
        let middle = plan.chunk(1).expect("middle spec");
        crate::storage::write_verified_chunk(
            &target,
            &crate::storage::read_chunk(
                &File::open(&source_path).expect("source for resume"),
                middle,
            )
            .expect("middle payload"),
        )
        .expect("seed resumed block");

        let (sender, receiver) = tokio::io::duplex(1024 * 1024);
        let send_task = tokio::spawn(async move {
            send_file_chunks(
                sender,
                File::open(source_path).expect("source file"),
                "transfer-1",
                "item-1",
                plan,
                resume,
            )
            .await
        });
        let received = receive_file_chunks(
            receiver,
            target,
            "transfer-1",
            "item-1",
            bytes.len() as u64,
            2,
        )
        .await
        .expect("receive chunks");
        send_task.await.expect("sender task").expect("send chunks");

        assert_eq!(received, 2, "the completed middle chunk is not resent");
        assert_eq!(std::fs::read(target_path).expect("target bytes"), bytes);
    }

    #[tokio::test]
    async fn corrupt_chunk_over_the_wire_is_rejected_without_write() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let source_path = directory.path().join("source.bin");
        let target_path = directory.path().join("target.part");
        let bytes: Vec<u8> = (0..(5 * 1024 * 1024))
            .map(|index| (index % 251) as u8)
            .collect();
        File::create(&source_path)
            .and_then(|mut file| file.write_all(&bytes))
            .expect("source fixture");
        let target = File::create(&target_path).expect("target file");
        target.set_len(bytes.len() as u64).expect("target length");
        let plan = ChunkPlan::new(bytes.len() as u64, 4 * 1024 * 1024).expect("chunk plan");
        let resume = ResumeMap::new(plan.chunk_count());

        // 接收端在本地监听，发送端经过中间的“篡改代理”传输：代理只翻转第一个块
        // payload 的第一个字节，保留头部中的原始 BLAKE3 哈希，模拟网络中损坏的数据。
        let receiver_listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("receiver listener");
        let receiver_address = receiver_listener.local_addr().expect("receiver address");
        let proxy_listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("proxy listener");
        let proxy_address = proxy_listener.local_addr().expect("proxy address");
        let total_length = bytes.len() as u64;

        let receive_task = tokio::spawn(async move {
            let (socket, _) = receiver_listener.accept().await.expect("accept receiver");
            receive_file_chunks(socket, target, "transfer-1", "item-1", total_length, 2).await
        });

        let proxy_task = tokio::spawn(async move {
            use tokio::io::{AsyncReadExt, AsyncWriteExt};
            let (mut sender_side, _) = proxy_listener.accept().await.expect("accept sender");
            let mut receiver_side = tokio::net::TcpStream::connect(receiver_address)
                .await
                .expect("connect receiver");
            let mut length = [0_u8; 4];
            sender_side
                .read_exact(&mut length)
                .await
                .expect("header length");
            let header_length = u32::from_be_bytes(length) as usize;
            let mut header = vec![0_u8; header_length];
            sender_side
                .read_exact(&mut header)
                .await
                .expect("header bytes");
            receiver_side
                .write_all(&length)
                .await
                .expect("forward length");
            receiver_side
                .write_all(&header)
                .await
                .expect("forward header");
            let header = crate::protocol::wire::ChunkHeader::decode(header.as_slice())
                .expect("decode forwarded header");
            let mut payload = vec![0_u8; header.length as usize];
            sender_side
                .read_exact(&mut payload)
                .await
                .expect("payload bytes");
            payload[0] ^= 0xff;
            receiver_side
                .write_all(&payload)
                .await
                .expect("forward payload");
            receiver_side.shutdown().await.expect("close data stream");
        });

        let send_result = tokio::time::timeout(std::time::Duration::from_secs(10), async {
            let socket = tokio::net::TcpStream::connect(proxy_address)
                .await
                .expect("connect proxy");
            send_file_chunks(
                socket,
                File::open(source_path).expect("source file"),
                "transfer-1",
                "item-1",
                plan,
                resume,
            )
            .await
        })
        .await;
        // 发送端可能因代理提前关闭而收到 BrokenPipe，属预期，忽略。
        if let Ok(result) = send_result {
            let _ = result;
        }
        proxy_task.await.expect("proxy task");
        let receive_result = receive_task.await.expect("receive task");

        assert!(
            matches!(
                receive_result,
                Err(TransferIoError::Storage(StorageError::HashMismatch { .. }))
            ),
            "损坏块必须被拒绝并返回哈希不匹配错误，实际: {receive_result:?}"
        );
        assert_eq!(
            std::fs::read(&target_path).expect("target bytes"),
            vec![0_u8; bytes.len()],
            "校验失败的块不得写入目标文件"
        );
    }
}
