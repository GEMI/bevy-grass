use bevy::MinimalPlugins;
use bevy::app::Update;
use bevy::asset::{AssetApp, AssetPlugin, Assets, RenderAssetUsages};
use bevy::camera::visibility::InheritedVisibility;
use bevy::ecs::reflect::AppTypeRegistry;
use bevy::gizmos::GizmoAsset;
use bevy::image::Image;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::transform::components::Transform;

use super::*;

/// A camera at `position` looking along `forward`, perspective (45°) or
/// orthographic, with High's radius.
fn camera_at(position: Vec3, forward: Vec3, orthographic: bool) -> TrampleView {
    use bevy::camera::{CameraProjection, OrthographicProjection, PerspectiveProjection};
    let transform = Transform::from_translation(position).looking_to(forward, Vec3::Y);
    let clip_from_view = if orthographic {
        OrthographicProjection::default_3d().get_clip_from_view()
    } else {
        PerspectiveProjection::default().get_clip_from_view()
    };
    TrampleView::new(
        &GlobalTransform::from(transform),
        &clip_from_view,
        GrassSettings::HIGH.radius,
    )
}
use crate::api::{GrassField, GrassGround};
use crate::style::GrassStyle;

/// An app with the plugin's tables, and fields visible as Bevy's
/// visibility propagation would leave them.
fn app() -> App {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, AssetPlugin::default()))
        .init_asset::<GrassStyle>()
        .init_asset::<Image>()
        .init_asset::<bevy::mesh::Mesh>()
        .init_resource::<GrassSettings>()
        .add_plugins(crate::fields::plugin);
    app
}

fn spawn(app: &mut App, field: GrassField, transform: Transform, visible: bool) {
    let mut entity = app
        .world_mut()
        .spawn((field, GlobalTransform::from(transform)));
    if visible {
        entity.insert(InheritedVisibility::VISIBLE);
    }
}

fn style(app: &mut App) -> bevy::asset::Handle<GrassStyle> {
    app.world_mut()
        .resource_mut::<Assets<GrassStyle>>()
        .add(GrassStyle::MEADOW)
}

fn tables(app: &mut App) -> FieldTables {
    app.update();
    app.update();
    app.world().resource::<FieldTables>().clone()
}

fn rects(shapes: &[Shape]) -> Vec<(Vec2, Vec2, f32, Color)> {
    shapes
        .iter()
        .filter_map(|shape| match *shape {
            Shape::Rect {
                min,
                size,
                y,
                color,
                ..
            } => Some((min, size, y, color)),
            Shape::Circle { .. } | Shape::Footprint { .. } => None,
        })
        .collect()
}

#[test]
fn a_field_is_drawn_where_the_tables_have_it_scale_and_visibility_included() {
    let mut app = app();
    let handle = style(&mut app);
    let field = |style| GrassField {
        size: Vec2::splat(32.0),
        style,
        ..Default::default()
    };
    // Scaled to 64 by 16 and moved; visible.
    spawn(
        &mut app,
        field(handle.clone()),
        Transform::from_xyz(10.0, 2.0, -6.0).with_scale(Vec3::new(2.0, 1.0, 0.5)),
        true,
    );
    // Not visible: the same, plainly.
    spawn(&mut app, field(handle), Transform::IDENTITY, false);
    let tables = tables(&mut app);
    let drawn = rects(&field_shapes(&tables));
    assert_eq!(drawn.len(), 2);
    let visible = drawn
        .iter()
        .find(|(_, size, ..)| *size == Vec2::new(64.0, 16.0))
        .expect("the scaled field");
    assert_eq!(visible.0, Vec2::new(10.0 - 32.0, -6.0 - 8.0));
    assert_eq!(visible.2, 2.0, "at the field's ground level");
    assert_eq!(visible.3, Color::from(css::LIME));
    let hidden = drawn
        .iter()
        .find(|(_, size, ..)| *size == Vec2::splat(32.0))
        .expect("the other field");
    assert_eq!(hidden.3, Color::from(css::GRAY), "a hidden field is grey");
}

#[test]
fn a_heightmap_field_also_shows_its_highest_ground() {
    let mut app = app();
    let handle = style(&mut app);
    let image = app
        .world_mut()
        .resource_mut::<Assets<Image>>()
        .add(Image::new_fill(
            Extent3d {
                width: 4,
                height: 4,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            &[128],
            TextureFormat::R8Unorm,
            RenderAssetUsages::default(),
        ));
    spawn(
        &mut app,
        GrassField {
            size: Vec2::splat(16.0),
            style: handle,
            ground: GrassGround::Heightmap {
                image,
                height_scale: 7.0,
            },
            ..Default::default()
        },
        Transform::from_xyz(0.0, 1.0, 0.0),
        true,
    );
    let drawn = rects(&field_shapes(&tables(&mut app)));
    let heights: Vec<f32> = drawn.iter().map(|(_, _, y, _)| *y).collect();
    // The heightmap is 128 everywhere: 128/255 of the 7 m scale above 1.
    assert_eq!(heights, vec![1.0, 1.0 + 128.0 / 255.0 * 7.0]);
}

#[test]
fn chunks_are_the_squares_the_gpu_culls_and_the_cap_holds() {
    let mut app = app();
    let handle = style(&mut app);
    spawn(
        &mut app,
        GrassField {
            size: Vec2::splat(32.0),
            style: handle,
            ..Default::default()
        },
        Transform::IDENTITY,
        true,
    );
    let tables = tables(&mut app);
    let chunks = rects(&chunk_shapes(&tables, 100));
    let mut mins: Vec<_> = chunks.iter().map(|(min, ..)| (min.x, min.y)).collect();
    mins.sort_by(|a, b| f32_pair_order(*a, *b));
    assert_eq!(
        mins,
        vec![(-16.0, -16.0), (-16.0, 0.0), (0.0, -16.0), (0.0, 0.0)]
    );
    assert!(
        chunks
            .iter()
            .all(|(_, size, ..)| *size == Vec2::splat(CHUNK_SIZE))
    );
    assert_eq!(chunk_shapes(&tables, 2).len(), 2, "capped");
    assert!(chunk_shapes(&tables, 0).is_empty());
}

fn f32_pair_order(a: (f32, f32), b: (f32, f32)) -> std::cmp::Ordering {
    a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1))
}

#[test]
fn an_interactor_is_drawn_as_the_disc_it_flattens_and_the_cut_off_part_is_red() {
    let at = Vec3::new(4.0, 0.5, -2.0);
    let shapes = interactor_shapes(
        [
            (at, 0.6, 1.0), // the default: a yellow disc
            (at, 4.0, 1.0), // a vehicle: cut to 2 m, with the ask in red
            (at, 0.6, 0.0), // flattens nothing: grey
        ]
        .into_iter(),
        GrassSettings::HIGH.max_interactor_radius,
    );
    let circle = |i: usize| match shapes[i] {
        Shape::Circle {
            center,
            radius,
            y,
            color,
        } => (center, radius, y, color),
        Shape::Rect { .. } | Shape::Footprint { .. } => panic!("not a circle"),
    };
    assert_eq!(shapes.len(), 4);
    assert_eq!(
        circle(0),
        (Vec2::new(4.0, -2.0), 0.6, 0.5, css::YELLOW.into())
    );
    assert_eq!(
        circle(1).1,
        GrassSettings::HIGH.max_interactor_radius,
        "what is flattened"
    );
    assert_eq!(circle(1).3, Color::from(css::YELLOW));
    assert_eq!(circle(2).1, 4.0, "what was asked for");
    assert_eq!(circle(2).3, Color::from(css::RED));
    assert_eq!(circle(3).3, Color::from(css::GRAY));
}

#[test]
fn a_trample_window_is_where_the_renderer_puts_it() {
    let mut app = app();
    let handle = style(&mut app);
    spawn(
        &mut app,
        GrassField {
            size: Vec2::splat(400.0),
            style: handle,
            ..Default::default()
        },
        Transform::IDENTITY,
        true,
    );
    let tables = tables(&mut app);

    // A perspective camera stands in its window.
    let position = Vec3::new(100.0, 1.7, -30.0);
    let window = trample_shapes(
        [camera_at(position, Vec3::NEG_Z, false)].into_iter(),
        &tables,
        GrassSettings::HIGH.trample_cell_size,
    );
    let [(min, size, y, _)] = rects(&window)[..] else {
        panic!("one window");
    };
    assert_eq!(size, Vec2::splat(64.0), "256 cells of 25 cm");
    assert_eq!(min, Vec2::new(68.0, -62.0));
    assert!(position.x > min.x && position.x < min.x + size.x);
    assert!(position.z > min.y && position.z < min.y + size.y);
    assert_eq!(y, 0.0);

    // An orthographic camera 40 m up, looking down and ahead, has its
    // window where its view meets the ground, not under the camera.
    let high = Vec3::new(0.0, 40.0, 40.0);
    let forward = Vec3::new(0.0, -1.0, -1.0).normalize();
    let window = trample_shapes(
        [camera_at(high, forward, true)].into_iter(),
        &tables,
        GrassSettings::HIGH.trample_cell_size,
    );
    let [(min, size, ..)] = rects(&window)[..] else {
        panic!("one window");
    };
    let center = min + size * 0.5;
    assert!(
        center.y.abs() < 1.0,
        "centred on the ground it looks at: {center}"
    );
}

#[test]
fn the_gizmos_follow_the_settings_that_move_the_limits() {
    let tables = FieldTables::default();
    // Cells of 50 cm make a window of 128 m.
    let window = trample_shapes(
        [camera_at(Vec3::new(10.0, 1.7, 10.0), Vec3::NEG_Z, false)].into_iter(),
        &tables,
        0.5,
    );
    let [(min, size, ..)] = rects(&window)[..] else {
        panic!("one window");
    };
    assert_eq!(size, Vec2::splat(128.0));
    assert_eq!(min, Vec2::splat(10.0 - 64.0));

    // An interactor of 3 m is cut at 2 m, and not at all at 4.
    let at = Vec3::ZERO;
    assert_eq!(
        interactor_shapes([(at, 3.0, 1.0)].into_iter(), 2.0).len(),
        2
    );
    let uncut = interactor_shapes([(at, 3.0, 1.0)].into_iter(), 4.0);
    assert_eq!(uncut.len(), 1, "nothing is cut, so no red disc");
    let Shape::Circle { radius, .. } = uncut[0] else {
        panic!("a circle");
    };
    assert_eq!(radius, 3.0);
}

#[test]
fn the_toggles_decide_what_is_shown() {
    let mut app = app();
    let handle = style(&mut app);
    spawn(
        &mut app,
        GrassField {
            size: Vec2::splat(32.0),
            style: handle,
            ..Default::default()
        },
        Transform::IDENTITY,
        true,
    );
    let tables = tables(&mut app);
    let camera = || [camera_at(Vec3::new(0.0, 1.7, 0.0), Vec3::NEG_Z, false)].into_iter();
    let bunny = || [(Vec3::ZERO, 0.6, 1.0)].into_iter();
    let count =
        |debug: GrassDebug| shapes(&debug, &GrassSettings::HIGH, &tables, bunny(), camera()).len();

    let none = GrassDebug {
        enabled: true,
        fields: false,
        chunks: false,
        interactors: false,
        trample_windows: false,
        ..GrassDebug::default()
    };
    assert_eq!(count(none), 0);
    assert_eq!(
        count(GrassDebug {
            fields: true,
            ..none
        }),
        1
    );
    assert_eq!(
        count(GrassDebug {
            chunks: true,
            ..none
        }),
        4
    );
    assert_eq!(
        count(GrassDebug {
            chunks: true,
            max_chunks: 3,
            ..none
        }),
        3
    );
    assert_eq!(
        count(GrassDebug {
            interactors: true,
            ..none
        }),
        1
    );
    assert_eq!(
        count(GrassDebug {
            trample_windows: true,
            ..none
        }),
        1
    );
    // The default shows fields, interactors and windows, not chunks.
    assert_eq!(count(GrassDebug::default()), 3);
    // And the master switch turns all of it off at once.
    assert_eq!(
        count(GrassDebug {
            enabled: false,
            chunks: true,
            ..GrassDebug::default()
        }),
        0
    );
}

#[test]
fn what_is_drawn_is_lines_in_the_right_place() {
    let mut buffer = GizmoBuffer::<GrassGizmos, ()>::default();
    draw(
        &mut buffer,
        &[
            Shape::Rect {
                min: Vec2::new(1.0, 2.0),
                size: Vec2::new(3.0, 4.0),
                along: Vec2::X,
                y: 5.0,
                color: css::LIME.into(),
            },
            Shape::Circle {
                center: Vec2::new(10.0, -20.0),
                radius: 2.0,
                y: 1.0,
                color: css::YELLOW.into(),
            },
        ],
    );
    // The rectangle is a closed strip of its four corners; the circle
    // follows it in the same buffer.
    let rect = &buffer.strip_positions[..5];
    assert_eq!(rect[0], Vec3::new(1.0, 5.0, 2.0));
    assert_eq!(rect[2], Vec3::new(4.0, 5.0, 6.0));
    assert_eq!(rect[0], rect[4], "closed");
    assert!(rect.iter().all(|p| p.y == 5.0), "flat");
    let circle: Vec<_> = buffer
        .strip_positions
        .iter()
        .filter(|p| p.y == 1.0)
        .collect();
    assert!(circle.len() >= 8, "a circle needs a few points");
    for point in circle {
        let from_center = Vec2::new(point.x - 10.0, point.z + 20.0).length();
        assert!(
            (from_center - 2.0).abs() < 1e-3,
            "{point} is {from_center} from the centre"
        );
    }
}

#[test]
fn the_plugin_runs_and_the_resource_is_reflected() {
    let mut app = app();
    // Not `GizmoPlugin`, which wants a renderer: the plugin needs only
    // the gizmo config store to draw into the buffers.
    app.init_asset::<GizmoAsset>()
        .add_plugins(GrassDebugPlugin)
        .add_systems(Update, || ());
    let handle = style(&mut app);
    spawn(
        &mut app,
        GrassField {
            size: Vec2::splat(16.0),
            style: handle,
            ..Default::default()
        },
        Transform::IDENTITY,
        true,
    );
    app.world_mut()
        .spawn((GlobalTransform::IDENTITY, GrassInteractor::default()));
    for _ in 0..3 {
        app.update();
    }
    assert_eq!(*app.world().resource::<GrassDebug>(), GrassDebug::default());
    let registry = app.world().resource::<AppTypeRegistry>().read();
    assert!(
        registry
            .get_type_data::<ReflectResource>(std::any::TypeId::of::<GrassDebug>())
            .is_some()
    );
}
