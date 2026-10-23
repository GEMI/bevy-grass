//! The render world: shared and per-view resources, compute passes, draws.
//! None of it runs on a device that cannot draw grass (see `platform`).

pub(crate) mod bevy_glue;
mod compute;
mod extract;
#[cfg(test)]
mod gpu_test;
pub(crate) mod platform;
pub(crate) mod resources;
mod shadows;
pub(crate) mod view;

use bevy::app::{App, Plugin};
use bevy::core_pipeline::core_3d::Opaque3d;
use bevy::core_pipeline::deferred::Opaque3dDeferred;
use bevy::core_pipeline::prepass::Opaque3dPrepass;
use bevy::core_pipeline::schedule::camera_driver;
use bevy::ecs::prelude::*;
use bevy::log::warn;
use bevy::pbr::Shadow;
use bevy::render::render_phase::AddRenderCommand;
use bevy::render::render_resource::SpecializedRenderPipelines;
use bevy::render::renderer::{RenderAdapter, RenderDevice, RenderGraph};
use bevy::render::{ExtractSchedule, Render, RenderApp, RenderStartup, RenderSystems};

use platform::GrassSupport;

/// Wires the render app. Added by `GrassPlugin`.
pub(crate) struct GrassRenderPlugin {
    pub shaders: resources::GrassShaders,
}

impl Plugin for GrassRenderPlugin {
    fn build(&self, app: &mut App) {
        // The stats the render world reads back go to the main world through
        // this shared handle.
        let readings = app.world().resource::<crate::stats::GpuReadings>().clone();
        app.init_resource::<GrassSupport>();
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
                    .chain()
                    .run_if(platform::supported),
            )
            .add_systems(
                ExtractSchedule,
                (resources::extract_grass, resources::extract_cameras).run_if(platform::supported),
            )
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
                )
                    .run_if(platform::supported),
            )
            .add_systems(
                RenderGraph,
                compute::run_generate
                    .before(camera_driver)
                    .run_if(platform::supported),
            );
    }

    /// Decides [`GrassSupport`] once the render device exists, which
    /// `RenderPlugin` sees to in its own `finish`, before this one.
    fn finish(&self, app: &mut App) {
        let world = app.world();
        let (Some(adapter), Some(device)) = (
            world.get_resource::<RenderAdapter>(),
            world.get_resource::<RenderDevice>(),
        ) else {
            return;
        };
        let support = platform::check(
            adapter.get_downlevel_capabilities().flags,
            &device.limits(),
            &resources::GrassLayouts::new(),
        );
        if !support.is_supported() {
            warn!("bevy_grass: no grass is drawn on this GPU: {support}. See GrassSupport.");
        }
        app.insert_resource(support);
        if let Some(render_app) = app.get_sub_app_mut(RenderApp) {
            render_app.insert_resource(support);
        }
    }
}
