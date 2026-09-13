use super::tls::{install_crypto_provider, pinned_tls_config};
use super::*;
use frank_protocol::*;

impl RemoteClient {
    pub fn new(config: ClientConfig) -> Result<Self> {
        install_crypto_provider();
        let base = Url::parse(&config.base_url)
            .map_err(|error| ClientError::InvalidAddress(error.to_string()))?;
        let secure = matches!(base.scheme(), "https");
        let loopback = base
            .host_str()
            .map(|host| matches!(host, "localhost" | "127.0.0.1" | "::1"))
            .unwrap_or(false);
        // Stated positively so the rule reads as the policy it encodes, and so
        // clippy::nonminimal_bool has nothing to rewrite: the transport is
        // acceptable when it is TLS, or when it is loopback and the caller has
        // explicitly opted into plaintext there. Truth table is unchanged.
        let transport_allowed = secure || (loopback && config.allow_insecure_local);
        if !transport_allowed {
            return Err(ClientError::InsecureTransport);
        }
        let mut builder = Client::builder().timeout(config.request_timeout);
        if let Some(pin) = &config.pinned_certificate_fingerprint {
            let tls = pinned_tls_config(pin)?;
            builder = builder.use_preconfigured_tls(tls);
        } else if let Some(pem) = &config.ca_certificate_pem {
            let certificate = reqwest::Certificate::from_pem(pem)?;
            builder = builder.add_root_certificate(certificate);
        }
        let http = builder.build()?;
        Ok(Self { config, http, base })
    }

    pub fn config(&self) -> &ClientConfig {
        &self.config
    }

    pub async fn health(&self) -> Result<serde_json::Value> {
        let response = self.http.get(self.endpoint("/health")).send().await?;
        self.decode_json(response).await
    }

    pub async fn auth_status(&self) -> Result<AuthStatusResponse> {
        let response = self.http.get(self.endpoint("/auth/status")).send().await?;
        self.decode_json(response).await
    }

    /// Authenticate a local owner account. The returned opaque token can be
    /// supplied to a fresh `ClientConfig::with_token`; this method never logs
    /// or persists it on behalf of callers.
    pub async fn login(
        &self,
        username: impl Into<String>,
        password: impl Into<String>,
        device_name: Option<String>,
    ) -> Result<AuthLoginResponse> {
        let response = self
            .http
            .post(self.endpoint("/auth/login"))
            .json(&AuthLoginRequest {
                username: username.into(),
                password: password.into(),
                device_name,
            })
            .send()
            .await?;
        self.decode_json(response).await
    }

    pub async fn auth_me(&self) -> Result<AuthMeResponse> {
        let response = self
            .authorized(self.http.get(self.endpoint("/auth/me")))
            .send()
            .await?;
        self.decode_json(response).await
    }

    pub async fn logout(&self) -> Result<AuthMutationResponse> {
        let response = self
            .authorized(self.http.post(self.endpoint("/auth/logout")))
            .send()
            .await?;
        self.decode_json(response).await
    }

    pub async fn logout_all(&self) -> Result<AuthMutationResponse> {
        let response = self
            .authorized(self.http.post(self.endpoint("/auth/logout-all")))
            .send()
            .await?;
        self.decode_json(response).await
    }

    pub async fn change_password(
        &self,
        current_password: impl Into<String>,
        new_password: impl Into<String>,
    ) -> Result<AuthMutationResponse> {
        let response = self
            .authorized(self.http.post(self.endpoint("/auth/password")))
            .json(&AuthPasswordRequest {
                current_password: current_password.into(),
                new_password: new_password.into(),
            })
            .send()
            .await?;
        self.decode_json(response).await
    }

    /// Fetch the owner-only sanitized diagnostic snapshot.  Unlike health,
    /// this endpoint deliberately requires the paired device bearer and is
    /// intended for the Settings/doctor surfaces rather than liveness probes.
    pub async fn diagnostics(&self) -> Result<DiagnosticSnapshot> {
        let response = self
            .authorized(self.http.get(self.endpoint("/diagnostics")))
            .send()
            .await?;
        self.decode_json(response).await
    }

    pub async fn capabilities(&self) -> Result<Capabilities> {
        let response = self
            .authorized(self.http.get(self.endpoint("/capabilities")))
            .send()
            .await?;
        let capabilities: Capabilities = self.decode_json(response).await?;
        if !capabilities.supported_versions.accepts(PROTOCOL_VERSION)
            || capabilities.minimum_compatible_client > PROTOCOL_VERSION
        {
            return Err(ClientError::Api(ApiError::new(
                ErrorCode::VersionMismatch,
                "this client is not compatible with the server protocol",
            )));
        }
        self.verify_pin(&capabilities, Some(&capabilities.certificate_fingerprint))?;
        Ok(capabilities)
    }

    /// Negotiate the wire version before subscribing to events. Capabilities
    /// remain available as a standalone bootstrap endpoint, but the explicit
    /// handshake gives onboarding one atomic compatibility decision and server
    /// identity document for reconnects.
    pub async fn handshake(&self, request: HandshakeRequest) -> Result<HandshakeResponse> {
        let response = self
            .http
            .post(self.endpoint("/handshake"))
            .json(&request)
            .send()
            .await?;
        let handshake: HandshakeResponse = self.decode_json(response).await?;
        if handshake.negotiated_version != PROTOCOL_VERSION
            || !handshake
                .capabilities
                .supported_versions
                .accepts(PROTOCOL_VERSION)
            || handshake.capabilities.minimum_compatible_client > PROTOCOL_VERSION
        {
            return Err(ClientError::Api(ApiError::new(
                ErrorCode::VersionMismatch,
                "this client is not compatible with the server protocol",
            )));
        }
        self.verify_pin(
            &handshake.capabilities,
            Some(&handshake.capabilities.certificate_fingerprint),
        )?;
        Ok(handshake)
    }

    pub async fn pair(&self, request: PairingRequest) -> Result<PairingResponse> {
        let response = self
            .http
            .post(self.endpoint("/pair"))
            .json(&request)
            .send()
            .await?;
        let pairing: PairingResponse = self.decode_json(response).await?;
        self.verify_pin(
            &Capabilities {
                protocol_version: PROTOCOL_VERSION,
                supported_versions: VersionRange::current(),
                minimum_compatible_client: MIN_COMPATIBLE_CLIENT,
                server_id: pairing.server_id,
                certificate_fingerprint: pairing.certificate_fingerprint.clone(),
                server_version: String::new(),
                features: Vec::new(),
                openrouter: frank_protocol::OpenRouterCapability::default(),
                limits: CapabilityLimits::default(),
            },
            Some(&pairing.certificate_fingerprint),
        )?;
        Ok(pairing)
    }

    pub async fn prepare_pairing(&self, role: DeviceRole) -> Result<serde_json::Value> {
        let response = self
            .http
            .post(self.endpoint("/pair/prepare"))
            .json(&role)
            .send()
            .await?;
        self.decode_json(response).await
    }

    pub async fn snapshot(&self) -> Result<Snapshot> {
        let response = self
            .authorized(self.http.get(self.endpoint("/snapshot")))
            .send()
            .await?;
        let snapshot: Snapshot = self.decode_json(response).await?;
        self.verify_pin(
            &Capabilities {
                protocol_version: PROTOCOL_VERSION,
                supported_versions: VersionRange::current(),
                minimum_compatible_client: MIN_COMPATIBLE_CLIENT,
                server_id: snapshot.server_id,
                certificate_fingerprint: snapshot.server.tls_fingerprint.clone(),
                server_version: String::new(),
                features: Vec::new(),
                openrouter: frank_protocol::OpenRouterCapability::default(),
                limits: CapabilityLimits::default(),
            },
            Some(&snapshot.server.tls_fingerprint),
        )?;
        Ok(snapshot)
    }

    /// Fetch an artifact through the authenticated server boundary. Artifact
    /// payloads are intentionally not decoded as JSON and use the larger
    /// protocol cap; metadata remains in snapshots/events.
    pub async fn artifact_bytes(&self, artifact_id: ArtifactId) -> Result<(String, Vec<u8>)> {
        let mut response = self
            .authorized(
                self.http
                    .get(self.endpoint(&format!("/artifacts/{artifact_id}"))),
            )
            .send()
            .await?;
        if response
            .content_length()
            .is_some_and(|length| length > MAX_ARTIFACT_BYTES)
        {
            return Err(ClientError::Api(ApiError::new(
                ErrorCode::PayloadTooLarge,
                "artifact response exceeded the client limit",
            )));
        }
        let status = response.status();
        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .unwrap_or("application/octet-stream")
            .to_string();
        if !status.is_success() {
            // Error envelopes are still bounded independently of the
            // artifact cap. A compromised server must not turn a failed
            // download into an unbounded allocation before the JSON error
            // can be decoded.
            let mut body = Vec::new();
            while let Some(chunk) = response.chunk().await? {
                if chunk.len() > MAX_COMMAND_BODY_BYTES
                    || body.len().saturating_add(chunk.len()) > MAX_COMMAND_BODY_BYTES
                {
                    return Err(ClientError::Api(ApiError::new(
                        ErrorCode::PayloadTooLarge,
                        "artifact error response exceeded the client limit",
                    )));
                }
                body.extend_from_slice(&chunk);
            }
            if let Ok(error) = serde_json::from_slice::<ApiError>(&body) {
                return Err(ClientError::Api(error));
            }
            return Err(ClientError::Api(ApiError::new(
                ErrorCode::NotFound,
                "artifact not found",
            )));
        }
        // Use reqwest's frame-by-frame `chunk()` API instead of `bytes()` so
        // the client applies the same hard cap while data is arriving. The
        // public convenience method still returns a Vec, but no response can
        // force allocation beyond the negotiated artifact limit.
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await? {
            if chunk.len() > MAX_TERMINAL_FRAME_BYTES * 4
                || (bytes.len() as u64).saturating_add(chunk.len() as u64) > MAX_ARTIFACT_BYTES
            {
                return Err(ClientError::Api(ApiError::new(
                    ErrorCode::PayloadTooLarge,
                    "artifact response exceeded the client limit",
                )));
            }
            bytes.extend_from_slice(&chunk);
        }
        Ok((content_type, bytes))
    }

    /// Append one bounded chunk to a server-side upload.  The offset is sent
    /// explicitly so retries cannot silently reorder bytes; the server is the
    /// authority on the returned contiguous byte count.
    pub async fn upload_artifact_chunk(
        &self,
        upload_id: UploadId,
        offset: u64,
        bytes: Vec<u8>,
    ) -> Result<ArtifactUploadChunkResponse> {
        if bytes.len() > MAX_TERMINAL_FRAME_BYTES * 4 {
            return Err(ClientError::Api(ApiError::new(
                ErrorCode::PayloadTooLarge,
                "artifact chunk is too large",
            )));
        }
        let response = self
            .authorized(
                self.http
                    .put(self.endpoint(&format!("/artifact-uploads/{upload_id}")))
                    .header("x-frank-offset", offset.to_string())
                    .header(reqwest::header::CONTENT_TYPE, "application/octet-stream")
                    .body(bytes),
            )
            .send()
            .await?;
        self.decode_json(response).await
    }

    pub async fn browse_projects(&self, path: Option<&str>) -> Result<BrowseResponse> {
        let mut endpoint = self.endpoint("/projects/browse");
        if let Some(path) = path {
            endpoint.query_pairs_mut().append_pair("path", path);
        }
        let response = self.authorized(self.http.get(endpoint)).send().await?;
        self.decode_json(response).await
    }

    pub async fn devices(&self) -> Result<Vec<DeviceSummary>> {
        let response = self
            .authorized(self.http.get(self.endpoint("/devices")))
            .send()
            .await?;
        self.decode_json(response).await
    }

    pub async fn revoke_device(&self, device_id: DeviceId) -> Result<serde_json::Value> {
        let response = self
            .authorized(
                self.http
                    .post(self.endpoint(&format!("/devices/{device_id}/revoke"))),
            )
            .send()
            .await?;
        self.decode_json(response).await
    }

    pub async fn command(
        &self,
        command: Command,
        expected_revision: Option<u64>,
    ) -> Result<CommandResponse> {
        let envelope = CommandEnvelope {
            protocol_version: PROTOCOL_VERSION,
            command_id: CommandId::new(),
            expected_revision,
            command,
        };
        let response = self.command_envelope(envelope).await?;
        if let Some(error) = response.error.clone() {
            return Err(ClientError::Api(error));
        }
        Ok(response)
    }

    pub async fn command_envelope(&self, envelope: CommandEnvelope) -> Result<CommandResponse> {
        let command_id = envelope.command_id;
        let response = self
            .authorized(self.http.post(self.endpoint("/commands")))
            .json(&envelope)
            .send()
            .await?;
        let status = response.status();
        // The server uses an HTTP error status for rejected commands but
        // returns the same CommandResponse envelope so a stale-revision error
        // can carry its latest snapshot. Keep that structured payload for the
        // GUI reducer instead of collapsing it into a string-only transport
        // error. Non-command API errors still use the normal decode path.
        let body = response.bytes().await?;
        if body.len() > MAX_COMMAND_BODY_BYTES {
            return Err(ClientError::Api(ApiError::new(
                ErrorCode::PayloadTooLarge,
                "server response exceeded the client limit",
            )));
        }
        let value = match serde_json::from_slice::<CommandResponse>(&body) {
            Ok(value) => value,
            Err(_) if !status.is_success() => {
                if let Ok(error) = serde_json::from_slice::<ApiError>(&body) {
                    let revision = error
                        .latest_snapshot
                        .as_deref()
                        .map(|snapshot| snapshot.revision)
                        .unwrap_or_default();
                    return Ok(CommandResponse::failed(command_id, revision, error));
                }
                return Err(ClientError::Api(ApiError::new(
                    ErrorCode::Internal,
                    "server request failed",
                )));
            }
            Err(error) => return Err(ClientError::Decode(error)),
        };
        if (status.is_client_error() || status.is_server_error()) && value.error.is_some() {
            // Preserve the command id even when an older server omitted it
            // from a rejected envelope. The response itself remains the
            // authoritative error payload for callers that need a resync.
            return Ok(CommandResponse {
                command_id: if value.command_id == CommandId::nil() {
                    command_id
                } else {
                    value.command_id
                },
                ..value
            });
        }
        Ok(value)
    }

    /// Execute one task-scoped connector tool through frankd. The agent
    /// session header is added by `authorized`; callers cannot widen the task
    /// scope or supply connector credentials through this method.
    pub async fn agent_tool(
        &self,
        name: &str,
        input: serde_json::Value,
    ) -> Result<serde_json::Value> {
        if name.trim().is_empty() || name.len() > 128 || name.chars().any(char::is_control) {
            return Err(ClientError::Api(ApiError::new(
                ErrorCode::Validation,
                "agent tool name is invalid",
            )));
        }
        let response = self
            .authorized(self.http.post(self.endpoint("/agent-tools")))
            .json(&serde_json::json!({"name": name, "input": input}))
            .send()
            .await?;
        self.decode_json(response).await
    }
}
