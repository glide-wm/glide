#[macro_use]
mod partial;
use partial::{PartialConfig, ValidationError};

use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use anyhow::bail;
use livesplit_hotkey::Hotkey;
use macro_rules_attribute::derive;
use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};

use crate::actor::wm_controller::WmCommand;

pub fn data_dir() -> PathBuf {
    dirs::home_dir().unwrap().join(".glide")
}

pub fn restore_file() -> PathBuf {
    data_dir().join("layout.ron")
}

pub fn config_file() -> PathBuf {
    dirs::home_dir().unwrap().join(".glide.toml")
}

#[derive(Serialize, Deserialize)]
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
    #[derive_args(GroupIndicatorsPartial)]
    pub group_indicators: GroupIndicators,
}

#[derive(PartialConfig!)]
#[derive_args(StatusIconPartial)]
#[derive(Serialize, Deserialize, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct StatusIcon {
    pub enable: bool,
}

#[derive(PartialConfig!)]
#[derive_args(GroupIndicatorsPartial)]
#[derive(Serialize, Deserialize, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct GroupIndicators {
    pub enable: bool,
    pub bar_thickness: f64,
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

impl GroupIndicators {
    /// Get the indicator thickness for layout space reservation
    pub fn indicator_thickness(&self) -> f64 {
        if self.enable { self.bar_thickness } else { 0.0 }
    }
}

impl ConfigPartial {
    fn default() -> Self {
        toml::from_str(include_str!("../glide.default.toml")).unwrap()
    }

    fn validate(self) -> Result<Config, anyhow::Error> {
        let mut keys = Vec::new();
        for (key, cmd) in self.keys.unwrap_or_default() {
            let Ok(key) = Hotkey::from_str(&key) else {
                bail!("Could not parse hotkey: {key}");
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
    pub fn read(path: &Path) -> anyhow::Result<Config> {
        let mut buf = String::new();
        File::open(path).unwrap().read_to_string(&mut buf)?;
        Self::parse(&buf)
    }

    pub fn default() -> Config {
        ConfigPartial::default().validate().unwrap()
    }

    fn parse(buf: &str) -> anyhow::Result<Config> {
        let c: ConfigPartial = toml::from_str(&buf)?;
        let defaults = ConfigPartial::default();
        ConfigPartial::merge(defaults, c).validate()
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
