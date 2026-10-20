//! The walk at the top of the README: a person walks through three presets,
//! a few seconds each, parting the grass, with the camera following 30° down
//! like DEMO ARPG GAME's. Each preset grows in a field of its own, on its own
//! ground (`common::setting`), and the film cuts from one to the next.
//!
//! Every field has its own walker and its own camera, all walking the whole
//! time, and a cut only changes which camera draws on top. A trail is the
//! camera's (each camera tramples a window around itself), so this way every
//! shot opens with the trail already behind the walker.
//!
//! Shadows, levels of detail and density are above Ultra and the wind is
//! stronger than the default breeze, for a film rather than a game. The
//! reeds stand in almost still air instead, against the waving fields.
//!
//! `--styles A,B,C` picks the presets (any number), `--shot SECONDS` how long
//! each is shown (3). `tools/record_walk.py` records the film and makes the
//! WebP.
//!
//! ```text
//! cargo run --release --example walk
//! cargo run --release --example walk -- --styles MEADOW,WHEAT --shot 5
//! cargo run --release --example walk -- --fixed-time --record target/walk --record-frames 720
//! ```

mod common;

use bevy::light::{CascadeShadowConfigBuilder, DirectionalLightShadowMap};
use bevy::prelude::*;
use bevy_grass::prelude::*;
use common::setting::{setting, spawn_setting};

const STYLES: [&str; 3] = ["WILDFLOWER_MEADOW", "RYE_AND_CORNFLOWERS", "REED_MARSH"];

/// The fields sit this far apart along Z, further than grass reaches, so a
/// camera sees only its own.
const FIELD_SPACING: f32 = 1000.0;
/// Side of each field of grass, and of its ground, in metres.
const FIELD: f32 = 120.0;
const GROUND: f32 = 300.0;
/// Presets are filmed at this many times their density, and at least
/// `MIN_DENSITY` blades a square metre before it: from 4.5 m up, a short
/// meadow at its own 60 shows its soil between the blades. (The style's
/// density, not `GrassSettings::density_scale`, which cannot add blades
/// beyond the ones the style's chunk layout has.)
const DENSITY: f32 = 2.0;
const MIN_DENSITY: f32 = 100.0;

/// A stronger wind than the breeze, with deeper waves rolling through.
const WIND: GrassWind = GrassWind {
    strength: 0.55,
    gust_scale: 20.0,
    gust_speed: 5.0,
    gust_contrast: 1.4,
    ..GrassWind::BREEZE
};

/// Almost still air, for presets that should stand against the waving
/// ones: the reeds barely sway.
const STILL: GrassWind = GrassWind {
    strength: 0.04,
    gust_speed: 1.0,
    gust_contrast: 0.5,
    sway: 0.6,
    flutter: 0.3,
    ..GrassWind::CALM
};

/// The wind preset `name` is filmed in. One wind blows on every field, so
/// it changes at each cut.
fn wind_for(name: &str) -> GrassWind {
    match name {
        "REED_MARSH" => STILL,
        _ => setting(name).wind.unwrap_or(WIND),
    }
}

/// The presets filmed, how long each is shown, and when the first shot
/// starts.
#[derive(Resource)]
struct Film {
    styles: Vec<String>,
    /// The wind of each preset, in the same order.
    winds: Vec<GrassWind>,
    shot: f32,
    /// Seconds of game time before the first shot: when `--record` starts
    /// saving frames, so the shots fall where the recording does.
    lead: f32,
}

impl Film {
    /// The shot `t` seconds in: the first until the film starts, and round
    /// again after the last.
    fn shot_at(&self, t: f32) -> usize {
        ((t - self.lead).max(0.0) / self.shot) as usize % self.styles.len()
    }

    /// Where field `index`'s walker is on its path `t` seconds in: in the
    /// middle of the field halfway through its shot.
    fn walked(&self, index: usize, t: f32) -> f32 {
        let cycle = self.shot * self.styles.len() as f32;
        let middle = self.lead + self.shot * (index as f32 + 0.5);
        // Round the field again once a cycle, while another field is shown.
        (t - middle + cycle * 0.5).rem_euclid(cycle) - cycle * 0.5
    }
}

/// Field `index`'s walker, and the camera that follows them.
#[derive(Component)]
struct Walker(usize);

#[derive(Component)]
struct Follower(usize);

/// The preset's name at the bottom of the screen.
#[derive(Component)]
struct Caption;

fn main() {
    let args = common::ExampleArgs::parse();
    let styles: Vec<String> = args.value("--styles").map_or_else(
        || STYLES.iter().map(|name| (*name).to_owned()).collect(),
        |list| list.split(',').map(str::to_owned).collect(),
    );
    for name in &styles {
        assert!(
            GrassStyle::preset(name).is_some(),
            "--styles takes preset names, such as {}; {name} is not one",
            STYLES.join(",")
        );
    }
    let shot = args
        .value("--shot")
        .and_then(|seconds| seconds.parse().ok())
        .unwrap_or(3.0);
    let lead = match (args.frames, &args.record) {
        (Some(frames), Some((_, count))) => frames.saturating_sub(*count) as f32 / 60.0,
        _ => 0.0,
    };
    let winds = styles.iter().map(|name| wind_for(name)).collect();
    let mut app = App::new();
    args.install(&mut app, "bevy_grass: walk");
    app.add_plugins(GrassPlugins)
        .insert_resource(Film {
            styles,
            winds,
            shot,
            lead,
        })
        .insert_resource(GrassSettings {
            mid_range_density: 1.0,
            instance_capacity: 3_000_000,
            max_chunk_slices: 32_768,
            ..GrassSettings::ULTRA
        })
        .insert_resource(DirectionalLightShadowMap { size: 4096 })
        .add_systems(Startup, setup)
        // The wind is read in the same frame, so it changes with the picture.
        .add_systems(Update, (walk, cut).chain().before(GrassWindSystems::Sample))
        .run();
}

/// The centre of field `index`.
fn field_center(index: usize) -> Vec3 {
    Vec3::new(0.0, 0.0, index as f32 * FIELD_SPACING)
}

fn setup(
    mut commands: Commands,
    film: Res<Film>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut styles: ResMut<Assets<GrassStyle>>,
) {
    commands.spawn((
        DirectionalLight {
            shadow_maps_enabled: true,
            ..default()
        },
        // A little lower than in the other examples, for longer shadows.
        Transform::from_rotation(Quat::from_euler(EulerRot::YXZ, -0.5, -0.6, 0.0)),
        // Each camera's first cascade 10 m deep in a 4096 map: a texel is
        // about 4 mm, far narrower than a blade.
        CascadeShadowConfigBuilder {
            num_cascades: 3,
            first_cascade_far_bound: 10.0,
            maximum_distance: 60.0,
            ..default()
        }
        .build(),
    ));
    for (index, name) in film.styles.iter().enumerate() {
        let center = field_center(index);
        let density_map = spawn_setting(
            &mut commands,
            &mut meshes,
            &mut materials,
            &mut images,
            &setting(name),
            center,
            Vec2::splat(GROUND),
        );
        let style = GrassStyle::preset(name).expect("checked in main");
        let density = style.density.max(MIN_DENSITY) * DENSITY;
        commands.spawn((
            GrassField {
                size: Vec2::splat(FIELD),
                style: styles.add(style.with_density(density)),
                density_map,
                ..default()
            },
            Transform::from_translation(center),
        ));
        let person = common::spawn_person(&mut commands, &mut meshes, &mut materials, center);
        commands
            .entity(person)
            .insert((Walker(index), GrassInteractor::default()));
        commands.spawn((
            Camera3d::default(),
            Camera {
                order: index as isize,
                ..default()
            },
            Follower(index),
        ));
    }
    commands.spawn((
        Caption,
        Text::default(),
        TextFont {
            font_size: 56.0.into(),
            ..default()
        },
        TextColor(Color::srgb(0.97, 0.95, 0.88)),
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(36.0),
            bottom: Val::Px(30.0),
            padding: UiRect::axes(Val::Px(20.0), Val::Px(8.0)),
            ..default()
        },
        BackgroundColor(Color::srgba(0.05, 0.04, 0.03, 0.72)),
    ));
}

/// Where the walk is `x` metres along: a stroll along +X, weaving a little.
fn walk_path(x: f32) -> Vec3 {
    Vec3::new(x, 0.0, 1.5 * (x * 0.25).sin())
}

fn walk(
    time: Res<Time>,
    film: Res<Film>,
    mut walkers: Query<(&mut Transform, &Walker), Without<Follower>>,
    mut cameras: Query<(&mut Transform, &Follower)>,
) {
    let t = time.elapsed_secs();
    let at = |index: usize| walk_path(1.3 * film.walked(index, t));
    for (mut transform, walker) in &mut walkers {
        // A little bob with every step.
        transform.translation =
            field_center(walker.0) + at(walker.0) + Vec3::Y * (0.9 + 0.04 * (t * 6.0).sin().abs());
    }
    for (mut transform, follower) in &mut cameras {
        // Behind and to the side, 30° down like DEMO ARPG GAME's camera, so the
        // trail of bent grass shows.
        let center = field_center(follower.0);
        let at = at(follower.0);
        let eye = Vec3::new(at.x - 3.5, 4.5, 5.6);
        *transform = Transform::from_translation(center + eye)
            .looking_at(center + Vec3::new(at.x + 0.5, 0.5, at.z * 0.5), Vec3::Y);
    }
}

/// Puts the camera of the shot on top of the others, its preset in the
/// caption, and its wind over the fields.
fn cut(
    time: Res<Time>,
    film: Res<Film>,
    mut cameras: Query<(&mut Camera, &Follower)>,
    mut captions: Query<&mut Text, With<Caption>>,
    mut wind: ResMut<GrassWind>,
) {
    let shot = film.shot_at(time.elapsed_secs());
    if *wind != film.winds[shot] {
        *wind = film.winds[shot];
    }
    for (mut camera, follower) in &mut cameras {
        let order = if follower.0 == shot {
            film.styles.len()
        } else {
            follower.0
        } as isize;
        if camera.order != order {
            camera.order = order;
        }
    }
    for mut text in &mut captions {
        if text.0 != film.styles[shot] {
            text.0.clone_from(&film.styles[shot]);
        }
    }
}
