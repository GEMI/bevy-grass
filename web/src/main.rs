//! The hosted `bevy_grass` demo: a meadow larger than the grass radius, a
//! clearing, a walker trampling a trail, and far grass beyond the blades.
//!
//! It starts at `GrassSettings::MEDIUM`, not the native default `HIGH`:
//! grass has measured many times dearer in a browser than natively on the
//! same GPU, for reasons not yet known (see `webgpu-plan.md`). The page switches the tier and the preset; natively, 1
//! to 4 pick the tier and `[` and `]` step through the presets. WASD and the
//! mouse fly the camera (hold the right button, or press M, to look).
//!
//! ```text
//! cargo run -p bevy_grass_web --release        # natively
//! python tools/web_demo.py serve               # in the browser
//! ```

#[cfg(target_arch = "wasm32")]
mod page;

use bevy::asset::RenderAssetUsages;
use bevy::camera_controller::free_camera::{FreeCamera, FreeCameraPlugin};
use bevy::light::CascadeShadowConfigBuilder;
use bevy::pbr::{DistanceFog, FogFalloff};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy_grass::prelude::*;

/// The tiers the page offers, in its order.
const QUALITIES: [(&str, GrassSettings); 4] = [
    ("LOW", GrassSettings::LOW),
    ("MEDIUM", GrassSettings::MEDIUM),
    ("HIGH", GrassSettings::HIGH),
    ("ULTRA", GrassSettings::ULTRA),
];
/// `MEDIUM`, where the demo starts.
const START_QUALITY: usize = 1;
const START_PRESET: &str = "MEADOW";

/// Side of the meadow, in metres: wider than Ultra's 300 m circle of blades,
/// so the blades end inside the field and far grass carries on beyond them.
const MEADOW: f32 = 400.0;
/// The walker's circle around the clearing, in metres, and its pace in
/// metres a second.
const WALK_RADIUS: f32 = 10.0;
const WALK_SPEED: f32 = 1.3;
/// The mist, and the sky behind it.
const SKY: Color = Color::srgb(0.62, 0.70, 0.78);

fn main() {
    let web = cfg!(target_arch = "wasm32");
    let mut app = App::new();
    app.add_plugins((
        DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "bevy_grass".into(),
                // In the browser, the canvas `index.html` lays out.
                canvas: web.then(|| "#grass".into()),
                fit_canvas_to_parent: web,
                ..default()
            }),
            ..default()
        }),
        GrassPlugins,
        FreeCameraPlugin,
    ))
    // The sky is the colour of the mist, so the far ground fades into it.
    .insert_resource(ClearColor(SKY))
    .insert_resource(QUALITIES[START_QUALITY].1)
    .init_resource::<Requests>()
    .add_systems(Startup, setup)
    .add_systems(Update, (walk, keyboard, apply_requests).chain());
    #[cfg(target_arch = "wasm32")]
    app.add_systems(Update, page::take_requests.before(apply_requests))
        .add_systems(Last, page::publish_status);
    app.run();
}

/// A tier or preset asked for since the last frame, by the keyboard or, in a
/// browser, by the page.
#[derive(Resource, Debug, Clone, Copy, Default, PartialEq)]
struct Requests {
    /// An index into `QUALITIES`.
    quality: Option<usize>,
    /// A preset's name, as `GrassStyle::presets` gives it.
    preset: Option<&'static str>,
}

/// What a preset change rewrites, and what the demo shows now.
#[derive(Resource, Debug)]
struct Meadow {
    style: Handle<GrassStyle>,
    ground: Handle<FarGrassMaterial>,
    /// An index into `QUALITIES`.
    quality: usize,
    preset: &'static str,
}

/// The preset called `name`, with the name as a `'static` string.
fn preset(name: &str) -> Option<(&'static str, GrassStyle)> {
    GrassStyle::presets().find(|(preset, _)| *preset == name)
}

/// The far grass of the meadow's ground, in the colours of `style`.
fn far_grass(style: &GrassStyle) -> FarGrass {
    let half = MEADOW * 0.5;
    FarGrass::new(style).with_area(Rect::new(-half, -half, half, half))
}

/// Walks a person around the clearing, through the grass.
#[derive(Component, Debug)]
struct Walker;

/// A mask with full density everywhere except a soft-edged round clearing
/// at the centre.
fn clearing_mask(size: u32, radius: f32) -> Image {
    let mut data = Vec::with_capacity((size * size * 4) as usize);
    for y in 0..size {
        for x in 0..size {
            let uv = (Vec2::new(x as f32, y as f32) + 0.5) / size as f32;
            // A soft edge about 3 m wide.
            let density =
                ((uv.distance(Vec2::splat(0.5)) - radius) / (3.2 / MEADOW)).clamp(0.0, 1.0);
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

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut far_materials: ResMut<Assets<FarGrassMaterial>>,
    mut styles: ResMut<Assets<GrassStyle>>,
    mut images: ResMut<Assets<Image>>,
) {
    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(-6.0, 3.0, 14.0).looking_at(Vec3::new(2.0, 0.0, 0.0), Vec3::Y),
        FreeCamera::default(),
        DistanceFog {
            color: SKY,
            falloff: FogFalloff::Linear {
                start: 40.0,
                end: 220.0,
            },
            ..default()
        },
    ));
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

    let (name, style) = preset(START_PRESET).expect("START_PRESET names a preset");
    let ground = far_materials.add(FarGrassMaterial {
        base: StandardMaterial {
            base_color: Color::srgb(0.20, 0.15, 0.09),
            perceptual_roughness: 0.95,
            ..default()
        },
        extension: far_grass(&style),
    });
    commands.spawn((
        Mesh3d(meshes.add(Plane3d::default().mesh().size(2000.0, 2000.0))),
        MeshMaterial3d(ground.clone()),
    ));
    let style = styles.add(style);
    // The meadow, with a clearing 12 m across at its centre.
    commands.spawn(GrassField {
        size: Vec2::splat(MEADOW),
        style: style.clone(),
        density_map: Some(images.add(clearing_mask(1024, 6.0 / MEADOW))),
        ..default()
    });
    // A 1.8 m capsule, the height of a person.
    commands.spawn((
        Walker,
        GrassInteractor::default(),
        Mesh3d(meshes.add(Capsule3d::new(0.25, 1.3))),
        MeshMaterial3d(materials.add(Color::srgb(0.15, 0.32, 0.75))),
        Transform::from_xyz(WALK_RADIUS, 0.9, 0.0),
    ));
    commands.insert_resource(Meadow {
        style,
        ground,
        quality: START_QUALITY,
        preset: name,
    });
}

fn walk(time: Res<Time>, mut walkers: Query<&mut Transform, With<Walker>>) {
    let angle = time.elapsed_secs() * WALK_SPEED / WALK_RADIUS;
    for mut transform in &mut walkers {
        transform.translation.x = angle.cos() * WALK_RADIUS;
        transform.translation.z = angle.sin() * WALK_RADIUS;
    }
}

/// 1 to 4 pick the tier, `[` and `]` step through the presets.
fn keyboard(keys: Res<ButtonInput<KeyCode>>, meadow: Res<Meadow>, mut requests: ResMut<Requests>) {
    let tiers = [
        KeyCode::Digit1,
        KeyCode::Digit2,
        KeyCode::Digit3,
        KeyCode::Digit4,
    ];
    if let Some(at) = tiers.iter().position(|key| keys.just_pressed(*key)) {
        requests.quality = Some(at);
    }
    let forward = keys.just_pressed(KeyCode::BracketRight);
    if !forward && !keys.just_pressed(KeyCode::BracketLeft) {
        return;
    }
    let names: Vec<&str> = GrassStyle::presets().map(|(name, _)| name).collect();
    let at = names
        .iter()
        .position(|name| *name == meadow.preset)
        .unwrap_or(0);
    // A step back is a step forward by one less than the whole list.
    let step = if forward { 1 } else { names.len() - 1 };
    requests.preset = Some(names[(at + step) % names.len()]);
}

fn apply_requests(
    mut requests: ResMut<Requests>,
    mut meadow: ResMut<Meadow>,
    mut settings: ResMut<GrassSettings>,
    mut styles: ResMut<Assets<GrassStyle>>,
    mut far_materials: ResMut<Assets<FarGrassMaterial>>,
    mut windows: Query<&mut Window>,
) {
    if *requests == Requests::default() {
        return;
    }
    let Requests {
        quality,
        preset: name,
    } = std::mem::take(&mut *requests);
    if let Some(quality) = quality.filter(|at| *at != meadow.quality) {
        *settings = QUALITIES[quality].1;
        meadow.quality = quality;
    }
    if let Some((name, style)) = name.filter(|name| *name != meadow.preset).and_then(preset) {
        if let Some(mut ground) = far_materials.get_mut(&meadow.ground) {
            ground.extension = far_grass(&style);
        }
        if let Some(mut meadow_style) = styles.get_mut(&meadow.style) {
            *meadow_style = style;
        }
        meadow.preset = name;
    }
    for mut window in &mut windows {
        window.title = format!(
            "bevy_grass: {} at {}",
            meadow.preset, QUALITIES[meadow.quality].0
        );
    }
}
