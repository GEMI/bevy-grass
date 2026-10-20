//! `GrassDebugPlugin`: gizmos that show what grass thinks, so "why is there no
//! grass here?" is answered by looking.
//!
//! Needs the `debug` cargo feature, which turns on Bevy's `bevy_gizmos`. A
//! game using `DefaultPlugins` has it already, so turning this on changes
//! nothing about how Bevy is compiled.
//!
//! What it draws, in the XZ plane at ground level:
//! - **fields** (green; grey while hidden): the rectangle the GPU tables hold,
//!   which is `size` times the scale, wherever the field really is and
//!   turned with it. A heightmap or mesh field also gets its highest ground.
//! - **chunks** (blue, off by default): the 16 m squares the GPU culls.
//! - **interactors** (yellow): every disc an interactor flattens, at most
//!   [`GrassSettings::max_interactor_radius`] (2 m by default), red where the
//!   radius asked for is cut off; and the outline of every footprint, as it
//!   lies on the ground.
//! - **trample windows** (magenta): the square around each camera's focus
//!   where interactors leave trails, 64 m across by default
//!   ([`GrassSettings::trample_cell_size`]). Interactors outside it leave
//!   none.

use std::f32::consts::FRAC_PI_2;

use bevy::app::{App, Plugin, PostUpdate};
use bevy::camera::{Camera3d, Projection};
use bevy::color::Color;
use bevy::color::palettes::css;
use bevy::ecs::prelude::*;
use bevy::ecs::reflect::ReflectResource;
use bevy::ecs::system::SystemParam;
use bevy::gizmos::AppGizmoBuilder;
use bevy::gizmos::config::GizmoConfigGroup;
use bevy::gizmos::gizmos::{GizmoBuffer, Gizmos};
use bevy::math::{Isometry3d, Quat, Vec2, Vec3, Vec4Swizzles};
use bevy::reflect::Reflect;
use bevy::reflect::std_traits::ReflectDefault;
use bevy::transform::components::GlobalTransform;

use crate::api::{
    GrassInteractor, GrassInteractorFootprint, GrassInteractorSpread, NoGrass, interactor_offsets,
};
use crate::fields::{CHUNK_SIZE, FieldTables};
use crate::trample;
use crate::{GrassSettings, GrassSystems};

/// Adds the grass debug gizmos. Toggle what they show with [`GrassDebug`].
///
/// ```no_run
/// # use bevy::prelude::*;
/// # use bevy_grass::prelude::*;
/// # use bevy_grass::GrassDebugPlugin;
/// # let mut app = App::new();
/// app.add_plugins((DefaultPlugins, GrassPlugins, GrassDebugPlugin));
/// ```
#[derive(Debug, Default, Clone, Copy)]
pub struct GrassDebugPlugin;

/// What [`GrassDebugPlugin`] draws. A resource: change it at any time.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq, Reflect)]
#[reflect(Resource, Default, Debug, PartialEq)]
pub struct GrassDebug {
    /// Draw anything at all. Off, nothing below is drawn: bind a key to this.
    pub enabled: bool,
    /// The rectangle of every field.
    pub fields: bool,
    /// The 16 m chunks of every field. A big field has thousands.
    pub chunks: bool,
    /// Every disc of every interactor.
    pub interactors: bool,
    /// Each camera's trample window.
    pub trample_windows: bool,
    /// Most chunks drawn, so a huge field does not drown the frame.
    pub max_chunks: usize,
}

impl Default for GrassDebug {
    fn default() -> Self {
        Self {
            enabled: true,
            fields: true,
            chunks: false,
            interactors: true,
            trample_windows: true,
            max_chunks: 4096,
        }
    }
}

/// The gizmo group the grass debug view draws in, for its line width and
/// on/off switch in `GizmoConfigStore`.
#[derive(Debug, Default, Reflect, GizmoConfigGroup)]
pub struct GrassGizmos;

impl Plugin for GrassDebugPlugin {
    fn build(&self, app: &mut App) {
        app.init_gizmo_group::<GrassGizmos>()
            .init_resource::<GrassDebug>()
            .register_type::<GrassDebug>()
            .add_systems(PostUpdate, draw_debug.after(GrassSystems::Rebuild));
    }
}

/// One thing to draw, in the XZ plane at height `y`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Shape {
    /// A rectangle from its corner `min`, `size.x` along `along` (a unit
    /// vector) and `size.y` along `along.perp()`. Axis-aligned when `along`
    /// is X.
    Rect {
        min: Vec2,
        size: Vec2,
        along: Vec2,
        y: f32,
        color: Color,
    },
    /// A circle.
    Circle {
        center: Vec2,
        radius: f32,
        y: f32,
        color: Color,
    },
    /// A footprint on the ground: a rectangle along `along`, `core` its half
    /// extents before its corners are rounded by `rounding`.
    Footprint {
        center: Vec2,
        along: Vec2,
        core: Vec2,
        rounding: f32,
        y: f32,
        color: Color,
    },
}

/// The rectangle of every field in the tables, as the GPU has it, turned
/// with the field. A hidden field is grey; a field on a heightmap or a mesh
/// also gets a rectangle at its highest ground.
pub(crate) fn field_shapes(tables: &FieldTables) -> Vec<Shape> {
    let mut shapes = Vec::new();
    for slot in tables.slots.iter() {
        let field = tables.fields[slot.first];
        let color = if field.access.x == 0 {
            css::GRAY.into()
        } else {
            css::LIME.into()
        };
        let (min, size, along) = (field.min_corner.xz(), field.extent.xy(), field.x_axis());
        shapes.push(Shape::Rect {
            min,
            size,
            along,
            y: field.min_corner.y,
            color,
        });
        let highest = tables
            .chunks
            .iter()
            .filter(|chunk| chunk.ids.x as usize == slot.first)
            .map(|chunk| chunk.bounds.w)
            .fold(field.min_corner.y, f32::max);
        if highest > field.min_corner.y {
            shapes.push(Shape::Rect {
                min,
                size,
                along,
                y: highest,
                color,
            });
        }
    }
    shapes
}

/// The squares the GPU culls, at most `max`.
pub(crate) fn chunk_shapes(tables: &FieldTables, max: usize) -> Vec<Shape> {
    tables
        .chunks
        .iter()
        .take(max)
        .filter_map(|chunk| {
            let field = tables.fields.get(chunk.ids.x as usize)?;
            Some(Shape::Rect {
                min: field.world_of(Vec2::new(chunk.bounds.x, chunk.bounds.y)),
                size: Vec2::splat(CHUNK_SIZE),
                along: field.x_axis(),
                y: chunk.bounds.z,
                color: css::DODGER_BLUE.into(),
            })
        })
        .collect()
}

/// A disc an interactor flattens: `(position, radius, strength)`. Grass
/// is pushed at most `largest` away (`GrassSettings::max_interactor_radius`), so a bigger radius
/// is drawn cut off, with the radius asked for in red; one that flattens
/// nothing (no radius or strength) is grey.
pub(crate) fn interactor_shapes(
    interactors: impl Iterator<Item = (Vec3, f32, f32)>,
    largest: f32,
) -> Vec<Shape> {
    let mut shapes = Vec::new();
    for (position, radius, strength) in interactors {
        let effective = radius.min(largest);
        let center = Vec2::new(position.x, position.z);
        let color = if radius > 0.0 && strength > 0.0 {
            css::YELLOW.into()
        } else {
            css::GRAY.into()
        };
        shapes.push(Shape::Circle {
            center,
            radius: effective.max(0.0),
            y: position.y,
            color,
        });
        if radius > largest {
            shapes.push(Shape::Circle {
                center,
                radius,
                y: position.y,
                color: css::RED.into(),
            });
        }
    }
    shapes
}

/// The trample window of each camera `(position, forward, orthographic)`:
/// where it really is, by the same functions the renderer uses.
pub(crate) fn trample_shapes(
    cameras: impl Iterator<Item = (Vec3, Vec3, bool)>,
    tables: &FieldTables,
    cell: f32,
) -> Vec<Shape> {
    let ground = tables
        .fields
        .first()
        .map_or(0.0, |field| field.min_corner.y);
    cameras
        .map(|(position, forward, orthographic)| {
            let center = trample::focus(position, forward, orthographic, tables);
            let origin = trample::window_origin(center, cell);
            Shape::Rect {
                min: origin.as_vec2() * cell,
                size: Vec2::splat(trample::window_side(cell)),
                along: Vec2::X,
                y: ground,
                color: css::MAGENTA.into(),
            }
        })
        .collect()
}

/// Draws `shapes`.
pub(crate) fn draw(buffer: &mut GizmoBuffer<GrassGizmos, ()>, shapes: &[Shape]) {
    // A circle is drawn in the XY plane of its isometry; turn it to lie flat.
    let flat = Quat::from_rotation_arc(Vec3::Z, Vec3::Y);
    for shape in shapes {
        match *shape {
            Shape::Rect {
                min,
                size,
                along,
                y,
                color,
            } => {
                let at = |x: f32, z: f32| {
                    let point = min + along * x + along.perp() * z;
                    Vec3::new(point.x, y, point.y)
                };
                buffer.linestrip(
                    [
                        at(0.0, 0.0),
                        at(size.x, 0.0),
                        at(size.x, size.y),
                        at(0.0, size.y),
                        at(0.0, 0.0),
                    ],
                    color,
                );
            }

            Shape::Circle {
                center,
                radius,
                y,
                color,
            } => {
                buffer.circle(
                    Isometry3d::new(Vec3::new(center.x, y, center.y), flat),
                    radius,
                    color,
                );
            }
            Shape::Footprint {
                center,
                along,
                core,
                rounding,
                y,
                color,
            } => {
                // Each corner's quarter circle, anticlockwise from +along.
                let across = along.perp();
                let corners = [(1.0, 1.0), (-1.0, 1.0), (-1.0, -1.0), (1.0, -1.0)];
                let points = corners
                    .into_iter()
                    .enumerate()
                    .flat_map(|(quarter, (x, z))| {
                        let corner = center + along * core.x * x + across * core.y * z;
                        (0..=8).map(move |step| {
                            let angle = (quarter as f32 + step as f32 / 8.0) * FRAC_PI_2;
                            let at =
                                corner + (along * angle.cos() + across * angle.sin()) * rounding;
                            Vec3::new(at.x, y, at.y)
                        })
                    });
                let points: Vec<Vec3> = points.collect();
                buffer.linestrip(points.iter().copied().chain(points.first().copied()), color);
            }
        }
    }
}

/// The outline of every footprint stamp, at height `y`: yellow, or grey if
/// it flattens nothing.
pub(crate) fn footprint_shapes(stamps: impl Iterator<Item = (trample::Stamp, f32)>) -> Vec<Shape> {
    stamps
        .map(|(stamp, y)| Shape::Footprint {
            center: stamp.disc.xy(),
            along: stamp.shape.xy(),
            core: stamp.shape.zw(),
            rounding: stamp.motion.z,
            y,
            color: if stamp.disc.w > 0.0 {
                css::YELLOW.into()
            } else {
                css::GRAY.into()
            },
        })
        .collect()
}

/// Everything the debug view shows this frame.
pub(crate) fn shapes(
    debug: &GrassDebug,
    settings: &GrassSettings,
    tables: &FieldTables,
    interactors: impl Iterator<Item = (Vec3, f32, f32)>,
    cameras: impl Iterator<Item = (Vec3, Vec3, bool)>,
) -> Vec<Shape> {
    let mut shapes = Vec::new();
    if !debug.enabled {
        return shapes;
    }
    if debug.fields {
        shapes.extend(field_shapes(tables));
    }
    if debug.chunks {
        shapes.extend(chunk_shapes(tables, debug.max_chunks));
    }
    if debug.interactors {
        shapes.extend(interactor_shapes(
            interactors,
            settings.max_interactor_radius,
        ));
    }
    if debug.trample_windows {
        shapes.extend(trample_shapes(cameras, tables, settings.trample_cell_size));
    }
    shapes
}

/// The world queries the debug gizmos read.
#[expect(clippy::type_complexity, reason = "Bevy query types are verbose")]
#[derive(SystemParam)]
struct DebugQueries<'w, 's> {
    interactors: Query<
        'w,
        's,
        (
            &'static GlobalTransform,
            &'static GrassInteractor,
            Option<&'static GrassInteractorSpread>,
        ),
        Without<GrassInteractorFootprint>,
    >,
    footprints: Query<
        'w,
        's,
        (
            &'static GlobalTransform,
            &'static GrassInteractor,
            &'static GrassInteractorFootprint,
        ),
    >,
    cameras: Query<
        'w,
        's,
        (&'static GlobalTransform, &'static Projection),
        (With<Camera3d>, Without<NoGrass>),
    >,
}

fn draw_debug(
    mut gizmos: Gizmos<GrassGizmos>,
    debug: Res<GrassDebug>,
    settings: Res<GrassSettings>,
    tables: Res<FieldTables>,
    queries: DebugQueries,
) {
    // The renderer clamps the settings; so do the gizmos, to show what it does.
    let settings = settings.sanitized();
    let mut shapes = shapes(
        &debug,
        &settings,
        &tables,
        queries
            .interactors
            .iter()
            .flat_map(|(transform, interactor, spread)| {
                interactor_offsets(spread).iter().map(move |&offset| {
                    let at = transform.transform_point(offset);
                    (at, interactor.radius, interactor.strength)
                })
            }),
        queries.cameras.iter().map(|(transform, projection)| {
            (
                transform.translation(),
                transform.forward().as_vec3(),
                matches!(projection, Projection::Orthographic(_)),
            )
        }),
    );
    if debug.enabled && debug.interactors {
        shapes.extend(footprint_shapes(queries.footprints.iter().map(
            |(transform, interactor, footprint)| {
                let stamp =
                    trample::box_stamp(transform, footprint, interactor.strength, Vec2::ZERO);
                (stamp, transform.transform_point(footprint.center).y)
            },
        )));
    }
    draw(&mut gizmos, &shapes);
}

#[cfg(test)]
mod tests;
