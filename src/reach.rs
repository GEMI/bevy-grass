//! Says so when a camera looks at grass it is too far away to draw.
//!
//! A perspective camera draws grass only within `GrassSettings::radius` of
//! itself, and that is a distance in 3D. A strategy camera 150 m above the
//! ground can have every field in view and none in reach, and then draws no
//! grass and logs nothing: to the game it looks as if grass does not work, or
//! as if the blades were too thin to see. This check costs one box test per
//! camera and field a frame, against boxes rebuilt only when the tables are,
//! and it warns once per camera.

use bevy::camera::primitives::{Aabb, Frustum};
use bevy::camera::visibility::{RenderLayers, VisibilitySystems};
use bevy::camera::{Camera, Camera3d, Projection};
use bevy::ecs::entity::Entity;
use bevy::ecs::query::{With, Without};
use bevy::ecs::schedule::IntoScheduleConfigs;
use bevy::ecs::system::{Local, Query, Res};
use bevy::log::warn;
use bevy::math::{Affine3A, Mat3, Vec2, Vec3};
use bevy::platform::collections::{HashMap, HashSet};
use bevy::time::{Real, Time};
use bevy::transform::components::GlobalTransform;

use crate::api::{GrassCamera, NoGrass};
use crate::fields::{FieldTables, layer_mask};
use crate::{GrassSettings, GrassSystems};

/// How long a camera must see fields and reach none before the warning.
/// A camera passing through, or a zoom that overshoots for a moment, is
/// normal; a camera that stays there has a radius too short for its height.
const WARN_AFTER_SECS: f32 = 2.0;

/// Where one field's blades can be: its rectangle, from its lowest ground to
/// its highest ground plus its tallest blade.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct FieldBox {
    /// The rectangle's minimum corner, at the lowest ground.
    corner: Vec3,
    /// The field's X axis in world XZ; its Z axis is the perpendicular.
    axis: Vec2,
    /// Size along the field's X and Z.
    size: Vec2,
    /// Highest ground plus the tallest blade.
    top: f32,
    /// The render layers the field is drawn on; 0 while hidden.
    layers: u32,
}

impl FieldBox {
    /// The box as an AABB in the field's frame and the transform to world.
    fn oriented(&self) -> (Aabb, Affine3A) {
        let height = self.top - self.corner.y;
        let half = Vec3::new(self.size.x, height, self.size.y) * 0.5;
        let world_from_local = Affine3A::from_mat3_translation(
            Mat3::from_cols(
                Vec3::new(self.axis.x, 0.0, self.axis.y),
                Vec3::Y,
                Vec3::new(-self.axis.y, 0.0, self.axis.x),
            ),
            self.corner,
        );
        (Aabb::from_min_max(Vec3::ZERO, half * 2.0), world_from_local)
    }

    /// The distance from `point` to the nearest place in the box.
    pub(crate) fn distance_from(&self, point: Vec3) -> f32 {
        let offset = Vec2::new(point.x - self.corner.x, point.z - self.corner.z);
        let local = Vec2::new(offset.dot(self.axis), offset.dot(self.axis.perp()));
        let across = (local - local.clamp(Vec2::ZERO, self.size)).length();
        let up = point.y - point.y.clamp(self.corner.y, self.top);
        Vec2::new(across, up).length()
    }
}

/// The box of every field in `tables`, one per field.
pub(crate) fn field_boxes(tables: &FieldTables) -> Vec<FieldBox> {
    let mut ground: HashMap<u32, (f32, f32)> = HashMap::default();
    for chunk in tables.chunks.iter() {
        let range = ground.entry(chunk.ids.x).or_insert((f32::MAX, f32::MIN));
        *range = (range.0.min(chunk.bounds.z), range.1.max(chunk.bounds.w));
    }
    tables
        .slots
        .iter()
        .filter_map(|slot| {
            let field = tables.fields.get(slot.first)?;
            let (low, high) = *ground.get(&(slot.first as u32))?;
            Some(FieldBox {
                corner: Vec3::new(field.min_corner.x, low, field.min_corner.z),
                axis: field.x_axis(),
                size: Vec2::new(field.extent.x, field.extent.y),
                top: high + field.mix.w,
                layers: field.access.x,
            })
        })
        .collect()
}

/// How far the nearest field this camera sees is, or `None` when it sees
/// none: none in its frustum, or none on its render layers.
pub(crate) fn nearest_seen(
    boxes: &[FieldBox],
    frustum: &Frustum,
    layers: u32,
    position: Vec3,
) -> Option<f32> {
    boxes
        .iter()
        .filter(|field| field.layers & layers != 0)
        .filter(|field| {
            let (aabb, world_from_local) = field.oriented();
            frustum.intersects_obb(&aabb, &world_from_local, true, true)
        })
        .map(|field| field.distance_from(position))
        .min_by(f32::total_cmp)
}

/// How far the distances a camera draws grass by are measured from beyond
/// the camera: its height above the ground with
/// [`GrassCamera::radius_from_ground`], else 0.
pub(crate) fn distance_offset(
    camera: Option<&GrassCamera>,
    position: Vec3,
    orthographic: bool,
    tables: &FieldTables,
) -> f32 {
    if orthographic || !camera.is_some_and(|camera| camera.radius_from_ground) {
        return 0.0;
    }
    (position.y - tables.ground_below(Vec2::new(position.x, position.z))).max(0.0)
}

/// Adds the check to the app.
pub(crate) fn plugin(app: &mut bevy::app::App) {
    app.add_systems(
        bevy::app::PostUpdate,
        warn_out_of_reach
            .after(GrassSystems::Rebuild)
            .after(VisibilitySystems::UpdateFrusta),
    );
}

/// The cameras the check looks at: those that draw grass.
type GrassCameras<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static Camera,
        &'static Projection,
        &'static Frustum,
        &'static GlobalTransform,
        Option<&'static RenderLayers>,
        Option<&'static GrassCamera>,
    ),
    (With<Camera3d>, Without<NoGrass>),
>;

/// Warns, once per camera, when a perspective camera has seen fields for
/// [`WARN_AFTER_SECS`] and every one of them was beyond its grass radius.
fn warn_out_of_reach(
    cameras: GrassCameras,
    tables: Res<FieldTables>,
    settings: Res<GrassSettings>,
    time: Res<Time<Real>>,
    mut boxes: Local<(u64, Vec<FieldBox>)>,
    mut out_of_reach: Local<HashMap<Entity, f32>>,
    mut warned: Local<HashSet<Entity>>,
) {
    if boxes.0 != tables.version {
        *boxes = (tables.version, field_boxes(&tables));
    }
    out_of_reach.retain(|camera, _| cameras.contains(*camera));
    warned.retain(|camera| cameras.contains(*camera));
    for (entity, camera, projection, frustum, transform, layers, grass_camera) in &cameras {
        if !camera.is_active
            || !matches!(projection, Projection::Perspective(_))
            || warned.contains(&entity)
        {
            continue;
        }
        let position = transform.translation();
        let radius = settings.for_camera(grass_camera).radius
            + distance_offset(grass_camera, position, false, &tables);
        let nearest = nearest_seen(&boxes.1, frustum, layer_mask(layers), position);
        let Some(distance) = nearest.filter(|distance| *distance >= radius) else {
            out_of_reach.remove(&entity);
            continue;
        };
        let waited = out_of_reach.entry(entity).or_insert(0.0);
        *waited += time.delta_secs();
        if *waited >= WARN_AFTER_SECS {
            warned.insert(entity);
            warn!(
                "bevy_grass: the camera {entity} sees grass fields, but the nearest is \
                 {distance:.0} m away, beyond its grass radius of {radius:.0} m, so it draws \
                 no grass. Grass is drawn within the radius of the camera itself; for a \
                 camera high above the ground, set GrassCamera::radius_from_ground (see \
                 docs/settings.md, \"Strategy cameras\")."
            );
        }
    }
}

#[cfg(test)]
mod tests;
