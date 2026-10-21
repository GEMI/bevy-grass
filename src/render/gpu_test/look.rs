//! Every optional look term compiled into one grass pipeline, on a real GPU.
//!
//! The unit tests next to `LookFeatures` check that each term has a
//! definition and a setting that asks for it. They cannot check that the
//! branches those definitions guard compile: a term is only compiled into a
//! pipeline when some style uses it, so a definition no style enables is
//! never seen by the shader compiler at all, and a mistake inside its `@if`
//! would ship unnoticed. This draws one field whose style uses every term at
//! once, which compiles them all in, and fails if any grass pipeline does.

use bevy::render::render_resource::{CachedPipelineState, PipelineCache, PipelineDescriptor};

use super::*;
use crate::api::{GrassBlade, GrassVariation, GroundTint};
use crate::fields::{FieldTables, LookFeatures};

/// A style that uses every optional look term at once.
fn every_term() -> GrassStyle {
    GrassStyle {
        blade: GrassBlade {
            specular: 0.5,
            twist: 0.5,
            droop: 0.5,
            fold: 0.5,
            head_length: 0.3,
            head_width: 0.03,
            ..GrassStyle::MEADOW.blade
        },
        toon_bands: 3,
        ground_tint: GroundTint {
            amount: 0.3,
            ..GrassStyle::MEADOW.ground_tint
        },
        variation: GrassVariation {
            patch_amount: 0.5,
            dry_tips: 0.5,
            dead_share: 0.1,
            tufts: 0.5,
            ..GrassStyle::MEADOW.variation
        },
        ..GrassStyle::MEADOW
    }
}

/// How many grass pipelines are compiled, panicking on any that failed: a
/// shader that does not compile draws no grass, logs an error and otherwise
/// looks like a fast frame.
fn compiled_grass_pipelines(app: &mut App) -> usize {
    let world = app.sub_app_mut(RenderApp).world();
    let cache = world.resource::<PipelineCache>();
    let mut compiled = 0;
    for pipeline in cache.pipelines() {
        let PipelineDescriptor::RenderPipelineDescriptor(descriptor) = &pipeline.descriptor else {
            continue;
        };
        let label = descriptor.label.as_deref().unwrap_or_default();
        if !label.starts_with("grass") {
            continue;
        }
        match &pipeline.state {
            CachedPipelineState::Err(error) => {
                panic!("{label} failed to compile with every look term on: {error}");
            }
            CachedPipelineState::Ok(_) => compiled += 1,
            CachedPipelineState::Queued | CachedPipelineState::Creating(_) => {}
        }
    }
    compiled
}

#[test]
#[ignore = "needs a GPU: cargo test -- --ignored gpu"]
fn gpu_a_style_using_every_look_term_compiles_and_draws() {
    let mut app = headless_app();

    // The harness's field, given the style that uses every term.
    let style = {
        let world = app.world_mut();
        let field = world
            .query_filtered::<&GrassField, ()>()
            .single(world)
            .expect("the harness's field");
        field.style.clone()
    };
    *app.world_mut()
        .resource_mut::<Assets<GrassStyle>>()
        .get_mut(&style)
        .expect("the harness's style") = every_term();

    // The tables agree the style asks for every term, so the pipelines below
    // are built with all of them compiled in; then every branch is real.
    app.update();
    let features = app.world().resource::<FieldTables>().features;
    for (term, definition) in LookFeatures::TERMS {
        assert!(
            features.contains(term),
            "{definition} is not asked for, so its shader branch is not compiled at all"
        );
    }

    // Give the pipelines time to compile; none may fail.
    let mut compiled = 0;
    for _ in 0..120 {
        app.update();
        compiled = compiled_grass_pipelines(&mut app);
    }
    assert!(
        compiled > 0,
        "no grass pipeline compiled, so nothing was checked"
    );
    assert!(
        blades_after(&mut app, 4) > 0,
        "the pass should place blades with every look term on"
    );
}
