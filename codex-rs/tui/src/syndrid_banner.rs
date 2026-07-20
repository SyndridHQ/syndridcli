//! Experimental, presentation-only Syndrid banner templates.
//!
//! The parser intentionally produces only text and semantic styles. It has no
//! access to commands, processes, files, or network clients.

use ratatui::style::Color;
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::text::Span;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use tracing::warn;
use unicode_width::UnicodeWidthStr;

const MISSING: &str = "—";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RendererMode {
    Builtin,
    Templates,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Token {
    pub(crate) text: String,
    pub(crate) role: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct BannerTemplate {
    pub(crate) lines: Vec<Vec<Token>>,
}

impl BannerTemplate {
    pub(crate) fn parse(source: &str) -> Result<Self, String> {
        let mut lines = Vec::new();
        for (line_number, source_line) in source.lines().enumerate() {
            let mut line = Vec::new();
            let mut remainder = source_line;
            while let Some(start) = remainder.find("{{") {
                if !remainder[..start].is_empty() {
                    line.push(Token {
                        text: remainder[..start].to_string(),
                        role: None,
                    });
                }
                let end = remainder[start + 2..].find("}}").ok_or_else(|| {
                    format!("line {} has an unterminated placeholder", line_number + 1)
                })? + start
                    + 2;
                let expression = remainder[start + 2..end].trim();
                let (role, value) = expression
                    .split_once(':')
                    .map_or((None, expression), |(role, value)| (Some(role), value));
                if !is_supported(role, value) {
                    return Err(format!("unsupported placeholder `{expression}`"));
                }
                line.push(Token {
                    text: value.to_string(),
                    role: Some(role.map_or_else(String::new, str::to_string)),
                });
                remainder = &remainder[end + 2..];
            }
            if !remainder.is_empty() {
                line.push(Token {
                    text: remainder.to_string(),
                    role: None,
                });
            }
            lines.push(line);
        }
        Ok(Self { lines })
    }

    pub(crate) fn render(
        &self,
        values: &BTreeMap<String, String>,
        theme: &Theme,
        width: usize,
    ) -> Vec<Line<'static>> {
        self.lines
            .iter()
            .map(|tokens| {
                let mut spans = Vec::new();
                let mut used = 0;
                for token in tokens {
                    let dynamic = token
                        .role
                        .as_deref()
                        .is_some_and(|role| role.is_empty() || role == "value" || role == "active");
                    let value = if dynamic {
                        values
                            .get(&token.text)
                            .cloned()
                            .unwrap_or_else(|| MISSING.to_string())
                    } else {
                        token.text.clone()
                    };
                    let value =
                        crate::syndrid_visuals::fit_text(&value, width.saturating_sub(used));
                    used += UnicodeWidthStr::width(value.as_str());
                    let style = theme.style(token.role.as_deref());
                    spans.push(Span::styled(value, style));
                }
                Line::from(spans)
            })
            .collect()
    }
}

fn is_supported(role: Option<&str>, value: &str) -> bool {
    const VALUES: &[&str] = &[
        "session_id",
        "version",
        "workspace",
        "model",
        "effort",
        "lifetime_tokens",
        "context",
        "approval",
        "access",
        "connected_marker",
        "plan_mode",
    ];
    const ROLES: &[&str] = &[
        "label", "value", "active", "muted", "warning", "success", "error",
    ];
    match role {
        None => VALUES.contains(&value),
        Some(role) => {
            ROLES.contains(&role)
                && !value.is_empty()
                && !value.contains('{')
                && !value.contains('}')
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Theme {
    pub(crate) colors: BTreeMap<String, Color>,
}

impl Default for Theme {
    fn default() -> Self {
        let entries = [
            ("canvas", crate::syndrid_visuals::BACKGROUND),
            ("panel", crate::syndrid_visuals::PANEL),
            ("focused", crate::syndrid_visuals::FOCUSED_SURFACE),
            ("active", crate::syndrid_visuals::GOLD),
            ("selected", crate::syndrid_visuals::BRIGHT_GOLD),
            ("border", crate::syndrid_visuals::BORDER),
            ("text", crate::syndrid_visuals::PRIMARY_TEXT),
            ("muted", crate::syndrid_visuals::MUTED_TEXT),
            ("inactive", crate::syndrid_visuals::INACTIVE_TEXT),
            ("success", crate::syndrid_visuals::SUCCESS),
            ("warning", crate::syndrid_visuals::GOLD),
            ("error", crate::syndrid_visuals::ERROR),
            ("addition", crate::syndrid_visuals::SUCCESS),
            ("deletion", crate::syndrid_visuals::ERROR),
        ];
        Self {
            colors: entries
                .into_iter()
                .map(|(name, color)| (name.to_string(), color))
                .collect(),
        }
    }
}

impl Theme {
    pub(crate) fn from_toml(source: &str) -> Self {
        Self::try_from_toml(source).unwrap_or_default()
    }

    fn try_from_toml(source: &str) -> Result<Self, String> {
        let mut theme = Self::default();
        let value: toml::Value = toml::from_str(source).map_err(|error| error.to_string())?;
        let Some(table) = value.get("colors").and_then(toml::Value::as_table) else {
            return Err("theme is missing [colors]".to_string());
        };
        for (name, value) in table {
            if let Some(color) = value.as_str().and_then(parse_color) {
                if theme.colors.contains_key(name) {
                    theme.colors.insert(name.clone(), color);
                }
            }
        }
        Ok(theme)
    }

    fn style(&self, role: Option<&str>) -> Style {
        let color = match role {
            Some("label") | Some("muted") => self.colors.get("muted"),
            Some("value") => self.colors.get("text"),
            Some("active") => self.colors.get("active"),
            Some("warning") => self.colors.get("warning"),
            Some("success") => self.colors.get("success"),
            Some("error") => self.colors.get("error"),
            _ => self.colors.get("text"),
        };
        Style::default().fg(color
            .copied()
            .unwrap_or(crate::syndrid_visuals::PRIMARY_TEXT))
    }
}

fn parse_color(value: &str) -> Option<Color> {
    let hex = value.strip_prefix('#')?;
    if hex.len() != 6 {
        return None;
    }
    let bytes = (0..6)
        .step_by(2)
        .map(|index| u8::from_str_radix(&hex[index..index + 2], 16))
        .collect::<Result<Vec<_>, _>>()
        .ok()?;
    Some(Color::Rgb(bytes[0], bytes[1], bytes[2]))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct LayoutSettings {
    pub(crate) preferred_width: u16,
    pub(crate) compact_below: u16,
    pub(crate) narrow_below: u16,
}

impl Default for LayoutSettings {
    fn default() -> Self {
        Self {
            preferred_width: 120,
            compact_below: 80,
            narrow_below: 50,
        }
    }
}

impl LayoutSettings {
    fn from_toml(source: &str) -> Self {
        Self::try_from_toml(source).unwrap_or_default()
    }

    fn try_from_toml(source: &str) -> Result<Self, String> {
        let mut layout = Self::default();
        let value: toml::Value = toml::from_str(source).map_err(|error| error.to_string())?;
        if let Some(home) = value.get("home").and_then(toml::Value::as_table) {
            if let Some(width) = home
                .get("preferred_width")
                .and_then(toml::Value::as_integer)
            {
                layout.preferred_width = u16::try_from(width).unwrap_or(layout.preferred_width);
            }
            if let Some(width) = home.get("compact_below").and_then(toml::Value::as_integer) {
                layout.compact_below = u16::try_from(width).unwrap_or(layout.compact_below);
            }
            if let Some(width) = home.get("narrow_below").and_then(toml::Value::as_integer) {
                layout.narrow_below = u16::try_from(width).unwrap_or(layout.narrow_below);
            }
        }
        Ok(layout)
    }
}

#[derive(Clone, Debug)]
pub(crate) struct BannerSet {
    pub(crate) home: BannerTemplate,
    pub(crate) input_regular: BannerTemplate,
    pub(crate) input_plan: BannerTemplate,
    pub(crate) theme: Theme,
    pub(crate) layout: LayoutSettings,
}

#[derive(Clone, Debug)]
pub(crate) struct BannerManager {
    directory: Option<PathBuf>,
    active: BannerSet,
}

impl BannerManager {
    pub(crate) fn new(directory: Option<PathBuf>) -> Result<Self, String> {
        let active = BannerSet::load_from_directory(directory.as_deref())?;
        Ok(Self { directory, active })
    }

    pub(crate) fn active(&self) -> &BannerSet {
        &self.active
    }

    pub(crate) fn reload(&mut self) -> Result<(), String> {
        let candidate = BannerSet::load_strict(self.directory.as_deref())?;
        self.active = candidate;
        Ok(())
    }
}

impl BannerSet {
    fn load_from_directory(directory: Option<&Path>) -> Result<Self, String> {
        let read_template = |name: &str, bundled: &str| {
            directory
                .and_then(|dir| fs::read_to_string(dir.join(name)).ok())
                .and_then(|source| BannerTemplate::parse(&source).ok())
                .unwrap_or_else(|| BannerTemplate::parse(bundled).expect("bundled banner is valid"))
        };
        Ok(Self {
            home: read_template("home.txt", include_str!("../../../banners/home.txt")),
            input_regular: read_template(
                "input-regular.txt",
                include_str!("../../../banners/input-regular.txt"),
            ),
            input_plan: read_template(
                "input-plan.txt",
                include_str!("../../../banners/input-plan.txt"),
            ),
            theme: directory.map_or_else(
                || Theme::from_toml(include_str!("../../../banners/theme.toml")),
                |dir| {
                    fs::read_to_string(dir.join("theme.toml")).map_or_else(
                        |_| Theme::from_toml(include_str!("../../../banners/theme.toml")),
                        |source| Theme::from_toml(&source),
                    )
                },
            ),
            layout: directory.map_or_else(
                || LayoutSettings::from_toml(include_str!("../../../banners/layout.toml")),
                |dir| {
                    fs::read_to_string(dir.join("layout.toml")).map_or_else(
                        |_| LayoutSettings::from_toml(include_str!("../../../banners/layout.toml")),
                        |source| LayoutSettings::from_toml(&source),
                    )
                },
            ),
        })
    }

    fn load_strict(directory: Option<&Path>) -> Result<Self, String> {
        let read_template = |name: &str, bundled: &str| {
            directory
                .and_then(|dir| fs::read_to_string(dir.join(name)).ok())
                .map_or_else(
                    || BannerTemplate::parse(bundled),
                    |source| BannerTemplate::parse(&source),
                )
        };
        Ok(Self {
            home: read_template("home.txt", include_str!("../../../banners/home.txt"))?,
            input_regular: read_template(
                "input-regular.txt",
                include_str!("../../../banners/input-regular.txt"),
            )?,
            input_plan: read_template(
                "input-plan.txt",
                include_str!("../../../banners/input-plan.txt"),
            )?,
            theme: directory.map_or_else(
                || Theme::try_from_toml(include_str!("../../../banners/theme.toml")),
                |dir| {
                    fs::read_to_string(dir.join("theme.toml")).map_or_else(
                        |_| Theme::try_from_toml(include_str!("../../../banners/theme.toml")),
                        |source| Theme::try_from_toml(&source),
                    )
                },
            )?,
            layout: directory.map_or_else(
                || LayoutSettings::try_from_toml(include_str!("../../../banners/layout.toml")),
                |dir| {
                    fs::read_to_string(dir.join("layout.toml")).map_or_else(
                        |_| {
                            LayoutSettings::try_from_toml(include_str!(
                                "../../../banners/layout.toml"
                            ))
                        },
                        |source| LayoutSettings::try_from_toml(&source),
                    )
                },
            )?,
        })
    }
}

pub(crate) fn active_banner_set() -> Result<BannerSet, String> {
    static MANAGER: OnceLock<Mutex<Option<BannerManager>>> = OnceLock::new();
    let directory = std::env::var_os("SYNDRID_BANNER_DIR").map(PathBuf::from);
    let mut manager = MANAGER
        .get_or_init(|| Mutex::new(None))
        .lock()
        .map_err(|_| "banner manager lock poisoned".to_string())?;
    if manager
        .as_ref()
        .is_none_or(|current| current.directory != directory)
    {
        *manager = Some(BannerManager::new(directory)?);
    } else if let Some(current) = manager.as_mut() {
        if current.directory.is_some() {
            if let Err(error) = current.reload() {
                warn!(%error, "invalid Syndrid banner reload; retaining previous valid bundle");
            }
        }
    }
    manager
        .as_ref()
        .map(|current| current.active().clone())
        .ok_or_else(|| "banner manager did not initialize".to_string())
}

pub(crate) fn renderer_mode(config_home: &Path) -> RendererMode {
    if let Ok(value) = std::env::var("SYNDRID_UI_RENDERER") {
        return if value.eq_ignore_ascii_case("templates") {
            RendererMode::Templates
        } else {
            RendererMode::Builtin
        };
    }
    let path = config_home.join("config.toml");
    let Ok(source) = fs::read_to_string(path) else {
        return RendererMode::Builtin;
    };
    let Ok(value) = toml::from_str::<toml::Value>(&source) else {
        return RendererMode::Builtin;
    };
    value
        .get("syndrid")
        .and_then(|v| v.get("ui"))
        .and_then(|v| v.get("renderer"))
        .and_then(toml::Value::as_str)
        .map_or(RendererMode::Builtin, |value| {
            if value == "templates" {
                RendererMode::Templates
            } else {
                RendererMode::Builtin
            }
        })
}

pub(crate) fn context(
    values: impl IntoIterator<Item = (&'static str, String)>,
) -> BTreeMap<String, String> {
    values
        .into_iter()
        .map(|(key, value)| (key.to_string(), value))
        .collect()
}

pub(crate) fn template_rule(width: usize, plan: bool) -> Option<Line<'static>> {
    if std::env::var("SYNDRID_UI_RENDERER")
        .ok()?
        .eq_ignore_ascii_case("templates")
    {
        let banners = active_banner_set().ok()?;
        let template = if plan {
            &banners.input_plan
        } else {
            &banners.input_regular
        };
        return template
            .render(&BTreeMap::new(), &banners.theme, width)
            .into_iter()
            .next();
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn write_bundle(directory: &Path, home: &str, theme: &str, layout: &str) {
        fs::write(directory.join("home.txt"), home).expect("home template");
        fs::write(directory.join("input-regular.txt"), "regular").expect("regular template");
        fs::write(directory.join("input-plan.txt"), "plan").expect("plan template");
        fs::write(directory.join("theme.toml"), theme).expect("theme");
        fs::write(directory.join("layout.toml"), layout).expect("layout");
    }

    #[test]
    fn parser_accepts_supported_placeholders_and_roles() {
        let template = BannerTemplate::parse("{{label:Directory}} {{value:workspace}} {{active:model}} {{muted:text}} {{warning:text}} {{success:text}} {{error:text}} {{session_id}}").expect("supported syntax");
        assert!(template.lines[0].len() >= 8);
    }

    #[test]
    fn parser_rejects_expressions_and_commands() {
        assert!(BannerTemplate::parse("{{workspace | shell(\"whoami\")}}").is_err());
        assert!(BannerTemplate::parse("{{if:command}}").is_err());
    }

    #[test]
    fn missing_values_render_as_em_dash() {
        let template = BannerTemplate::parse("{{workspace}}").expect("valid");
        let lines = template.render(&BTreeMap::new(), &Theme::default(), 20);
        assert_eq!(lines[0].to_string(), MISSING);
    }

    #[test]
    fn invalid_theme_colors_fall_back_individually() {
        let theme = Theme::from_toml("[colors]\ntext = \"#ffffff\"\nerror = \"wat\"");
        assert_eq!(theme.colors["text"], Color::Rgb(255, 255, 255));
        assert_eq!(theme.colors["error"], Theme::default().colors["error"]);
    }

    #[test]
    fn valid_initial_load_and_valid_reload_replace_the_bundle() {
        let directory = tempfile::tempdir().expect("temp directory");
        write_bundle(
            directory.path(),
            "old {{workspace}}",
            "[colors]\ntext = \"#ffffff\"",
            "[home]\npreferred_width = 90",
        );
        let mut manager =
            BannerManager::new(Some(directory.path().to_path_buf())).expect("initial bundle");
        assert_eq!(manager.active().home.lines[0][0].text, "old ");
        write_bundle(
            directory.path(),
            "new {{workspace}}",
            "[colors]\ntext = \"#000000\"",
            "[home]\npreferred_width = 100",
        );
        manager.reload().expect("valid reload");
        assert_eq!(manager.active().home.lines[0][0].text, "new ");
        assert_eq!(manager.active().layout.preferred_width, 100);
    }

    #[test]
    fn invalid_template_reload_keeps_previous_home_and_no_blank_render() {
        let directory = tempfile::tempdir().expect("temp directory");
        write_bundle(
            directory.path(),
            "valid",
            "[colors]\ntext = \"#ffffff\"",
            "[home]",
        );
        let mut manager =
            BannerManager::new(Some(directory.path().to_path_buf())).expect("initial bundle");
        fs::write(directory.path().join("home.txt"), "{{unterminated").expect("invalid home");
        assert!(manager.reload().is_err());
        assert!(!manager.active().home.lines.is_empty());
        assert_eq!(manager.active().home.lines[0][0].text, "valid");
    }

    #[test]
    fn invalid_theme_and_layout_reload_keep_previous_values() {
        let directory = tempfile::tempdir().expect("temp directory");
        write_bundle(
            directory.path(),
            "valid",
            "[colors]\ntext = \"#ffffff\"",
            "[home]\npreferred_width = 90",
        );
        let mut manager =
            BannerManager::new(Some(directory.path().to_path_buf())).expect("initial bundle");
        fs::write(directory.path().join("theme.toml"), "[").expect("invalid theme");
        assert!(manager.reload().is_err());
        assert_eq!(
            manager.active().theme.colors["text"],
            Color::Rgb(255, 255, 255)
        );
        fs::write(
            directory.path().join("theme.toml"),
            "[colors]\ntext = \"#ffffff\"",
        )
        .expect("valid theme");
        fs::write(directory.path().join("layout.toml"), "[").expect("invalid layout");
        assert!(manager.reload().is_err());
        assert_eq!(manager.active().layout.preferred_width, 90);
    }

    #[test]
    fn missing_external_files_fall_back_to_bundled_templates() {
        let directory = tempfile::tempdir().expect("temp directory");
        let manager =
            BannerManager::new(Some(directory.path().to_path_buf())).expect("bundled fallback");
        assert!(!manager.active().home.lines.is_empty());
        assert!(!manager.active().input_regular.lines.is_empty());
        assert!(!manager.active().input_plan.lines.is_empty());
    }
}
