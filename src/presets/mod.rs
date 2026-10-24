//! The preset styles and the one list they come from.
//!
//! Each group lives in its own file (`general.rs`, `seasons.rs`, ...). To add a
//! preset, add one entry to the block for its group:
//!
//! ```text
//! MY_STYLE => Self { /* the style */ };
//! ```
//!
//! Then run `BLESS_STYLES=1 cargo test` to write
//! `assets/grass_styles/my_style.grass.ron` from the constant. The file name is
//! the lower-case constant name. The group arrays, `all_presets()` and the
//! embedded preset assets all come from these blocks, so no other file needs a
//! style list.

use std::borrow::Cow;

use bevy::color::Color;
use bevy::math::Vec2;

use crate::style::{GrassBlade, GrassSpecies, GrassStyle, GrassVariation, GroundTint};

/// Declares one group of presets: its constants, its group array and the
/// `(name, style)` slice the registry reads.
macro_rules! preset_group {
    (
        $(#[$group_attr:meta])*
        $group:ident {
            $(
                $(#[$attr:meta])*
                $name:ident => $value:expr;
            )*
        }
    ) => {
        impl GrassStyle {
            $(
                $(#[$attr])*
                pub const $name: Self = $value;
            )*

            $(#[$group_attr])*
            pub const $group: [Self; preset_group!(@count $($name)*)] = [
                $( Self::$name, )*
            ];
        }

        pub(crate) const PRESETS: &[(&str, GrassStyle)] = &[
            $( (stringify!($name), GrassStyle::$name), )*
        ];
    };
    (@count) => { 0 };
    (@count $head:ident $($tail:ident)*) => {
        1 + preset_group!(@count $($tail)*)
    };
}

mod biomes;
mod blades;
mod fantasy;
mod forest;
mod general;
mod plants;
mod seasons;
mod toon;
mod wild;

/// Every group's presets, in the order `all_presets()` returns them.
static PRESET_GROUPS: &[&[(&str, GrassStyle)]] = &[
    general::PRESETS,
    seasons::PRESETS,
    wild::PRESETS,
    plants::PRESETS,
    forest::PRESETS,
    biomes::PRESETS,
    fantasy::PRESETS,
    toon::PRESETS,
];

/// Every preset constant and its name, in group order.
pub(crate) fn all_presets() -> impl Iterator<Item = (&'static str, &'static GrassStyle)> {
    PRESET_GROUPS
        .iter()
        .flat_map(|group| group.iter())
        .map(|(name, style)| (*name, style))
}

impl GrassStyle {
    /// The preset constant called `name`, such as `"SUMMER_TALL"`.
    #[must_use]
    pub fn preset(name: &str) -> Option<Self> {
        all_presets()
            .find(|(preset, _)| *preset == name)
            .map(|(_, style)| style.clone())
    }

    /// Every preset constant, with its name.
    pub fn presets() -> impl Iterator<Item = (&'static str, Self)> {
        all_presets().map(|(name, style)| (name, style.clone()))
    }
}

#[cfg(test)]
mod tests;
