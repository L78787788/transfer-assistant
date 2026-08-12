use std::{fmt, fs, io, path::Path, sync::Arc};

use rustls::{
    ClientConfig, DigitallySignedStruct, DistinguishedName, Error as TlsError, ServerConfig,
    SignatureScheme,
    client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier},
    crypto::{WebPkiSupportedAlgorithms, verify_tls12_signature, verify_tls13_signature},
    pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer, ServerName, UnixTime},
    server::danger::{ClientCertVerified, ClientCertVerifier},
};
use sha2::{Digest, Sha256};
use thiserror::Error;

const PAIRING_DOMAIN: &[u8] = b"transassist-pair-v1\0";

#[derive(Clone)]
pub struct DeviceIdentity {
    certificate_der: Vec<u8>,
    private_key_der: Vec<u8>,
    fingerprint: [u8; 32],
    device_id: String,
}

impl DeviceIdentity {
    pub fn generate() -> Result<Self, IdentityError> {
        let signing_key = rcgen::KeyPair::generate_for(&rcgen::PKCS_ED25519)?;
        let certificate = rcgen::CertificateParams::new(vec!["transassist.local".to_owned()])?
            .self_signed(&signing_key)?;
        Self::from_der(certificate.der().to_vec(), signing_key.serialize_der())
    }

    pub fn from_der(
        certificate_der: Vec<u8>,
        private_key_der: Vec<u8>,
    ) -> Result<Self, IdentityError> {
        if certificate_der.is_empty() || private_key_der.is_empty() {
            return Err(IdentityError::EmptyMaterial);
        }
        let fingerprint: [u8; 32] = Sha256::digest(&certificate_der).into();
        let device_id = fingerprint
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        Ok(Self {
            certificate_der,
            private_key_der,
            fingerprint,
            device_id,
        })
    }

    pub fn load_or_generate(
        path: &Path,
        wrapping_key: Option<&[u8]>,
    ) -> Result<Self, IdentityError> {
        match fs::read(path) {
            Ok(encoded) => Self::decode_persisted(&encoded, wrapping_key),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                let identity = Self::generate()?;
                if let Some(parent) = path.parent() {
                    fs::create_dir_all(parent)?;
                }
                let temporary = path.with_extension("tmp");
                fs::write(&temporary, identity.encode_persisted(wrapping_key)?)?;
                fs::rename(temporary, path)?;
                Ok(identity)
            }
            Err(error) => Err(error.into()),
        }
    }

    fn encode_persisted(&self, wrapping_key: Option<&[u8]>) -> Result<Vec<u8>, IdentityError> {
        let protected_key = protect_private_key(&self.private_key_der, wrapping_key)?;
        let certificate_length = u32::try_from(self.certificate_der.len())
            .map_err(|_| IdentityError::InvalidPersistedIdentity)?;
        let key_length = u32::try_from(protected_key.len())
            .map_err(|_| IdentityError::InvalidPersistedIdentity)?;
        let mut encoded = Vec::with_capacity(8 + self.certificate_der.len() + protected_key.len());
        encoded.extend_from_slice(&certificate_length.to_be_bytes());
        encoded.extend_from_slice(&key_length.to_be_bytes());
        encoded.extend_from_slice(&self.certificate_der);
        encoded.extend_from_slice(&protected_key);
        Ok(encoded)
    }

    fn decode_persisted(
        encoded: &[u8],
        wrapping_key: Option<&[u8]>,
    ) -> Result<Self, IdentityError> {
        if encoded.len() < 8 {
            return Err(IdentityError::InvalidPersistedIdentity);
        }
        let certificate_length =
            u32::from_be_bytes(encoded[0..4].try_into().expect("four byte slice")) as usize;
        let key_length =
            u32::from_be_bytes(encoded[4..8].try_into().expect("four byte slice")) as usize;
        let certificate_end = 8_usize
            .checked_add(certificate_length)
            .ok_or(IdentityError::InvalidPersistedIdentity)?;
        let key_end = certificate_end
            .checked_add(key_length)
            .ok_or(IdentityError::InvalidPersistedIdentity)?;
        if key_end != encoded.len() {
            return Err(IdentityError::InvalidPersistedIdentity);
        }
        let private_key = unprotect_private_key(&encoded[certificate_end..key_end], wrapping_key)?;
        Self::from_der(encoded[8..certificate_end].to_vec(), private_key)
    }

    pub fn certificate_der(&self) -> &[u8] {
        &self.certificate_der
    }

    pub fn private_key_der(&self) -> &[u8] {
        &self.private_key_der
    }

    pub fn fingerprint(&self) -> [u8; 32] {
        self.fingerprint
    }

    pub fn device_id(&self) -> &str {
        &self.device_id
    }

    pub fn server_config(&self) -> Result<ServerConfig, IdentityError> {
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let verifier = AcceptAnyClientCertificate::new(provider.signature_verification_algorithms);
        let config = ServerConfig::builder_with_provider(provider)
            .with_protocol_versions(&[&rustls::version::TLS13])?
            .with_client_cert_verifier(Arc::new(verifier))
            .with_single_cert(self.certificate_chain(), self.private_key())?;
        Ok(config)
    }

    pub fn client_config(&self) -> Result<ClientConfig, IdentityError> {
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let verifier = AcceptAnyServerCertificate {
            supported: provider.signature_verification_algorithms,
        };
        let config = ClientConfig::builder_with_provider(provider)
            .with_protocol_versions(&[&rustls::version::TLS13])?
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(verifier))
            .with_client_auth_cert(self.certificate_chain(), self.private_key())?;
        Ok(config)
    }

    fn certificate_chain(&self) -> Vec<CertificateDer<'static>> {
        vec![CertificateDer::from(self.certificate_der.clone())]
    }

    fn private_key(&self) -> PrivateKeyDer<'static> {
        PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(self.private_key_der.clone()))
    }
}

pub fn certificate_fingerprint(certificate_der: &[u8]) -> [u8; 32] {
    Sha256::digest(certificate_der).into()
}

struct AcceptAnyServerCertificate {
    supported: WebPkiSupportedAlgorithms,
}

impl fmt::Debug for AcceptAnyServerCertificate {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AcceptAnyServerCertificate")
    }
}

impl ServerCertVerifier for AcceptAnyServerCertificate {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, TlsError> {
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        certificate: &CertificateDer<'_>,
        signature: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, TlsError> {
        verify_tls12_signature(message, certificate, signature, &self.supported)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        certificate: &CertificateDer<'_>,
        signature: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, TlsError> {
        verify_tls13_signature(message, certificate, signature, &self.supported)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.supported.supported_schemes()
    }
}

struct AcceptAnyClientCertificate {
    supported: WebPkiSupportedAlgorithms,
    root_hints: Vec<DistinguishedName>,
}

impl AcceptAnyClientCertificate {
    fn new(supported: WebPkiSupportedAlgorithms) -> Self {
        Self {
            supported,
            root_hints: Vec::new(),
        }
    }
}

impl fmt::Debug for AcceptAnyClientCertificate {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AcceptAnyClientCertificate")
    }
}

impl ClientCertVerifier for AcceptAnyClientCertificate {
    fn offer_client_auth(&self) -> bool {
        true
    }

    fn client_auth_mandatory(&self) -> bool {
        true
    }

    fn root_hint_subjects(&self) -> &[DistinguishedName] {
        &self.root_hints
    }

    fn verify_client_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _now: UnixTime,
    ) -> Result<ClientCertVerified, TlsError> {
        Ok(ClientCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        certificate: &CertificateDer<'_>,
        signature: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, TlsError> {
        verify_tls12_signature(message, certificate, signature, &self.supported)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        certificate: &CertificateDer<'_>,
        signature: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, TlsError> {
        verify_tls13_signature(message, certificate, signature, &self.supported)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.supported.supported_schemes()
    }
}

#[derive(Debug, Error)]
pub enum IdentityError {
    #[error("设备身份材料不能为空")]
    EmptyMaterial,
    #[error("无法生成设备身份: {0}")]
    Certificate(#[from] rcgen::Error),
    #[error("无法创建 TLS 配置: {0}")]
    Tls(#[from] rustls::Error),
    #[error("设备身份文件读写失败: {0}")]
    Io(#[from] io::Error),
    #[error("设备身份文件格式无效")]
    InvalidPersistedIdentity,
    #[error("设备身份加密或解密失败")]
    Encryption,
}

#[cfg(windows)]
fn protect_private_key(
    private_key: &[u8],
    _wrapping_key: Option<&[u8]>,
) -> Result<Vec<u8>, IdentityError> {
    crypt_protect(private_key, true)
}

#[cfg(windows)]
fn unprotect_private_key(
    private_key: &[u8],
    _wrapping_key: Option<&[u8]>,
) -> Result<Vec<u8>, IdentityError> {
    crypt_protect(private_key, false)
}

#[cfg(windows)]
fn crypt_protect(bytes: &[u8], protect: bool) -> Result<Vec<u8>, IdentityError> {
    use std::ptr;
    use windows_sys::Win32::{
        Foundation::LocalFree,
        Security::Cryptography::{
            CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN, CryptProtectData, CryptUnprotectData,
        },
    };

    let input_length =
        u32::try_from(bytes.len()).map_err(|_| IdentityError::InvalidPersistedIdentity)?;
    let input = CRYPT_INTEGER_BLOB {
        cbData: input_length,
        pbData: bytes.as_ptr().cast_mut(),
    };
    let mut output = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: ptr::null_mut(),
    };
    // SAFETY: Both DATA_BLOB values point to valid memory for the duration of the call. Windows
    // allocates the output with LocalAlloc and we release it with LocalFree below.
    let succeeded = unsafe {
        if protect {
            CryptProtectData(
                &input,
                ptr::null(),
                ptr::null(),
                ptr::null_mut(),
                ptr::null_mut(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        } else {
            CryptUnprotectData(
                &input,
                ptr::null_mut(),
                ptr::null(),
                ptr::null_mut(),
                ptr::null_mut(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        }
    };
    if succeeded == 0 {
        return Err(io::Error::last_os_error().into());
    }
    // SAFETY: A successful DPAPI call returns output.cbData initialized bytes at output.pbData.
    let result = unsafe {
        let value = std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec();
        LocalFree(output.pbData.cast());
        value
    };
    Ok(result)
}

#[cfg(target_os = "android")]
fn protect_private_key(
    private_key: &[u8],
    wrapping_key: Option<&[u8]>,
) -> Result<Vec<u8>, IdentityError> {
    use aes_gcm::{
        Aes256Gcm, Nonce,
        aead::{Aead, KeyInit, OsRng, rand_core::RngCore},
    };

    let key = wrapping_key.ok_or(IdentityError::Encryption)?;
    let cipher = Aes256Gcm::new_from_slice(key).map_err(|_| IdentityError::Encryption)?;
    let mut nonce = [0_u8; 12];
    OsRng.fill_bytes(&mut nonce);
    let ciphertext = cipher
        .encrypt(Nonce::from_slice(&nonce), private_key)
        .map_err(|_| IdentityError::Encryption)?;
    let mut protected = nonce.to_vec();
    protected.extend_from_slice(&ciphertext);
    Ok(protected)
}

#[cfg(target_os = "android")]
fn unprotect_private_key(
    protected_key: &[u8],
    wrapping_key: Option<&[u8]>,
) -> Result<Vec<u8>, IdentityError> {
    use aes_gcm::{
        Aes256Gcm, Nonce,
        aead::{Aead, KeyInit},
    };

    if protected_key.len() < 12 {
        return Err(IdentityError::InvalidPersistedIdentity);
    }
    let key = wrapping_key.ok_or(IdentityError::Encryption)?;
    let cipher = Aes256Gcm::new_from_slice(key).map_err(|_| IdentityError::Encryption)?;
    cipher
        .decrypt(
            Nonce::from_slice(&protected_key[..12]),
            &protected_key[12..],
        )
        .map_err(|_| IdentityError::Encryption)
}

#[cfg(all(not(windows), not(target_os = "android")))]
fn protect_private_key(
    private_key: &[u8],
    _wrapping_key: Option<&[u8]>,
) -> Result<Vec<u8>, IdentityError> {
    Ok(private_key.to_vec())
}

#[cfg(all(not(windows), not(target_os = "android")))]
fn unprotect_private_key(
    private_key: &[u8],
    _wrapping_key: Option<&[u8]>,
) -> Result<Vec<u8>, IdentityError> {
    Ok(private_key.to_vec())
}

pub fn derive_pairing_code(
    local_fingerprint: &[u8; 32],
    remote_fingerprint: &[u8; 32],
    handshake_transcript: &[u8],
) -> String {
    let (first, second) = if local_fingerprint <= remote_fingerprint {
        (local_fingerprint, remote_fingerprint)
    } else {
        (remote_fingerprint, local_fingerprint)
    };

    let digest = Sha256::new()
        .chain_update(PAIRING_DOMAIN)
        .chain_update(first)
        .chain_update(second)
        .chain_update(handshake_transcript)
        .finalize();
    let number = u32::from_be_bytes([digest[0], digest[1], digest[2], digest[3]]) % 1_000_000;
    format!("{number:06}")
}

#[cfg(test)]
mod tests {
    use super::{DeviceIdentity, derive_pairing_code};

    #[test]
    fn pairing_code_is_symmetric_and_six_digits() {
        let first = [0x11; 32];
        let second = [0x22; 32];
        let transcript = b"transassist-test";

        let forward = derive_pairing_code(&first, &second, transcript);
        let reverse = derive_pairing_code(&second, &first, transcript);

        assert_eq!(forward, "152361");
        assert_eq!(reverse, forward);
    }

    #[test]
    fn serialized_identity_keeps_the_same_certificate_fingerprint() {
        let generated = DeviceIdentity::generate().expect("generate identity");
        let restored = DeviceIdentity::from_der(
            generated.certificate_der().to_vec(),
            generated.private_key_der().to_vec(),
        )
        .expect("restore identity");

        assert_eq!(restored.fingerprint(), generated.fingerprint());
        assert_eq!(restored.device_id(), generated.device_id());
    }
}
