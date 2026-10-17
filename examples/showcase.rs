//! Twelve presets in small square patches, side by side, under an ARPG camera
//! (orthographic, 30° below the horizon, a 45° yaw), each with its name under
//! it. One picture to show what the presets look like.
//!
//! The patches sit on a straight grid of the screen, four across and three
//! deep; each is a square of ground, so it shows as a diamond.
//!
//! ```text
//! cargo run --release --example showcase
//! cargo run --release --example showcase -- --fixed-time --screenshot shots/showcase.png
//! ```

mod common;

use bevy::camera::ScalingMode;
use bevy::light::CascadeShadowConfigBuilder;
use bevy::prelude::*;
use bevy_grass::prelude::*;

const PITCH_DEGREES: f32 = 30.0;
const YAW_DEGREES: f32 = 45.0;
/// The frame is this many metres tall, so about 29 m wide.
const VIEW_HEIGHT_METRES: f32 = 16.5;
const CAMERA_DISTANCE: f32 = 40.0;
/// Side of each patch of grass, in metres.
const PATCH: f32 = 4.2;
/// Screen distance between the centres of neighbouring patches, in metres of
/// ground seen along the screen's horizontal and, projected, vertical.
const PITCH_ACROSS: f32 = 6.9;
const PITCH_DOWN: f32 = 5.2;

/// The presets shown, row by row from the back. The names are the constants
/// of `GrassStyle`, and what is written under each patch.
const PRESETS: [[&str; 4]; 3] = [
    [
        "SUMMER_WILD",
        "AUTUMN_WILD",
        "WILDFLOWER_MEADOW",
        "AUTUMN_TALL",
    ],
    ["SUMMER_TALL", "SUMMER_LUSH", "TALL_PRAIRIE", "WHEAT"],
    ["MEADOW", "DRY_SAVANNA", "STYLIZED", "LAWN"],
];

/// A name under a patch: where on the ground its front corner is.
#[derive(Component)]
struct Label {
    corner: Vec3,
}

fn main() {
    let mut app = App::new();
    common::ExampleArgs::parse().install(&mut app, "bevy_grass: showcase");
    app.add_plugins(GrassPlugins)
        .insert_resource(ClearColor(Color::srgb(0.16, 0.13, 0.08)))
        .add_systems(Startup, setup)
        .add_systems(Update, place_labels)
        .run();
}

#[allow(clippy::needless_pass_by_value)]
fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut styles: ResMut<Assets<GrassStyle>>,
) {
    let rotation = Quat::from_euler(
        EulerRot::YXZ,
        YAW_DEGREES.to_radians(),
        -PITCH_DEGREES.to_radians(),
        0.0,
    );
    let mut transform = Transform::from_rotation(rotation);
    transform.translation = Vec3::ZERO - transform.forward() * CAMERA_DISTANCE;
    // The screen's horizontal, and the way "up the screen" runs on the
    // ground: moving a metre along it rises sin(pitch) metres on the screen.
    let right = transform.right().as_vec3();
    let away = Vec3::new(transform.forward().x, 0.0, transform.forward().z).normalize();
    commands.spawn((
        Camera3d::default(),
        // Far from the ground, blades are thin on screen: at least 2 pixels.
        GrassCamera {
            min_width_px: Some(2.0),
            ..default()
        },
        Projection::Orthographic(OrthographicProjection {
            scaling_mode: ScalingMode::FixedVertical {
                viewport_height: VIEW_HEIGHT_METRES,
            },
            ..OrthographicProjection::default_3d()
        }),
        transform,
    ));
    commands.spawn((
        DirectionalLight {
            shadow_maps_enabled: true,
            ..default()
        },
        Transform::from_rotation(Quat::from_euler(
            EulerRot::YXZ,
            (YAW_DEGREES + 70.0).to_radians(),
            -32.0_f32.to_radians(),
            0.0,
        )),
        CascadeShadowConfigBuilder {
            num_cascades: 1,
            minimum_distance: CAMERA_DISTANCE - VIEW_HEIGHT_METRES,
            maximum_distance: CAMERA_DISTANCE + VIEW_HEIGHT_METRES,
            ..default()
        }
        .build(),
    ));
    commands.spawn((
        Mesh3d(meshes.add(Plane3d::default().mesh().size(200.0, 200.0))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.16, 0.13, 0.08),
            perceptual_roughness: 0.95,
            ..default()
        })),
    ));

    let pitch = PITCH_DEGREES.to_radians().sin();
    for (row, names) in PRESETS.iter().enumerate() {
        for (column, name) in names.iter().enumerate() {
            let across = (column as f32 - 1.5) * PITCH_ACROSS;
            // Rows run from the back (top of the screen) to the front.
            let up = (1.0 - row as f32) * PITCH_DOWN;
            let center = right * across + away * (up / pitch);
            let style = GrassStyle::preset(name).expect("a preset constant");
            commands.spawn((
                GrassField {
                    size: Vec2::splat(PATCH),
                    style: styles.add(style),
                    ..default()
                },
                Transform::from_translation(center),
            ));
            // The patch is a square of ground, a diamond from here: its
            // nearest corner is half a diagonal toward the camera.
            let corner = center - away * (PATCH * std::f32::consts::FRAC_1_SQRT_2);
            commands
                .spawn((
                    Label { corner },
                    Node {
                        position_type: PositionType::Absolute,
                        width: Val::Px(280.0),
                        justify_content: JustifyContent::Center,
                        // Hidden until the camera knows where the corner is.
                        display: Display::None,
                        ..default()
                    },
                ))
                .with_child((
                    Text::new(*name),
                    TextFont {
                        font_size: 26.0.into(),
                        ..default()
                    },
                    TextColor(Color::srgb(0.95, 0.93, 0.85)),
                    TextLayout::justify(Justify::Center),
                    Node {
                        padding: UiRect::axes(Val::Px(12.0), Val::Px(4.0)),
                        ..default()
                    },
                    BackgroundColor(Color::srgba(0.05, 0.04, 0.03, 0.72)),
                ));
        }
    }
}

/// Puts each name under its patch, from where the camera sees the patch's
/// front corner.
fn place_labels(
    camera: Single<(&Camera, &GlobalTransform)>,
    mut labels: Query<(&Label, &mut Node)>,
) {
    let (camera, transform) = *camera;
    for (label, mut node) in &mut labels {
        let Ok(at) = camera.world_to_viewport(transform, label.corner) else {
            continue;
        };
        node.display = Display::Flex;
        node.left = Val::Px(at.x - 140.0);
        node.top = Val::Px(at.y + 6.0);
    }
}
