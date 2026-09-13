use super::*;
use frank_protocol::*;
use reqwest::{RequestBuilder, StatusCode, Url};
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{DigitallySignedStruct, SignatureScheme};
use sha2::Digest;
use std::io::Cursor;
use std::sync::Arc;
use tokio_tungstenite::{
    Connector,
    tungstenite::{
        client::IntoClientRequest,
        http::{HeaderValue, Request},
    },
};

impl RemoteClient {
    pub(crate) fn endpoint(&self, path: &str) -> Url {
        let normalized = path
            .strip_prefix(frank_protocol::API_PREFIX)
            .unwrap_or(path);
        let versioned = format!(
            "{}{}",
            frank_protocol::API_PREFIX,
            if normalized.starts_with('/') {
                normalized.to_string()
            } else {
                format!("/{normalized}")
            }
        );
        self.base
            .join(&versioned)
            .unwrap_or_else(|_| self.base.clone())
    }

    pub(crate) fn websocket_connector(&self) -> Result<Option<Connector>> {
        if let Some(pin) = &self.config.pinned_certificate_fingerprint {
            return Ok(Some(Connector::Rustls(Arc::new(pinned_tls_config(pin)?))));
        }
        let Some(pem) = &self.config.ca_certificate_pem else {
            return Ok(None);
        };
        let mut reader = Cursor::new(pem);
        let certificates = rustls_pemfile::certs(&mut reader)
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|error| ClientError::Tls(error.to_string()))?;
        if certificates.is_empty() {
            return Err(ClientError::Tls(
                "CA PEM did not contain a certificate".into(),
            ));
        }
        let mut roots = rustls::RootCertStore::empty();
        for certificate in certificates {
            roots
                .add(certificate)
                .map_err(|error| ClientError::Tls(error.to_string()))?;
        }
        let config = rustls::ClientConfig::builder()
            .with_root_certificates(roots)
            .with_no_client_auth();
        Ok(Some(Connector::Rustls(Arc::new(config))))
    }

    pub(crate) fn websocket_request(&self, url: Url) -> Result<Request<()>> {
        // Let tungstenite create the complete RFC 6455 handshake first. A
        // hand-built request is subtly incomplete: without the
        // `Sec-WebSocket-Key`/Upgrade headers Axum rejects it before the
        // event stream handler runs, which sends the actor into reconnect
        // backoff forever.
        let mut request = url.as_str().into_client_request()?;
        if let Some(token) = &self.config.device_token {
            // Keep bearer credentials in the TLS-protected header rather than
            // the URL.  This prevents accidental leakage through proxy/access
            // logs while retaining compatibility with the server's header
            // authentication path.
            request.headers_mut().insert(
                "authorization",
                HeaderValue::from_str(&format!("Bearer {token}"))
                    .map_err(|error| ClientError::InvalidAddress(error.to_string()))?,
            );
        }
        if let Some(token) = &self.config.agent_session_token {
            request.headers_mut().insert(
                "x-frank-agent-token",
                HeaderValue::from_str(token)
                    .map_err(|error| ClientError::InvalidAddress(error.to_string()))?,
            );
        }
        Ok(request)
    }

    pub(crate) fn authorized(&self, request: RequestBuilder) -> RequestBuilder {
        let request = match &self.config.device_token {
            Some(token) => request.bearer_auth(token),
            None => request,
        };
        match &self.config.agent_session_token {
            Some(token) => request.header("x-frank-agent-token", token),
            None => request,
        }
    }

    pub(crate) async fn decode_json<T: serde::de::DeserializeOwned>(
        &self,
        response: reqwest::Response,
    ) -> Result<T> {
        let status = response.status();
        let body = response.bytes().await?;
        if body.len() > MAX_COMMAND_BODY_BYTES {
            return Err(ClientError::Api(ApiError::new(
                ErrorCode::PayloadTooLarge,
                "server response exceeded the client limit",
            )));
        }
        if status == StatusCode::UPGRADE_REQUIRED {
            return Err(ClientError::Api(ApiError::new(
                ErrorCode::VersionMismatch,
                "client update required",
            )));
        }
        if !status.is_success() {
            if let Ok(error) = serde_json::from_slice::<ApiError>(&body) {
                return Err(ClientError::Api(error));
            }
            return Err(ClientError::Api(ApiError::new(
                ErrorCode::Internal,
                "server request failed",
            )));
        }
        Ok(serde_json::from_slice(&body)?)
    }

    pub(crate) fn verify_pin(
        &self,
        _capabilities: &Capabilities,
        snapshot_fingerprint: Option<&str>,
    ) -> Result<()> {
        let Some(expected) = &self.config.pinned_certificate_fingerprint else {
            return Ok(());
        };
        let observed = snapshot_fingerprint
            .filter(|fingerprint| !fingerprint.is_empty())
            .map(str::to_string);
        if observed.as_deref() == Some(expected.as_str()) {
            Ok(())
        } else if observed.is_none() {
            // The TLS connector already verified the CA/hostname.  The
            // capabilities endpoint has no certificate field in older v1
            // servers, so retain the pin until snapshot supplies one.
            Ok(())
        } else {
            Err(ClientError::CertificatePinMismatch)
        }
    }
}

/// Rustls 0.23 cannot choose a process-wide backend when both `ring` and
/// `aws-lc-rs` are linked through the workspace.  Install the same backend as
/// `frankd` before constructing reqwest or a pinned WebSocket connector.
pub(crate) fn install_crypto_provider() {
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
}

/// Build a TLS client that trusts exactly one certificate leaf.  The pin is
/// checked before any HTTP/WebSocket bytes are accepted, while Rustls still
/// validates the server's handshake signatures through its normal crypto
/// provider.  This avoids the insecure `danger_accept_invalid_certs` escape
/// hatch for generated, self-signed Frank identities.
pub(crate) fn pinned_tls_config(fingerprint: &str) -> Result<rustls::ClientConfig> {
    let expected = fingerprint.trim().to_ascii_lowercase();
    let valid_shape = expected
        .strip_prefix("sha256:")
        .is_some_and(|hex| hex.len() == 64 && hex.bytes().all(|byte| byte.is_ascii_hexdigit()));
    if !valid_shape {
        return Err(ClientError::Tls(
            "certificate fingerprint must be sha256:<64 lowercase hexadecimal characters>".into(),
        ));
    }
    let verifier = PinnedCertificateVerifier::new(expected);
    let config = rustls::ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(verifier))
        .with_no_client_auth();
    Ok(config)
}

#[derive(Debug)]
struct PinnedCertificateVerifier {
    expected: String,
    algorithms: rustls::crypto::WebPkiSupportedAlgorithms,
}

impl PinnedCertificateVerifier {
    fn new(expected: String) -> Self {
        let provider = rustls::crypto::CryptoProvider::get_default()
            .expect("rustls crypto provider must be installed")
            .clone();
        Self {
            expected,
            algorithms: provider.signature_verification_algorithms,
        }
    }
}

impl ServerCertVerifier for PinnedCertificateVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> std::result::Result<ServerCertVerified, rustls::Error> {
        let digest = sha2::Sha256::digest(end_entity.as_ref());
        let observed = format!("sha256:{}", hex::encode(digest));
        if observed == self.expected {
            Ok(ServerCertVerified::assertion())
        } else {
            Err(rustls::Error::General(
                "server certificate fingerprint does not match the pinned identity".into(),
            ))
        }
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(message, cert, dss, &self.algorithms)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(message, cert, dss, &self.algorithms)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.algorithms.supported_schemes()
    }
}
