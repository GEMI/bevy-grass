use bevy::MinimalPlugins;
use bevy::app::App;
use bevy::asset::{AssetApp, AssetPlugin};
use bevy::camera::visibility::InheritedVisibility;
use bevy::math::{Quat, Vec4Swizzles};
use bevy::mesh::{Indices, Meshable};
use bevy::prelude::Plane3d;
use bevy::transform::components::{GlobalTransform, Transform};

use super::*;
use crate::fields::{FieldTables, plugin};
use crate::style::GrassStyle;

/// A square of `size` centred on the origin, rising from 0 at −X to
/// `rise` at +X: its height at `x` is `rise * (x / size.x + 0.5)`.
pub(crate) fn slope(size: Vec2, rise: f32) -> Mesh {
    let (hx, hz) = (size.x * 0.5, size.y * 0.5);
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(
        Mesh::ATTRIBUTE_POSITION,
        vec![
            [-hx, 0.0, -hz],
            [hx, rise, -hz],
            [hx, rise, hz],
            [-hx, 0.0, hz],
        ],
    )
    .with_inserted_indices(Indices::U32(vec![0, 2, 1, 0, 3, 2]))
}

/// The texel centres of a bake of `size`, `texels` across, in the mesh's XZ.
fn centre(size: Vec2, texels: UVec2, i: u32, j: u32) -> Vec2 {
    (Vec2::new(i as f32, j as f32) + 0.5) / texels.as_vec2() * size - size * 0.5
}

#[test]
fn a_mesh_is_baked_to_the_height_over_each_texel_centre() {
    let size = Vec2::new(20.0, 10.0);
    let texels = UVec2::new(8, 4);
    let heights = bake(&slope(size, 4.0), size, texels).unwrap();
    assert_eq!(heights.len(), 32);
    for j in 0..texels.y {
        for i in 0..texels.x {
            let x = centre(size, texels, i, j).x;
            let expected = 4.0 * (x / size.x + 0.5);
            let got = heights[(j * texels.x + i) as usize];
            assert!(
                (got - expected).abs() < 1e-5,
                "({i}, {j}): {got} vs {expected}"
            );
        }
    }
}

#[test]
fn a_plane_of_the_fields_size_covers_every_texel() {
    let size = Vec2::new(64.0, 48.0);
    let plane =
        Mesh::from(Plane3d::default().mesh().size(size.x, size.y)).translated_by(Vec3::Y * 1.5);
    let heights = bake(&plane, size, texels(size, 0.5)).unwrap();
    assert!(
        heights.iter().all(|&h| h == 1.5),
        "no holes along the edges"
    );
}

#[test]
fn nothing_grows_where_the_mesh_does_not_reach_and_the_top_surface_wins() {
    let size = Vec2::splat(8.0);
    let texels = UVec2::splat(8);
    // A ledge over the +X half, 1 m up, with a roof over its +Z half at 3 m.
    let mut ledge = Mesh::from(Plane3d::default().mesh().size(4.0, 8.0))
        .translated_by(Vec3::new(2.0, 1.0, 0.0));
    ledge
        .merge(
            &Mesh::from(Plane3d::default().mesh().size(4.0, 4.0))
                .translated_by(Vec3::new(2.0, 3.0, 2.0)),
        )
        .unwrap();
    let heights = bake(&ledge, size, texels).unwrap();
    for j in 0..texels.y {
        for i in 0..texels.x {
            let at = centre(size, texels, i, j);
            let expected = match (at.x > 0.0, at.y > 0.0) {
                (false, _) => HOLE,
                (true, false) => 1.0,
                (true, true) => 3.0,
            };
            assert_eq!(heights[(j * texels.x + i) as usize], expected, "({i}, {j})");
        }
    }
}

#[test]
fn unindexed_meshes_and_strips_are_baked_and_lines_are_not() {
    let size = Vec2::splat(4.0);
    let texels = UVec2::splat(4);
    let corners = vec![
        [-2.0, 1.0, -2.0],
        [-2.0, 1.0, 2.0],
        [2.0, 1.0, -2.0],
        [2.0, 1.0, 2.0],
    ];
    let strip = Mesh::new(
        PrimitiveTopology::TriangleStrip,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, corners.clone());
    assert!(
        bake(&strip, size, texels)
            .unwrap()
            .iter()
            .all(|&h| h == 1.0)
    );

    let list = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(
        Mesh::ATTRIBUTE_POSITION,
        vec![
            corners[0], corners[1], corners[2], corners[2], corners[1], corners[3],
        ],
    );
    assert!(bake(&list, size, texels).unwrap().iter().all(|&h| h == 1.0));

    let lines = Mesh::new(PrimitiveTopology::LineList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, corners);
    assert_eq!(
        bake(&lines, size, texels),
        Err(BakeError::Topology(PrimitiveTopology::LineList))
    );
    let empty = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    );
    assert_eq!(bake(&empty, size, texels), Err(BakeError::NoPositions));
}

#[test]
fn the_texel_count_follows_the_cell_size_within_limits() {
    assert_eq!(texels(Vec2::new(64.0, 30.0), 0.5), UVec2::new(128, 60));
    assert_eq!(texels(Vec2::new(64.0, 30.1), 0.5), UVec2::new(128, 61));
    assert_eq!(texels(Vec2::splat(1e5), 0.5), UVec2::splat(MAX_TEXELS));
    assert_eq!(texels(Vec2::splat(0.1), 0.5), UVec2::ONE);
    assert_eq!(texels(Vec2::splat(8.0), 0.0), UVec2::splat(MAX_TEXELS));
}

/// An app with the field tables, its fields always visible.
fn app() -> App {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, AssetPlugin::default()))
        .init_asset::<GrassStyle>()
        .init_asset::<Image>()
        .init_asset::<Mesh>()
        .add_plugins(plugin);
    app
}

fn spawn(app: &mut App, ground: GrassGround, transform: Transform) -> Entity {
    let style = app
        .world_mut()
        .resource_mut::<Assets<GrassStyle>>()
        .add(GrassStyle::MEADOW);
    app.world_mut()
        .spawn((
            GrassField {
                size: Vec2::splat(32.0),
                style,
                ground,
                ..Default::default()
            },
            GlobalTransform::from(transform),
            InheritedVisibility::VISIBLE,
        ))
        .id()
}

fn update(app: &mut App) -> FieldTables {
    for _ in 0..3 {
        app.update();
    }
    app.world().resource::<FieldTables>().clone()
}

/// The bakes so far, counted by the images they made.
fn bakes(app: &App) -> usize {
    app.world().resource::<Assets<Image>>().len()
}

#[test]
fn a_field_on_a_mesh_stands_on_it_and_is_baked_again_only_when_the_mesh_changes() {
    let mut app = app();
    let hill = app
        .world_mut()
        .resource_mut::<Assets<Mesh>>()
        .add(slope(Vec2::splat(32.0), 8.0));
    let field = spawn(
        &mut app,
        GrassGround::mesh(hill.clone()),
        Transform::from_xyz(0.0, 10.0, 0.0),
    );
    let tables = update(&mut app);
    assert_eq!(tables.fields.len(), 1);
    assert_eq!(bakes(&app), 1);
    // The bake is one height per half metre, in the mesh's units, which the
    // Y scale (1 here) turns into metres above the field's height.
    let entry = tables.fields[0];
    assert_eq!(entry.map_sizes.zw(), UVec2::splat(64));
    assert_eq!(entry.extent.z, 1.0);
    assert_eq!(tables.heights.len(), 64 * 64);
    // Two chunks across the slope: the low one from 10 m, the high one to 18.
    let (low, high) = (tables.chunks[0].bounds, tables.chunks[1].bounds);
    assert!((low.z - 10.0).abs() < 0.2 && (high.w - 18.0).abs() < 0.2);
    assert!(low.w < high.w);

    // Moving, turning and scaling the field reuses the bake; a Y scale
    // stretches the ground, as it stretches the mesh.
    *app.world_mut().get_mut::<GlobalTransform>(field).unwrap() = GlobalTransform::from(
        Transform::from_xyz(5.0, 0.0, 3.0)
            .with_rotation(Quat::from_rotation_y(0.5))
            .with_scale(Vec3::new(1.0, 2.0, 1.0)),
    );
    let tables = update(&mut app);
    assert_eq!(bakes(&app), 1, "not baked again");
    assert_eq!(tables.fields[0].extent.z, 2.0);
    assert!((tables.chunks[1].bounds.w - 16.0).abs() < 0.4);

    // A changed mesh is baked again, and the old bake let go.
    app.world_mut()
        .resource_mut::<Assets<Mesh>>()
        .insert(&hill, slope(Vec2::splat(32.0), 2.0))
        .unwrap();
    let tables = update(&mut app);
    assert_eq!(bakes(&app), 1);
    assert!((tables.chunks[1].bounds.w - 4.0).abs() < 0.2);

    // So is a new cell size.
    app.world_mut().get_mut::<GrassField>(field).unwrap().ground = GrassGround::Mesh {
        mesh: hill,
        cell_size: 1.0,
    };
    let tables = update(&mut app);
    assert_eq!(tables.fields[0].map_sizes.zw(), UVec2::splat(32));

    // A field off the mesh lets its bake go.
    app.world_mut().get_mut::<GrassField>(field).unwrap().ground = GrassGround::Flat;
    let tables = update(&mut app);
    assert_eq!(bakes(&app), 0);
    assert!(tables.heights.is_empty());
}

#[test]
fn a_mesh_that_cannot_be_baked_draws_nothing_and_holds_no_other_field_back() {
    let mut app = app();
    let lines = app
        .world_mut()
        .resource_mut::<Assets<Mesh>>()
        .add(Mesh::new(
            PrimitiveTopology::LineList,
            RenderAssetUsages::default(),
        ));
    spawn(&mut app, GrassGround::mesh(lines), Transform::IDENTITY);
    spawn(
        &mut app,
        GrassGround::Flat,
        Transform::from_xyz(100.0, 0.0, 0.0),
    );
    let tables = update(&mut app);
    assert_eq!(tables.slots.len(), 2, "both fields are in the tables");
    assert!(tables.heights.iter().all(|&h| h == HOLE));
    assert!(
        tables
            .chunks
            .iter()
            .all(|chunk| chunk.bounds.z == chunk.bounds.w),
        "holes leave the chunk bounds at the field's height"
    );
}

#[test]
fn a_field_waits_for_its_mesh_to_load() {
    let mut app = app();
    let later = app
        .world_mut()
        .resource_mut::<Assets<Mesh>>()
        .reserve_handle();
    spawn(
        &mut app,
        GrassGround::mesh(later.clone()),
        Transform::IDENTITY,
    );
    assert!(
        update(&mut app).slots.is_empty(),
        "nothing at a wrong height"
    );
    app.world_mut()
        .resource_mut::<Assets<Mesh>>()
        .insert(&later, slope(Vec2::splat(32.0), 1.0))
        .unwrap();
    assert_eq!(update(&mut app).slots.len(), 1);
}
