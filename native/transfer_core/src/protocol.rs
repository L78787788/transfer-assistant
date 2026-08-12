use prost::Message;
use thiserror::Error;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

pub const PROTOCOL_MAJOR: u32 = 1;
pub const PROTOCOL_MINOR: u32 = 0;
pub const MAX_CONTROL_FRAME_BYTES: usize = 1024 * 1024;

pub mod wire {
    include!(concat!(env!("OUT_DIR"), "/transassist.v1.rs"));
}

pub async fn write_envelope<W>(
    writer: &mut W,
    envelope: &wire::Envelope,
) -> Result<(), ProtocolError>
where
    W: AsyncWrite + Unpin,
{
    let encoded = envelope.encode_to_vec();
    if encoded.len() > MAX_CONTROL_FRAME_BYTES {
        return Err(ProtocolError::FrameTooLarge {
            actual: encoded.len(),
            maximum: MAX_CONTROL_FRAME_BYTES,
        });
    }
    let length = u32::try_from(encoded.len()).expect("control frame limit fits u32");
    writer.write_all(&length.to_be_bytes()).await?;
    writer.write_all(&encoded).await?;
    writer.flush().await?;
    Ok(())
}

pub async fn read_envelope<R>(reader: &mut R) -> Result<wire::Envelope, ProtocolError>
where
    R: AsyncRead + Unpin,
{
    let mut length_bytes = [0_u8; 4];
    reader.read_exact(&mut length_bytes).await?;
    let length = u32::from_be_bytes(length_bytes) as usize;
    if length > MAX_CONTROL_FRAME_BYTES {
        return Err(ProtocolError::FrameTooLarge {
            actual: length,
            maximum: MAX_CONTROL_FRAME_BYTES,
        });
    }

    let mut encoded = vec![0_u8; length];
    reader.read_exact(&mut encoded).await?;
    let envelope = wire::Envelope::decode(encoded.as_slice())?;
    if envelope.protocol_major != PROTOCOL_MAJOR {
        return Err(ProtocolError::IncompatibleVersion {
            local_major: PROTOCOL_MAJOR,
            remote_major: envelope.protocol_major,
        });
    }
    Ok(envelope)
}

#[derive(Debug, Error)]
pub enum ProtocolError {
    #[error("网络读写失败: {0}")]
    Io(#[from] std::io::Error),
    #[error("控制消息无法解析: {0}")]
    Decode(#[from] prost::DecodeError),
    #[error("控制消息长度 {actual} 超过上限 {maximum}")]
    FrameTooLarge { actual: usize, maximum: usize },
    #[error("协议主版本不兼容，本机 {local_major}，对端 {remote_major}")]
    IncompatibleVersion { local_major: u32, remote_major: u32 },
}

#[cfg(test)]
mod tests {
    use tokio::io::{AsyncReadExt, AsyncWriteExt, duplex};

    use super::{MAX_CONTROL_FRAME_BYTES, ProtocolError, read_envelope, wire, write_envelope};

    #[tokio::test]
    async fn control_frames_round_trip_and_reject_oversized_input() {
        let envelope = wire::Envelope {
            protocol_major: 1,
            protocol_minor: 0,
            payload: Some(wire::envelope::Payload::Hello(wire::Hello {
                device_id: "peer-a".to_owned(),
                display_name: "客厅手机".to_owned(),
                certificate_fingerprint: vec![7; 32],
                capabilities: 3,
            })),
        };
        let (mut writer, mut reader) = duplex(2 * 1024 * 1024);

        write_envelope(&mut writer, &envelope)
            .await
            .expect("write frame");
        assert_eq!(
            read_envelope(&mut reader).await.expect("read frame"),
            envelope
        );

        let oversized = u32::try_from(MAX_CONTROL_FRAME_BYTES + 1).expect("frame limit");
        writer
            .write_all(&oversized.to_be_bytes())
            .await
            .expect("malicious prefix");
        let error = read_envelope(&mut reader)
            .await
            .expect_err("must reject before allocation");
        assert!(matches!(error, ProtocolError::FrameTooLarge { .. }));
    }

    #[tokio::test]
    async fn protocol_v1_hello_keeps_golden_wire_bytes() {
        let envelope = wire::Envelope {
            protocol_major: 1,
            protocol_minor: 0,
            payload: Some(wire::envelope::Payload::Hello(wire::Hello {
                device_id: "a".to_owned(),
                display_name: String::new(),
                certificate_fingerprint: Vec::new(),
                capabilities: 0,
            })),
        };
        let (mut writer, mut reader) = duplex(32);

        write_envelope(&mut writer, &envelope)
            .await
            .expect("write golden frame");
        let mut encoded = [0_u8; 11];
        reader
            .read_exact(&mut encoded)
            .await
            .expect("read golden frame");

        assert_eq!(
            encoded,
            [0, 0, 0, 7, 0x08, 0x01, 0x52, 0x03, 0x0a, 0x01, b'a']
        );
    }
}
