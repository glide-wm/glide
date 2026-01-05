//! Helpers for configuration merging and validation.

use std::error::Error;
use std::fmt::{Display, Formatter};

/// A partial config struct, which contains optional values of everything.
/// This is usually generated with the `PartialConfig` macro.
pub trait PartialConfig {
    /// Merges the values from `high` and `low`, where `high` takes precedence
    /// in the case of conflicts.
    fn merge(low: Self, high: Self) -> Self;

    /// Validates the final struct. All required values should exist.
    fn validate(self) -> Result<Self::Output, ValidationError>;

    /// The full config type, returned after validation.
    type Output;
}

#[derive(Default, Debug)]
pub struct ValidationError {
    pub fields: Vec<&'static str>,
    pub path: Vec<String>,
}

impl Display for ValidationError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Missing fields {:?} at path {}",
            self.fields,
            self.path.join(".")
        )
    }
}
impl Error for ValidationError {}

macro_rules! PartialConfig {
    (
        #[derive_args($SourceStructName:ident)]
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
        PartialConfig!(
            @source(
                // Identifiers to be used in pushdown outputs, supplied here for
                // hygiene reasons.
                low, high, self, err
            ) [
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

    // Base case: All fields have been processed. Build the final definition.
    (@source($low:ident, $high:ident, $self:ident, $err:ident) [
        $(#[$struct_meta:meta])*
        struct $StructName:ident => $SourceStructName:ident { }
    ] -> [
        [ $($fields:tt)* ]; [ $($merge:tt)* ]; [ $($check:tt)* ]; [ $($validate:tt)* ]
    ]) => {
        $(#[$struct_meta])*
        // We can derive(Default) because all fields are Option or another
        // Source struct.
        #[derive(Default)]
        #[serde(default)]
        struct $SourceStructName {
            $( $fields )*
        }

        impl PartialConfig for $SourceStructName {
            type Output = $StructName;

            fn merge($low: Self, $high: Self) -> Self {
                Self {
                    $( $merge )*
                }
            }

            fn validate($self) -> Result<$StructName, ValidationError> {
                #[allow(unused_mut)]
                let mut $err = ValidationError::default();
                $($check)*
                if !$err.fields.is_empty() {
                    return Err($err);
                }
                Ok($StructName {
                    $($validate)*
                })
            }
        }
    };

    // `#[derive_args(source)]` field case: Use the source field type.
    (@source($low:ident, $high:ident, $self:ident, $err:ident) [
        $(#[$struct_meta:meta])*
        struct $StructName:ident => $SourceStructName:ident {
            #[derive_args($source_field_ty:ident)]
            $(#[$($field_meta:tt)*])*
            $field_name:ident: $field_ty:ty,
            $($rest:tt)*
        }
    ] -> [
        [ $($fields:tt)* ]; [ $($merge:tt)* ]; [ $($check:tt)* ]; [ $($validate:tt)* ]
    ]) => {
        PartialConfig! {
            @source($low, $high, $self, $err) [
                $(#[$struct_meta])*
                struct $StructName => $SourceStructName { $($rest)* }
            ] -> [
                [
                    $($fields)*

                    $(#[$field_meta])*
                    $field_name: $source_field_ty,
                ];
                [
                    $($merge)*
                    $field_name: <$source_field_ty as PartialConfig>::merge($high.$field_name, $low.$field_name),
                ];
                [
                    $($check)*
                    // Checking happens via the call to validate below.
                ];
                [
                    $($validate)*
                    $field_name: $self.$field_name.validate()?,
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
        [ $($fields:tt)* ]; [ $($merge:tt)* ]; [ $($check:tt)* ]; [ $($validate:tt)* ]
    ]) => {
        PartialConfig! {
            @source($low, $high, $self, $err) [
                $(#[$struct_meta])*
                struct $StructName => $SourceStructName { $($rest)* }
            ] -> [
                [
                    $($fields)*

                    $(#[$field_meta])*
                    $field_name: Option<$field_ty>,
                ];
                [
                    $($merge)*
                    $field_name: $high.$field_name.or($low.$field_name),
                ];
                [
                    $($check)*
                    if $self.$field_name.is_none() {
                        $err.fields.push(stringify!($field_name));
                    }
                ];
                [
                    $($validate)*
                    // We can unwrap because we will have returned if any fields
                    // were None already.
                    $field_name: $self.$field_name.unwrap(),
                ]
            ]
        }
    };
}
