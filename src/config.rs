// Copyright The Glide Authors
// SPDX-License-Identifier: MIT OR Apache-2.0

#[macro_use]
mod partial;
use std::fs::File;
use std::io::Read;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use livesplit_hotkey::Hotkey;
use macro_rules_attribute::derive;

use partial::{PartialConfig, ValidationError};
use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};

use crate::actor::wm_controller::WmCommand;

pub fn data_dir() -> PathBuf {
    dirs::home_dir().unwrap().join(".glide")
}

pub fn restore_file() -> PathBuf {
    data_dir().join("layout.ron")
}

pub fn config_path_default() -> PathBuf {
    dirs::home_dir().unwrap().join(".glide.toml")
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Config {
    pub settings: Settings,
    pub keys: Vec<(Hotkey, WmCommand)>,
}

#[derive(Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
#[serde(default)]
struct ConfigPartial {
    settings: SettingsPartial,
    keys: Option<FxHashMap<String, WmCommand>>,
}

#[derive(PartialConfig!)]
#[derive_args(SettingsPartial)]
#[derive(Serialize, Deserialize, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Settings {
    pub animate: bool,
    pub default_disable: bool,
    pub mouse_follows_focus: bool,
    pub mouse_hides_on_focus: bool,
    pub focus_follows_mouse: bool,
    pub outer_gap: f64,
    pub inner_gap: f64,
    #[derive_args(GroupBarsPartial)]
    pub group_bars: GroupBars,
    #[derive_args(ExperimentalPartial)]
    pub experimental: Experimental,
}

#[derive(PartialConfig!)]
#[derive_args(ExperimentalPartial)]
#[derive(Serialize, Deserialize, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Experimental {
    #[derive_args(StatusIconPartial)]
    pub status_icon: StatusIcon,
}

#[derive(PartialConfig!)]
#[derive_args(StatusIconPartial)]
#[derive(Serialize, Deserialize, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct StatusIcon {
    pub enable: bool,
}

#[derive(PartialConfig!)]
#[derive_args(GroupBarsPartial)]
#[derive(Serialize, Deserialize, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct GroupBars {
    pub enable: bool,
    pub thickness: f64,
    pub horizontal_placement: HorizontalPlacement,
    pub vertical_placement: VerticalPlacement,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Clone, Copy)]
#[serde(rename_all = "snake_case")]
pub enum HorizontalPlacement {
    Top,
    Bottom,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Clone, Copy)]
#[serde(rename_all = "snake_case")]
pub enum VerticalPlacement {
    Left,
    Right,
}

impl GroupBars {
    /// Get the indicator thickness for layout space reservation
    pub fn indicator_thickness(&self) -> f64 {
        if self.enable { self.thickness } else { 0.0 }
    }
}

impl ConfigPartial {
    fn default() -> Self {
        toml::from_str(include_str!("../glide.default.toml")).unwrap()
    }

    fn validate(self) -> Result<Config, SpannedError> {
        let mut keys = Vec::new();
        for (key, cmd) in self.keys.unwrap_or_default() {
            let Ok(key) = Hotkey::from_str(&key) else {
                return Err(SpannedError {
                    message: format!("Could not parse hotkey: {key}"),
                    span: None,
                });
            };
            keys.push((key, cmd));
        }
        Ok(Config {
            settings: self.settings.validate()?,
            keys,
        })
    }

    fn merge(low: Self, high: Self) -> Self {
        Self {
            settings: SettingsPartial::merge(low.settings, high.settings),
            keys: high.keys.or(low.keys),
        }
    }
}

impl Config {
    pub fn load(custom_path: Option<&Path>) -> anyhow::Result<Config> {
        let mut buf = String::new();
        let default = config_path_default();
        let (mut file, path) = match custom_path {
            Some(path) => (File::open(path)?, path),
            None => match File::open(&default) {
                Ok(file) => (file, &*default),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Config::default()),
                Err(e) => return Err(e.into()),
            },
        };
        file.read_to_string(&mut buf)?;
        Self::parse(&buf).map_err(|e| anyhow::anyhow!("{}", format_toml_error(e, &buf, path)))
    }

    pub fn default() -> Config {
        ConfigPartial::default().validate().unwrap()
    }

    fn parse(buf: &str) -> Result<Self, SpannedError> {
        let c: ConfigPartial = toml::from_str(buf)?;
        let defaults = ConfigPartial::default();
        ConfigPartial::merge(defaults, c).validate()
    }
}

fn format_toml_error(error: SpannedError, input: &str, path: &Path) -> String {
    use annotate_snippets::{AnnotationKind, Level, Renderer, Snippet};

    let message = error.message;
    let Some(span) = error.span else {
        return format!("could not parse config: {}", message);
    };

    let snippet = Snippet::source(input)
        .path(path.to_string_lossy())
        .annotation(AnnotationKind::Primary.span(span.start..span.end).label(message));

    let report = Level::ERROR.primary_title("could not parse config").element(snippet);

    let renderer = Renderer::styled();
    format!("{}", renderer.render(&[report]))
}

#[derive(Debug)]
struct SpannedError {
    message: String,
    span: Option<Range<usize>>,
}

impl From<toml::de::Error> for SpannedError {
    fn from(e: toml::de::Error) -> Self {
        Self {
            message: e.message().to_owned(),
            span: e.span(),
        }
    }
}

impl From<ValidationError> for SpannedError {
    fn from(e: ValidationError) -> Self {
        Self {
            message: format!("{e}"),
            span: None, // TODO
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_is_valid() {
        Config::default();
    }

    #[test]
    fn default_settings_match_unspecified_setting_values() {
        assert_eq!(Config::default().settings, Config::parse("").unwrap().settings);
    }
}
