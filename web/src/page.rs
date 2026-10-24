//! The page's way in: the calls `index.html` makes into the demo, and the
//! status and settings it reads back for its panel and the smoke test.
//!
//! The page calls in between frames, on the one thread the browser gives both,
//! so the mutex is never contended: it is only how a `static` is shared
//! without `unsafe`.

use std::fmt::Write as _;
use std::sync::{Mutex, MutexGuard, PoisonError};

use bevy::prelude::*;
use bevy_grass::prelude::*;
use wasm_bindgen::prelude::wasm_bindgen;

use crate::tune::{self, TuneEdit, Tuning};
use crate::{Meadow, QUALITIES, Requests};

/// What the page asked for, and the status it reads.
struct Shared {
    requests: Requests,
    /// Settings to change, in the order the page changed them.
    edits: Vec<TuneEdit>,
    /// JSON, rewritten by `publish_status`.
    status: String,
    /// Every setting the page tunes, as JSON (see `tune::write_node`).
    tunables: String,
}

static SHARED: Mutex<Shared> = Mutex::new(Shared {
    requests: Requests {
        quality: None,
        preset: None,
        shadows: None,
        antialiasing: None,
    },
    edits: Vec::new(),
    status: String::new(),
    tunables: String::new(),
});

fn shared() -> MutexGuard<'static, Shared> {
    // A panic ends the app, so a poisoned lock has no later reader to mislead.
    SHARED.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Asks for the tier called `name`, `"LOW"` to `"ULTRA"`, from the next frame.
/// False if there is no such tier.
#[wasm_bindgen]
pub fn set_quality(name: &str) -> bool {
    let at = QUALITIES.iter().position(|(quality, _)| *quality == name);
    if at.is_some() {
        shared().requests.quality = at;
    }
    at.is_some()
}

/// Asks for the preset called `name`, such as `"WHEAT"`, from the next frame.
/// False if there is no such preset.
#[wasm_bindgen]
pub fn set_preset(name: &str) -> bool {
    let found = crate::preset(name).map(|(name, _)| name);
    if found.is_some() {
        shared().requests.preset = found;
    }
    found.is_some()
}

/// Asks for shadows to be enabled or disabled from the next frame.
#[wasm_bindgen]
pub fn set_shadows(enabled: bool) -> bool {
    shared().requests.shadows = Some(enabled);
    true
}

/// Asks for the sample count to be set from the next frame.
/// False if the sample count is not 1, 2, 4 or 8.
#[wasm_bindgen]
pub fn set_antialiasing(samples: u32) -> bool {
    if [1, 2, 4, 8].contains(&samples) {
        shared().requests.antialiasing = Some(samples);
        true
    } else {
        false
    }
}

/// Sets one setting from the next frame: `target` is one of `tune::TARGETS`,
/// `path` a reflect path into it (`blade.height.y`), and `value` a number,
/// `true` or `false`, a `#rrggbb` colour or an enum variant (`"Some:150"`
/// also sets its value). A value that does not fit is logged and skipped.
#[wasm_bindgen]
pub fn set(target: &str, path: &str, value: &str) {
    shared().edits.push(TuneEdit::Set {
        target: target.to_owned(),
        path: path.to_owned(),
        value: value.to_owned(),
    });
}

/// Puts `target` back as its tier, preset or default has it.
#[wasm_bindgen]
pub fn reset(target: &str) {
    shared().edits.push(TuneEdit::Reset(target.to_owned()));
}

/// Asks for the wind preset called `name`, such as `"STORM"`. False if there
/// is no such preset.
#[wasm_bindgen]
pub fn set_wind(name: &str) -> bool {
    let found = tune::WINDS.iter().any(|(wind, _)| *wind == name);
    if found {
        shared().edits.push(TuneEdit::Wind(name.to_owned()));
    }
    found
}

/// Adds a species of the plant called `name`, such as `"POPPY"`, to the
/// style. False if there is no such plant.
#[wasm_bindgen]
pub fn add_species(name: &str) -> bool {
    let found = tune::BLADES.iter().any(|(blade, _)| *blade == name);
    if found {
        shared().edits.push(TuneEdit::AddSpecies(name.to_owned()));
    }
    found
}

/// Removes the style's species at `index`.
#[wasm_bindgen]
pub fn remove_species(index: usize) {
    shared().edits.push(TuneEdit::RemoveSpecies(index));
}

/// Replaces the style with one read from a `.grass.ron` file. An empty string
/// if it reads, or why not.
#[wasm_bindgen]
pub fn import_style(ron: &str) -> String {
    match GrassStyle::from_ron(ron) {
        Ok(style) => {
            shared().edits.push(TuneEdit::Style(Box::new(style)));
            String::new()
        }
        Err(error) => error.to_string(),
    }
}

/// Grows the meadow back as it was before the brush.
#[wasm_bindgen]
pub fn restore_field() {
    shared().edits.push(TuneEdit::RestoreField);
}

/// Every setting the page tunes, as JSON: a revision that changes when values
/// changed other than through `set`, and per target its value, its Rust and,
/// for the style, its `.grass.ron`.
#[wasm_bindgen]
pub fn tunables() -> String {
    shared().tunables.clone()
}

/// The wind presets.
#[wasm_bindgen]
pub fn winds() -> Vec<String> {
    tune::WINDS
        .iter()
        .map(|(name, _)| (*name).to_owned())
        .collect()
}

/// The plants a species can start from.
#[wasm_bindgen]
pub fn blades() -> Vec<String> {
    tune::BLADES
        .iter()
        .map(|(name, _)| (*name).to_owned())
        .collect()
}

/// The tiers, in the order the page shows them.
#[wasm_bindgen]
pub fn qualities() -> Vec<String> {
    QUALITIES
        .iter()
        .map(|(name, _)| (*name).to_owned())
        .collect()
}

/// Every preset's name.
#[wasm_bindgen]
pub fn presets() -> Vec<String> {
    GrassStyle::presets()
        .map(|(name, _)| name.to_owned())
        .collect()
}

/// The demo's status as JSON: whether this device draws grass and why not,
/// the tier and preset shown, the blades of the latest GPU reading, and every
/// `GrassStats` number.
#[wasm_bindgen]
pub fn status() -> String {
    shared().status.clone()
}

pub(crate) fn take_requests(mut requests: ResMut<Requests>, mut tuning: ResMut<Tuning>) {
    let mut shared = shared();
    if !shared.edits.is_empty() {
        tuning.edits.append(&mut shared.edits);
    }
    let page = std::mem::take(&mut shared.requests);
    if page != Requests::default() {
        requests.quality = page.quality.or(requests.quality);
        requests.preset = page.preset.or(requests.preset);
        requests.shadows = page.shadows.or(requests.shadows);
        requests.antialiasing = page.antialiasing.or(requests.antialiasing);
    }
}

pub(crate) fn publish_status(
    meadow: Option<Res<Meadow>>,
    stats: Res<GrassStats>,
    support: Res<GrassSupport>,
    tuning: Res<Tuning>,
    cameras: Query<Ref<Msaa>, With<Camera3d>>,
) {
    let Some(meadow) = meadow else {
        return;
    };
    if tuning.is_changed() && !tuning.json.is_empty() && shared().tunables != tuning.json {
        shared().tunables.clone_from(&tuning.json);
    }
    // What the camera draws with: grass lowers a sample count the GPU lacks.
    let msaa = cameras.iter().next();
    let samples = msaa
        .as_ref()
        .map_or(meadow.antialiasing, |msaa| msaa.samples());
    let msaa_changed = msaa.is_some_and(|msaa| msaa.is_changed());
    if !(meadow.is_changed() || stats.is_changed() || support.is_changed() || msaa_changed) {
        return;
    }
    // Tier and preset names are identifiers; the support message is prose.
    let message = support
        .to_string()
        .replace('\\', "\\\\")
        .replace('"', "\\\"");
    let mut shared = shared();
    let json = &mut shared.status;
    json.clear();
    let [near, mid, far] = stats.gpu.blades;
    let _ = write!(
        json,
        "{{\"support\":\"{message}\",\"decided\":{},\"supported\":{},\
         \"quality\":\"{}\",\"preset\":\"{}\",\"blades\":[{near},{mid},{far}],\
         \"readings\":{},\"overflows\":{},\
         \"shadows\":{},\"antialiasing\":{},\"stats\":",
        *support != GrassSupport::Unknown,
        support.is_supported(),
        QUALITIES[meadow.quality].0,
        meadow.preset,
        stats.gpu.readings,
        stats.gpu.overflows,
        meadow.shadows,
        samples,
    );
    tune::write_node(&*stats, json);
    json.push('}');
}
