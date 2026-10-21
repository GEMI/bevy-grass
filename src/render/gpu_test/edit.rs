use super::*;
use crate::edit::EditLayers;
use crate::{GrassBrushShape, GrassEdit};

/// The mask buffer the shaders read, as the GPU holds it.
fn gpu_masks(app: &mut App) -> Vec<u32> {
    let world = app.sub_app_mut(RenderApp).world_mut();
    let device = world.resource::<RenderDevice>().clone();
    let queue = world.resource::<RenderQueue>().clone();
    let shared = world.resource::<GrassShared>();
    let buffer = shared.masks_buffer().expect("a mask buffer");
    read_buffer(&device, &queue, buffer, buffer.size())
}

#[test]
#[ignore = "needs a GPU: cargo test -- --ignored gpu"]
fn gpu_an_edit_reaches_the_mask_the_shaders_read_the_same_frame() {
    let mut app = headless_app();
    wait_for_grass(&mut app);
    let at = Vec2::new(2.0, 14.0);
    let radius = 3.0;
    let inside = |app: &mut App| {
        gpu_bends(app)
            .into_iter()
            .filter(|(root, _)| root.distance(at) < radius - 0.5)
            .count()
    };
    assert!(inside(&mut app) > 100, "the test spot should be in view");

    // The field has no mask: its first edit gives it one, which takes a
    // rebuild; the frame after, the edit is drawn.
    app.world_mut()
        .write_message(GrassEdit::clear(GrassBrushShape::circle(at, radius)).with_softness(0.0));
    app.update();
    assert_eq!(inside(&mut app), 0, "blades are left in the cleared disc");

    // A second edit is a patch of a few rows, drawn the frame it is made.
    let other = Vec2::new(-2.0, 12.0);
    let near_other = |app: &mut App| {
        gpu_bends(app)
            .into_iter()
            .filter(|(root, _)| root.distance(other) < 1.5)
            .count()
    };
    assert!(near_other(&mut app) > 20);
    app.world_mut()
        .write_message(GrassEdit::clear(GrassBrushShape::circle(other, 2.0)).with_softness(0.0));
    app.update();
    assert_eq!(
        near_other(&mut app),
        0,
        "the patch was not drawn this frame"
    );

    let tables = app.world().resource::<FieldTables>().clone();
    let words = app
        .world()
        .resource::<EditLayers>()
        .layers
        .values()
        .next()
        .expect("an edit layer")
        .words
        .clone();
    let at = (tables.fields[0].placement.z / 2) as usize;
    let gpu = gpu_masks(&mut app);
    assert!(
        gpu[at..at + words.len()] == words[..],
        "the GPU's mask differs from the edit layer"
    );
}
