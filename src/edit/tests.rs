use std::time::Duration;

use bevy::MinimalPlugins;
use bevy::app::{App, PostUpdate};
use bevy::asset::{AssetApp, AssetPlugin};
use bevy::camera::visibility::{InheritedVisibility, Visibility, VisibilitySystems};
use bevy::ecs::system::SystemState;
use bevy::image::Image;
use bevy::math::{Quat, Vec3};
use bevy::time::TimeUpdateStrategy;

use super::*;
use crate::api::{GrassFieldOverrides, GrassStyle};
use crate::fields::MaskKey;

/// Every entity is visible: these tests do not hide fields, except where
/// they say so.
fn visible(mut inherited: Query<(&Visibility, &mut InheritedVisibility)>) {
    for (visibility, mut inherited) in &mut inherited {
        let shown = *visibility != Visibility::Hidden;
        if inherited.get() != shown {
            *inherited = if shown {
                InheritedVisibility::VISIBLE
            } else {
                InheritedVisibility::HIDDEN
            };
        }
    }
}

/// The plugin's main-world systems in their real order, with a clock that
/// advances 0.1 s a frame.
fn app() -> App {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, AssetPlugin::default()))
        .init_asset::<GrassStyle>()
        .init_asset::<Image>()
        .init_asset::<bevy::mesh::Mesh>()
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
            100,
        )))
        .add_systems(
            PostUpdate,
            visible.in_set(VisibilitySystems::VisibilityPropagate),
        )
        .add_plugins(crate::fields::plugin);
    app
}

fn style(app: &mut App, style: GrassStyle) -> Handle<GrassStyle> {
    app.world_mut()
        .resource_mut::<Assets<GrassStyle>>()
        .add(style)
}

/// A field of `size` metres centred on the origin, in the meadow style.
fn spawn(app: &mut App, size: f32, mask: Option<Handle<Image>>) -> Entity {
    let style = style(app, GrassStyle::MEADOW);
    app.world_mut()
        .spawn((
            GrassField {
                size: Vec2::splat(size),
                style,
                density_map: mask,
                ..Default::default()
            },
            GlobalTransform::IDENTITY,
        ))
        .id()
}

/// A square mask, every texel red `red` and green `green`.
fn mask(app: &mut App, side: u32, red: u8, green: u8) -> Handle<Image> {
    let data = [red, green, 0, 255].repeat((side * side) as usize);
    app.world_mut()
        .resource_mut::<Assets<Image>>()
        .add(Image::new(
            Extent3d {
                width: side,
                height: side,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            data,
            TextureFormat::Rgba8Unorm,
            RenderAssetUsages::default(),
        ))
}

/// Runs until the style's `Added` event has been seen and the tables built.
fn settle(app: &mut App) {
    app.update();
    app.update();
}

fn edit(app: &mut App, edit: GrassEdit) {
    app.world_mut().write_message(edit);
    app.update();
}

fn sample(app: &mut App, point: Vec2) -> Option<GrassSample> {
    let mut state = SystemState::<GrassSampler>::new(app.world_mut());
    state.get(app.world()).unwrap().sample(point)
}

fn density(app: &mut App, point: Vec2) -> f32 {
    sample(app, point).map_or(-1.0, |sample| sample.density)
}

fn layer(app: &App, field: Entity) -> &Layer {
    &app.world().resource::<EditLayers>().layers[&field]
}

fn stats(app: &App) -> GrassStats {
    app.world().resource::<GrassStats>().clone()
}

#[test]
fn a_clear_removes_the_grass_under_its_shape_and_nowhere_else() {
    let mut app = app();
    let field = spawn(&mut app, 32.0, None);
    settle(&mut app);
    assert_eq!(density(&mut app, Vec2::ZERO), 1.0);

    edit(
        &mut app,
        GrassEdit::clear(GrassBrushShape::circle(Vec2::new(4.0, -2.0), 2.0)).with_softness(0.0),
    );
    assert_eq!(density(&mut app, Vec2::new(4.0, -2.0)), 0.0);
    assert_eq!(density(&mut app, Vec2::new(5.5, -2.0)), 0.0);
    assert_eq!(density(&mut app, Vec2::new(4.0, 1.0)), 1.0);
    assert_eq!(density(&mut app, Vec2::new(-4.0, -2.0)), 1.0);
    assert_eq!(density(&mut app, Vec2::new(20.0, 0.0)), -1.0, "outside");

    // A field without a mask got one of 0.25 m texels, which the tables now
    // hold.
    assert_eq!(layer(&app, field).size, UVec2::splat(128));
    let tables = app.world().resource::<FieldTables>();
    assert_ne!(tables.fields[0].placement.z, NO_MAP);
    assert_eq!(*tables.mask_ids, vec![MaskKey::Edited(field)]);
}

#[test]
fn a_soft_edge_fades_out() {
    let mut app = app();
    spawn(&mut app, 32.0, None);
    settle(&mut app);
    edit(
        &mut app,
        GrassEdit::clear(GrassBrushShape::circle(Vec2::ZERO, 4.0)).with_softness(1.0),
    );
    let along: Vec<f32> = (0..9)
        .map(|step| density(&mut app, Vec2::new(step as f32 * 0.5, 0.0)))
        .collect();
    assert!(along[0] < 0.05, "{along:?}");
    assert!(along.windows(2).all(|pair| pair[1] >= pair[0]), "{along:?}");
    assert!(along[4] > 0.3 && along[4] < 0.7, "{along:?}");
    assert_eq!(density(&mut app, Vec2::new(4.5, 0.0)), 1.0);
}

#[test]
fn the_first_edit_of_a_masked_field_moves_nothing_in_the_tables() {
    let mut app = app();
    let image = mask(&mut app, 256, 255, 255);
    spawn(&mut app, 64.0, Some(image));
    settle(&mut app);
    let before = stats(&app).tables;
    let version = app.world().resource::<FieldTables>().version;

    edit(
        &mut app,
        GrassEdit::clear(GrassBrushShape::circle(Vec2::ZERO, 1.0)),
    );
    assert_eq!(density(&mut app, Vec2::ZERO), 0.0);
    assert_eq!(
        app.world().resource::<FieldTables>().version,
        version,
        "an edit is a patch, not a rebuild"
    );
    assert_eq!(stats(&app).tables, before);

    // Only the rows the edit touched go up: a 2 m disc on 0.25 m texels is
    // 8 rows of 8 texels, 4 words each.
    let patches = app.world().resource::<MaskPatches>().0.clone();
    assert_eq!(patches.runs.len(), 8);
    assert!(
        patches.words.len() <= 8 * 5,
        "{} words",
        patches.words.len()
    );
    assert_eq!(
        stats(&app).edits.bytes_uploaded,
        patches.words.len() as u64 * 4
    );

    app.update();
    assert!(
        app.world().resource::<MaskPatches>().0.is_empty(),
        "a quiet frame uploads nothing"
    );
}

/// What the render world does with the tables and patches: a full upload
/// when the layout changes, then each frame's patches.
#[derive(Default)]
struct Gpu {
    structure: Option<u64>,
    masks: Vec<u32>,
}

impl Gpu {
    fn frame(&mut self, app: &App) {
        let tables = app.world().resource::<FieldTables>();
        if self.structure != Some(tables.structure_version) {
            self.structure = Some(tables.structure_version);
            self.masks = tables.masks.as_ref().clone();
        }
        let patches = &app.world().resource::<MaskPatches>().0;
        let mut words = patches.words.as_slice();
        for &(at, len) in &patches.runs {
            let (run, rest) = words.split_at(len as usize);
            words = rest;
            self.masks[at as usize..(at + len) as usize].copy_from_slice(run);
        }
    }

    /// Asserts that the GPU's copy of each field's mask is its layer.
    fn check(&self, app: &App, step: &str) {
        let tables = app.world().resource::<FieldTables>();
        let edits = app.world().resource::<EditLayers>();
        for slot in tables.slots.iter() {
            let Some(layer) = edits.layers.get(&slot.entity) else {
                continue;
            };
            let at = (tables.fields[slot.first].placement.z / 2) as usize;
            assert!(
                self.masks[at..at + layer.words.len()] == layer.words[..],
                "{step}: the GPU's mask differs from the edits"
            );
        }
    }
}

#[test]
fn the_gpu_always_holds_exactly_the_edited_masks() {
    let mut app = app();
    let mut gpu = Gpu::default();
    // Odd sizes, so rows share words.
    let first = mask(&mut app, 99, 200, 255);
    let second = mask(&mut app, 37, 255, 128);
    let a = spawn(&mut app, 30.0, Some(first));
    let b = spawn(&mut app, 20.0, Some(second));
    let c = spawn(&mut app, 16.0, None);
    let height_map = mask(&mut app, 8, 128, 0);
    app.world_mut().get_mut::<GrassField>(c).unwrap().ground = crate::api::GrassGround::Heightmap {
        image: height_map,
        height_scale: 2.0,
    };
    for _ in 0..2 {
        app.update();
        gpu.frame(&app);
    }

    let step = |app: &mut App, gpu: &mut Gpu, what: &str, change: &dyn Fn(&mut App)| {
        change(app);
        app.update();
        gpu.frame(app);
        gpu.check(app, what);
    };
    step(&mut app, &mut gpu, "a clear", &|app| {
        app.world_mut()
            .write_message(GrassEdit::clear(GrassBrushShape::circle(Vec2::ZERO, 3.0)));
    });
    step(&mut app, &mut gpu, "a stroke", &|app| {
        app.world_mut().write_message(GrassEdit::mow(
            GrassBrushShape::stroke(Vec2::new(-9.0, -9.0), Vec2::new(9.0, 7.0), 1.0),
            0.25,
        ));
    });
    // A style change rewrites entries only, and keeps the old masks.
    step(&mut app, &mut gpu, "a style change", &|app| {
        let handle = app.world().get::<GrassField>(a).unwrap().style.clone();
        app.world_mut()
            .resource_mut::<Assets<GrassStyle>>()
            .get_mut(&handle)
            .unwrap()
            .blade
            .tip_color = bevy::color::Color::WHITE;
    });
    step(
        &mut app,
        &mut gpu,
        "an edit after the style change",
        &|app| {
            app.world_mut().write_message(GrassEdit::paint(
                GrassBrushShape::rect(Vec2::new(2.0, 1.0), Vec2::new(3.0, 1.0), 0.6),
                0.4,
            ));
        },
    );
    // Moving a field up uploads the chunks, and with them every mask.
    step(&mut app, &mut gpu, "a field moving up", &|app| {
        *app.world_mut().get_mut::<GlobalTransform>(c).unwrap() =
            GlobalTransform::from_xyz(0.0, 1.0, 0.0);
    });
    step(&mut app, &mut gpu, "a restore", &|app| {
        app.world_mut()
            .write_message(GrassEdit::restore(GrassBrushShape::circle(
                Vec2::new(1.0, 0.0),
                2.0,
            )));
    });
    // A new field shifts every mask after it.
    step(&mut app, &mut gpu, "a new field", &|app| {
        spawn(app, 8.0, None);
    });
    step(&mut app, &mut gpu, "an edit after the shift", &|app| {
        app.world_mut()
            .write_message(GrassEdit::clear(GrassBrushShape::circle(Vec2::ZERO, 5.0)));
    });
    step(&mut app, &mut gpu, "a field despawned", &|app| {
        app.world_mut().despawn(b);
    });
    step(&mut app, &mut gpu, "the last edit", &|app| {
        app.world_mut().write_message(GrassEdit::paint(
            GrassBrushShape::circle(Vec2::new(-3.0, 3.0), 2.0),
            0.7,
        ));
    });
    let edits = app.world().resource::<EditLayers>();
    assert!(edits.layers.contains_key(&a) && edits.layers.contains_key(&c));
    assert!(
        !edits.layers.contains_key(&b),
        "a despawned field's edits are dropped"
    );
}

#[test]
fn mowing_lowers_the_height_only_and_never_raises_it() {
    let mut app = app();
    let image = mask(&mut app, 64, 255, 255);
    let field = spawn(&mut app, 16.0, Some(image));
    settle(&mut app);
    let everywhere = GrassBrushShape::circle(Vec2::ZERO, 20.0);
    edit(&mut app, GrassEdit::mow(everywhere, 0.3));
    let words = &layer(&app, field).words;
    assert!(
        (0..64 * 64).all(|index| texel(words, index) == (255, 77)),
        "density stays, height is cut to 0.3"
    );
    edit(&mut app, GrassEdit::mow(everywhere, 0.6));
    assert!(
        (0..64 * 64).all(|index| texel(&layer(&app, field).words, index) == (255, 77)),
        "mowing higher than the grass does nothing"
    );
    let sampled = sample(&mut app, Vec2::ZERO).unwrap();
    assert!((sampled.height_scale - 77.0 / 255.0).abs() < 1e-6);
    assert_eq!(sampled.density, 1.0);
}

#[test]
fn restore_returns_to_the_authored_mask() {
    let mut app = app();
    let image = mask(&mut app, 50, 180, 90);
    let field = spawn(&mut app, 16.0, Some(image.clone()));
    settle(&mut app);
    let authored = layer_words_of_image(&mut app, &image);
    let everywhere = GrassBrushShape::circle(Vec2::ZERO, 20.0);
    edit(&mut app, GrassEdit::clear(everywhere));
    edit(
        &mut app,
        GrassEdit::new(everywhere, GrassEditOp::Height(1.0)),
    );
    assert_ne!(layer(&app, field).words, authored);
    edit(&mut app, GrassEdit::restore(everywhere));
    assert_eq!(layer(&app, field).words, authored);
}

fn layer_words_of_image(app: &mut App, image: &Handle<Image>) -> Vec<u32> {
    let image = app
        .world()
        .resource::<Assets<Image>>()
        .get(image)
        .unwrap()
        .clone();
    let mut cache = PackedImages::default();
    cache.mask(AssetId::default(), &image).as_ref().clone()
}

#[test]
fn weak_edits_add_up_and_land_exactly_on_the_target() {
    let mut app = app();
    let field = spawn(&mut app, 16.0, None);
    settle(&mut app);
    let everywhere = GrassBrushShape::circle(Vec2::ZERO, 20.0);
    let mean = |app: &App| {
        let layer = layer(app, field);
        let count = layer.size.x * layer.size.y;
        (0..count)
            .map(|index| f32::from(texel(&layer.words, index).0))
            .sum::<f32>()
            / count as f32
    };
    // A step of 0.255 texel units: less than one, and still felt.
    for _ in 0..400 {
        app.world_mut()
            .write_message(GrassEdit::clear(everywhere).with_strength(0.001));
    }
    app.update();
    let worn = mean(&app);
    assert!((worn - (255.0 - 102.0)).abs() < 3.0, "mean {worn}");

    for _ in 0..12 {
        app.world_mut()
            .write_message(GrassEdit::paint(everywhere, 0.5).with_strength(0.1));
    }
    app.update();
    let layer = layer(&app, field);
    assert!(
        (0..layer.size.x * layer.size.y).all(|index| texel(&layer.words, index).0 == 128),
        "every texel ends exactly on the target"
    );
}

#[test]
fn edits_with_impossible_numbers_do_nothing() {
    let mut app = app();
    let field = spawn(&mut app, 16.0, None);
    settle(&mut app);
    let everywhere = GrassBrushShape::circle(Vec2::ZERO, 20.0);
    edit(&mut app, GrassEdit::paint(everywhere, f32::NAN));
    edit(
        &mut app,
        GrassEdit::clear(everywhere).with_strength(f32::NAN),
    );
    edit(
        &mut app,
        GrassEdit::clear(GrassBrushShape::circle(Vec2::NAN, 20.0)),
    );
    edit(
        &mut app,
        GrassEdit::clear(GrassBrushShape::rect(Vec2::ZERO, Vec2::ONE, f32::INFINITY)),
    );
    assert!(
        !app.world()
            .resource::<EditLayers>()
            .layers
            .contains_key(&field)
    );
    assert_eq!(density(&mut app, Vec2::ZERO), 1.0);
}

#[test]
fn a_turned_field_is_edited_where_it_lies() {
    let mut app = app();
    let field = spawn(&mut app, 0.0, None);
    // 40 × 8 m, turned a quarter: it runs along Z.
    app.world_mut().get_mut::<GrassField>(field).unwrap().size = Vec2::new(40.0, 8.0);
    *app.world_mut().get_mut::<GlobalTransform>(field).unwrap() = GlobalTransform::from(
        Transform::from_rotation(Quat::from_rotation_y(std::f32::consts::FRAC_PI_2)),
    );
    settle(&mut app);
    edit(
        &mut app,
        GrassEdit::clear(GrassBrushShape::circle(Vec2::new(0.0, 15.0), 1.5)).with_softness(0.0),
    );
    assert_eq!(density(&mut app, Vec2::new(0.0, 15.0)), 0.0);
    assert_eq!(density(&mut app, Vec2::new(0.0, -15.0)), 1.0);
    assert_eq!(density(&mut app, Vec2::new(15.0, 0.0)), -1.0, "outside");
}

#[test]
fn a_rect_clears_a_turned_plot() {
    let mut app = app();
    spawn(&mut app, 32.0, None);
    settle(&mut app);
    // 8 × 2 m, turned a quarter: it runs along Z.
    let yaw = std::f32::consts::FRAC_PI_2;
    edit(
        &mut app,
        GrassEdit::clear(GrassBrushShape::rect(Vec2::ZERO, Vec2::new(4.0, 1.0), yaw))
            .with_softness(0.0),
    );
    assert_eq!(density(&mut app, Vec2::new(0.0, 3.5)), 0.0);
    assert_eq!(density(&mut app, Vec2::new(0.0, -3.5)), 0.0);
    assert_eq!(density(&mut app, Vec2::new(3.5, 0.0)), 1.0);
}

#[test]
fn edits_wait_for_a_mask_that_has_not_loaded() {
    let mut app = app();
    let handle = app.world().resource::<Assets<Image>>().reserve_handle();
    let field = spawn(&mut app, 16.0, Some(handle.clone()));
    settle(&mut app);
    let here = GrassBrushShape::circle(Vec2::ZERO, 2.0);
    edit(&mut app, GrassEdit::clear(here));
    edit(&mut app, GrassEdit::paint(here, 0.5).with_softness(0.0));
    assert_eq!(stats(&app).edits.waiting, 2);

    let loaded = mask(&mut app, 64, 255, 255);
    let image = app
        .world()
        .resource::<Assets<Image>>()
        .get(&loaded)
        .unwrap()
        .clone();
    app.world_mut()
        .resource_mut::<Assets<Image>>()
        .insert(&handle, image)
        .unwrap();
    settle(&mut app);
    assert_eq!(stats(&app).edits.waiting, 0);
    // Applied in order: cleared, then painted to half.
    let words = &layer(&app, field).words;
    assert_eq!(texel(words, 32 * 64 + 32).0, 128);
}

#[test]
fn a_new_density_map_starts_the_field_again() {
    let mut app = app();
    let image = mask(&mut app, 64, 255, 255);
    let field = spawn(&mut app, 16.0, Some(image.clone()));
    settle(&mut app);
    edit(
        &mut app,
        GrassEdit::clear(GrassBrushShape::circle(Vec2::ZERO, 2.0)),
    );
    assert_eq!(density(&mut app, Vec2::ZERO), 0.0);

    // The image itself changed: reloaded, or edited by the game.
    app.world_mut()
        .resource_mut::<Assets<Image>>()
        .get_mut(&image)
        .unwrap()
        .into_inner();
    settle(&mut app);
    assert_eq!(density(&mut app, Vec2::ZERO), 1.0);
    assert!(
        !app.world()
            .resource::<EditLayers>()
            .layers
            .contains_key(&field)
    );

    // Another image.
    edit(
        &mut app,
        GrassEdit::clear(GrassBrushShape::circle(Vec2::ZERO, 2.0)),
    );
    let other = mask(&mut app, 32, 100, 255);
    app.world_mut()
        .get_mut::<GrassField>(field)
        .unwrap()
        .density_map = Some(other);
    settle(&mut app);
    assert!((density(&mut app, Vec2::ZERO) - 100.0 / 255.0).abs() < 1e-6);
}

#[test]
fn a_brush_sweeps_its_path_without_gaps_and_rests_without_work() {
    let mut app = app();
    let field = spawn(&mut app, 64.0, None);
    let brush = app
        .world_mut()
        .spawn((
            GrassBrush {
                softness: 0.0,
                ..GrassBrush::new(GrassEditOp::Density(0.0), 0.5)
            },
            GlobalTransform::from_xyz(-20.0, 0.0, 0.0),
        ))
        .id();
    settle(&mut app);
    // 3 m a frame, six times its width.
    for step in 1..=10 {
        *app.world_mut().get_mut::<GlobalTransform>(brush).unwrap() =
            GlobalTransform::from_xyz(-20.0 + 3.0 * step as f32, 0.0, 0.0);
        app.update();
    }
    for step in 0..=120 {
        let x = -20.0 + step as f32 * 0.25;
        assert_eq!(density(&mut app, Vec2::new(x, 0.0)), 0.0, "a gap at {x}");
    }
    assert_eq!(density(&mut app, Vec2::new(0.0, 1.0)), 1.0);

    let changed = stats(&app).edits.texels_changed;
    let applied = stats(&app).edits.applied;
    for _ in 0..5 {
        app.update();
    }
    assert_eq!(
        stats(&app).edits.applied,
        applied,
        "a resting brush does nothing"
    );
    assert_eq!(stats(&app).edits.texels_changed, changed);

    // Teleported: it edits where it lands, not along the way.
    *app.world_mut().get_mut::<GlobalTransform>(brush).unwrap() =
        GlobalTransform::from_xyz(10.0, 0.0, 25.0);
    app.update();
    assert_eq!(density(&mut app, Vec2::new(10.0, 25.0)), 0.0);
    assert_eq!(density(&mut app, Vec2::new(10.0, 12.0)), 1.0);
    assert!(
        app.world()
            .resource::<EditLayers>()
            .layers
            .contains_key(&field)
    );
}

#[test]
fn a_swept_soft_brush_fades_evenly_along_its_path() {
    let mut app = app();
    let field = spawn(&mut app, 32.0, None);
    let brush = app
        .world_mut()
        .spawn((
            GrassBrush {
                softness: 1.0,
                ..GrassBrush::new(GrassEditOp::Density(0.0), 1.0)
            },
            GlobalTransform::from_xyz(-10.0, 0.0, 0.0),
        ))
        .id();
    settle(&mut app);
    for step in 1..=40 {
        *app.world_mut().get_mut::<GlobalTransform>(brush).unwrap() =
            GlobalTransform::from_xyz(-10.0 + 0.5 * step as f32, 0.0, 0.0);
        app.update();
    }
    // Half a metre off the centreline, along the path: one value, not a
    // bead at every frame's joint.
    let layer = layer(&app, field);
    let row = 64 + 2;
    let values: Vec<u8> = (32..64)
        .map(|x| texel(&layer.words, row * 128 + x).0)
        .collect();
    let (low, high) = values
        .iter()
        .fold((u8::MAX, 0), |(low, high), v| (low.min(*v), high.max(*v)));
    assert!(high - low <= 1, "{values:?}");
}

#[test]
fn a_brush_with_a_rate_works_for_as_long_as_it_stays() {
    let mut app = app();
    spawn(&mut app, 32.0, None);
    app.world_mut().spawn((
        GrassBrush {
            softness: 0.0,
            ..GrassBrush::new(GrassEditOp::Density(0.0), 1.0).with_rate(0.5)
        },
        GlobalTransform::IDENTITY,
    ));
    settle(&mut app);
    // 0.1 s a frame: a second.
    let start = density(&mut app, Vec2::ZERO);
    for _ in 0..10 {
        app.update();
    }
    let after = density(&mut app, Vec2::ZERO);
    assert!(start > 0.85, "{start}");
    assert!((after - (start - 0.5)).abs() < 0.03, "{start} then {after}");
    for _ in 0..20 {
        app.update();
    }
    assert_eq!(density(&mut app, Vec2::ZERO), 0.0);
}

#[test]
fn regrowth_raises_the_grass_back_to_its_mask_and_stops() {
    let mut app = app();
    let image = mask(&mut app, 64, 200, 255);
    let field = spawn(&mut app, 16.0, Some(image));
    app.world_mut().entity_mut(field).insert(GrassRegrowth {
        density_rate: 0.25,
        height_rate: 0.0,
    });
    settle(&mut app);
    let here = GrassBrushShape::circle(Vec2::ZERO, 2.0);
    let there = GrassBrushShape::circle(Vec2::new(5.0, 5.0), 1.0);
    app.world_mut()
        .write_message(GrassEdit::clear(here).with_softness(0.0));
    app.world_mut()
        .write_message(GrassEdit::mow(here, 0.5).with_softness(0.0));
    edit(&mut app, GrassEdit::paint(there, 1.0).with_softness(0.0));
    // Two seconds at 0.25 a second: half way back to 200.
    for _ in 0..20 {
        app.update();
    }
    let regrown = density(&mut app, Vec2::ZERO);
    assert!((regrown - 0.5).abs() < 0.05, "{regrown}");
    for _ in 0..20 {
        app.update();
    }
    let sampled = sample(&mut app, Vec2::ZERO).unwrap();
    assert!(
        (sampled.density - 200.0 / 255.0).abs() < 1e-6,
        "back to the mask"
    );
    assert!(
        (sampled.height_scale - 0.5).abs() < 0.01,
        "the height does not regrow at a rate of 0"
    );
    assert_eq!(
        density(&mut app, Vec2::new(5.0, 5.0)),
        1.0,
        "painted grass is not cut back"
    );
    assert!(!layer(&app, field).any_lowered, "regrowth stopped looking");

    // Turning the height's regrowth on looks at every tile again.
    app.world_mut()
        .get_mut::<GrassRegrowth>(field)
        .unwrap()
        .height_rate = 1.0;
    for _ in 0..10 {
        app.update();
    }
    assert_eq!(sample(&mut app, Vec2::ZERO).unwrap().height_scale, 1.0);
}

#[test]
fn the_sampler_reads_what_each_field_grows() {
    let mut app = app();
    let image = mask(&mut app, 16, 255, 128);
    let meadow = spawn(&mut app, 16.0, Some(image));
    let tall = spawn(&mut app, 4.0, None);
    let tall_style = style(&mut app, GrassStyle::TALL_PRAIRIE);
    app.world_mut().entity_mut(tall).insert((
        GrassFieldOverrides {
            height_scale: 2.0,
            density_scale: 0.5,
            ..Default::default()
        },
        GlobalTransform::from_xyz(2.0, 0.0, 0.0),
    ));
    app.world_mut().get_mut::<GrassField>(tall).unwrap().style = tall_style;
    let hidden = spawn(&mut app, 4.0, None);
    app.world_mut().entity_mut(hidden).insert((
        Visibility::Hidden,
        GlobalTransform::from_xyz(-5.0, 0.0, 5.0),
    ));
    settle(&mut app);

    let sampled = sample(&mut app, Vec2::new(-5.0, -5.0)).unwrap();
    let meadow_blade = GrassStyle::MEADOW.blade;
    assert_eq!(sampled.field, meadow);
    assert_eq!(sampled.density, 1.0);
    assert!((sampled.height_scale - 128.0 / 255.0).abs() < 1e-6);
    let typical = f32::midpoint(meadow_blade.height.x, meadow_blade.height.y);
    assert!((sampled.height - typical * 128.0 / 255.0).abs() < 1e-5);
    assert!((sampled.blades_per_m2 - GrassStyle::MEADOW.density).abs() < 1e-3);

    // Where the two overlap, the taller grass is the answer, and both are
    // there to read.
    let sampled = sample(&mut app, Vec2::new(2.0, 0.0)).unwrap();
    assert_eq!(sampled.field, tall);
    let prairie = GrassStyle::TALL_PRAIRIE;
    assert!((sampled.blades_per_m2 - prairie.density * 0.5).abs() < 1e-3);
    assert!((sampled.tallest - prairie.blade.height.y * 2.0).abs() < 1e-5);
    let mut state = SystemState::<GrassSampler>::new(app.world_mut());
    let both = state
        .get(app.world())
        .unwrap()
        .samples(Vec2::new(2.0, 0.0))
        .count();
    assert_eq!(both, 2);

    // Cleared grass is no cover, however tall it would be.
    edit(
        &mut app,
        GrassEdit::clear(GrassBrushShape::circle(Vec2::new(2.0, 0.0), 0.5)).on_field(tall),
    );
    assert_eq!(sample(&mut app, Vec2::new(2.0, 0.0)).unwrap().field, meadow);

    // A hidden field has no grass.
    assert_eq!(
        sample(&mut app, Vec2::new(-5.0, 5.0)).unwrap().field,
        meadow
    );
    let mut state = SystemState::<GrassSampler>::new(app.world_mut());
    let hidden_too = state
        .get(app.world())
        .unwrap()
        .samples(Vec2::new(-5.0, 5.0))
        .count();
    assert_eq!(hidden_too, 1);
}

#[test]
fn the_sampler_sees_bare_patches() {
    let mut app = app();
    let mut patchy = GrassStyle::MEADOW;
    patchy.variation.bare_share = 0.5;
    patchy.variation.bare_patch_size = 2.0;
    let field = spawn(&mut app, 32.0, None);
    let handle = style(&mut app, patchy);
    app.world_mut().get_mut::<GrassField>(field).unwrap().style = handle;
    settle(&mut app);
    let bare = (0..400)
        .filter(|i| {
            let point = Vec2::new((i % 20) as f32, (i / 20) as f32) * 1.5 - 14.0;
            density(&mut app, point) < 0.5
        })
        .count();
    assert!((120..280).contains(&bare), "{bare} of 400 points bare");
}

#[test]
fn a_mask_image_holds_the_edits_and_reads_back_the_same() {
    let mut app = app();
    let field = spawn(&mut app, 16.0, None);
    settle(&mut app);
    let mut state = SystemState::<GrassSampler>::new(app.world_mut());
    assert!(state.get(app.world()).unwrap().mask_image(field).is_none());

    edit(
        &mut app,
        GrassEdit::mow(GrassBrushShape::circle(Vec2::new(2.0, 3.0), 3.0), 0.4),
    );
    let mut state = SystemState::<GrassSampler>::new(app.world_mut());
    let image = state.get(app.world()).unwrap().mask_image(field).unwrap();
    assert_eq!(image.size(), UVec2::splat(64));
    let mut cache = PackedImages::default();
    let read = cache.mask(AssetId::default(), &image);
    assert_eq!(*read, layer(&app, field).words);
}

#[test]
fn a_despawned_or_unused_field_leaves_nothing_behind() {
    let mut app = app();
    let handle = app.world().resource::<Assets<Image>>().reserve_handle();
    let field = spawn(&mut app, 16.0, Some(handle));
    settle(&mut app);
    edit(
        &mut app,
        GrassEdit::clear(GrassBrushShape::circle(Vec2::ZERO, 1.0)),
    );
    assert_eq!(stats(&app).edits.waiting, 1);
    app.world_mut().despawn(field);
    settle(&mut app);
    assert_eq!(stats(&app).edits.waiting, 0);
    assert_eq!(stats(&app).edits.edited_fields, 0);
}

#[test]
fn toward_steps_at_random_in_proportion_and_settles_on_the_goal() {
    assert_eq!(toward(100, 100.0, 5.0, 0.9), 100);
    assert_eq!(toward(100, 0.0, 5.0, 0.0), 95);
    assert_eq!(toward(100, 255.0, 5.0, 0.0), 105);
    assert_eq!(toward(100, 102.0, 5.0, 0.99), 102, "lands on the goal");
    assert_eq!(toward(100, 127.5, 255.0, 0.0), 128);
    // A step of 0.3: one unit three times in ten.
    let moved = (0..1000)
        .filter(|i| toward(100, 0.0, 0.3, *i as f32 / 1000.0) == 99)
        .count();
    assert!((295..=305).contains(&moved), "{moved}");
    assert_eq!(toward(0, -50.0, 255.0, 0.5), 0);
    assert_eq!(toward(255, 400.0, 255.0, 0.5), 255);
}

#[test]
fn patches_join_rows_that_touch() {
    let mut list = PatchList::default();
    list.push(10, 0, &[1, 2, 3]);
    // An odd-width row starts in the word the last one ended in.
    list.push(10, 2, &[3, 4]);
    list.push(10, 9, &[5]);
    assert_eq!(list.runs, vec![(10, 4), (19, 1)]);
    assert_eq!(list.words, vec![1, 2, 3, 4, 5]);
}

#[test]
fn shapes_weigh_points_as_documented() {
    let edit = |shape| Edit::of(&GrassEdit::clear(shape).with_softness(0.5)).unwrap();
    let disc = edit(GrassBrushShape::circle(Vec2::ZERO, 2.0));
    assert_eq!(disc.weight(Vec2::new(0.9, 0.0)), 1.0);
    assert!((disc.weight(Vec2::new(1.5, 0.0)) - 0.5).abs() < 1e-6);
    assert_eq!(disc.weight(Vec2::new(2.1, 0.0)), 0.0);

    let stroke = edit(GrassBrushShape::stroke(
        Vec2::ZERO,
        Vec2::new(4.0, 0.0),
        1.0,
    ));
    assert_eq!(stroke.weight(Vec2::new(2.0, 0.4)), 1.0);
    assert_eq!(stroke.weight(Vec2::new(-0.3, 0.0)), 1.0, "a round end");
    assert_eq!(stroke.weight(Vec2::new(2.0, 1.1)), 0.0);

    let rect = edit(GrassBrushShape::rect(Vec2::ZERO, Vec2::new(4.0, 1.0), 0.0));
    assert_eq!(rect.weight(Vec2::new(3.4, 0.0)), 1.0);
    assert!((rect.weight(Vec2::new(0.0, 0.75)) - 0.5).abs() < 1e-6);
    assert_eq!(rect.weight(Vec2::new(0.0, 1.1)), 0.0);
    let (low, high) = rect.shape.bounds();
    assert_eq!((low, high), (Vec2::new(-4.0, -1.0), Vec2::new(4.0, 1.0)));

    // Turned as `Quat::from_rotation_y` turns a building.
    let yaw = 0.7;
    let turned = edit(GrassBrushShape::rect(Vec2::ZERO, Vec2::new(4.0, 1.0), yaw));
    let tip = Quat::from_rotation_y(yaw) * Vec3::new(3.0, 0.0, 0.0);
    assert_eq!(turned.weight(tip.xz()), 1.0);
}

#[test]
fn a_full_strength_edit_twice_is_the_same_edit_once() {
    let mut app = app();
    let field = spawn(&mut app, 16.0, None);
    settle(&mut app);
    let soft = GrassEdit::clear(GrassBrushShape::circle(Vec2::ZERO, 3.0)).with_softness(1.0);
    edit(&mut app, soft);
    let once = layer(&app, field).words.clone();
    edit(&mut app, soft);
    assert_eq!(layer(&app, field).words, once);
    // A weaker edit, on the other hand, adds up.
    edit(&mut app, soft.with_strength(0.5));
    edit(&mut app, soft.with_strength(0.5));
    assert_ne!(layer(&app, field).words, once);
}

#[test]
fn a_profile_never_moves_grass_away_from_the_target() {
    // Lowering toward 0: a ceiling from 255 at weight 0 to 0 at weight 1.
    assert_eq!(limit(255, 0.0, 0.25), 191);
    assert_eq!(limit(100, 0.0, 0.25), 100, "already below the ceiling");
    assert_eq!(limit(255, 0.0, 1.0), 0);
    // Raising toward 200: a floor from 0 at weight 0 to 200 at weight 1.
    assert_eq!(limit(0, 200.0, 0.5), 100);
    assert_eq!(limit(150, 200.0, 0.5), 150);
    assert_eq!(limit(10, 127.5, 1.0), 128);
    assert_eq!(limit(255, 127.5, 1.0), 128);
}

#[test]
fn a_slid_rectangle_is_as_deep_as_its_deepest_moment() {
    // Against trying every moment of the slide.
    let mut seed = 7u32;
    let mut random = |low: f32, high: f32| {
        seed = hash2(seed, 0x51d);
        low + unit(seed) * (high - low)
    };
    for _ in 0..2000 {
        let yaw = random(0.0, std::f32::consts::TAU);
        let axis = Vec2::new(yaw.cos(), -yaw.sin());
        let half = Vec2::new(random(0.1, 3.0), random(0.1, 3.0));
        let from = Vec2::new(random(-2.0, 2.0), random(-2.0, 2.0));
        let to = from + Vec2::new(random(-4.0, 4.0), random(-4.0, 4.0));
        let point = Vec2::new(random(-6.0, 6.0), random(-6.0, 6.0));
        let tried = (0..=4000)
            .map(|step| {
                let center = from.lerp(to, step as f32 / 4000.0);
                let offset = point - center;
                let q = Vec2::new(offset.dot(axis).abs(), offset.dot(axis.perp()).abs());
                (half - q).min_element()
            })
            .fold(f32::MIN, f32::max);
        let exact = slid_depth(point, (from, to), half, axis);
        assert!(exact >= tried - 1e-4, "{exact} below {tried}");
        assert!(exact - tried < 3e-3, "{exact} above {tried}");
    }
}

/// A box brush that clears, at `transform`.
fn spawn_box_brush(app: &mut App, half_size: Vec2, transform: Transform) -> Entity {
    app.world_mut()
        .spawn((
            GrassBrush {
                softness: 0.0,
                ..GrassBrush::new(GrassEditOp::Density(0.0), 0.0)
            },
            GrassBrushFootprint::new(half_size),
            GlobalTransform::from(transform),
        ))
        .id()
}

#[test]
fn a_box_brush_edits_the_turned_and_scaled_rectangle_under_it() {
    let mut app = app();
    spawn(&mut app, 32.0, None);
    // 2 × 0.5 m, scaled to 4 × 1 m, turned a quarter: it runs along Z.
    spawn_box_brush(
        &mut app,
        Vec2::new(1.0, 0.25),
        Transform::from_xyz(3.0, 0.5, 2.0)
            .with_rotation(Quat::from_rotation_y(std::f32::consts::FRAC_PI_2))
            .with_scale(Vec3::splat(2.0)),
    );
    settle(&mut app);
    assert_eq!(
        density(&mut app, Vec2::new(3.0, 3.5)),
        0.0,
        "along its length"
    );
    assert_eq!(density(&mut app, Vec2::new(3.0, 0.5)), 0.0);
    assert_eq!(density(&mut app, Vec2::new(3.0, 4.6)), 1.0, "past its end");
    assert_eq!(density(&mut app, Vec2::new(4.0, 2.0)), 1.0, "past its side");
}

#[test]
fn a_fast_box_brush_leaves_a_swath_without_gaps() {
    let mut app = app();
    spawn(&mut app, 64.0, None);
    // A deck 2 m wide across its path, 0.5 m long, moving 3 m a frame along
    // X while facing along X.
    let brush = spawn_box_brush(
        &mut app,
        Vec2::new(0.25, 1.0),
        Transform::from_xyz(-20.0, 0.0, 0.0),
    );
    settle(&mut app);
    for step in 1..=10 {
        *app.world_mut().get_mut::<GlobalTransform>(brush).unwrap() =
            GlobalTransform::from_xyz(-20.0 + 3.0 * step as f32, 0.0, 0.0);
        app.update();
    }
    for step in 0..=120 {
        let x = -20.0 + step as f32 * 0.25;
        for z in [-0.75, 0.0, 0.75] {
            assert_eq!(density(&mut app, Vec2::new(x, z)), 0.0, "a gap at {x}, {z}");
        }
    }
    assert_eq!(
        density(&mut app, Vec2::new(0.0, 1.25)),
        1.0,
        "past its side"
    );
    assert_eq!(density(&mut app, Vec2::new(10.5, 0.0)), 1.0, "ahead of it");
}

#[test]
fn a_box_brush_turning_in_place_edits_its_new_heading() {
    let mut app = app();
    spawn(&mut app, 32.0, None);
    let brush = spawn_box_brush(&mut app, Vec2::new(2.0, 0.25), Transform::IDENTITY);
    settle(&mut app);
    assert_eq!(density(&mut app, Vec2::new(0.0, 1.5)), 1.0);
    *app.world_mut().get_mut::<GlobalTransform>(brush).unwrap() = GlobalTransform::from(
        Transform::from_rotation(Quat::from_rotation_y(std::f32::consts::FRAC_PI_2)),
    );
    app.update();
    assert_eq!(density(&mut app, Vec2::new(0.0, 1.5)), 0.0);
    assert_eq!(
        density(&mut app, Vec2::new(1.5, 0.0)),
        0.0,
        "and its old one stays"
    );
}

#[test]
fn a_box_brush_with_a_rate_works_where_it_stands() {
    let mut app = app();
    spawn(&mut app, 32.0, None);
    app.world_mut().spawn((
        GrassBrush {
            softness: 0.0,
            ..GrassBrush::new(GrassEditOp::Density(0.0), 0.0).with_rate(0.5)
        },
        GrassBrushFootprint::new(Vec2::new(2.0, 1.0)),
        GlobalTransform::IDENTITY,
    ));
    settle(&mut app);
    let start = density(&mut app, Vec2::new(1.5, 0.0));
    for _ in 0..10 {
        app.update();
    }
    let after = density(&mut app, Vec2::new(1.5, 0.0));
    assert!((after - (start - 0.5)).abs() < 0.03, "{start} then {after}");
    assert_eq!(density(&mut app, Vec2::new(0.0, 1.5)), 1.0, "outside");
}
