use std::error::Error;
use std::fmt::Display;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use anyhow::bail;
use livesplit_hotkey::Hotkey;
use macro_rules_attribute::derive;
use paste::paste;
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

#[derive(Default, Debug)]
struct ResolveError {
    fields: Vec<&'static str>,
    path: Vec<String>,
}

impl Display for ResolveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Missing fields {:?} at path {}",
            self.fields,
            self.path.join(".")
        )
    }
}
impl Error for ResolveError {}

macro_rules! ConfigSource {
    (
        #[derive_args(source = $SourceStructName:ident)]
        $(#[$struct_meta:meta])*
        $struct_vis:vis
        struct $StructName:ident {
            $(
                $(#[$($field_meta:tt)*])*
                $field_vis:vis
                $field_name:ident: $field_ty:ty
            ),* $(,)?
        }
    ) => {
        ConfigSource!(
            // Give identifiers to be used in pushdown outputs for hygiene reasons.
            @source(low, high, self, err)
            [
                // Input struct and fields left to process.
                $(#[$struct_meta])*
                struct $StructName => $SourceStructName {
                    $( $(#[$($field_meta)*])* $field_name: $field_ty, )*
                }
            ] -> [
                // Pushdown outputs.
                []; []; []; []
            ]
        );
    };

    // Base case: Build the final definition.
    (@source($low:ident, $high:ident, $self:ident, $err:ident) [
        $(#[$struct_meta:meta])*
        struct $StructName:ident => $SourceStructName:ident { }
    ] -> [
        [ $($fields:tt)* ]; [ $($merge:tt)* ]; [ $($validate:tt)* ]; [ $($resolve:tt)* ]
    ]) => { paste! {
        $(#[$struct_meta])*
        // We can derive(Default) because all fields are Option or another
        // Source struct.
        #[derive(Default)]
        #[serde(default)]
        struct $SourceStructName {
            $( $fields )*
        }

        impl $SourceStructName {
            fn merge($low: Self, $high: Self) -> Self {
                Self {
                    $( $merge )*
                }
            }

            fn resolve($self) -> Result<$StructName, ResolveError> {
                #[allow(unused_mut)]
                let mut $err = ResolveError::default();
                $($validate)*
                if !$err.fields.is_empty() {
                    return Err($err);
                }
                Ok($StructName {
                    $($resolve)*
                })
            }
        }
    } };

    // `#[derive_args(source)]` field case: Use the source field type.
    (@source($low:ident, $high:ident, $self:ident, $err:ident) [
        $(#[$struct_meta:meta])*
        struct $StructName:ident => $SourceStructName:ident {
            #[derive_args(source = $source_field_ty:ident)]
            $(#[$($field_meta:tt)*])*
            $field_name:ident: $field_ty:ty,
            $($rest:tt)*
        }
    ] -> [
        [ $($fields:tt)* ]; [ $($merge:tt)* ]; [ $($validate:tt)* ]; [ $($resolve:tt)* ]
    ]) => {
        ConfigSource! {
            @source($low, $high, $self, $err) [
                $(#[$struct_meta])*
                struct $StructName => $SourceStructName { $($rest)* }
            ] -> [
                [
                    $($fields)*

                    $(#[$field_meta])*
                    // $field_vis
                    $field_name: $source_field_ty,
                ];
                [
                    $($merge)*
                    $field_name: $source_field_ty::merge($high.$field_name, $low.$field_name),
                ];
                [
                    $($validate)*
                    // Validation happens via the call to resolve below.
                ];
                [
                    $($resolve)*
                    $field_name: $self.$field_name.resolve()?,
                ]
            ]
        }
    };

    // Default field case: Wrap the field type in Option.
    (@source($low:ident, $high:ident, $self:ident, $err:ident) [
        $(#[$struct_meta:meta])*
        struct $StructName:ident => $SourceStructName:ident {
            $(#[$($field_meta:tt)*])*
            $field_name:ident: $field_ty:ty,

            $($rest:tt)*
        }
    ] -> [
        [ $($fields:tt)* ]; [ $($merge:tt)* ]; [ $($validate:tt)* ]; [ $($resolve:tt)* ]
    ]) => {
        ConfigSource! {
            @source($low, $high, $self, $err) [
                $(#[$struct_meta])*
                struct $StructName => $SourceStructName { $($rest)* }
            ] -> [
                [
                    $($fields)*

                    $(#[$field_meta])*
                    // $field_vis
                    $field_name: Option<$field_ty>,
                ];
                [
                    $($merge)*
                    $field_name: $high.$field_name.or($low.$field_name),
                ];
                [
                    $($validate)*
                    if $self.$field_name.is_none() {
                        $err.fields.push(stringify!($field_name));
                    }
                ];
                [
                    $($resolve)*
                    // We can unwrap because we will have returned if any fields
                    // were None already.
                    $field_name: $self.$field_name.unwrap(),
                ]
            ]
        }
    };
}

#[derive(Serialize, Deserialize)]
pub struct Config {
    pub settings: Settings,
    pub keys: Vec<(Hotkey, WmCommand)>,
}

#[derive(Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
#[serde(default)]
struct ConfigSource {
    settings: SettingsSource,
    keys: Option<FxHashMap<String, WmCommand>>,
}

#[derive(ConfigSource!)]
#[derive_args(source = SettingsSource)]
#[derive(Serialize, Deserialize, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Settings {
    pub animate: bool,
    pub default_disable: bool,
    pub mouse_follows_focus: bool,
    pub mouse_hides_on_focus: bool,
    pub focus_follows_mouse: bool,
    #[derive_args(source = ExperimentalSource)]
    pub experimental: Experimental,
}

#[derive(ConfigSource!)]
#[derive_args(source = ExperimentalSource)]
#[derive(Serialize, Deserialize, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Experimental {
    #[derive_args(source = StatusIconSource)]
    pub status_icon: StatusIcon,
}

#[derive(ConfigSource!)]
#[derive_args(source = StatusIconSource)]
#[derive(Serialize, Deserialize, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct StatusIcon {
    pub enable: bool,
}

impl ConfigSource {
    fn default() -> Self {
        toml::from_str(include_str!("../glide.default.toml")).unwrap()
    }

    fn resolve(self) -> Result<Config, anyhow::Error> {
        let mut keys = Vec::new();
        for (key, cmd) in self.keys.unwrap_or_default() {
            let Ok(key) = Hotkey::from_str(&key) else {
                bail!("Could not parse hotkey: {key}");
            };
            keys.push((key, cmd));
        }
        Ok(Config {
            settings: self.settings.resolve()?,
            keys,
        })
    }

    fn merge(low: Self, high: Self) -> Self {
        Self {
            settings: SettingsSource::merge(low.settings, high.settings),
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
        ConfigSource::default().resolve().unwrap()
    }

    fn parse(buf: &str) -> anyhow::Result<Config> {
        let c: ConfigSource = toml::from_str(&buf)?;
        let defaults = ConfigSource::default();
        ConfigSource::merge(defaults, c).resolve()
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn default_config_parses() {
        super::Config::default();
    }

    #[test]
    fn default_settings_match_unspecified_setting_values() {
        assert_eq!(super::Config::default().settings, toml::from_str("").unwrap());
    }
}
