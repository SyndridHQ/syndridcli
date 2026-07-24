use super::credential_store::CredentialStore;
use super::credential_store::CredentialStoreError;
use super::invocation::ProviderInvocation;
use super::invocation::ProviderInvocationError;
use super::invocation::ProviderInvocationRequest;
use super::invocation::ProviderInvocationResult;
use super::invocation::ProviderInvocationUsage;
use super::native_credential_store::NativeCredentialStore;
use super::openai_compatible::EndpointPolicy;
use super::openai_compatible::OpenAiCompatibleRequest;
use super::openai_compatible::OpenAiCompatibleTransportError;
use super::openai_compatible::ReqwestOpenAiCompatibleTransport;
use super::provider_connection::AuthenticationMethod;
use super::provider_connection::ConnectionLabel;
use super::provider_connection::ConnectionValidationResult;
use super::provider_connection::CredentialReference;
use super::provider_connection::CredentialSecret;
use super::provider_connection::EndpointUrl;
use super::provider_connection::ProviderConnection;
use super::provider_connection::ProviderConnectionId;
use super::provider_connection::ProviderId;
use serde::Deserialize;
use serde::Serialize;
use std::collections::BTreeMap;
use std::fmt;
use std::path::Path;
use std::time::Duration;
use tokio_util::sync::CancellationToken;
use url::Url;

pub const OMNIROUTE_PROVIDER_ID: &str = "omniroute";
const OPENROUTER_PROVIDER_ID: &str = "openrouter";
const CODEX_PROVIDER_ID: &str = "codex";
pub const OMNIROUTE_DEFAULT_BASE_URL: &str = "http://localhost:20128";
const OMNIROUTE_MODELS_PATH: &str = "/v1/models";
const OMNIROUTE_CHAT_PATH: &str = "/v1/chat/completions";
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(10);
const MAX_RESPONSE_BYTES: usize = 256 * 1024;
const MAX_MODEL_COUNT: usize = 512;
const MAX_MODEL_ID_BYTES: usize = 256;
const MAX_REGISTRY_BYTES: usize = 256 * 1024;
const MAX_CONNECTIONS: usize = 32;

#[derive(Clone, Eq, PartialEq)]
pub struct OmniRouteConnectionSetupRequest {
    pub connection_id: String,
    pub label: String,
    pub base_url: String,
    pub credential_reference: String,
    pub api_key: String,
    pub allow_remote_https: bool,
}

impl fmt::Debug for OmniRouteConnectionSetupRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OmniRouteConnectionSetupRequest")
            .field("connection_id", &self.connection_id)
            .field("label", &self.label)
            .field("base_url", &self.base_url)
            .field("credential_reference", &"<redacted>")
            .field("has_api_key", &(!self.api_key.is_empty()))
            .field("allow_remote_https", &self.allow_remote_https)
            .finish()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OmniRouteSetupError {
    InvalidConnection,
    InvalidBaseUrl,
    RemotePlaintextRejected,
    UnsupportedAuthenticationMethod,
    DuplicateConnection,
    CredentialRejected,
    CredentialStoreUnavailable,
    CredentialStoreRejected,
    ConnectionUnavailable,
    Unauthorized,
    Forbidden,
    RateLimited,
    ProviderUnavailable,
    ResponseTooLarge,
    InvalidContentType,
    InvalidResponse,
    Cancelled,
    RequestTimedOut,
}

impl fmt::Display for OmniRouteSetupError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::InvalidConnection => "OmniRoute connection is invalid",
            Self::InvalidBaseUrl => "OmniRoute base URL is invalid",
            Self::RemotePlaintextRejected => "remote HTTP OmniRoute endpoints are not allowed",
            Self::UnsupportedAuthenticationMethod => "OmniRoute requires API-key authentication",
            Self::DuplicateConnection => "provider connection ID already exists",
            Self::CredentialRejected => "OmniRoute credential was rejected",
            Self::CredentialStoreUnavailable => "credential store is unavailable",
            Self::CredentialStoreRejected => "credential store rejected the credential",
            Self::ConnectionUnavailable => "OmniRoute is unavailable",
            Self::Unauthorized => "OmniRoute authorization was rejected",
            Self::Forbidden => "OmniRoute request was forbidden",
            Self::RateLimited => "OmniRoute rate limit was reached",
            Self::ProviderUnavailable => "OmniRoute is unavailable",
            Self::ResponseTooLarge => "OmniRoute response is too large",
            Self::InvalidContentType => "OmniRoute response content type is invalid",
            Self::InvalidResponse => "OmniRoute response is invalid",
            Self::Cancelled => "OmniRoute request was cancelled",
            Self::RequestTimedOut => "OmniRoute request timed out",
        };
        formatter.write_str(message)
    }
}

impl std::error::Error for OmniRouteSetupError {}

#[derive(Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct OmniRouteConnectionMetadata {
    pub connection_id: String,
    pub provider_id: String,
    pub label: String,
    pub base_url: String,
    pub credential_reference: String,
    pub enabled: bool,
    pub validation: ConnectionValidationResult,
    pub models: Vec<String>,
    pub validated_at: Option<u64>,
}

impl fmt::Debug for OmniRouteConnectionMetadata {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OmniRouteConnectionMetadata")
            .field("connection_id", &self.connection_id)
            .field("provider_id", &self.provider_id)
            .field("label", &self.label)
            .field("base_url", &self.base_url)
            .field("credential_reference", &"<redacted>")
            .field("enabled", &self.enabled)
            .field("validation", &self.validation)
            .field("model_count", &self.models.len())
            .field("validated_at", &self.validated_at)
            .finish()
    }
}

impl fmt::Display for OmniRouteConnectionMetadata {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{} ({}, {} models)",
            self.label,
            self.connection_id,
            self.models.len()
        )
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct OmniRouteRegistry {
    connections: BTreeMap<String, OmniRouteConnectionMetadata>,
}

impl Default for OmniRouteRegistry {
    fn default() -> Self {
        Self {
            connections: BTreeMap::new(),
        }
    }
}

impl OmniRouteRegistry {
    pub fn load(path: &Path) -> Result<Self, OmniRouteRegistryError> {
        let bytes = match std::fs::read(path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self::default());
            }
            Err(_) => return Err(OmniRouteRegistryError::Unavailable),
        };
        if bytes.len() > MAX_REGISTRY_BYTES {
            return Err(OmniRouteRegistryError::TooLarge);
        }
        let registry: Self =
            serde_json::from_slice(&bytes).map_err(|_| OmniRouteRegistryError::Malformed)?;
        registry.validate()?;
        Ok(registry)
    }

    pub fn save(&self, path: &Path) -> Result<(), OmniRouteRegistryError> {
        self.validate()?;
        let bytes =
            serde_json::to_vec_pretty(self).map_err(|_| OmniRouteRegistryError::Malformed)?;
        if bytes.len() > MAX_REGISTRY_BYTES {
            return Err(OmniRouteRegistryError::TooLarge);
        }
        let parent = path.parent().ok_or(OmniRouteRegistryError::Unavailable)?;
        std::fs::create_dir_all(parent).map_err(|_| OmniRouteRegistryError::Unavailable)?;
        let temporary = path.with_extension(format!("tmp-{}", std::process::id()));
        std::fs::write(&temporary, bytes).map_err(|_| OmniRouteRegistryError::Unavailable)?;
        if let Err(error) = std::fs::rename(&temporary, path) {
            let _ = std::fs::remove_file(&temporary);
            return Err(if error.kind() == std::io::ErrorKind::AlreadyExists {
                OmniRouteRegistryError::Unavailable
            } else {
                OmniRouteRegistryError::Unavailable
            });
        }
        Ok(())
    }

    pub fn insert(
        &mut self,
        connection: OmniRouteConnectionMetadata,
    ) -> Result<(), OmniRouteRegistryError> {
        validate_connection_metadata(&connection)?;
        if self.connections.len() >= MAX_CONNECTIONS {
            return Err(OmniRouteRegistryError::TooManyConnections);
        }
        if self.connections.contains_key(&connection.connection_id) {
            return Err(OmniRouteRegistryError::DuplicateConnection);
        }
        self.connections
            .insert(connection.connection_id.clone(), connection);
        Ok(())
    }

    pub fn get(&self, connection_id: &str) -> Option<&OmniRouteConnectionMetadata> {
        self.connections.get(connection_id)
    }

    pub fn connections(&self) -> impl Iterator<Item = &OmniRouteConnectionMetadata> {
        self.connections.values()
    }

    fn validate(&self) -> Result<(), OmniRouteRegistryError> {
        if self.connections.len() > MAX_CONNECTIONS {
            return Err(OmniRouteRegistryError::TooManyConnections);
        }
        for connection in self.connections.values() {
            validate_connection_metadata(connection)?;
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OmniRouteRegistryError {
    Unavailable,
    Malformed,
    TooLarge,
    TooManyConnections,
    DuplicateConnection,
    InvalidConnection,
}

impl fmt::Display for OmniRouteRegistryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::Unavailable => "provider registry is unavailable",
            Self::Malformed => "provider registry is malformed",
            Self::TooLarge => "provider registry is too large",
            Self::TooManyConnections => "provider registry has too many connections",
            Self::DuplicateConnection => "provider connection ID already exists",
            Self::InvalidConnection => "provider registry contains an invalid connection",
        };
        formatter.write_str(message)
    }
}

impl std::error::Error for OmniRouteRegistryError {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderSelection {
    pub connection_id: String,
    pub provider_id: String,
    pub model_id: String,
}

impl ProviderSelection {
    pub fn new(
        connection_id: impl Into<String>,
        provider_id: impl Into<String>,
        model_id: impl Into<String>,
    ) -> Result<Self, ProviderSelectionError> {
        let selection = Self {
            connection_id: connection_id.into(),
            provider_id: provider_id.into(),
            model_id: model_id.into(),
        };
        if selection.connection_id.trim().is_empty()
            || selection.provider_id.trim().is_empty()
            || selection.model_id.trim().is_empty()
        {
            return Err(ProviderSelectionError::InvalidSelection);
        }
        if !matches!(
            selection.provider_id.as_str(),
            OMNIROUTE_PROVIDER_ID | OPENROUTER_PROVIDER_ID | CODEX_PROVIDER_ID
        ) {
            return Err(ProviderSelectionError::UnsupportedProvider);
        }
        Ok(selection)
    }

    pub fn resolve<'a>(
        &self,
        registry: &'a OmniRouteRegistry,
    ) -> Result<&'a OmniRouteConnectionMetadata, ProviderSelectionError> {
        let connection = registry
            .get(&self.connection_id)
            .ok_or(ProviderSelectionError::ConnectionNotFound)?;
        if connection.provider_id != self.provider_id {
            return Err(ProviderSelectionError::UnsupportedProvider);
        }
        if !connection
            .models
            .iter()
            .any(|model| model == &self.model_id)
        {
            return Err(ProviderSelectionError::ModelNotFound);
        }
        Ok(connection)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProviderSelectionError {
    InvalidSelection,
    UnsupportedProvider,
    ConnectionNotFound,
    ModelNotFound,
}

impl fmt::Display for ProviderSelectionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::InvalidSelection => "provider selection is invalid",
            Self::UnsupportedProvider => "provider is unsupported",
            Self::ConnectionNotFound => "provider connection was not found",
            Self::ModelNotFound => "provider model was not found",
        };
        formatter.write_str(message)
    }
}

impl std::error::Error for ProviderSelectionError {}

pub async fn setup_omniroute(
    request: OmniRouteConnectionSetupRequest,
    cancellation: CancellationToken,
) -> Result<OmniRouteConnectionMetadata, OmniRouteSetupError> {
    let store = NativeCredentialStore::new();
    setup_omniroute_with_dependencies(request, store, cancellation).await
}

async fn setup_omniroute_with_dependencies<S: CredentialStore>(
    request: OmniRouteConnectionSetupRequest,
    store: S,
    cancellation: CancellationToken,
) -> Result<OmniRouteConnectionMetadata, OmniRouteSetupError> {
    let (base_url, policy) = validate_base_url(&request.base_url, request.allow_remote_https)?;
    let mut request = request;
    request.base_url = base_url.clone();
    let connection = build_connection(&request)?;
    let transport = ReqwestOpenAiCompatibleTransport::new(
        &base_url,
        OMNIROUTE_MODELS_PATH,
        policy,
        DEFAULT_TIMEOUT,
        MAX_RESPONSE_BYTES,
    )
    .map_err(map_transport_setup_error)?;
    let models = OmniRouteModelCatalogClient::new(transport)
        .list_with_bearer(&request.api_key, cancellation)
        .await
        .map_err(map_catalog_setup_error)?;
    let secret = CredentialSecret::new(request.api_key)
        .map_err(|_| OmniRouteSetupError::CredentialRejected)?;
    let credential_reference = connection
        .credential_reference
        .as_ref()
        .ok_or(OmniRouteSetupError::InvalidConnection)?
        .as_str()
        .to_string();
    let metadata = OmniRouteConnectionMetadata {
        connection_id: connection.connection_id.as_str().to_string(),
        provider_id: connection.provider_id.as_str().to_string(),
        label: connection.label.as_str().to_string(),
        base_url,
        credential_reference,
        enabled: true,
        validation: ConnectionValidationResult::valid(),
        models,
        validated_at: Some(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |duration| duration.as_secs()),
        ),
    };
    store
        .store(
            &CredentialReference::new(metadata.credential_reference.clone())
                .map_err(|_| OmniRouteSetupError::InvalidConnection)?,
            secret,
        )
        .map_err(map_store_setup_error)?;
    Ok(metadata)
}

pub async fn list_omniroute_models(
    connection: &OmniRouteConnectionMetadata,
    cancellation: CancellationToken,
) -> Result<Vec<String>, OmniRouteModelCatalogError> {
    let (base_url, policy) = validate_base_url(&connection.base_url, true)
        .map_err(|_| OmniRouteModelCatalogError::InvalidResponse)?;
    let transport = ReqwestOpenAiCompatibleTransport::new(
        &base_url,
        OMNIROUTE_MODELS_PATH,
        policy,
        DEFAULT_TIMEOUT,
        MAX_RESPONSE_BYTES,
    )
    .map_err(map_catalog_transport_error)?;
    let reference = CredentialReference::new(connection.credential_reference.clone())
        .map_err(|_| OmniRouteModelCatalogError::InvalidResponse)?;
    let credential =
        NativeCredentialStore::new()
            .retrieve(&reference)
            .map_err(|error| match error {
                CredentialStoreError::Unavailable => {
                    OmniRouteModelCatalogError::ConnectionUnavailable
                }
                CredentialStoreError::NotFound
                | CredentialStoreError::InvalidReference
                | CredentialStoreError::Rejected => OmniRouteModelCatalogError::InvalidResponse,
            })?;
    OmniRouteModelCatalogClient::new(transport)
        .list_with_bearer(credential.expose_for_auth(), cancellation)
        .await
}

pub async fn invoke_omniroute(
    connection: OmniRouteConnectionMetadata,
    request: ProviderInvocationRequest,
    cancellation: CancellationToken,
) -> Result<ProviderInvocationResult, ProviderInvocationError> {
    let adapter = native_omniroute_adapter(connection)
        .map_err(|_| ProviderInvocationError::InvalidConfiguration)?;
    adapter.invoke(request, cancellation).await
}

pub fn delete_omniroute_credential(
    connection: &OmniRouteConnectionMetadata,
) -> Result<(), OmniRouteSetupError> {
    let reference = CredentialReference::new(connection.credential_reference.clone())
        .map_err(|_| OmniRouteSetupError::InvalidConnection)?;
    NativeCredentialStore::new()
        .delete(&reference)
        .map_err(map_store_setup_error)
}

pub struct OmniRouteModelCatalogClient<T> {
    transport: T,
}

impl<T> OmniRouteModelCatalogClient<T> {
    pub(crate) fn new(transport: T) -> Self {
        Self { transport }
    }
}

impl OmniRouteModelCatalogClient<ReqwestOpenAiCompatibleTransport> {
    pub async fn list_with_bearer(
        &self,
        bearer: &str,
        cancellation: CancellationToken,
    ) -> Result<Vec<String>, OmniRouteModelCatalogError> {
        let body = self
            .transport
            .get_bounded_json(bearer, cancellation)
            .await
            .map_err(map_catalog_transport_error)?;
        parse_model_catalog(&body)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OmniRouteModelCatalogError {
    ConnectionUnavailable,
    RequestTimedOut,
    Cancelled,
    Unauthorized,
    Forbidden,
    RateLimited,
    ProviderUnavailable,
    ResponseTooLarge,
    InvalidContentType,
    InvalidResponse,
    TooManyModels,
    InvalidModelId,
}

impl fmt::Display for OmniRouteModelCatalogError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::ConnectionUnavailable => "OmniRoute model catalog is unavailable",
            Self::RequestTimedOut => "OmniRoute model catalog request timed out",
            Self::Cancelled => "OmniRoute model catalog request was cancelled",
            Self::Unauthorized => "OmniRoute model catalog authorization was rejected",
            Self::Forbidden => "OmniRoute model catalog request was forbidden",
            Self::RateLimited => "OmniRoute model catalog rate limit was reached",
            Self::ProviderUnavailable => "OmniRoute model catalog provider is unavailable",
            Self::ResponseTooLarge => "OmniRoute model catalog response is too large",
            Self::InvalidContentType => "OmniRoute model catalog content type is invalid",
            Self::InvalidResponse => "OmniRoute model catalog response is invalid",
            Self::TooManyModels => "OmniRoute model catalog contains too many models",
            Self::InvalidModelId => "OmniRoute model catalog contains an invalid model ID",
        };
        formatter.write_str(message)
    }
}

impl std::error::Error for OmniRouteModelCatalogError {}

#[derive(Deserialize)]
struct ModelCatalogResponse {
    data: Vec<ModelCatalogEntry>,
}

#[derive(Deserialize)]
struct ModelCatalogEntry {
    id: String,
}

fn parse_model_catalog(body: &[u8]) -> Result<Vec<String>, OmniRouteModelCatalogError> {
    let response: ModelCatalogResponse =
        serde_json::from_slice(body).map_err(|_| OmniRouteModelCatalogError::InvalidResponse)?;
    let mut models = Vec::new();
    for entry in response.data {
        if entry.id.trim().is_empty() || entry.id.len() > MAX_MODEL_ID_BYTES {
            return Err(OmniRouteModelCatalogError::InvalidModelId);
        }
        if !models.contains(&entry.id) {
            models.push(entry.id);
        }
        if models.len() > MAX_MODEL_COUNT {
            return Err(OmniRouteModelCatalogError::TooManyModels);
        }
    }
    models.sort();
    Ok(models)
}

pub struct OmniRouteInvocationAdapter<S, T> {
    connection: OmniRouteConnectionMetadata,
    store: S,
    transport: T,
}

impl<S, T> fmt::Debug for OmniRouteInvocationAdapter<S, T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OmniRouteInvocationAdapter")
            .field("provider", &self.connection.provider_id)
            .field("connection_id", &self.connection.connection_id)
            .field("base_url", &self.connection.base_url)
            .field("credential_reference", &"<redacted>")
            .field("model_count", &self.connection.models.len())
            .finish()
    }
}

impl<S, T> OmniRouteInvocationAdapter<S, T> {
    pub(crate) fn new(connection: OmniRouteConnectionMetadata, store: S, transport: T) -> Self {
        Self {
            connection,
            store,
            transport,
        }
    }
}

impl<S> OmniRouteInvocationAdapter<S, ReqwestOpenAiCompatibleTransport>
where
    S: CredentialStore,
{
    pub fn from_connection(
        connection: OmniRouteConnectionMetadata,
        store: S,
    ) -> Result<Self, OmniRouteSetupError> {
        let (_, policy) = validate_base_url(&connection.base_url, true)?;
        let transport = ReqwestOpenAiCompatibleTransport::new(
            &connection.base_url,
            OMNIROUTE_CHAT_PATH,
            policy,
            DEFAULT_TIMEOUT,
            MAX_RESPONSE_BYTES,
        )
        .map_err(map_transport_setup_error)?;
        Ok(Self::new(connection, store, transport))
    }
}

impl<S, T> ProviderInvocation for OmniRouteInvocationAdapter<S, T>
where
    S: CredentialStore,
    T: super::openai_compatible::OpenAiCompatibleTransport,
{
    async fn invoke(
        &self,
        request: ProviderInvocationRequest,
        cancellation: CancellationToken,
    ) -> Result<ProviderInvocationResult, ProviderInvocationError> {
        validate_invocation_connection(&self.connection)?;
        if request.provider != OMNIROUTE_PROVIDER_ID {
            return Err(ProviderInvocationError::UnsupportedProvider);
        }
        if !self
            .connection
            .models
            .iter()
            .any(|model| model == &request.model)
        {
            return Err(ProviderInvocationError::InvalidModelId);
        }
        let reference = CredentialReference::new(self.connection.credential_reference.clone())
            .map_err(|_| ProviderInvocationError::MissingCredentialReference)?;
        let credential = self.store.retrieve(&reference).map_err(map_store_error)?;
        let provider_request = OpenAiCompatibleRequest::new(
            request.model.clone(),
            request.system,
            request.user,
            request.max_output_tokens,
        )
        .map_err(map_transport_invocation_error)?;
        let response = self
            .transport
            .invoke(credential.expose_for_auth(), provider_request, cancellation)
            .await
            .map_err(map_transport_invocation_error)?;
        Ok(ProviderInvocationResult {
            provider: OMNIROUTE_PROVIDER_ID.to_string(),
            model: response.model.unwrap_or(request.model),
            text: response.text,
            finish_reason: response.finish_reason,
            usage: response.usage.map(|usage| ProviderInvocationUsage {
                input_tokens: usage.input_tokens,
                output_tokens: usage.output_tokens,
                total_tokens: usage.total_tokens,
            }),
            request_id: response.request_id,
        })
    }
}

fn build_connection(
    request: &OmniRouteConnectionSetupRequest,
) -> Result<ProviderConnection, OmniRouteSetupError> {
    let connection_id = ProviderConnectionId::new(request.connection_id.clone())
        .map_err(|_| OmniRouteSetupError::InvalidConnection)?;
    let label = ConnectionLabel::new(request.label.clone())
        .map_err(|_| OmniRouteSetupError::InvalidConnection)?;
    let credential_reference = CredentialReference::new(request.credential_reference.clone())
        .map_err(|_| OmniRouteSetupError::InvalidConnection)?;
    let endpoint = EndpointUrl::new(request.base_url.clone())
        .map_err(|_| OmniRouteSetupError::InvalidBaseUrl)?;
    ProviderConnection::new(
        connection_id,
        ProviderId::new(OMNIROUTE_PROVIDER_ID).expect("static provider ID is valid"),
        label,
        AuthenticationMethod::ApiKey,
        Some(credential_reference),
        Some(endpoint),
        true,
    )
    .map_err(|_| OmniRouteSetupError::InvalidConnection)
}

fn validate_base_url(
    value: &str,
    allow_remote_https: bool,
) -> Result<(String, EndpointPolicy), OmniRouteSetupError> {
    let url = Url::parse(value).map_err(|_| OmniRouteSetupError::InvalidBaseUrl)?;
    if url.username() != ""
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || !matches!(url.path(), "" | "/")
        || url.host_str().is_none()
        || matches!(url.host_str(), Some("0.0.0.0" | "::"))
    {
        return Err(OmniRouteSetupError::InvalidBaseUrl);
    }
    let local = matches!(url.host_str(), Some("localhost" | "127.0.0.1"))
        && url.port().is_some_and(|port| port != 0);
    let policy = match (url.scheme(), local, allow_remote_https) {
        ("http", true, _) => EndpointPolicy::LoopbackHttp,
        ("http", false, _) => return Err(OmniRouteSetupError::RemotePlaintextRejected),
        ("https", _, true) => EndpointPolicy::HttpsOnly,
        ("https", true, false) => EndpointPolicy::LoopbackHttp,
        ("https", false, false) => return Err(OmniRouteSetupError::InvalidBaseUrl),
        _ => return Err(OmniRouteSetupError::InvalidBaseUrl),
    };
    Ok((value.trim_end_matches('/').to_string(), policy))
}

fn validate_connection_metadata(
    connection: &OmniRouteConnectionMetadata,
) -> Result<(), OmniRouteRegistryError> {
    if connection.provider_id != OMNIROUTE_PROVIDER_ID
        || connection.connection_id.trim().is_empty()
        || connection.label.trim().is_empty()
        || connection.credential_reference.trim().is_empty()
        || validate_base_url(&connection.base_url, true).is_err()
        || connection.models.len() > MAX_MODEL_COUNT
        || connection
            .models
            .iter()
            .any(|model| model.trim().is_empty() || model.len() > MAX_MODEL_ID_BYTES)
    {
        return Err(OmniRouteRegistryError::InvalidConnection);
    }
    Ok(())
}

fn validate_invocation_connection(
    connection: &OmniRouteConnectionMetadata,
) -> Result<(), ProviderInvocationError> {
    if connection.provider_id != OMNIROUTE_PROVIDER_ID {
        return Err(ProviderInvocationError::UnsupportedProvider);
    }
    if !connection.enabled {
        return Err(ProviderInvocationError::InvalidConfiguration);
    }
    if connection.validation.status != super::provider_connection::ConnectionValidationStatus::Valid
    {
        return Err(ProviderInvocationError::InvalidConfiguration);
    }
    Ok(())
}

fn map_store_error(error: CredentialStoreError) -> ProviderInvocationError {
    match error {
        CredentialStoreError::NotFound => ProviderInvocationError::CredentialNotFound,
        CredentialStoreError::Unavailable => ProviderInvocationError::CredentialStoreUnavailable,
        CredentialStoreError::InvalidReference | CredentialStoreError::Rejected => {
            ProviderInvocationError::CredentialStoreRejected
        }
    }
}

fn map_store_setup_error(error: CredentialStoreError) -> OmniRouteSetupError {
    match error {
        CredentialStoreError::Unavailable => OmniRouteSetupError::CredentialStoreUnavailable,
        CredentialStoreError::InvalidReference
        | CredentialStoreError::NotFound
        | CredentialStoreError::Rejected => OmniRouteSetupError::CredentialStoreRejected,
    }
}

fn map_catalog_transport_error(
    error: OpenAiCompatibleTransportError,
) -> OmniRouteModelCatalogError {
    match error {
        OpenAiCompatibleTransportError::TransportUnavailable => {
            OmniRouteModelCatalogError::ConnectionUnavailable
        }
        OpenAiCompatibleTransportError::RequestTimedOut => {
            OmniRouteModelCatalogError::RequestTimedOut
        }
        OpenAiCompatibleTransportError::Cancelled => OmniRouteModelCatalogError::Cancelled,
        OpenAiCompatibleTransportError::Unauthorized => OmniRouteModelCatalogError::Unauthorized,
        OpenAiCompatibleTransportError::Forbidden => OmniRouteModelCatalogError::Forbidden,
        OpenAiCompatibleTransportError::RateLimited { .. } => {
            OmniRouteModelCatalogError::RateLimited
        }
        OpenAiCompatibleTransportError::ProviderUnavailable => {
            OmniRouteModelCatalogError::ProviderUnavailable
        }
        OpenAiCompatibleTransportError::ResponseTooLarge => {
            OmniRouteModelCatalogError::ResponseTooLarge
        }
        OpenAiCompatibleTransportError::InvalidContentType => {
            OmniRouteModelCatalogError::InvalidContentType
        }
        OpenAiCompatibleTransportError::InvalidResponse => {
            OmniRouteModelCatalogError::InvalidResponse
        }
        OpenAiCompatibleTransportError::InvalidConfiguration
        | OpenAiCompatibleTransportError::InvalidRequest
        | OpenAiCompatibleTransportError::InputTooLarge
        | OpenAiCompatibleTransportError::OutputLimitInvalid
        | OpenAiCompatibleTransportError::PaymentRequired
        | OpenAiCompatibleTransportError::ProviderRejected
        | OpenAiCompatibleTransportError::MissingOutput => {
            OmniRouteModelCatalogError::InvalidResponse
        }
    }
}

fn map_catalog_setup_error(error: OmniRouteModelCatalogError) -> OmniRouteSetupError {
    match error {
        OmniRouteModelCatalogError::ConnectionUnavailable => {
            OmniRouteSetupError::ConnectionUnavailable
        }
        OmniRouteModelCatalogError::RequestTimedOut => OmniRouteSetupError::RequestTimedOut,
        OmniRouteModelCatalogError::Cancelled => OmniRouteSetupError::Cancelled,
        OmniRouteModelCatalogError::Unauthorized => OmniRouteSetupError::Unauthorized,
        OmniRouteModelCatalogError::Forbidden => OmniRouteSetupError::Forbidden,
        OmniRouteModelCatalogError::RateLimited => OmniRouteSetupError::RateLimited,
        OmniRouteModelCatalogError::ProviderUnavailable => OmniRouteSetupError::ProviderUnavailable,
        OmniRouteModelCatalogError::ResponseTooLarge => OmniRouteSetupError::ResponseTooLarge,
        OmniRouteModelCatalogError::InvalidContentType => OmniRouteSetupError::InvalidContentType,
        OmniRouteModelCatalogError::InvalidResponse
        | OmniRouteModelCatalogError::TooManyModels
        | OmniRouteModelCatalogError::InvalidModelId => OmniRouteSetupError::InvalidResponse,
    }
}

fn map_transport_setup_error(error: OpenAiCompatibleTransportError) -> OmniRouteSetupError {
    map_catalog_setup_error(map_catalog_transport_error(error))
}

fn map_transport_invocation_error(
    error: OpenAiCompatibleTransportError,
) -> ProviderInvocationError {
    match error {
        OpenAiCompatibleTransportError::InvalidConfiguration => {
            ProviderInvocationError::InvalidConfiguration
        }
        OpenAiCompatibleTransportError::InvalidRequest => ProviderInvocationError::InvalidRequest,
        OpenAiCompatibleTransportError::InputTooLarge => ProviderInvocationError::InputTooLarge,
        OpenAiCompatibleTransportError::OutputLimitInvalid => {
            ProviderInvocationError::OutputLimitInvalid
        }
        OpenAiCompatibleTransportError::TransportUnavailable => {
            ProviderInvocationError::TransportUnavailable
        }
        OpenAiCompatibleTransportError::RequestTimedOut => ProviderInvocationError::RequestTimedOut,
        OpenAiCompatibleTransportError::Cancelled => ProviderInvocationError::Cancelled,
        OpenAiCompatibleTransportError::Unauthorized => ProviderInvocationError::Unauthorized,
        OpenAiCompatibleTransportError::PaymentRequired => ProviderInvocationError::PaymentRequired,
        OpenAiCompatibleTransportError::Forbidden => ProviderInvocationError::Forbidden,
        OpenAiCompatibleTransportError::RateLimited { .. } => ProviderInvocationError::RateLimited,
        OpenAiCompatibleTransportError::ProviderUnavailable => {
            ProviderInvocationError::ProviderUnavailable
        }
        OpenAiCompatibleTransportError::ProviderRejected => {
            ProviderInvocationError::ProviderRejected
        }
        OpenAiCompatibleTransportError::InvalidContentType => {
            ProviderInvocationError::InvalidContentType
        }
        OpenAiCompatibleTransportError::ResponseTooLarge => {
            ProviderInvocationError::ResponseTooLarge
        }
        OpenAiCompatibleTransportError::InvalidResponse => ProviderInvocationError::InvalidResponse,
        OpenAiCompatibleTransportError::MissingOutput => ProviderInvocationError::MissingOutput,
    }
}

pub(super) fn native_omniroute_adapter(
    connection: OmniRouteConnectionMetadata,
) -> Result<
    OmniRouteInvocationAdapter<NativeCredentialStore, ReqwestOpenAiCompatibleTransport>,
    OmniRouteSetupError,
> {
    OmniRouteInvocationAdapter::from_connection(connection, NativeCredentialStore::new())
}

#[cfg(test)]
#[path = "omniroute_setup_tests.rs"]
mod omniroute_setup_tests;

#[cfg(test)]
#[path = "omniroute_model_catalog_tests.rs"]
mod omniroute_model_catalog_tests;

#[cfg(test)]
#[path = "omniroute_invocation_tests.rs"]
mod omniroute_invocation_tests;
