//! The per-view compute passes (`shaders/generate.wesl`): trample decay and
//! stamping, chunk cull, dispatch sizing, blade generation and draw
//! arguments. Recorded once per
//! frame before any camera renders, so the main and shadow passes of every
//! view read instances that match this frame.

use bevy::ecs::prelude::*;
use bevy::render::diagnostic::RecordDiagnostics;
use bevy::render::render_resource::{
    CachedComputePipelineId, ComputePassDescriptor, ComputePipelineDescriptor, PipelineCache,
};
use bevy::render::renderer::RenderContext;

use super::resources::{GrassLayouts, GrassShaders, GrassShared, GrassSharedBindGroups};
use super::trample;
use super::view::{GrassViewBindGroups, GrassViewBuffers};

/// Cells in a trample window.
const TRAMPLE_CELLS: u32 = (trample::TEXELS * trample::TEXELS) as u32;

/// The compute pipelines, one per entry point.
#[derive(Resource, Debug)]
pub(crate) struct GeneratePipelines {
    trample_decay: CachedComputePipelineId,
    trample_stamp: CachedComputePipelineId,
    cull: CachedComputePipelineId,
    size_dispatch: CachedComputePipelineId,
    generate: CachedComputePipelineId,
    write_draw_args: CachedComputePipelineId,
}

pub(crate) fn init_generate_pipelines(
    mut commands: Commands,
    pipeline_cache: Res<PipelineCache>,
    layouts: Res<GrassLayouts>,
    shaders: Res<GrassShaders>,
) {
    let queue = |entry: &'static str| {
        let mut layout = vec![layouts.shared.clone(), layouts.view.clone()];
        if entry == "size_dispatch" {
            layout.push(layouts.dispatch.clone());
        }
        pipeline_cache.queue_compute_pipeline(ComputePipelineDescriptor {
            label: Some(format!("grass_{entry}").into()),
            layout,
            shader: if entry.starts_with("trample") {
                shaders.trample.clone()
            } else {
                shaders.generate.clone()
            },
            entry_point: Some(entry.into()),
            ..Default::default()
        })
    };
    commands.insert_resource(GeneratePipelines {
        trample_decay: queue("trample_decay"),
        trample_stamp: queue("trample_stamp"),
        cull: queue("cull"),
        size_dispatch: queue("size_dispatch"),
        generate: queue("generate"),
        write_draw_args: queue("write_draw_args"),
    });
}

/// Records every grass view's compute passes.
pub(crate) fn run_generate(
    mut ctx: RenderContext,
    pipelines: Res<GeneratePipelines>,
    pipeline_cache: Res<PipelineCache>,
    shared: Res<GrassShared>,
    shared_groups: Res<GrassSharedBindGroups>,
    mut views: Query<(&mut GrassViewBuffers, &GrassViewBindGroups)>,
) {
    let (
        Some(trample_decay),
        Some(trample_stamp),
        Some(cull),
        Some(size_dispatch),
        Some(generate),
        Some(write_draw_args),
        Some(shared_group),
    ) = (
        pipeline_cache.get_compute_pipeline(pipelines.trample_decay),
        pipeline_cache.get_compute_pipeline(pipelines.trample_stamp),
        pipeline_cache.get_compute_pipeline(pipelines.cull),
        pipeline_cache.get_compute_pipeline(pipelines.size_dispatch),
        pipeline_cache.get_compute_pipeline(pipelines.generate),
        pipeline_cache.get_compute_pipeline(pipelines.write_draw_args),
        shared_groups.compute.as_ref(),
    )
    else {
        return;
    };
    let diagnostics = ctx.diagnostic_recorder();
    let diagnostics = diagnostics.as_deref();

    for (mut buffers, groups) in &mut views {
        let (Some(view_group), Some(dispatch_group)) =
            (groups.compute.as_ref(), groups.dispatch.as_ref())
        else {
            continue;
        };
        let encoder = ctx.command_encoder();
        encoder.clear_buffer(&buffers.counters, 0, None);

        let span = diagnostics.time_span(encoder, "grass_trample");
        {
            let mut pass = encoder.begin_compute_pass(&ComputePassDescriptor {
                label: Some("grass_trample"),
                ..Default::default()
            });
            pass.set_bind_group(0, shared_group, &[]);
            pass.set_bind_group(1, view_group, &[]);
            pass.set_pipeline(trample_decay);
            pass.dispatch_workgroups(TRAMPLE_CELLS.div_ceil(64), 1, 1);
            if buffers.interactor_count > 0 {
                pass.set_pipeline(trample_stamp);
                pass.dispatch_workgroups(buffers.interactor_count, 1, 1);
            }
        }
        span.end(encoder);

        let span = diagnostics.time_span(encoder, "grass_cull");
        {
            let mut pass = encoder.begin_compute_pass(&ComputePassDescriptor {
                label: Some("grass_cull"),
                ..Default::default()
            });
            pass.set_bind_group(0, shared_group, &[]);
            pass.set_bind_group(1, view_group, &[]);
            if shared.chunk_count > 0 {
                pass.set_pipeline(cull);
                pass.dispatch_workgroups(shared.chunk_count.div_ceil(64), 1, 1);
            }
            pass.set_pipeline(size_dispatch);
            pass.set_bind_group(2, dispatch_group, &[]);
            pass.dispatch_workgroups(1, 1, 1);
        }
        span.end(encoder);

        let span = diagnostics.time_span(encoder, "grass_generate");
        {
            let mut pass = encoder.begin_compute_pass(&ComputePassDescriptor {
                label: Some("grass_generate"),
                ..Default::default()
            });
            pass.set_bind_group(0, shared_group, &[]);
            pass.set_bind_group(1, view_group, &[]);
            pass.set_pipeline(generate);
            pass.dispatch_workgroups_indirect(&buffers.dispatch, 0);
            pass.set_pipeline(write_draw_args);
            pass.dispatch_workgroups(1, 1, 1);
        }
        span.end(encoder);

        if buffers.wants_readback() {
            encoder.copy_buffer_to_buffer(&buffers.counters, 0, &buffers.readback, 0, 16);
            buffers.readback_copied();
        }
    }
}
