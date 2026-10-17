//! A perspective meadow with a free-flying camera: a field of `MEADOW` larger
//! than the grass radius, with a round clearing cut by a density mask, and a
//! patch of `TALL_PRAIRIE`. A low sun makes the grass shadows on the
//! clearing's bare ground easy to see.
//!
//! The ground is a `FarGrassMaterial`: beyond the grass radius, where no
//! blades are drawn, it takes on the meadow's colour and its gusts, so there
//! is no seam where the blades end. `--no-far` uses a plain ground to compare
//! with, and `--no-fog` clears the mist that would otherwise hide the seam.
//!
//! WASD and the mouse fly the camera (hold the left button to look).
//!
//! ```text
//! cargo run --example meadow
//! cargo run --example meadow -- --fixed-time --screenshot shots/meadow.png
//! cargo run --example meadow -- --no-fog --no-far
//! ```

mod common;

use bevy::asset::RenderAssetUsages;
use bevy::camera_controller::free_camera::{FreeCamera, FreeCameraPlugin};
use bevy::light::CascadeShadowConfigBuilder;
use bevy::pbr::{DistanceFog, FogFalloff};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy_grass::prelude::*;

fn main() {
    let mut app = App::new();
    common::ExampleArgs::parse().install(&mut app, "bevy_grass: meadow");
    app.add_plugins((GrassPlugin::default(), FreeCameraPlugin, GrassDebugPlugin))
        // F3 shows what grass thinks: fields, interactors, trample windows.
        .insert_resource(GrassDebug {
            enabled: false,
            ..default()
        })
        .add_systems(Startup, setup)
        .add_systems(Update, toggle_debug)
        .run();
}

fn toggle_debug(keys: Res<ButtonInput<KeyCode>>, mut debug: ResMut<GrassDebug>) {
    if keys.just_pressed(KeyCode::F3) {
        debug.enabled = !debug.enabled;
    }
}

/// Side of the meadow, in metres: beyond the High tier's 100 m radius.
const MEADOW: f32 = 400.0;

/// A mask with full density everywhere except a soft-edged round clearing.
fn clearing_mask(size: u32, center: Vec2, radius: f32) -> Image {
    let mut data = Vec::with_capacity((size * size * 4) as usize);
    for y in 0..size {
        for x in 0..size {
            let uv = (Vec2::new(x as f32, y as f32) + 0.5) / size as f32;
            // A soft edge about 3 m wide.
            let density = ((uv.distance(center) - radius) / (3.2 / MEADOW)).clamp(0.0, 1.0);
            data.extend_from_slice(&[(density * 255.0) as u8, 255, 0, 255]);
        }
    }
    Image::new(
        Extent3d {
            width: size,
            height: size,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8Unorm,
        RenderAssetUsages::MAIN_WORLD,
    )
}

#[allow(clippy::too_many_arguments, clippy::needless_pass_by_value)]
fn setup(
    mut commands: Commands,
    args: Res<common::ExampleArgs>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut far_materials: ResMut<Assets<FarGrassMaterial>>,
    mut styles: ResMut<Assets<GrassStyle>>,
    mut images: ResMut<Assets<Image>>,
) {
    let mut camera = commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(-6.0, 3.0, 14.0).looking_at(Vec3::new(2.0, 0.0, 0.0), Vec3::Y),
        FreeCamera::default(),
    ));
    if !args.has("--no-fog") {
        camera.insert(DistanceFog {
            color: Color::srgb(0.62, 0.70, 0.78),
            falloff: FogFalloff::Linear {
                start: 40.0,
                end: 220.0,
            },
            ..default()
        });
    }

    commands.spawn((
        DirectionalLight {
            shadow_maps_enabled: true,
            ..default()
        },
        Transform::from_rotation(Quat::from_euler(EulerRot::YXZ, -0.6, -0.4, 0.0)),
        CascadeShadowConfigBuilder {
            num_cascades: 2,
            maximum_distance: 60.0,
            first_cascade_far_bound: 15.0,
            ..default()
        }
        .build(),
    ));

    let soil = StandardMaterial {
        base_color: Color::srgb(0.20, 0.15, 0.09),
        perceptual_roughness: 0.95,
        ..default()
    };
    let ground = Mesh3d(meshes.add(Plane3d::default().mesh().size(2000.0, 2000.0)));
    if args.has("--no-far") {
        commands.spawn((ground, MeshMaterial3d(materials.add(soil))));
    } else {
        let half = MEADOW * 0.5;
        commands.spawn((
            ground,
            MeshMaterial3d(
                far_materials.add(FarGrassMaterial {
                    base: soil,
                    extension: FarGrass::new(&GrassStyle::MEADOW, GrassQuality::High)
                        .with_area(Rect::new(-half, -half, half, half)),
                }),
            ),
        ));
    }
    let _ = common::spawn_person(
        &mut commands,
        &mut meshes,
        &mut materials,
        Vec3::new(2.0, 0.0, 2.0),
    );

    // The meadow, with a clearing 12 m across at its centre.
    commands.spawn(GrassField {
        size: Vec2::splat(MEADOW),
        style: styles.add(GrassStyle::MEADOW),
        density: Some(images.add(clearing_mask(1024, Vec2::splat(0.5), 6.0 / MEADOW))),
        ..default()
    });
    // A patch of tall prairie beyond the clearing.
    commands.spawn((
        GrassField {
            size: Vec2::new(24.0, 16.0),
            style: styles.add(GrassStyle::TALL_PRAIRIE),
            ..default()
        },
        Transform::from_xyz(4.0, 0.0, -16.0),
    ));
}
