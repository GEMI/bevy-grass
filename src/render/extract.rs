//! Extracting the main world's grass state for the render world.

use bevy::ecs::prelude::*;
use bevy::light::DirectionalLight;
use bevy::math::{UVec4, Vec2, Vec3, Vec4};
use bevy::render::Extract;
use bevy::transform::components::GlobalTransform;

use super::resources::FrameUniform;
use crate::GrassSettings;
use crate::api::{GrassInteractor, GrassInteractorSpread, GrassSun, interactor_offsets};
use crate::clock::GrassClock;
use crate::fields::{CHUNK_SIZE, FieldTables};
use crate::sun::{SunCandidate, pick_sun};
use crate::trample;
use crate::wind::GrassWindFrame;

/// What the main world handed over this frame.
#[derive(Resource, Debug, Default)]
pub(crate) struct ExtractedGrass {
    pub tables: FieldTables,
    pub frame: FrameUniform,
    /// This frame of wind, as the shaders read it.
    pub wind: GrassWindFrame,
    /// Direction the first shadow-casting directional light shines, if any.
    pub shadow_light: Option<Vec3>,
    /// Direction to the light the deferred G-buffer treats as the sun: the
    /// first shadow-casting directional light, or else the first one.
    pub sun: Option<Vec3>,
    /// Every interactor, as `(x, z, radius, strength)`.
    pub interactors: Vec<Vec4>,
    /// This frame's settings, in range.
    pub settings: GrassSettings,
}

/// A directional light, for choosing the one grass follows.
type SunLight = (
    Entity,
    &'static DirectionalLight,
    &'static GlobalTransform,
    Has<GrassSun>,
);

/// Copies the tables (only when rebuilt, by reference), the wind, the clock,
/// the shadow-casting light and mode, and the interactors.
pub(crate) fn extract_grass(
    mut extracted: ResMut<ExtractedGrass>,
    tables: Extract<Res<FieldTables>>,
    wind: Extract<Res<GrassWindFrame>>,
    clock: Extract<Res<GrassClock>>,
    lights: Extract<Query<SunLight>>,
    interactors: Extract<
        Query<(
            &GrassInteractor,
            &GlobalTransform,
            Option<&GrassInteractorSpread>,
        )>,
    >,
    settings: Extract<Res<GrassSettings>>,
) {
    extracted.settings = settings.sanitized();
    let rows = extracted.settings.blade_rows;
    if tables.version != extracted.tables.version {
        extracted.tables = tables.clone();
    }
    extracted.frame = FrameUniform {
        time: clock.time,
        previous_time: clock.previous_time,
        chunk_size: CHUNK_SIZE,
        chunk_count: extracted.tables.chunks.len() as u32,
        lod_rows: UVec4::new(rows[0], rows[1], rows[2], 0),
    };
    // The renderer clamps the wind to non-negative values, as it does the
    // settings, so a custom source cannot make the blades misbehave.
    extracted.wind = GrassWindFrame {
        sway: wind.sway.max(0.0),
        sway_speed: wind.sway_speed.max(0.0),
        flutter: wind.flutter.max(0.0),
        gust_contrast: wind.gust_contrast.max(0.0),
        ..**wind
    };
    // The light grass follows: the one marked `GrassSun`, or else the first
    // that casts shadows. The G-buffer's sun is that light, or else the first.
    let (shadow_light, sun) =
        pick_sun(
            lights
                .iter()
                .map(|(entity, light, transform, marked)| SunCandidate {
                    entity,
                    casts: light.shadow_maps_enabled,
                    marked,
                    shines: transform.forward().as_vec3(),
                }),
        );
    extracted.shadow_light = shadow_light;
    extracted.sun = sun.map(|shines| -shines);
    // Take the old list so its capacity survives the frame, then fill it
    // while the tables are borrowed.
    let mut stamps = std::mem::take(&mut extracted.interactors);
    stamps.clear();
    let tables = &extracted.tables;
    for (interactor, transform, spread) in &interactors {
        for &offset in interactor_offsets(spread) {
            let at = transform.transform_point(offset);
            // Only an interactor with a height limit needs the ground.
            let ground = interactor
                .max_height
                .and_then(|_| tables.highest_ground_at(Vec2::new(at.x, at.z)));
            let reach = trample::reach(interactor.max_height, at.y, ground);
            stamps.push(Vec4::new(
                at.x,
                at.z,
                interactor.radius,
                interactor.strength * reach,
            ));
        }
    }
    extracted.interactors = stamps;
}
