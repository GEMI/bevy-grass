//! The render world: shared and per-view resources, compute passes, draws.

pub(crate) mod bevy_glue;
mod compute;
#[cfg(test)]
mod gpu_test;
pub(crate) mod resources;
pub(crate) mod view;

use bevy::app::{App, Plugin};
use bevy::core_pipeline::core_3d::Opaque3d;
use bevy::core_pipeline::deferred::Opaque3dDeferred;
use bevy::core_pipeline::prepass::Opaque3dPrepass;
use bevy::core_pipeline::schedule::camera_driver;
use bevy::ecs::prelude::*;
use bevy::pbr::Shadow;
use bevy::render::extract_component::ExtractComponentPlugin;
use bevy::render::render_phase::AddRenderCommand;
use bevy::render::render_resource::SpecializedRenderPipelines;
use bevy::render::renderer::RenderGraph;
use bevy::render::{ExtractSchedule, Render, RenderApp, RenderStartup, RenderSystems};

use crate::api::{GrassCamera, NoGrass};

/// Wires the render app. Added by `GrassPlugin`.
pub(crate) struct GrassRenderPlugin {
    pub shaders: resources::GrassShaders,
}

impl Plugin for GrassRenderPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            ExtractComponentPlugin::<NoGrass>::default(),
            ExtractComponentPlugin::<GrassCamera>::default(),
        ));
        // The stats the render world reads back go to the main world through
        // this shared handle.
        let readings = app.world().resource::<crate::stats::GpuReadings>().clone();
        let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
            return;
        };
        render_app
            .insert_resource(readings)
            .insert_resource(self.shaders.clone())
            .init_resource::<resources::ExtractedGrass>()
            .init_resource::<resources::GrassSharedBindGroups>()
            .init_resource::<SpecializedRenderPipelines<bevy_glue::GrassMainPipeline>>()
            .init_resource::<SpecializedRenderPipelines<bevy_glue::GrassPrepassPipeline>>()
            .init_resource::<SpecializedRenderPipelines<bevy_glue::GrassShadowPipeline>>()
            .add_render_command::<Opaque3d, bevy_glue::DrawGrassMain>()
            .add_render_command::<Opaque3dPrepass, bevy_glue::DrawGrassPrepass>()
            .add_render_command::<Opaque3dDeferred, bevy_glue::DrawGrassPrepass>()
            .add_render_command::<Shadow, bevy_glue::DrawGrassShadow>()
            .add_systems(
                RenderStartup,
                (
                    resources::init_shared,
                    (
                        bevy_glue::init_pipelines
                            .after(bevy::pbr::MeshPipelineSystems)
                            .after(bevy::pbr::init_prepass_pipeline),
                        compute::init_generate_pipelines,
                    ),
                )
                    .chain(),
            )
            .add_systems(ExtractSchedule, resources::extract_grass)
            .add_systems(
                Render,
                (
                    bevy_glue::queue_grass.in_set(RenderSystems::Queue),
                    (resources::prepare_shared, view::prepare_views)
                        .in_set(RenderSystems::PrepareResources),
                    (
                        resources::prepare_shared_bind_groups,
                        view::prepare_view_bind_groups,
                    )
                        .in_set(RenderSystems::PrepareBindGroups),
                ),
            )
            .add_systems(RenderGraph, compute::run_generate.before(camera_driver));
    }
}
