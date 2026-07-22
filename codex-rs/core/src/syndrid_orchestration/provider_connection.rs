use serde::Deserialize;
use serde::Serialize;
use std::collections::BTreeMap;
use std::fmt;
use url::Url;

const MAX_IDENTIFIER_BYTES: usize = 128;
const MAX_LABEL_BYTES: usize = 256;
const MAX_ENDPOINT_BYTES: usize = 2048;
const MAX_SECRET_BYTES: usize = 64 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub(super) enum ProviderConnectionError {
    EmptyIdentifier,
    IdentifierTooLong,
    EmptyLabel,
    LabelTooLong,
    EmptyCredentialReference,
    CredentialReferenceTooLong,
    EmptyCredential,
    CredentialTooLong,
    InvalidEndpoint,
    EndpointTooLong,
    CredentialRequired,
    CredentialForbidden,
    DuplicateConnectionId,
}

impl fmt::Display for ProviderConnectionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::EmptyIdentifier => "identifier must not be empty or whitespace-only",
            Self::IdentifierTooLong => "identifier exceeds its bounded length",
            Self::EmptyLabel => "label must not be empty or whitespace-only",
            Self::LabelTooLong => "label exceeds its bounded length",
            Self::EmptyCredentialReference => {
                "credential reference must not be empty or whitespace-only"
            }
            Self::CredentialReferenceTooLong => "credential reference exceeds its bounded length",
            Self::EmptyCredential => "credential must not be empty or whitespace-only",
            Self::CredentialTooLong => "credential exceeds its bounded length",
            Self::InvalidEndpoint => "endpoint is invalid",
            Self::EndpointTooLong => "endpoint exceeds its bounded length",
            Self::CredentialRequired => "authentication method requires a credential reference",
            Self::CredentialForbidden => {
                "authentication method must not carry a credential reference"
            }
            Self::DuplicateConnectionId => "provider connection ID is already registered",
        };
        formatter.write_str(message)
    }
}

impl std::error::Error for ProviderConnectionError {}

macro_rules! bounded_text {
    ($name:ident, $max:expr, $empty:ident, $too_long:ident) => {
        #[derive(Clone, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
        #[serde(transparent)]
        pub(super) struct $name(String);

        impl $name {
            pub(super) fn new(value: impl Into<String>) -> Result<Self, ProviderConnectionError> {
                let value = value.into();
                if value.trim().is_empty() {
                    return Err(ProviderConnectionError::$empty);
                }
                if value.len() > $max {
                    return Err(ProviderConnectionError::$too_long);
                }
                Ok(Self(value))
            }

            pub(super) fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Debug for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter
                    .debug_tuple(stringify!($name))
                    .field(&self.0)
                    .finish()
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                Self::new(String::deserialize(deserializer)?).map_err(serde::de::Error::custom)
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(&self.0)
            }
        }
    };
}

bounded_text!(
    ProviderId,
    MAX_IDENTIFIER_BYTES,
    EmptyIdentifier,
    IdentifierTooLong
);
bounded_text!(
    ProviderConnectionId,
    MAX_IDENTIFIER_BYTES,
    EmptyIdentifier,
    IdentifierTooLong
);
bounded_text!(
    CredentialReference,
    MAX_IDENTIFIER_BYTES,
    EmptyCredentialReference,
    CredentialReferenceTooLong
);
bounded_text!(ConnectionLabel, MAX_LABEL_BYTES, EmptyLabel, LabelTooLong);

/// Authentication metadata only; credential material is held by a credential store.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum AuthenticationMethod {
    ApiKey,
    OAuth,
    OAuthPkce,
    BearerToken,
    NoAuthentication,
    LocalEndpoint,
}

impl AuthenticationMethod {
    fn requires_credential(self) -> bool {
        matches!(
            self,
            Self::ApiKey | Self::OAuth | Self::OAuthPkce | Self::BearerToken
        )
    }
}

/// A bounded URL for a provider endpoint without embedded userinfo or query credentials.
#[derive(Clone, Eq, Hash, PartialEq, Serialize)]
#[serde(transparent)]
pub(super) struct EndpointUrl(String);

impl EndpointUrl {
    pub(super) fn new(value: impl Into<String>) -> Result<Self, ProviderConnectionError> {
        let value = value.into();
        if value.len() > MAX_ENDPOINT_BYTES {
            return Err(ProviderConnectionError::EndpointTooLong);
        }
        let parsed = Url::parse(&value).map_err(|_| ProviderConnectionError::InvalidEndpoint)?;
        if !matches!(parsed.scheme(), "http" | "https")
            || parsed.host_str().is_none()
            || !parsed.username().is_empty()
            || parsed.password().is_some()
            || parsed.query().is_some()
            || parsed.fragment().is_some()
        {
            return Err(ProviderConnectionError::InvalidEndpoint);
        }
        Ok(Self(value))
    }

    pub(super) fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for EndpointUrl {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_tuple("EndpointUrl").field(&self.0).finish()
    }
}

impl<'de> Deserialize<'de> for EndpointUrl {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        Self::new(String::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

impl fmt::Display for EndpointUrl {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// Secret material that may cross only the credential-store/provider-auth boundary.
pub(super) struct CredentialSecret(Vec<u8>);

impl CredentialSecret {
    pub(super) fn new(value: impl Into<String>) -> Result<Self, ProviderConnectionError> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err(ProviderConnectionError::EmptyCredential);
        }
        if value.len() > MAX_SECRET_BYTES {
            return Err(ProviderConnectionError::CredentialTooLong);
        }
        Ok(Self(value.into_bytes()))
    }

    pub(super) fn expose_for_auth(&self) -> &str {
        std::str::from_utf8(&self.0).expect("credential secret bytes remain valid UTF-8")
    }
}

impl Drop for CredentialSecret {
    fn drop(&mut self) {
        self.0.fill(0);
    }
}

impl fmt::Debug for CredentialSecret {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("CredentialSecret(<redacted>)")
    }
}

impl fmt::Display for CredentialSecret {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("<redacted>")
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum ConnectionValidationStatus {
    Unvalidated,
    Valid,
    Invalid,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub(super) struct ConnectionValidationResult {
    pub(super) status: ConnectionValidationStatus,
    pub(super) error: Option<ProviderConnectionError>,
}

impl ConnectionValidationResult {
    pub(super) fn unvalidated() -> Self {
        Self {
            status: ConnectionValidationStatus::Unvalidated,
            error: None,
        }
    }

    pub(super) fn valid() -> Self {
        Self {
            status: ConnectionValidationStatus::Valid,
            error: None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub(super) struct ProviderConnection {
    pub(super) connection_id: ProviderConnectionId,
    pub(super) provider_id: ProviderId,
    pub(super) label: ConnectionLabel,
    pub(super) authentication_method: AuthenticationMethod,
    pub(super) credential_reference: Option<CredentialReference>,
    pub(super) endpoint: Option<EndpointUrl>,
    pub(super) enabled: bool,
    pub(super) validation: ConnectionValidationResult,
}

impl ProviderConnection {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn new(
        connection_id: ProviderConnectionId,
        provider_id: ProviderId,
        label: ConnectionLabel,
        authentication_method: AuthenticationMethod,
        credential_reference: Option<CredentialReference>,
        endpoint: Option<EndpointUrl>,
        enabled: bool,
    ) -> Result<Self, ProviderConnectionError> {
        if authentication_method.requires_credential() && credential_reference.is_none() {
            return Err(ProviderConnectionError::CredentialRequired);
        }
        if !authentication_method.requires_credential() && credential_reference.is_some() {
            return Err(ProviderConnectionError::CredentialForbidden);
        }
        Ok(Self {
            connection_id,
            provider_id,
            label,
            authentication_method,
            credential_reference,
            endpoint,
            enabled,
            validation: ConnectionValidationResult::unvalidated(),
        })
    }
}

#[derive(Default, Debug)]
pub(super) struct ProviderConnectionRegistry {
    connections: BTreeMap<ProviderConnectionId, ProviderConnection>,
}

impl ProviderConnectionRegistry {
    pub(super) fn insert(
        &mut self,
        connection: ProviderConnection,
    ) -> Result<(), ProviderConnectionError> {
        if self.connections.contains_key(&connection.connection_id) {
            return Err(ProviderConnectionError::DuplicateConnectionId);
        }
        self.connections
            .insert(connection.connection_id.clone(), connection);
        Ok(())
    }

    pub(super) fn get(&self, id: &ProviderConnectionId) -> Option<&ProviderConnection> {
        self.connections.get(id)
    }
}

#[cfg(test)]
#[path = "provider_connection_tests.rs"]
mod provider_connection_tests;
