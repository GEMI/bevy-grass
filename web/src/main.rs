//! The hosted `bevy_grass` demo: a meadow larger than the grass radius, a
//! clearing, a walker trampling a trail, and far grass beyond the blades.
//!
//! It starts at `GrassSettings::ULTRA`, the highest tier, with the
//! `WILDFLOWER_MEADOW` preset. Grass has measured many times dearer in a
//! browser than natively on the same GPU, for reasons not yet known (see
//! `docs/web.md`); the page switches the tier and the preset, and natively
//! 1 to 4 pick the tier and `[` and `]` step through the presets. WASD and
//! the mouse fly the camera (hold the right button, or press M, to look), and
//! the left button paints with a `GrassBrush`. In the browser the page's panel
//! tunes every public setting of grass, through `tune`.
//!
//! ```text
//! cargo run -p bevy_grass_web --release        # natively
//! python tools/web_demo.py serve               # in the browser
//! ```

#[cfg(target_arch = "wasm32")]
mod page;
mod tune;

use bevy::camera_controller::free_camera::{FreeCamera, FreeCameraPlugin};
use bevy::light::CascadeShadowConfigBuilder;
use bevy::pbr::{DistanceFog, FogFalloff};
use bevy::prelude::*;
use bevy::render::view::Msaa;
use bevy_grass::prelude::*;

use tune::{Brush, Tuning};

/// The tiers the page offers, in its order.
const QUALITIES: [(&str, GrassSettings); 4] = [
    ("LOW", GrassSettings::LOW),
    ("MEDIUM", GrassSettings::MEDIUM),
    ("HIGH", GrassSettings::HIGH),
    ("ULTRA", GrassSettings::ULTRA),
];
/// `ULTRA`, where the demo starts.
const START_QUALITY: usize = 3;
const START_PRESET: &str = "WILDFLOWER_MEADOW";

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
        GrassDebugPlugin,
        FreeCameraPlugin,
    ))
    // The sky is the colour of the mist, so the far ground fades into it.
    .insert_resource(ClearColor(SKY))
    .insert_resource(QUALITIES[START_QUALITY].1)
    // The debug view starts hidden; the page turns it on.
    .insert_resource(tune::start_debug())
    .insert_resource(Tuning {
        wind: "BREEZE",
        dirty: true,
        ..default()
    })
    .init_resource::<Requests>()
    .init_resource::<Brush>()
    .add_systems(Startup, setup)
    .add_systems(
        Update,
        (walk, keyboard, apply_requests, tune::apply_tuning, paint).chain(),
    )
    .add_systems(Last, tune::publish_tunables);
    #[cfg(target_arch = "wasm32")]
    app.add_systems(Update, page::take_requests.before(apply_requests))
        .add_systems(Last, page::publish_status.after(tune::publish_tunables));
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
    /// Whether shadows are enabled.
    shadows: Option<bool>,
    /// The number of MSAA samples, 1, 2, 4 or 8.
    antialiasing: Option<u32>,
}

/// What a preset change rewrites, what the page tunes, and what the demo
/// shows now.
#[derive(Resource, Debug)]
struct Meadow {
    style: Handle<GrassStyle>,
    ground: Handle<FarGrassMaterial>,
    /// The field, with its `GrassFieldOverrides` and `GrassRegrowth`.
    field: Entity,
    /// The walker's `GrassInteractor`.
    walker: Entity,
    /// The camera's `GrassCamera`.
    camera: Entity,
    /// An index into `QUALITIES`.
    quality: usize,
    preset: &'static str,
    /// Whether shadows are enabled.
    shadows: bool,
    /// The number of MSAA samples, 1, 2, 4 or 8.
    antialiasing: u32,
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

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut far_materials: ResMut<Assets<FarGrassMaterial>>,
    mut styles: ResMut<Assets<GrassStyle>>,
) {
    let camera = commands
        .spawn((
            Camera3d::default(),
            GrassCamera::default(),
            Msaa::Sample4,
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
        ))
        .id();
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
    // The meadow. The overrides change nothing until the page tunes them;
    // regrowth closes what the brush clears.
    let field = commands
        .spawn((
            GrassField {
                size: Vec2::splat(MEADOW),
                style: style.clone(),
                ..default()
            },
            GrassFieldOverrides::default(),
            GrassRegrowth::default(),
        ))
        .id();
    // A 1.8 m capsule, the height of a person.
    let walker = commands
        .spawn((
            Walker,
            GrassInteractor::default(),
            Mesh3d(meshes.add(Capsule3d::new(0.25, 1.3))),
            MeshMaterial3d(materials.add(Color::srgb(0.15, 0.32, 0.75))),
            Transform::from_xyz(WALK_RADIUS, 0.9, 0.0),
        ))
        .id();
    commands.insert_resource(Meadow {
        style,
        ground,
        field,
        walker,
        camera,
        quality: START_QUALITY,
        preset: name,
        shadows: true,
        antialiasing: 4,
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

#[expect(
    clippy::too_many_arguments,
    reason = "a Bevy system: each request changes its own resource or query"
)]
fn apply_requests(
    mut requests: ResMut<Requests>,
    mut meadow: ResMut<Meadow>,
    mut settings: ResMut<GrassSettings>,
    mut styles: ResMut<Assets<GrassStyle>>,
    mut far_materials: ResMut<Assets<FarGrassMaterial>>,
    mut windows: Query<&mut Window>,
    mut tuning: ResMut<Tuning>,
    mut lights: Query<&mut DirectionalLight>,
    mut cameras: Query<&mut Msaa, With<Camera3d>>,
) {
    if *requests == Requests::default() {
        return;
    }
    let Requests {
        quality,
        preset: name,
        shadows,
        antialiasing,
    } = std::mem::take(&mut *requests);
    if let Some(enabled) = shadows.filter(|enabled| *enabled != meadow.shadows) {
        for mut light in &mut lights {
            light.shadow_maps_enabled = enabled;
        }
        meadow.shadows = enabled;
    }
    if let Some(samples) = antialiasing.filter(|samples| *samples != meadow.antialiasing) {
        let msaa = match samples {
            1 => Msaa::Off,
            2 => Msaa::Sample2,
            8 => Msaa::Sample8,
            // 4, and anything else the page sends.
            _ => Msaa::Sample4,
        };
        for mut msaa_comp in &mut cameras {
            *msaa_comp = msaa;
        }
        meadow.antialiasing = samples;
    }
    if let Some(quality) = quality.filter(|at| *at != meadow.quality) {
        *settings = QUALITIES[quality].1;
        meadow.quality = quality;
        tuning.bump();
    }
    if let Some((name, style)) = name.filter(|name| *name != meadow.preset).and_then(preset) {
        if let Some(mut ground) = far_materials.get_mut(&meadow.ground) {
            ground.extension = far_grass(&style);
        }
        if let Some(mut meadow_style) = styles.get_mut(&meadow.style) {
            *meadow_style = style;
        }
        meadow.preset = name;
        tuning.bump();
    }
    for mut window in &mut windows {
        window.title = format!(
            "bevy_grass: {} at {}",
            meadow.preset, QUALITIES[meadow.quality].0
        );
    }
}

/// Marks the brush entity, there while the left mouse button is held.
#[derive(Component, Debug)]
struct Painting;

/// Paints with the page's brush where the left mouse button holds the
/// cursor over the ground.
fn paint(
    mut commands: Commands,
    buttons: Res<ButtonInput<MouseButton>>,
    brush: Res<Brush>,
    windows: Query<&Window>,
    cameras: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    mut painting: Query<(Entity, &mut Transform, &mut GrassBrush), With<Painting>>,
    mut gizmos: Gizmos,
) {
    let ground = buttons
        .pressed(MouseButton::Left)
        .then(|| {
            let cursor = windows.iter().find_map(Window::cursor_position)?;
            let (camera, transform) = cameras.iter().next()?;
            let ray = camera.viewport_to_world(transform, cursor).ok()?;
            let distance = ray.intersect_plane(Vec3::ZERO, InfinitePlane3d::new(Vec3::Y))?;
            Some(ray.get_point(distance))
        })
        .flatten();
    match (ground, painting.single_mut()) {
        (Some(point), Ok((_, mut transform, mut current))) => {
            transform.translation = point;
            if *current != brush.0 {
                *current = brush.0;
            }
        }
        (Some(point), Err(_)) => {
            commands.spawn((Painting, brush.0, Transform::from_translation(point)));
        }
        (None, Ok((entity, ..))) => commands.entity(entity).despawn(),
        (None, Err(_)) => {}
    }
    if let Some(point) = ground {
        gizmos.circle(
            Isometry3d::new(
                point + Vec3::Y * 0.05,
                Quat::from_rotation_arc(Vec3::Z, Vec3::Y),
            ),
            brush.0.radius,
            Color::WHITE,
        );
    }
}
