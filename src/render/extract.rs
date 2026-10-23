//! Extracting the main world's grass state for the render world.

use std::sync::Arc;

use bevy::camera::Camera;
use bevy::ecs::prelude::*;
use bevy::light::DirectionalLight;
use bevy::math::{UVec4, Vec2, Vec3};
use bevy::platform::collections::HashMap;
use bevy::render::Extract;
use bevy::render::sync_world::RenderEntity;
use bevy::transform::components::GlobalTransform;

use super::resources::{FrameUniform, WindUniform};
use crate::GrassSettings;
use crate::api::{
    GrassCamera, GrassInteractor, GrassInteractorFootprint, GrassInteractorSpread, GrassSun,
    NoGrass, interactor_offsets,
};
use crate::clock::GrassClock;
use crate::edit::{MaskPatches, PatchList};
use crate::fields::{CHUNK_SIZE, FieldTables};
use crate::sun::{SunCandidate, pick_sun};
use crate::trample::{self, Stamp, Tracked};
use crate::wind::GrassWindFrame;

/// What the main world handed over this frame.
#[derive(Resource, Debug, Default)]
pub(crate) struct ExtractedGrass {
    pub tables: FieldTables,
    /// This frame's edits to the masks, written after `tables`.
    pub mask_patches: Arc<PatchList>,
    pub frame: FrameUniform,
    /// This frame of wind, as the shaders read it.
    pub wind: WindUniform,
    /// Direction the first shadow-casting directional light shines, if any.
    pub shadow_light: Option<Vec3>,
    /// Direction to the light the deferred G-buffer treats as the sun: the
    /// first shadow-casting directional light, or else the first one.
    pub sun: Option<Vec3>,
    /// Every interactor's stamps: a disc per spread offset, or one box.
    pub interactors: Vec<Stamp>,
    /// Where each footprint was last frame and how it moved, for the
    /// direction its stamp lays the grass.
    pub tracked: HashMap<Entity, Tracked>,
    /// This frame's settings, in range.
    pub settings: GrassSettings,
}

/// An interactor, with its spread or footprint if it has one.
type Interactor = (
    Entity,
    &'static GrassInteractor,
    &'static GlobalTransform,
    Option<&'static GrassInteractorSpread>,
    Option<&'static GrassInteractorFootprint>,
);

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
    interactors: Extract<Query<Interactor>>,
    (settings, patches): (Extract<Res<GrassSettings>>, Extract<Res<MaskPatches>>),
) {
    extracted.settings = settings.sanitized();
    let rows = extracted.settings.blade_rows;
    if tables.version != extracted.tables.version {
        extracted.tables = tables.clone();
    }
    extracted.mask_patches = patches.0.clone();
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
    }
    .into();
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
    let mut tracked = std::mem::take(&mut extracted.tracked);
    let tables = &extracted.tables;
    let elapsed = (clock.time - clock.previous_time).max(0.0);
    // Boxes no longer here are forgotten; the rest are found again below.
    tracked.retain(|entity, _| {
        interactors
            .get(*entity)
            .is_ok_and(|(.., footprint)| footprint.is_some())
    });
    for (entity, interactor, transform, spread, footprint) in &interactors {
        if let Some(footprint) = footprint {
            let at = transform.transform_point(footprint.center);
            let ground = interactor
                .max_height
                .and_then(|_| tables.highest_ground_at(Vec2::new(at.x, at.z)));
            let reach = trample::reach(interactor.max_height, at.y, ground);
            let now = Vec2::new(at.x, at.z);
            let motion = tracked
                .entry(entity)
                .and_modify(|seen| *seen = seen.update(now, elapsed))
                .or_insert(Tracked {
                    at: now,
                    ..Tracked::default()
                })
                .motion();
            stamps.push(trample::box_stamp(
                transform,
                footprint,
                interactor.strength * reach,
                motion,
            ));
            continue;
        }
        for &offset in interactor_offsets(spread) {
            let at = transform.transform_point(offset);
            // Only an interactor with a height limit needs the ground.
            let ground = interactor
                .max_height
                .and_then(|_| tables.highest_ground_at(Vec2::new(at.x, at.z)));
            let reach = trample::reach(interactor.max_height, at.y, ground);
            stamps.push(Stamp::disc(
                at.x,
                at.z,
                interactor.radius,
                interactor.strength * reach,
            ));
        }
    }
    extracted.interactors = stamps;
    extracted.tracked = tracked;
}

/// A camera's render entity and the grass components it may carry.
type CameraComponents = (RenderEntity, Option<&'static GrassCamera>, Has<NoGrass>);

/// Copies each camera's [`GrassCamera`] and [`NoGrass`] to its render
/// entity, and takes them off again when the camera loses them.
///
/// A system rather than Bevy's `ExtractComponent`, so the public components
/// carry no render-world code, and this, the only place that knows how they
/// reach the render world, follows Bevy's extraction API alone.
pub(crate) fn extract_cameras(
    mut commands: Commands,
    cameras: Extract<Query<CameraComponents, With<Camera>>>,
) {
    for (entity, grass_camera, no_grass) in &cameras {
        let Ok(mut camera) = commands.get_entity(entity) else {
            continue;
        };
        match grass_camera {
            Some(grass_camera) => camera.insert(*grass_camera),
            None => camera.remove::<GrassCamera>(),
        };
        if no_grass {
            camera.insert(NoGrass);
        } else {
            camera.remove::<NoGrass>();
        }
    }
}
