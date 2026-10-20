//! Where each preset is shown: the ground it grows on, water, rows, and the
//! wind it looks best in. A bog sits in a bog and marram on sand, instead of
//! every preset on the same brown soil.

use bevy::asset::RenderAssetUsages;
use bevy::light::NotShadowCaster;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy_grass::prelude::*;

/// The place a preset is shown in.
pub(crate) struct Setting {
    /// The ground under the grass, and how rough it is (low for wet ground).
    pub(crate) ground: Color,
    pub(crate) roughness: f32,
    /// Standing water over the ground.
    pub(crate) water: Option<Water>,
    /// Planted in rows this far apart, in metres, through a striped density
    /// mask.
    pub(crate) rows: Option<f32>,
    /// The wind the preset is meant for, when it is not the usual breeze.
    pub(crate) wind: Option<GrassWind>,
}

/// Water standing on the ground: how deep, and its colour (alpha is how much
/// it hides).
pub(crate) struct Water {
    pub(crate) depth: f32,
    pub(crate) color: Color,
}

/// How far a water box reaches below the ground, in metres.
const WATER_BELOW_GROUND: f32 = 0.05;

/// Dry ground of this colour.
const fn soil(red: f32, green: f32, blue: f32) -> Setting {
    Setting {
        ground: Color::srgb(red, green, blue),
        roughness: 0.95,
        water: None,
        rows: None,
        wind: None,
    }
}

/// A slow current for kelp: little push, long slow waves and a wide, slow
/// sway, so the ribbons drift instead of flapping.
pub(crate) const CURRENT: GrassWind = GrassWind {
    strength: 0.45,
    gust_scale: 30.0,
    gust_speed: 1.2,
    sway: 3.0,
    sway_speed: 0.3,
    flutter: 0.0,
    gust_contrast: 1.5,
    ..GrassWind::BREEZE
};

/// Where the preset called `name` is shown.
pub(crate) fn setting(name: &str) -> Setting {
    match name {
        "TALL_PRAIRIE" | "SUMMER_TALL" => soil(0.17, 0.13, 0.08),
        "WHEAT" | "RYE_AND_CORNFLOWERS" => soil(0.34, 0.27, 0.17),
        "DRY_SAVANNA" => soil(0.50, 0.35, 0.21),
        "LAWN" => soil(0.10, 0.11, 0.06),
        "STYLIZED" => soil(0.24, 0.36, 0.17),
        "AUTUMN_TALL" | "AUTUMN_WILD" => soil(0.24, 0.17, 0.10),
        "FOREST_MOSS" | "FOREST_FERNS" | "FOREST_FLOOR" | "FOREST_BRACKEN" => {
            soil(0.09, 0.07, 0.04)
        }
        // Dark peat, wet enough to shine where the pools are.
        "COTTON_BOG" => Setting {
            roughness: 0.12,
            ..soil(0.11, 0.07, 0.05)
        },
        "REED_MARSH" => Setting {
            water: Some(Water {
                depth: 0.3,
                color: Color::srgba(0.10, 0.17, 0.15, 0.82),
            }),
            ..soil(0.14, 0.12, 0.08)
        },
        "DUNE_MARRAM" => soil(0.80, 0.72, 0.55),
        "PAMPAS" => soil(0.30, 0.24, 0.15),
        "SPINIFEX" => soil(0.62, 0.30, 0.15),
        "ELEPHANT_GRASS" => soil(0.13, 0.09, 0.05),
        "FROSTED_MEADOW" => Setting {
            roughness: 0.6,
            ..soil(0.92, 0.94, 0.97)
        },
        "BURNT_REGROWTH" => soil(0.14, 0.13, 0.12),
        "LAVENDER" => Setting {
            rows: Some(1.4),
            ..soil(0.52, 0.42, 0.30)
        },
        "CRYSTAL_SPIRES" => Setting {
            roughness: 0.4,
            ..soil(0.08, 0.08, 0.11)
        },
        "CORKSCREW_ALIEN" => soil(0.12, 0.06, 0.16),
        // The seabed, under water deeper than the tallest kelp.
        "KELP_BED" => Setting {
            water: Some(Water {
                depth: 3.3,
                color: Color::srgba(0.03, 0.20, 0.30, 0.3),
            }),
            wind: Some(CURRENT),
            ..soil(0.40, 0.40, 0.32)
        },
        "BLIGHTED" => soil(0.10, 0.06, 0.11),
        "CEL_FIELD" => soil(0.30, 0.48, 0.20),
        "COZY_PASTEL" => soil(0.55, 0.72, 0.50),
        _ => soil(0.20, 0.15, 0.09),
    }
}

/// Spawns the ground (and water) of a patch of `size` metres centred on
/// `center`, and returns the density mask its field should use, if any.
/// The ground is a square at `center`'s height, so the backdrop around it
/// should sit a little lower.
pub(crate) fn spawn_setting(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    images: &mut Assets<Image>,
    setting: &Setting,
    center: Vec3,
    size: Vec2,
) -> Option<Handle<Image>> {
    commands.spawn((
        Mesh3d(meshes.add(Plane3d::default().mesh().size(size.x, size.y))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: setting.ground,
            perceptual_roughness: setting.roughness,
            ..default()
        })),
        Transform::from_translation(center),
    ));
    if let Some(water) = &setting.water {
        // The box reaches a little below the ground. With its floor on the
        // ground the two z-fought as the camera moved, and the blade shadows
        // on the ground blinked as the floor hid them in some frames only.
        let height = water.depth + WATER_BELOW_GROUND;
        commands.spawn((
            Mesh3d(meshes.add(Cuboid::new(size.x, height, size.y))),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: water.color,
                alpha_mode: AlphaMode::Blend,
                perceptual_roughness: 0.05,
                reflectance: 0.6,
                // Seen from inside too, so diving in stays under water.
                double_sided: true,
                cull_mode: None,
                ..default()
            })),
            Transform::from_translation(center + Vec3::Y * (water.depth - height * 0.5)),
            NotShadowCaster,
        ));
    }
    setting.rows.map(|pitch| images.add(rows_mask(size, pitch)))
}

/// A density mask of rows running along X, `pitch` metres apart, for a field
/// of `size`: about half of each pitch planted, with soft edges.
pub(crate) fn rows_mask(size: Vec2, pitch: f32) -> Image {
    const SIDE: u32 = 256;
    let mut data = Vec::with_capacity((SIDE * SIDE * 4) as usize);
    for y in 0..SIDE {
        // Distance from the middle of the nearest row, as a share of the
        // pitch: 0 on the row, 0.5 halfway between rows.
        let z = (y as f32 + 0.5) / SIDE as f32 * size.y / pitch;
        let off_row = (z.fract() - 0.5).abs();
        let density = 1.0 - ((off_row - 0.22) / 0.08).clamp(0.0, 1.0);
        for _ in 0..SIDE {
            data.extend_from_slice(&[(density * 255.0) as u8, 255, 0, 255]);
        }
    }
    Image::new(
        Extent3d {
            width: SIDE,
            height: SIDE,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8Unorm,
        RenderAssetUsages::MAIN_WORLD,
    )
}
