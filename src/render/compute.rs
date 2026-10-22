//! The per-view compute passes (`shaders/generate.wesl`): trample decay and
//! stamping, chunk cull (which splits each visible chunk into slices),
//! dispatch sizing, blade generation (one workgroup per slice) and draw
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
use super::view::{GrassViewBindGroups, GrassViewBuffers};
use crate::trample;

/// A compute pipeline: one entry point in `trample.wesl` or `generate.wesl`.
///
/// The one list of them: the pipelines, their layouts and the storage-buffer
/// check are all made from [`ComputePass::ALL`], so none can miss a pass.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ComputePass {
    TrampleDecay,
    TrampleStamp,
    Cull,
    SizeDispatch,
    Generate,
    WriteDrawArgs,
}

impl ComputePass {
    /// Every pass, in declaration order, so `pass as usize` indexes it.
    pub(crate) const ALL: [Self; 6] = [
        Self::TrampleDecay,
        Self::TrampleStamp,
        Self::Cull,
        Self::SizeDispatch,
        Self::Generate,
        Self::WriteDrawArgs,
    ];

    /// The entry point's name in the shader.
    pub(crate) fn entry_point(self) -> &'static str {
        // No wildcard: a new pass stops the build here. Add it to `ALL` too,
        // or it gets no pipeline and the storage-buffer check misses it.
        match self {
            Self::TrampleDecay => "trample_decay",
            Self::TrampleStamp => "trample_stamp",
            Self::Cull => "cull",
            Self::SizeDispatch => "size_dispatch",
            Self::Generate => "generate",
            Self::WriteDrawArgs => "write_draw_args",
        }
    }

    /// Whether the entry point is in `trample.wesl` rather than
    /// `generate.wesl`.
    fn is_trample(self) -> bool {
        match self {
            Self::TrampleDecay | Self::TrampleStamp => true,
            Self::Cull | Self::SizeDispatch | Self::Generate | Self::WriteDrawArgs => false,
        }
    }
}

/// Cells in a trample window.
const TRAMPLE_CELLS: u32 = (trample::TEXELS * trample::TEXELS) as u32;

/// The compute pipelines, one per pass, in the order of [`ComputePass::ALL`].
#[derive(Resource, Debug)]
pub(crate) struct GeneratePipelines([CachedComputePipelineId; ComputePass::ALL.len()]);

pub(crate) fn init_generate_pipelines(
    mut commands: Commands,
    pipeline_cache: Res<PipelineCache>,
    layouts: Res<GrassLayouts>,
    shaders: Res<GrassShaders>,
) {
    let queue = |pass: ComputePass| {
        let entry = pass.entry_point();
        pipeline_cache.queue_compute_pipeline(ComputePipelineDescriptor {
            label: Some(format!("grass_{entry}").into()),
            layout: layouts.compute(pass),
            shader: if pass.is_trample() {
                shaders.trample.clone()
            } else {
                shaders.generate.clone()
            },
            entry_point: Some(entry.into()),
            ..Default::default()
        })
    };
    commands.insert_resource(GeneratePipelines(ComputePass::ALL.map(queue)));
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
    let get = |pass: ComputePass| pipeline_cache.get_compute_pipeline(pipelines.0[pass as usize]);
    let (
        Some(trample_decay),
        Some(trample_stamp),
        Some(cull),
        Some(size_dispatch),
        Some(generate),
        Some(write_draw_args),
        Some(shared_group),
    ) = (
        get(ComputePass::TrampleDecay),
        get(ComputePass::TrampleStamp),
        get(ComputePass::Cull),
        get(ComputePass::SizeDispatch),
        get(ComputePass::Generate),
        get(ComputePass::WriteDrawArgs),
        shared_groups.compute.as_ref(),
    )
    else {
        return;
    };
    let diagnostics = ctx.diagnostic_recorder();
    let diagnostics = diagnostics.as_deref();

    for (mut buffers, groups) in &mut views {
        let (Some(generate_group), Some(args_group), Some(trample_group), Some(dispatch_group)) = (
            groups.generate.as_ref(),
            groups.args.as_ref(),
            groups.trample.as_ref(),
            groups.dispatch.as_ref(),
        ) else {
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
            pass.set_bind_group(1, trample_group, &[]);
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
            pass.set_bind_group(1, generate_group, &[]);
            if shared.chunk_count > 0 {
                pass.set_pipeline(cull);
                pass.dispatch_workgroups(shared.chunk_count.div_ceil(64), 1, 1);
            }
            pass.set_pipeline(size_dispatch);
            pass.set_bind_group(1, args_group, &[]);
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
            pass.set_bind_group(1, generate_group, &[]);
            pass.set_pipeline(generate);
            pass.dispatch_workgroups_indirect(&buffers.dispatch, 0);
            pass.set_pipeline(write_draw_args);
            pass.set_bind_group(1, args_group, &[]);
            pass.dispatch_workgroups(1, 1, 1);
        }
        span.end(encoder);

        if buffers.wants_readback() {
            encoder.copy_buffer_to_buffer(&buffers.counters, 0, &buffers.readback, 0, 16);
            buffers.readback_copied();
        }
    }
}
