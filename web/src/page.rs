//! The page's way in: the calls `index.html` makes into the demo, and the
//! status it reads back for its panel and the smoke test.
//!
//! The page calls in between frames, on the one thread the browser gives both,
//! so the mutex is never contended: it is only how a `static` is shared
//! without `unsafe`.

use std::fmt::Write as _;
use std::sync::{Mutex, MutexGuard, PoisonError};

use bevy::prelude::*;
use bevy_grass::prelude::*;
use wasm_bindgen::prelude::wasm_bindgen;

use crate::{Meadow, QUALITIES, Requests};

/// What the page asked for, and the status it reads.
struct Shared {
    requests: Requests,
    /// JSON, rewritten by `publish_status`.
    status: String,
}

static SHARED: Mutex<Shared> = Mutex::new(Shared {
    requests: Requests {
        quality: None,
        preset: None,
    },
    status: String::new(),
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
/// the tier and preset shown, and the blades of the latest GPU reading.
#[wasm_bindgen]
pub fn status() -> String {
    shared().status.clone()
}

pub(crate) fn take_requests(mut requests: ResMut<Requests>) {
    let page = std::mem::take(&mut shared().requests);
    if page != Requests::default() {
        requests.quality = page.quality.or(requests.quality);
        requests.preset = page.preset.or(requests.preset);
    }
}

pub(crate) fn publish_status(
    meadow: Option<Res<Meadow>>,
    stats: Res<GrassStats>,
    support: Res<GrassSupport>,
) {
    let Some(meadow) = meadow else {
        return;
    };
    if !(meadow.is_changed() || stats.is_changed() || support.is_changed()) {
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
         \"readings\":{},\"overflows\":{}}}",
        *support != GrassSupport::Unknown,
        support.is_supported(),
        QUALITIES[meadow.quality].0,
        meadow.preset,
        stats.gpu.readings,
        stats.gpu.overflows,
    );
}
