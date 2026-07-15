use crate::PublicBrand;

/// Runtime-only distribution policy selected from the public executable identity.
///
/// This controls update and install behavior only. It must not be serialized or
/// used for protocol, authentication, provider, storage, telemetry, or sandbox
/// identifiers.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum DistributionChannel {
    #[default]
    CodexUpstream,
    SyndridManual,
}

impl DistributionChannel {
    pub const fn allows_upstream_updates(self) -> bool {
        matches!(self, Self::CodexUpstream)
    }
}

impl From<PublicBrand> for DistributionChannel {
    fn from(public_brand: PublicBrand) -> Self {
        match public_brand {
            PublicBrand::Codex => Self::CodexUpstream,
            PublicBrand::Syndrid => Self::SyndridManual,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derives_distribution_channel_from_public_brand() {
        assert_eq!(
            DistributionChannel::from(PublicBrand::Codex),
            DistributionChannel::CodexUpstream
        );
        assert_eq!(
            DistributionChannel::from(PublicBrand::Syndrid),
            DistributionChannel::SyndridManual
        );
    }

    #[test]
    fn only_codex_channel_allows_upstream_updates() {
        assert!(DistributionChannel::CodexUpstream.allows_upstream_updates());
        assert!(!DistributionChannel::SyndridManual.allows_upstream_updates());
    }
}
