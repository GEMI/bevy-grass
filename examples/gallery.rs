//! The gallery: the biome, fantasy and toon presets, each on a plot of its
//! own ground (sand, snow, peat, a pond, the seabed), to fly around and look
//! at up close. Every plot loads its style from `assets/grass_styles`, so
//! editing a file and saving it changes the plot while it runs. A person
//! walks a circle in each plot, trampling a trail.
//!
//! Tab and Shift+Tab fly to the next and previous plot; G switches between
//! a view over the plot and eye level inside it. Hold the right mouse button
//! to look; WASD fly, E and Q rise and sink, Shift goes faster. 1–6 pick a
//! wind (calm, breeze, gusty, storm, steady, lively), 0 gives each plot its
//! own (a slow current over the kelp). H turns grass shadows off and on, P
//! stops and starts the walkers.
//!
//! ```text
//! cargo run --release --example gallery
//! cargo run --release --example gallery -- --plot KELP_BED
//! cargo run --release --example gallery -- --plot PAMPAS --fixed-time --screenshot shots/gallery_pampas.png
//! ```

mod common;

use bevy::camera_controller::free_camera::{FreeCamera, FreeCameraPlugin, FreeCameraState};
use bevy::light::CascadeShadowConfigBuilder;
use bevy::pbr::{DistanceFog, FogFalloff};
use bevy::prelude::*;
use bevy_grass::prelude::*;
use common::setting::{setting, spawn_setting};

/// The plots, near to far and left to right, four to a row: the preset and
/// one line on what to look for.
const PLOTS: [(&str, &str); 16] = [
    (
        "COTTON_BOG",
        "Sedge tussocks on red sphagnum, cotton grass, dark pools",
    ),
    (
        "REED_MARSH",
        "Reeds over your head in standing water, with cattails",
    ),
    ("DUNE_MARRAM", "Blue-grey marram in clumps on open sand"),
    (
        "RYE_AND_CORNFLOWERS",
        "Ripening rye with blue cornflowers and red poppies",
    ),
    (
        "PAMPAS",
        "Fountains of arching leaves under tall cream plumes",
    ),
    ("SPINIFEX", "Stiff, needle-thin hummocks on bare red earth"),
    (
        "ELEPHANT_GRASS",
        "A translucent wall twice as tall as a person",
    ),
    (
        "FROSTED_MEADOW",
        "Stiff, glinting frosted blades growing out of snow",
    ),
    (
        "BURNT_REGROWTH",
        "Charcoal stubble in ash, with new green shoots",
    ),
    (
        "LAVENDER",
        "Lavender bushes planted in rows by a density mask",
    ),
    (
        "CRYSTAL_SPIRES",
        "Faceted, glassy crystal clusters that barely sway",
    ),
    (
        "CORKSCREW_ALIEN",
        "Ribbons twisted two full turns, two-band toon light",
    ),
    (
        "KELP_BED",
        "Kelp ribbons and seagrass on the seabed, in a slow current",
    ),
    (
        "BLIGHTED",
        "Corrupted violet grass, sickly patches, bone-white heads",
    ),
    (
        "CEL_FIELD",
        "Cel-shaded chunky tufts with red and white flowers",
    ),
    (
        "COZY_PASTEL",
        "Soft pastel mint grass and flowers, four toon bands",
    ),
];

/// Side of each plot, the path between plots, and plots per row, in metres.
const PLOT: f32 = 18.0;
const PATH: f32 = 6.0;
const COLUMNS: usize = 4;

/// The centre of plot `index`, on the ground.
fn plot_center(index: usize) -> Vec3 {
    let column = (index % COLUMNS) as f32;
    let row = (index / COLUMNS) as f32;
    Vec3::new(
        (column - (COLUMNS as f32 - 1.0) * 0.5) * (PLOT + PATH),
        0.0,
        -row * (PLOT + PATH),
    )
}

/// Where the camera goes to look at plot `index`: from the near side, over
/// it or at eye level just inside its edge.
fn vantage(index: usize, eye_level: bool) -> Transform {
    let center = plot_center(index);
    if eye_level {
        Transform::from_translation(center + Vec3::new(-2.0, 1.6, PLOT * 0.5 - 2.0))
            .looking_at(center + Vec3::new(0.0, 1.0, 0.0), Vec3::Y)
    } else {
        Transform::from_translation(center + Vec3::new(-4.0, 7.0, PLOT * 0.5 + 9.0))
            .looking_at(center + Vec3::new(0.0, 0.5, 0.0), Vec3::Y)
    }
}

const WINDS: [(&str, GrassWind); 6] = [
    ("calm", GrassWind::CALM),
    ("breeze", GrassWind::BREEZE),
    ("gusty", GrassWind::GUSTY),
    ("storm", GrassWind::STORM),
    (
        "steady",
        GrassWind {
            sway: 0.0,
            gust_contrast: 0.0,
            ..GrassWind::BREEZE
        },
    ),
    (
        "lively",
        GrassWind {
            sway: 2.5,
            sway_speed: 2.0,
            gust_contrast: 2.5,
            ..GrassWind::BREEZE
        },
    ),
];

/// Where the tour is: the plot last flown to, the view, and the wind picked
/// with a number key (`None`: each plot's own).
#[derive(Resource)]
struct Tour {
    plot: usize,
    eye_level: bool,
    wind: Option<usize>,
    walking: bool,
}

/// A person walking a circle in a plot.
#[derive(Component)]
struct Walker {
    center: Vec3,
    radius: f32,
    phase: f32,
}

/// A plot's name, shown over its near edge.
#[derive(Component)]
struct PlotLabel {
    anchor: Vec3,
}

/// The line at the top of the screen.
#[derive(Component)]
struct Hud;

fn main() {
    let mut app = App::new();
    let args = common::ExampleArgs::parse();
    let start = args.value("--plot").map_or(0, |name| {
        PLOTS
            .iter()
            .position(|(plot, _)| plot.eq_ignore_ascii_case(name))
            .unwrap_or_else(|| {
                let names: Vec<_> = PLOTS.iter().map(|(plot, _)| *plot).collect();
                panic!("--plot takes one of {}", names.join(", "))
            })
    });
    args.install(&mut app, "bevy_grass: gallery");
    app.insert_resource(Tour {
        plot: start,
        eye_level: false,
        wind: None,
        walking: true,
    })
    .add_plugins((GrassPlugins, FreeCameraPlugin))
    .insert_resource(ClearColor(Color::srgb(0.62, 0.72, 0.82)))
    .add_systems(Startup, setup)
    .add_systems(
        Update,
        (
            fly_to_plot,
            pick_wind,
            toggle_shadows,
            walk,
            (follow_wind, place_labels, update_hud).after(fly_to_plot),
        ),
    )
    .run();
}

fn setup(
    mut commands: Commands,
    tour: Res<Tour>,
    assets: Res<AssetServer>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
) {
    commands.spawn((
        Camera3d::default(),
        vantage(tour.plot, tour.eye_level),
        FreeCamera::default(),
        DistanceFog {
            color: Color::srgb(0.62, 0.72, 0.82),
            falloff: FogFalloff::Linear {
                start: 60.0,
                end: 200.0,
            },
            ..default()
        },
    ));
    commands.spawn((
        DirectionalLight {
            shadow_maps_enabled: true,
            illuminance: 12_000.0,
            ..default()
        },
        Transform::from_rotation(Quat::from_euler(EulerRot::YXZ, -0.7, -0.55, 0.0)),
        CascadeShadowConfigBuilder {
            num_cascades: 2,
            first_cascade_far_bound: 20.0,
            maximum_distance: 80.0,
            ..default()
        }
        .build(),
    ));
    // Gravel paths between the plots, a little below the plots' own ground.
    commands.spawn((
        Mesh3d(meshes.add(Plane3d::default().mesh().size(600.0, 600.0))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.36, 0.34, 0.31),
            perceptual_roughness: 0.95,
            ..default()
        })),
        Transform::from_xyz(0.0, -0.02, -40.0),
    ));

    let person = materials.add(Color::srgb(0.15, 0.32, 0.75));
    let body = meshes.add(Capsule3d::new(0.25, 1.3));
    for (index, (name, _)) in PLOTS.iter().enumerate() {
        let center = plot_center(index);
        let density_map = spawn_setting(
            &mut commands,
            &mut meshes,
            &mut materials,
            &mut images,
            &setting(name),
            center,
            Vec2::splat(PLOT),
        );
        commands.spawn((
            GrassField {
                size: Vec2::splat(PLOT),
                style: assets.load(format!(
                    "grass_styles/{}.grass.ron",
                    name.to_ascii_lowercase()
                )),
                density_map,
                ..default()
            },
            Transform::from_translation(center),
        ));
        // A 1.8 m person, for scale, trampling a trail.
        commands.spawn((
            Mesh3d(body.clone()),
            MeshMaterial3d(person.clone()),
            Transform::from_translation(center + Vec3::Y * 0.9),
            GrassInteractor::default(),
            Walker {
                center,
                radius: 4.5,
                phase: index as f32 * 0.7,
            },
        ));
        spawn_label(
            &mut commands,
            name,
            center + Vec3::new(0.0, 0.1, PLOT * 0.5),
        );
    }
    spawn_hud(&mut commands);
}

/// A plot's name, placed over `anchor` by `place_labels`.
fn spawn_label(commands: &mut Commands, name: &str, anchor: Vec3) {
    commands
        .spawn((
            PlotLabel { anchor },
            Node {
                position_type: PositionType::Absolute,
                display: Display::None,
                ..default()
            },
        ))
        .with_child((
            Text::new(name),
            TextFont {
                font_size: 22.0.into(),
                ..default()
            },
            TextColor(Color::srgb(0.97, 0.95, 0.88)),
            Node {
                padding: UiRect::axes(Val::Px(10.0), Val::Px(4.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.05, 0.04, 0.03, 0.72)),
        ));
}

/// The panel at the top left: the plot looked at, and the keys.
fn spawn_hud(commands: &mut Commands) {
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px(12.0),
                left: Val::Px(12.0),
                padding: UiRect::axes(Val::Px(14.0), Val::Px(8.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.05, 0.04, 0.03, 0.72)),
        ))
        .with_child((
            Hud,
            Text::default(),
            TextFont {
                font_size: 18.0.into(),
                ..default()
            },
            TextColor(Color::srgb(0.97, 0.95, 0.88)),
        ));
}

/// Tab and Shift+Tab fly to the next and previous plot; G switches between
/// the view over a plot and eye level.
fn fly_to_plot(
    keys: Res<ButtonInput<KeyCode>>,
    mut tour: ResMut<Tour>,
    camera: Single<(&mut Transform, &mut FreeCameraState)>,
) {
    let shift = keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]);
    let (mut transform, mut state) = camera.into_inner();
    let mut moved = false;
    if keys.just_pressed(KeyCode::Tab) {
        // From the plot being looked at, wherever the camera has flown.
        tour.plot = looked_at(&transform);
        tour.plot = if shift {
            (tour.plot + PLOTS.len() - 1) % PLOTS.len()
        } else {
            (tour.plot + 1) % PLOTS.len()
        };
        moved = true;
    }
    if keys.just_pressed(KeyCode::KeyG) {
        tour.plot = looked_at(&transform);
        tour.eye_level = !tour.eye_level;
        moved = true;
    }
    if moved {
        *transform = vantage(tour.plot, tour.eye_level);
        // The controller keeps its own yaw and pitch; turn it to match.
        let (yaw, pitch, _) = transform.rotation.to_euler(EulerRot::YXZ);
        state.yaw = yaw;
        state.pitch = pitch;
        state.velocity = Vec3::ZERO;
    }
}

/// The plot the camera looks at: where its view meets the ground, or, looking
/// up or far away, the plot nearest to the camera.
fn looked_at(camera: &Transform) -> usize {
    let forward = camera.forward();
    let ahead = -camera.translation.y / forward.y.min(-1e-3);
    let focus = if (0.0..60.0).contains(&ahead) {
        camera.translation + forward * ahead
    } else {
        camera.translation
    };
    nearest_plot(focus)
}

/// The plot nearest to `at`.
fn nearest_plot(at: Vec3) -> usize {
    (0..PLOTS.len())
        .min_by(|a, b| {
            let a = plot_center(*a).xz().distance_squared(at.xz());
            let b = plot_center(*b).xz().distance_squared(at.xz());
            a.total_cmp(&b)
        })
        .unwrap_or(0)
}

fn pick_wind(keys: Res<ButtonInput<KeyCode>>, mut tour: ResMut<Tour>) {
    const KEYS: [KeyCode; 6] = [
        KeyCode::Digit1,
        KeyCode::Digit2,
        KeyCode::Digit3,
        KeyCode::Digit4,
        KeyCode::Digit5,
        KeyCode::Digit6,
    ];
    if let Some(index) = KEYS.iter().position(|key| keys.just_pressed(*key)) {
        tour.wind = Some(index);
    }
    if keys.just_pressed(KeyCode::Digit0) {
        tour.wind = None;
    }
    if keys.just_pressed(KeyCode::KeyP) {
        tour.walking = !tour.walking;
    }
}

/// The wind picked, or the one the plot under the camera is meant for.
fn follow_wind(
    tour: Res<Tour>,
    camera: Single<&Transform, With<Camera3d>>,
    mut wind: ResMut<GrassWind>,
) {
    let wanted = match tour.wind {
        Some(index) => WINDS[index].1,
        None => setting(PLOTS[looked_at(&camera)].0)
            .wind
            .unwrap_or(GrassWind::BREEZE),
    };
    wind.set_if_neq(wanted);
}

fn toggle_shadows(keys: Res<ButtonInput<KeyCode>>, mut grass: ResMut<GrassSettings>) {
    if keys.just_pressed(KeyCode::KeyH) {
        grass.shadows = if grass.shadows == GrassShadows::Off {
            GrassShadows::Near
        } else {
            GrassShadows::Off
        };
    }
}

fn walk(
    time: Res<Time>,
    tour: Res<Tour>,
    mut clock: Local<f32>,
    mut walkers: Query<(&Walker, &mut Transform)>,
) {
    if tour.walking {
        *clock += time.delta_secs();
    }
    for (walker, mut transform) in &mut walkers {
        // About walking pace: 1.3 m/s around the circle.
        let angle = *clock * 1.3 / walker.radius + walker.phase;
        transform.translation = walker.center
            + Vec3::new(angle.cos(), 0.0, angle.sin()) * walker.radius
            + Vec3::Y * 0.9;
    }
}

/// Puts each plot's name over its near edge, while it is in front of the
/// camera and not too far away.
fn place_labels(
    camera: Single<(&Camera, &GlobalTransform)>,
    mut labels: Query<(&PlotLabel, &mut Node)>,
) {
    let (camera, transform) = *camera;
    for (label, mut node) in &mut labels {
        let near = transform.translation().distance(label.anchor) < 70.0;
        match camera.world_to_viewport(transform, label.anchor) {
            Ok(at) if near => {
                node.display = Display::Flex;
                node.left = Val::Px(at.x - 60.0);
                node.top = Val::Px(at.y);
            }
            _ => node.display = Display::None,
        }
    }
}

fn update_hud(
    tour: Res<Tour>,
    grass: Res<GrassSettings>,
    camera: Single<&Transform, With<Camera3d>>,
    mut hud: Single<&mut Text, With<Hud>>,
) {
    let plot = looked_at(&camera);
    let (name, about) = PLOTS[plot];
    let wind = match tour.wind {
        Some(index) => WINDS[index].0,
        None if setting(name).wind.is_some() => "the plot's own",
        None => "breeze",
    };
    let text = format!(
        "{name} ({}/{}): {about}\n\
         wind: {wind}   shadows: {:?}   walkers: {}\n\
         Tab / Shift+Tab: next / previous plot   G: eye level / over the plot\n\
         right mouse: look   WASD, E, Q: fly   Shift: faster   \
         1-6: wind (0: the plot's own)   H: shadows   P: walkers",
        plot + 1,
        PLOTS.len(),
        grass.shadows,
        if tour.walking { "on" } else { "off" },
    );
    if hud.0 != text {
        hud.0 = text;
    }
}
