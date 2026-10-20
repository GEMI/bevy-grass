//! The 36 presets, two rows at a time, under an ARPG camera (orthographic,
//! 30° below the horizon, a 45° yaw), each with its name in large type under
//! it. Five pages show them all without shrinking the pictures until the
//! names stop being readable.
//!
//! The patches sit on a straight grid of the screen, three or four across and
//! two deep; each is a square of its own ground, so it shows as a diamond.
//! Every preset is shown where it grows (`common::setting`): marram on sand,
//! reeds in standing water, frost on snow, kelp under the sea.
//!
//! ```text
//! cargo run --release --example showcase
//! cargo run --release --example showcase -- --page 2
//! cargo run --release --example showcase -- --page 2 --fixed-time --screenshot shots/showcase_page2.png
//! ```

mod common;

use bevy::camera::ScalingMode;
use bevy::light::CascadeShadowConfigBuilder;
use bevy::prelude::*;
use bevy_grass::prelude::*;

const PITCH_DEGREES: f32 = 30.0;
const YAW_DEGREES: f32 = 45.0;
/// The frame is this tall, in metres of screen. The rows and labels are laid
/// out to fill it.
const VIEW_HEIGHT_METRES: f32 = 19.0;
const CAMERA_DISTANCE: f32 = 46.0;
/// Behind and between the patches: a neutral slate, so each patch's own
/// ground stands out.
const BACKDROP: Color = Color::srgb(0.12, 0.12, 0.13);

/// The pages, row by row from the back (top of the screen) to the front. Six
/// or eight presets each: three or four across and two deep. The tallest
/// presets stand in the back row, where they cover no name.
const PAGES: [&[&[&str]]; 5] = [
    &[
        &["MEADOW", "TALL_PRAIRIE", "WHEAT"],
        &["DRY_SAVANNA", "LAWN", "STYLIZED"],
    ],
    &[
        &["SUMMER_TALL", "AUTUMN_TALL", "SUMMER_WILD", "AUTUMN_WILD"],
        &[
            "WILDFLOWER_MEADOW",
            "SEED_HEADS",
            "WHITE_FLOWERS",
            "YELLOW_FLOWERS",
        ],
    ],
    &[
        &["FOREST_MOSS", "FOREST_FERNS", "FOREST_FLOOR"],
        &["FOREST_BRACKEN", "CLOVER", "BROAD_LEAF"],
    ],
    &[
        &[
            "REED_MARSH",
            "PAMPAS",
            "ELEPHANT_GRASS",
            "RYE_AND_CORNFLOWERS",
        ],
        &["COTTON_BOG", "DUNE_MARRAM", "SPINIFEX", "FROSTED_MEADOW"],
    ],
    &[
        &["KELP_BED", "CORKSCREW_ALIEN", "CRYSTAL_SPIRES", "BLIGHTED"],
        &["BURNT_REGROWTH", "LAVENDER", "CEL_FIELD", "COZY_PASTEL"],
    ],
];

/// How big a patch and the layout are, for the number of presets in a row.
struct Layout {
    /// Side of each patch of grass, in metres.
    patch: f32,
    /// Screen distance between the centres of neighbouring patches: across,
    /// and (projected) up the screen.
    across: f32,
    down: f32,
    /// Width and font size of the name under a patch.
    label_width: f32,
    label_font: f32,
}

impl Layout {
    fn for_columns(columns: usize) -> Self {
        match columns {
            3 => Self {
                patch: 6.2,
                across: 10.8,
                down: 8.0,
                label_width: 560.0,
                label_font: 48.0,
            },
            4 => Self {
                patch: 5.6,
                across: 8.2,
                down: 7.4,
                label_width: 460.0,
                label_font: 34.0,
            },
            _ => unreachable!("a showcase page has three or four presets in a row"),
        }
    }
}

/// A name under a patch: where on the ground its front corner is.
#[derive(Component)]
struct Label {
    corner: Vec3,
}

fn main() {
    let mut app = App::new();
    let args = common::ExampleArgs::parse();
    let page = args.value("--page").map_or(1, |value| {
        value
            .parse::<usize>()
            .unwrap_or_else(|_| panic!("--page takes a number from 1 to {}", PAGES.len()))
    });
    assert!(
        (1..=PAGES.len()).contains(&page),
        "--page takes a number from 1 to {}",
        PAGES.len()
    );
    args.install(
        &mut app,
        &format!("bevy_grass: showcase (page {page}/{})", PAGES.len()),
    );
    app.insert_resource(ShowcasePage(page - 1))
        .add_plugins(GrassPlugins)
        .insert_resource(ClearColor(BACKDROP))
        .add_systems(Startup, setup)
        .add_systems(Update, place_labels)
        .run();
}

/// The page being drawn, zero-based.
#[derive(Resource)]
struct ShowcasePage(usize);

fn setup(
    mut commands: Commands,
    page: Res<ShowcasePage>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut styles: ResMut<Assets<GrassStyle>>,
) {
    let rows = PAGES[page.0];
    let columns = rows[0].len();
    assert!(
        rows.iter().all(|row| row.len() == columns),
        "every row of a showcase page has the same number of presets"
    );
    let layout = Layout::for_columns(columns);
    let (right, away) = spawn_camera_and_ground(&mut commands, &mut meshes, &mut materials);

    let pitch = PITCH_DEGREES.to_radians().sin();
    let last_row = rows.len() as f32 - 1.0;
    for (row, names) in rows.iter().enumerate() {
        // Centre the two rows on the screen.
        let up = (last_row * 0.5 - row as f32) * layout.down;
        for (column, name) in names.iter().enumerate() {
            let across = (column as f32 - (columns as f32 - 1.0) * 0.5) * layout.across;
            let center = right * across + away * (up / pitch);
            let style = GrassStyle::preset(name).expect("a preset constant");
            let density_map = common::setting::spawn_setting(
                &mut commands,
                &mut meshes,
                &mut materials,
                &mut images,
                &common::setting::setting(name),
                center,
                Vec2::splat(layout.patch),
            );
            commands.spawn((
                GrassField {
                    size: Vec2::splat(layout.patch),
                    style: styles.add(style),
                    density_map,
                    ..default()
                },
                Transform::from_translation(center),
            ));
            // The patch is a square of ground, a diamond from here: its
            // nearest corner is half a diagonal toward the camera.
            let corner = center - away * (layout.patch * std::f32::consts::FRAC_1_SQRT_2);
            commands
                .spawn((
                    Label { corner },
                    Node {
                        position_type: PositionType::Absolute,
                        width: Val::Px(layout.label_width),
                        justify_content: JustifyContent::Center,
                        // Hidden until the camera knows where the corner is.
                        display: Display::None,
                        ..default()
                    },
                ))
                .with_child((
                    Text::new(*name),
                    TextFont {
                        font_size: layout.label_font.into(),
                        ..default()
                    },
                    TextColor(Color::srgb(0.97, 0.95, 0.88)),
                    TextLayout::justify(Justify::Center),
                    Node {
                        padding: UiRect::axes(Val::Px(16.0), Val::Px(6.0)),
                        ..default()
                    },
                    BackgroundColor(Color::srgba(0.05, 0.04, 0.03, 0.78)),
                ));
        }
    }
}

/// The camera, its light and the backdrop. Returns the screen's horizontal and
/// the direction "up the screen" runs on the ground.
fn spawn_camera_and_ground(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
) -> (Vec3, Vec3) {
    let rotation = Quat::from_euler(
        EulerRot::YXZ,
        YAW_DEGREES.to_radians(),
        -PITCH_DEGREES.to_radians(),
        0.0,
    );
    let mut transform = Transform::from_rotation(rotation);
    transform.translation = Vec3::ZERO - transform.forward() * CAMERA_DISTANCE;
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
    // A little below the patches' own ground, so it never shows through.
    commands.spawn((
        Mesh3d(meshes.add(Plane3d::default().mesh().size(200.0, 200.0))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: BACKDROP,
            perceptual_roughness: 0.95,
            ..default()
        })),
        Transform::from_xyz(0.0, -0.02, 0.0),
    ));
    (right, away)
}

/// Puts each name under its patch, from where the camera sees the patch's
/// front corner.
fn place_labels(
    camera: Single<(&Camera, &GlobalTransform)>,
    page: Res<ShowcasePage>,
    mut labels: Query<(&Label, &mut Node)>,
) {
    let (camera, transform) = *camera;
    let columns = PAGES[page.0][0].len();
    let layout = Layout::for_columns(columns);
    for (label, mut node) in &mut labels {
        let Ok(at) = camera.world_to_viewport(transform, label.corner) else {
            continue;
        };
        node.display = Display::Flex;
        node.left = Val::Px(at.x - layout.label_width * 0.5);
        node.top = Val::Px(at.y + 8.0);
    }
}
