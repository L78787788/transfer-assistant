use uuid::Uuid;

use crate::{
    core::{CoreError, CoreInner},
    identity::{certificate_fingerprint, derive_pairing_code},
    lan::LanError,
    manifest::EntryKind,
    outgoing::OutgoingJob,
    protocol::{PROTOCOL_MAJOR, PROTOCOL_MINOR, wire},
};

pub(crate) fn hello_envelope(inner: &CoreInner) -> Result<wire::Envelope, CoreError> {
    Ok(envelope(wire::envelope::Payload::Hello(wire::Hello {
        device_id: inner.identity.device_id().to_owned(),
        display_name: inner.config()?.device_name,
        certificate_fingerprint: inner.identity.fingerprint().to_vec(),
        capabilities: if cfg!(target_os = "android") { 1 } else { 0 },
    })))
}

pub(crate) fn connection_open(
    kind: wire::ConnectionKind,
    transfer_id: Uuid,
    token: &[u8],
    channel_index: u32,
) -> wire::Envelope {
    envelope(wire::envelope::Payload::ConnectionOpen(
        wire::ConnectionOpen {
            kind: kind as i32,
            transfer_id: transfer_id.to_string(),
            transfer_token: token.to_vec(),
            channel_index,
        },
    ))
}

pub(crate) fn offer_envelope(transfer_id: Uuid, job: &OutgoingJob) -> wire::Envelope {
    envelope(wire::envelope::Payload::TransferOffer(
        wire::TransferOffer {
            transfer_id: transfer_id.to_string(),
            item_count: job.item_count,
            directory_count: job
                .entries
                .iter()
                .filter(|entry| entry.kind == EntryKind::Directory)
                .count() as u32,
            total_bytes: job.total_bytes,
            top_level_names: job.top_level_names.clone(),
        },
    ))
}

pub(crate) fn pairing_confirmation(confirmed: bool, remember_peer: bool) -> wire::Envelope {
    envelope(wire::envelope::Payload::PairingConfirmation(
        wire::PairingConfirmation {
            confirmed,
            remember_peer,
        },
    ))
}

pub(crate) fn decision_envelope(
    transfer_id: Uuid,
    accepted: bool,
    reason: &str,
    token: &[u8],
    channel_count: u32,
) -> wire::Envelope {
    envelope(wire::envelope::Payload::OfferDecision(
        wire::OfferDecision {
            transfer_id: transfer_id.to_string(),
            accepted,
            reason: reason.to_owned(),
            transfer_token: token.to_vec(),
            data_channel_count: channel_count,
        },
    ))
}

pub(crate) fn result_envelope(transfer_id: Uuid, completed: bool, error: &str) -> wire::Envelope {
    envelope(wire::envelope::Payload::TransferResult(
        wire::TransferResult {
            transfer_id: transfer_id.to_string(),
            completed,
            error: error.to_owned(),
        },
    ))
}

pub(crate) fn control_envelope(transfer_id: Uuid, action: wire::ControlAction) -> wire::Envelope {
    envelope(wire::envelope::Payload::TransferControl(
        wire::TransferControl {
            transfer_id: transfer_id.to_string(),
            action: action as i32,
        },
    ))
}

pub(crate) fn envelope(payload: wire::envelope::Payload) -> wire::Envelope {
    wire::Envelope {
        protocol_major: PROTOCOL_MAJOR,
        protocol_minor: PROTOCOL_MINOR,
        payload: Some(payload),
    }
}

macro_rules! expect_payload {
    ($name:ident, $variant:ident, $type:ty) => {
        #[allow(dead_code)]
        pub(crate) fn $name(envelope: wire::Envelope) -> Result<$type, LanError> {
            match envelope.payload {
                Some(wire::envelope::Payload::$variant(value)) => Ok(value),
                _ => Err(LanError::UnexpectedMessage(stringify!($variant))),
            }
        }
    };
}

expect_payload!(expect_connection_open, ConnectionOpen, wire::ConnectionOpen);
expect_payload!(expect_hello, Hello, wire::Hello);
expect_payload!(expect_offer, TransferOffer, wire::TransferOffer);
expect_payload!(expect_manifest, ManifestPage, wire::ManifestPage);
expect_payload!(
    expect_pairing,
    PairingConfirmation,
    wire::PairingConfirmation
);
expect_payload!(expect_decision, OfferDecision, wire::OfferDecision);
expect_payload!(expect_resume, ResumeState, wire::ResumeState);
expect_payload!(expect_result, TransferResult, wire::TransferResult);
expect_payload!(expect_control, TransferControl, wire::TransferControl);

pub(crate) fn validate_hello(
    hello: &wire::Hello,
    tls_fingerprint: &[u8; 32],
) -> Result<(), LanError> {
    if hello.certificate_fingerprint.as_slice() != tls_fingerprint {
        return Err(LanError::CertificateFingerprintMismatch);
    }
    let expected_id = tls_fingerprint
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    if hello.device_id != expected_id || hello.display_name.trim().is_empty() {
        return Err(LanError::InvalidHello);
    }
    Ok(())
}

pub(crate) fn tls_peer_fingerprint(
    certificates: Option<&[rustls::pki_types::CertificateDer<'static>]>,
) -> Result<[u8; 32], LanError> {
    let certificate = certificates
        .and_then(|values| values.first())
        .ok_or(LanError::MissingPeerCertificate)?;
    Ok(certificate_fingerprint(certificate.as_ref()))
}

pub(crate) fn pairing_code(inner: &CoreInner, remote: &[u8; 32], remote_id: &str) -> String {
    let mut ids = [inner.identity.device_id(), remote_id];
    ids.sort_unstable();
    derive_pairing_code(
        &inner.identity.fingerprint(),
        remote,
        format!("{}|{}", ids[0], ids[1]).as_bytes(),
    )
}

pub(crate) fn transfer_token(
    transfer_id: Uuid,
    local_fingerprint: &[u8; 32],
    remote_fingerprint: &[u8; 32],
) -> [u8; 32] {
    let nonce = Uuid::new_v4();
    *blake3::hash(
        &[
            transfer_id.as_bytes().as_slice(),
            nonce.as_bytes().as_slice(),
            local_fingerprint.as_slice(),
            remote_fingerprint.as_slice(),
        ]
        .concat(),
    )
    .as_bytes()
}

pub(crate) fn device_kind(capabilities: u64) -> String {
    if capabilities & 1 != 0 {
        "phone".to_owned()
    } else {
        "computer".to_owned()
    }
}
