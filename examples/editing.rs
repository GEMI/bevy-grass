//! Runtime editing: grass cleared, mowed, grown and put back while the game
//! runs, and read back for gameplay.
//!
//! - A lawnmower drives in circles with a `GrassBrush` that mows, its deck a
//!   `GrassBrushFootprint`.
//! - A walker paces between two points with a slow brush: each passing wears
//!   the grass down a little, and over a minute or two a path forms.
//! - At start, a building's plot is cleared with one `GrassEdit` and a strip
//!   is mowed.
//! - The field has `GrassRegrowth`: mowed grass grows back in about half a
//!   minute, cleared ground fills in over a few minutes. R turns it off and on.
//!
//! Paint with the left mouse button: 1 clears, 2 mows, 3 grows grass, 4 puts
//! back what was there. The mouse wheel sizes the brush. The text reads the
//! grass under the cursor with `GrassSampler`, the way a game decides whether
//! a player is hidden.
//!
//! ```text
//! cargo run --release --example editing
//! cargo run --release --example editing -- --fixed-time --screenshot shots/editing.png
//! ```

mod common;

use bevy::light::CascadeShadowConfigBuilder;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_grass::prelude::*;

fn main() {
    let mut app = App::new();
    common::ExampleArgs::parse().install(&mut app, "bevy_grass: editing");
    app.add_plugins(GrassPlugins)
        .init_resource::<Tool>()
        .add_systems(Startup, setup)
        .add_systems(
            Update,
            (choose_tool, paint, drive_mower, walk, toggle_regrowth, show),
        )
        .run();
}

/// What the mouse does.
#[derive(Resource)]
struct Tool {
    op: GrassEditOp,
    name: &'static str,
    radius: f32,
}

impl Default for Tool {
    fn default() -> Self {
        Self {
            op: GrassEditOp::Density(0.0),
            name: "clear",
            radius: 1.5,
        }
    }
}

#[derive(Component)]
struct Meadow;

#[derive(Component)]
struct Mower;

/// Paces between `from` and `to`.
#[derive(Component)]
struct Walker {
    from: Vec2,
    to: Vec2,
}

#[derive(Component)]
struct Readout;

const REGROWTH: GrassRegrowth = GrassRegrowth {
    density_rate: 0.004,
    height_rate: 0.03,
};

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut styles: ResMut<Assets<GrassStyle>>,
    mut edits: MessageWriter<GrassEdit>,
) {
    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(0.0, 11.0, 30.0).looking_at(Vec3::new(0.0, 0.0, -4.0), Vec3::Y),
    ));
    commands.spawn((
        DirectionalLight {
            shadow_maps_enabled: true,
            ..default()
        },
        CascadeShadowConfigBuilder {
            num_cascades: 3,
            first_cascade_far_bound: 20.0,
            maximum_distance: 90.0,
            ..default()
        }
        .build(),
        Transform::from_xyz(4.0, 8.0, 3.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    commands.spawn((
        Mesh3d(meshes.add(Plane3d::default().mesh().size(64.0, 64.0))),
        MeshMaterial3d(materials.add(Color::srgb(0.2, 0.15, 0.09))),
    ));
    commands.spawn((
        GrassField::new(Vec2::splat(64.0), GrassStyle::TALL_PRAIRIE, &mut styles),
        REGROWTH,
        Meadow,
    ));

    // A building's plot, cleared as it is placed, and a mowed strip.
    let plot = Transform::from_xyz(-12.0, 1.5, -8.0).with_rotation(Quat::from_rotation_y(0.4));
    commands.spawn((
        Mesh3d(meshes.add(Cuboid::new(8.0, 3.0, 5.0))),
        MeshMaterial3d(materials.add(Color::srgb(0.55, 0.5, 0.45))),
        plot,
    ));
    edits.write(GrassEdit::clear(GrassBrushShape::rect(
        plot.translation.xz(),
        Vec2::new(5.0, 3.5),
        0.4,
    )));
    edits.write(GrassEdit::mow(
        GrassBrushShape::stroke(Vec2::new(-24.0, 14.0), Vec2::new(24.0, 14.0), 2.0),
        0.1,
    ));

    // A mower, its 1.6 × 1 m deck cutting the grass to a tenth of its
    // height.
    commands.spawn((
        Mesh3d(meshes.add(Cuboid::new(1.6, 0.6, 1.0))),
        MeshMaterial3d(materials.add(Color::srgb(0.8, 0.15, 0.1))),
        Transform::from_xyz(20.0, 0.3, -6.0),
        GrassBrush::new(GrassEditOp::Mow(0.1), 0.0),
        GrassBrushFootprint::new(Vec2::new(0.8, 0.5)),
        Mower,
    ));
    // A walker who wears a path: each passing takes about a quarter of the
    // grass, and regrowth puts back less than that between passings.
    commands.spawn((
        Mesh3d(meshes.add(Capsule3d::new(0.25, 1.2))),
        MeshMaterial3d(materials.add(Color::srgb(0.2, 0.3, 0.8))),
        Transform::from_xyz(-20.0, 0.85, 4.0),
        GrassBrush::new(GrassEditOp::Density(0.0), 0.35).with_rate(1.0),
        Walker {
            from: Vec2::new(-20.0, 4.0),
            to: Vec2::new(6.0, 24.0),
        },
    ));

    commands.spawn((
        Text::default(),
        Node {
            position_type: PositionType::Absolute,
            top: px(12),
            left: px(12),
            ..default()
        },
        Readout,
    ));
}

/// 1–4 pick the tool; the mouse wheel sizes it.
fn choose_tool(
    keys: Res<ButtonInput<KeyCode>>,
    mut wheel: MessageReader<bevy::input::mouse::MouseWheel>,
    mut tool: ResMut<Tool>,
) {
    let choices = [
        (KeyCode::Digit1, GrassEditOp::Density(0.0), "clear"),
        (KeyCode::Digit2, GrassEditOp::Mow(0.2), "mow"),
        (KeyCode::Digit3, GrassEditOp::Density(1.0), "grow"),
        (KeyCode::Digit4, GrassEditOp::Restore, "restore"),
    ];
    for (key, op, name) in choices {
        if keys.just_pressed(key) {
            tool.op = op;
            tool.name = name;
        }
    }
    for event in wheel.read() {
        tool.radius = (tool.radius * 1.15f32.powf(event.y.signum())).clamp(0.3, 8.0);
    }
}

/// The ground under the cursor, on the plane y = 0.
fn cursor_on_ground(
    window: &Window,
    (camera, transform): (&Camera, &GlobalTransform),
) -> Option<Vec2> {
    let ray = camera
        .viewport_to_world(transform, window.cursor_position()?)
        .ok()?;
    let distance = ray.intersect_plane(Vec3::ZERO, InfinitePlane3d::new(Vec3::Y))?;
    Some(ray.get_point(distance).xz())
}

/// While the left button is held, a stroke from where the cursor was to
/// where it is. At full strength an edit sent again changes nothing, so the
/// strokes overlap as a paint program's do.
fn paint(
    buttons: Res<ButtonInput<MouseButton>>,
    window: Single<&Window, With<PrimaryWindow>>,
    camera: Single<(&Camera, &GlobalTransform)>,
    tool: Res<Tool>,
    mut last: Local<Option<Vec2>>,
    mut edits: MessageWriter<GrassEdit>,
) {
    let held = buttons.pressed(MouseButton::Left);
    let Some(at) = cursor_on_ground(&window, *camera).filter(|_| held) else {
        *last = None;
        return;
    };
    let from = last.replace(at).unwrap_or(at);
    edits.write(GrassEdit::new(
        GrassBrushShape::stroke(from, at, tool.radius),
        tool.op,
    ));
}

fn drive_mower(time: Res<Time>, mut mower: Single<&mut Transform, With<Mower>>) {
    let angle = time.elapsed_secs() * 0.35;
    let center = Vec2::new(10.0, -6.0);
    let at = center + Vec2::from_angle(angle) * 10.0;
    mower.translation = Vec3::new(at.x, 0.3, at.y);
    mower.rotation = Quat::from_rotation_y(-angle);
}

fn walk(time: Res<Time>, mut walkers: Query<(&mut Transform, &Walker)>) {
    for (mut transform, walker) in &mut walkers {
        // There and back every 24 s, at about 2.7 m/s.
        let t = (time.elapsed_secs() / 12.0 * std::f32::consts::PI).sin() * 0.5 + 0.5;
        let at = walker.from.lerp(walker.to, t);
        transform.translation = Vec3::new(at.x, 0.85, at.y);
    }
}

fn toggle_regrowth(
    keys: Res<ButtonInput<KeyCode>>,
    meadow: Single<(Entity, Has<GrassRegrowth>), With<Meadow>>,
    mut commands: Commands,
) {
    if !keys.just_pressed(KeyCode::KeyR) {
        return;
    }
    let (entity, regrowing) = *meadow;
    if regrowing {
        commands.entity(entity).remove::<GrassRegrowth>();
    } else {
        commands.entity(entity).insert(REGROWTH);
    }
}

/// The tool, the grass under the cursor, and what editing costs.
fn show(
    grass: GrassSampler,
    window: Single<&Window, With<PrimaryWindow>>,
    camera: Single<(&Camera, &GlobalTransform)>,
    tool: Res<Tool>,
    stats: Res<GrassStats>,
    regrowing: Query<(), (With<Meadow>, With<GrassRegrowth>)>,
    mut readout: Single<&mut Text, With<Readout>>,
) {
    let under = cursor_on_ground(&window, *camera).and_then(|at| grass.sample(at));
    let cursor = match under {
        Some(sample) => format!(
            "density {:.2} ({:.0} blades per m2), height {:.2} m{}",
            sample.density,
            sample.blades_per_m2,
            sample.height,
            if sample.density > 0.5 && sample.height > 0.8 {
                ": hidden"
            } else {
                ""
            },
        ),
        None => "no grass".to_owned(),
    };
    let edits = stats.edits;
    readout.0 = format!(
        "[1] clear [2] mow [3] grow [4] restore, wheel: size. Tool: {} {:.1} m\n\
         [R] regrowth: {}\n\
         Under the cursor: {cursor}\n\
         Texels changed {}, uploaded {} kB, {:.3} ms this frame",
        tool.name,
        tool.radius,
        if regrowing.is_empty() { "off" } else { "on" },
        edits.texels_changed,
        edits.bytes_uploaded / 1024,
        edits.last_apply.as_secs_f64() * 1000.0,
    );
}
