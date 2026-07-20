use std::ffi::OsStr;
use std::path::Path;

/// User-facing identity selected by the executable name.
///
/// This is presentation-only. It must not be used for protocol, authentication,
/// provider, storage, telemetry, or sandbox identifiers.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum PublicBrand {
    #[default]
    Codex,
    Syndrid,
}

impl PublicBrand {
    pub fn from_argv0(argv0: Option<&OsStr>) -> Self {
        let Some(file_name) = argv0
            .and_then(|argv0| Path::new(argv0).file_name())
            .and_then(OsStr::to_str)
        else {
            return Self::Codex;
        };

        if file_name.eq_ignore_ascii_case("syndrid")
            || file_name.eq_ignore_ascii_case("syndrid.exe")
        {
            Self::Syndrid
        } else {
            Self::Codex
        }
    }

    pub const fn command_name(self) -> &'static str {
        match self {
            Self::Codex => "codex",
            Self::Syndrid => "syndrid",
        }
    }

    pub const fn product_name(self) -> &'static str {
        match self {
            Self::Codex => "Codex",
            Self::Syndrid => "SyndridCLI",
        }
    }

    pub const fn tui_header(self) -> &'static str {
        match self {
            Self::Codex => "OpenAI Codex",
            Self::Syndrid => "SyndridCLI",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_syndrid_executable_names() {
        assert_eq!(
            PublicBrand::from_argv0(Some(OsStr::new("syndrid"))),
            PublicBrand::Syndrid
        );
        assert_eq!(
            PublicBrand::from_argv0(Some(OsStr::new("tools/syndrid.exe"))),
            PublicBrand::Syndrid
        );
        assert_eq!(
            PublicBrand::from_argv0(Some(OsStr::new("SYNDRID.EXE"))),
            PublicBrand::Syndrid
        );
    }

    #[test]
    fn defaults_non_syndrid_names_to_codex() {
        assert_eq!(
            PublicBrand::from_argv0(Some(OsStr::new("codex"))),
            PublicBrand::Codex
        );
        assert_eq!(
            PublicBrand::from_argv0(Some(OsStr::new("codex-x86_64-unknown-linux-musl"))),
            PublicBrand::Codex
        );
        assert_eq!(
            PublicBrand::from_argv0(Some(OsStr::new("renamed-binary"))),
            PublicBrand::Codex
        );
        assert_eq!(PublicBrand::from_argv0(None), PublicBrand::Codex);
    }
}
