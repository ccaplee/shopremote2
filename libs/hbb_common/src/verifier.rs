use crate::ResultType;
use rustls_pki_types::{ServerName, UnixTime};
use std::sync::Arc;
use tokio_rustls::rustls::{self, client::WebPkiServerVerifier, ClientConfig};
use tokio_rustls::rustls::{
    client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier},
    DigitallySignedStruct, Error as TLSError, SignatureScheme,
};

/// 무조건 인증서를 수락하는 검증자입니다.
/// 보안 설정이 비활성화될 때만 사용합니다.
/// 참고: https://github.com/seanmonstar/reqwest/blob/fd61bc93e6f936454ce0b978c6f282f06eee9287/src/tls.rs#L608
#[derive(Debug)]
pub(crate) struct NoVerifier;

impl ServerCertVerifier for NoVerifier {
    fn verify_server_cert(
        &self,
        _end_entity: &rustls_pki_types::CertificateDer,
        _intermediates: &[rustls_pki_types::CertificateDer],
        _server_name: &ServerName,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, TLSError> {
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &rustls_pki_types::CertificateDer,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, TLSError> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &rustls_pki_types::CertificateDer,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, TLSError> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        vec![
            SignatureScheme::RSA_PKCS1_SHA1,
            SignatureScheme::ECDSA_SHA1_Legacy,
            SignatureScheme::RSA_PKCS1_SHA256,
            SignatureScheme::ECDSA_NISTP256_SHA256,
            SignatureScheme::RSA_PKCS1_SHA384,
            SignatureScheme::ECDSA_NISTP384_SHA384,
            SignatureScheme::RSA_PKCS1_SHA512,
            SignatureScheme::ECDSA_NISTP521_SHA512,
            SignatureScheme::RSA_PSS_SHA256,
            SignatureScheme::RSA_PSS_SHA384,
            SignatureScheme::RSA_PSS_SHA512,
            SignatureScheme::ED25519,
            SignatureScheme::ED448,
        ]
    }
}

/// 기본 검증자로 시도하고, 실패 시 플랫폼 검증자로 폴백합니다.
/// Android와 iOS에서 시스템 인증서 저장소를 활용합니다.
#[cfg(any(target_os = "android", target_os = "ios"))]
#[derive(Debug)]
struct FallbackPlatformVerifier {
    // 기본 검증자 (WebPki)
    primary: Arc<dyn ServerCertVerifier>,
    // 폴백 검증자 (플랫폼 네이티브)
    fallback: Arc<dyn ServerCertVerifier>,
}

#[cfg(any(target_os = "android", target_os = "ios"))]
impl FallbackPlatformVerifier {
    fn with_platform_fallback(
        primary: Arc<dyn ServerCertVerifier>,
        provider: Arc<rustls::crypto::CryptoProvider>,
    ) -> Result<Self, TLSError> {
        #[cfg(target_os = "android")]
        if !crate::config::ANDROID_RUSTLS_PLATFORM_VERIFIER_INITIALIZED
            .load(std::sync::atomic::Ordering::Relaxed)
        {
            return Err(TLSError::General(
                "rustls-platform-verifier not initialized".to_string(),
            ));
        }
        let fallback = Arc::new(rustls_platform_verifier::Verifier::new(provider)?);
        Ok(Self { primary, fallback })
    }
}

#[cfg(any(target_os = "android", target_os = "ios"))]
impl ServerCertVerifier for FallbackPlatformVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &rustls_pki_types::CertificateDer<'_>,
        intermediates: &[rustls_pki_types::CertificateDer<'_>],
        server_name: &ServerName<'_>,
        ocsp_response: &[u8],
        now: UnixTime,
    ) -> Result<ServerCertVerified, TLSError> {
        match self.primary.verify_server_cert(
            end_entity,
            intermediates,
            server_name,
            ocsp_response,
            now,
        ) {
            Ok(verified) => Ok(verified),
            Err(primary_err) => {
                match self.fallback.verify_server_cert(
                    end_entity,
                    intermediates,
                    server_name,
                    ocsp_response,
                    now,
                ) {
                    Ok(verified) => Ok(verified),
                    Err(fallback_err) => {
                        log::error!(
                            "Both primary and fallback verifiers failed to verify server certificate, primary error: {:?}, fallback error: {:?}",
                            primary_err,
                            fallback_err
                        );
                        Err(primary_err)
                    }
                }
            }
        }
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &rustls_pki_types::CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, TLSError> {
        // Both WebPkiServerVerifier and rustls_platform_verifier use the same signature verification implementation.
        // https://github.com/rustls/rustls/blob/1ee126adb3352a2dcd72420dcd6040351a6ddc1e/rustls/src/webpki/server_verifier.rs#L278
        // https://github.com/rustls/rustls/blob/1ee126adb3352a2dcd72420dcd6040351a6ddc1e/rustls/src/crypto/mod.rs#L17
        // https://github.com/rustls/rustls-platform-verifier/blob/1099f161bfc5e3ac7f90aad88b1bf788e72906cb/rustls-platform-verifier/src/verification/android.rs#L9
        // https://github.com/rustls/rustls-platform-verifier/blob/1099f161bfc5e3ac7f90aad88b1bf788e72906cb/rustls-platform-verifier/src/verification/apple.rs#L6
        self.primary.verify_tls12_signature(message, cert, dss)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &rustls_pki_types::CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, TLSError> {
        // Same implementation as verify_tls12_signature.
        self.primary.verify_tls13_signature(message, cert, dss)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        // Both WebPkiServerVerifier and rustls_platform_verifier use the same crypto provider,
        // so their supported signature schemes are identical.
        // https://github.com/rustls/rustls/blob/1ee126adb3352a2dcd72420dcd6040351a6ddc1e/rustls/src/webpki/server_verifier.rs#L172C52-L172C85
        // https://github.com/rustls/rustls-platform-verifier/blob/1099f161bfc5e3ac7f90aad88b1bf788e72906cb/rustls-platform-verifier/src/verification/android.rs#L327
        // https://github.com/rustls/rustls-platform-verifier/blob/1099f161bfc5e3ac7f90aad88b1bf788e72906cb/rustls-platform-verifier/src/verification/apple.rs#L304
        self.primary.supported_verify_schemes()
    }
}

/// WebPki 서버 검증자를 생성합니다.
/// 번들된 webpki_roots와 시스템 네이티브 인증서 저장소에서 루트 인증서를 로드합니다.
/// 이 방식은 reqwest와 tokio-tungstenite의 방식과 동일합니다.
/// 참고:
/// - https://github.com/snapview/tokio-tungstenite/blob/35d110c24c9d030d1608ec964d70c789dfb27452/src/tls.rs#L95
/// - https://github.com/seanmonstar/reqwest/blob/b126ca49da7897e5d676639cdbf67a0f6838b586/src/async_impl/client.rs#L643
fn webpki_server_verifier(
    provider: Arc<rustls::crypto::CryptoProvider>,
) -> ResultType<Arc<WebPkiServerVerifier>> {
    // 번들된 webpki_roots와 시스템 네이티브 인증서 저장소에서 루트 인증서 로드
    let mut root_cert_store = rustls::RootCertStore::empty();
    root_cert_store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    let rustls_native_certs::CertificateResult { certs, errors, .. } =
        rustls_native_certs::load_native_certs();
    if !errors.is_empty() {
        log::warn!("native root CA certificate loading errors: {errors:?}");
    }
    root_cert_store.add_parsable_certificates(certs);

    // with_root_certificates 동작을 사용하여 검증자 빌드 (CRL 없음)
    // reqwest와 tokio-tungstenite 모두 이 방식 사용
    // https://github.com/seanmonstar/reqwest/blob/b126ca49da7897e5d676639cdbf67a0f6838b586/src/async_impl/client.rs#L749
    // https://github.com/snapview/tokio-tungstenite/blob/35d110c24c9d030d1608ec964d70c789dfb27452/src/tls.rs#L127
    // https://github.com/rustls/rustls/blob/1ee126adb3352a2dcd72420dcd6040351a6ddc1e/rustls/src/client/builder.rs#L47
    // with_root_certificates creates a WebPkiServerVerifier without revocation checking:
    // https://github.com/rustls/rustls/blob/1ee126adb3352a2dcd72420dcd6040351a6ddc1e/rustls/src/webpki/server_verifier.rs#L177
    // https://github.com/rustls/rustls/blob/1ee126adb3352a2dcd72420dcd6040351a6ddc1e/rustls/src/webpki/server_verifier.rs#L168
    // Since no CRL is provided (as is the case here), we must explicitly set allow_unknown_revocation_status()
    // to match the behavior of with_root_certificates, which allows unknown revocation status by default.
    // https://github.com/rustls/rustls/blob/1ee126adb3352a2dcd72420dcd6040351a6ddc1e/rustls/src/webpki/server_verifier.rs#L37
    // Note: build() only returns an error if the root certificate store is empty, which won't happen here.
    let verifier = rustls::client::WebPkiServerVerifier::builder_with_provider(
        Arc::new(root_cert_store),
        provider.clone(),
    )
    .allow_unknown_revocation_status()
    .build()
    .map_err(|e| anyhow::anyhow!(e))?;
    Ok(verifier)
}

/// TLS 클라이언트 설정을 생성합니다.
/// danger_accept_invalid_cert: true일 때 무효한 인증서도 수락합니다 (보안 위험).
pub fn client_config(danger_accept_invalid_cert: bool) -> ResultType<ClientConfig> {
    if danger_accept_invalid_cert {
        client_config_danger()
    } else {
        client_config_safe()
    }
}

/// 안전한 TLS 클라이언트 설정을 생성합니다 (인증서 검증 활성화).
pub fn client_config_safe() -> ResultType<ClientConfig> {
    // Use the default builder which uses the default protocol versions and crypto provider.
    // The with_protocol_versions API has been removed in rustls master branch:
    // https://github.com/rustls/rustls/pull/2599
    // This approach is consistent with tokio-tungstenite's usage:
    // https://github.com/snapview/tokio-tungstenite/blob/35d110c24c9d030d1608ec964d70c789dfb27452/src/tls.rs#L126
    let config_builder = rustls::ClientConfig::builder();
    let provider = config_builder.crypto_provider().clone();
    let webpki_verifier = webpki_server_verifier(provider.clone())?;
    #[cfg(any(target_os = "android", target_os = "ios"))]
    {
        match FallbackPlatformVerifier::with_platform_fallback(webpki_verifier.clone(), provider) {
            Ok(fallback_verifier) => {
                let config = config_builder
                    .dangerous()
                    .with_custom_certificate_verifier(Arc::new(fallback_verifier))
                    .with_no_client_auth();
                Ok(config)
            }
            Err(e) => {
                log::error!(
                    "Failed to create fallback verifier: {:?}, use webpki verifier instead",
                    e
                );
                let config = config_builder
                    .with_webpki_verifier(webpki_verifier)
                    .with_no_client_auth();
                Ok(config)
            }
        }
    }
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        let config = config_builder
            .with_webpki_verifier(webpki_verifier)
            .with_no_client_auth();
        Ok(config)
    }
}

/// 위험한 TLS 클라이언트 설정을 생성합니다 (인증서 검증 비활성화).
/// 자체 서명 인증서나 만료된 인증서도 수락합니다.
/// 중간자 공격(MITM)에 취약하므로 개발/테스트 환경에서만 사용하세요.
pub fn client_config_danger() -> ResultType<ClientConfig> {
    let config = ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(NoVerifier))
        .with_no_client_auth();
    Ok(config)
}
