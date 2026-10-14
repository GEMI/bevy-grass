//! Every touch point with Bevy's render internals: pipeline specialization,
//! phase items in `Opaque3d` and `Shadow`, and the render commands that draw
//! them. A Bevy upgrade should need changes here and little else.
//!
//! Grass is one `NonMesh` binned phase item per camera, drawn with three
//! `draw_indexed_indirect` calls (one per bucket) whatever the number of
//! fields. The item *is* the camera's render entity, which carries that
//! camera's instance buffers, so the same item queued into the camera's
//! shadow views draws the camera's blades into its shadow maps.
//!
//! Binned phases keep items between frames, so each frame the item is
//! removed and re-added: that keeps it correct when a camera gains `NoGrass`,
//! its MSAA changes, or the grass goes away, at the cost of two map operations.

use bevy::asset::{AssetId, RenderAssetUsages};
use bevy::core_pipeline::core_3d::{
    CORE_3D_DEPTH_FORMAT, Opaque3d, Opaque3dBatchSetKey, Opaque3dBinKey,
};
use bevy::ecs::prelude::*;
use bevy::ecs::query::ROQueryItem;
use bevy::ecs::system::SystemParamItem;
use bevy::ecs::system::lifetimeless::{Read, SRes};
use bevy::mesh::{Mesh, MeshVertexBufferLayoutRef, PrimitiveTopology};
use bevy::pbr::{
    LightEntity, MeshPipeline, MeshPipelineKey, SetMeshViewBindGroup,
    SetMeshViewBindingArrayBindGroup, Shadow, ShadowBatchSetKey, ShadowBinKey, ViewKeyCache,
    ViewLightEntities,
};
use bevy::render::mesh::MeshVertexBufferLayouts;
use bevy::render::mesh::allocator::MeshSlabs;
use bevy::render::render_phase::{
    BinnedRenderPhaseType, DrawFunctions, InputUniformIndex, PhaseItem, RenderCommand,
    RenderCommandResult, SetItemPipeline, TrackedRenderPass, ViewBinnedRenderPhases,
};
use bevy::render::render_resource::{
    CachedRenderPipelineId, CompareFunction, DepthBiasState, DepthStencilState, IndexFormat,
    MultisampleState, PipelineCache, PrimitiveState, RenderPipelineDescriptor,
    SpecializedMeshPipeline, SpecializedRenderPipeline, SpecializedRenderPipelines, StencilState,
    VertexState,
};
use bevy::render::renderer::RenderDevice;
use bevy::render::settings::WgpuFeatures;
use bevy::render::sync_world::MainEntity;
use bevy::render::view::{ExtractedView, ViewUniformOffset};
use bevy::shader::Shader;

use super::resources::{GrassLayouts, GrassShaders, GrassShared, GrassSharedBindGroups};
use super::view::{DRAW_ARGS_SIZE, GrassSettings, GrassViewBindGroups, GrassViewBuffers};
use crate::api::NoGrass;

/// The main-pass pipeline: Bevy's mesh pipeline, so the view bindings and
/// shader definitions match what its PBR functions expect, with the vertex
/// buffers removed and the mesh bind group replaced by the grass one.
#[derive(Resource, Clone)]
pub(crate) struct GrassMainPipeline {
    mesh_pipeline: MeshPipeline,
    shader: bevy::asset::Handle<Shader>,
    grass_layout: bevy::render::render_resource::BindGroupLayoutDescriptor,
    /// The layout of a mesh with positions and normals, which is what
    /// `MeshPipeline::specialize` needs to produce a lit pipeline. No vertex
    /// buffer is bound; the grass shader builds vertices from indices.
    vertex_layout: MeshVertexBufferLayoutRef,
}

// Not a `SpecializedMeshPipeline`: Bevy's cache for those assumes a vertex
// buffer, and grass has none.
impl SpecializedRenderPipeline for GrassMainPipeline {
    type Key = MeshPipelineKey;

    fn specialize(&self, key: Self::Key) -> RenderPipelineDescriptor {
        let mut descriptor = self
            .mesh_pipeline
            .specialize(key, &self.vertex_layout)
            .expect("the template layout has every attribute the mesh pipeline requires");
        descriptor.label = Some("grass_main".into());
        descriptor.vertex.shader = self.shader.clone();
        descriptor.vertex.buffers.clear();
        if let Some(fragment) = descriptor.fragment.as_mut() {
            fragment.shader = self.shader.clone();
            fragment.entry_point = Some("fragment".into());
        }
        // Keep the view (0) and binding-array (1) groups; grass data is 2.
        descriptor.layout.truncate(2);
        descriptor.layout.push(self.grass_layout.clone());
        descriptor.primitive.cull_mode = None;
        // Grass is not drawn in Bevy's depth prepass yet (M4), so it writes
        // and tests depth itself even when the view has a prepass. A
        // depth-only grass pass ahead of the lit one was measured and made
        // the frame slower (see the plan's status section).
        if let Some(depth) = descriptor.depth_stencil.as_mut() {
            depth.depth_write_enabled = Some(true);
            depth.depth_compare = Some(CompareFunction::GreaterEqual);
        }
        descriptor
    }
}

/// The shadow pipeline: depth only, for directional-light views.
#[derive(Resource, Debug, Clone, Copy)]
pub(crate) struct GrassShadowPipeline(Option<CachedRenderPipelineId>);

pub(crate) fn init_pipelines(
    mut commands: Commands,
    mesh_pipeline: Res<MeshPipeline>,
    shaders: Res<GrassShaders>,
    layouts: Res<GrassLayouts>,
    mut vertex_layouts: ResMut<MeshVertexBufferLayouts>,
    pipeline_cache: Res<PipelineCache>,
    device: Res<RenderDevice>,
) {
    let template = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, vec![[0.0f32; 3]])
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0f32, 1.0, 0.0]]);
    commands.insert_resource(GrassMainPipeline {
        mesh_pipeline: mesh_pipeline.clone(),
        shader: shaders.main.clone(),
        grass_layout: layouts.draw.clone(),
        vertex_layout: template.get_mesh_vertex_buffer_layout(&mut vertex_layouts),
    });

    // Directional shadow maps need geometry behind the near plane clamped
    // rather than clipped, as Bevy's own shadow pipelines do. Without the
    // feature grass casts no shadow rather than a wrong one.
    let shadow = device
        .features()
        .contains(WgpuFeatures::DEPTH_CLIP_CONTROL)
        .then(|| {
            pipeline_cache.queue_render_pipeline(RenderPipelineDescriptor {
                label: Some("grass_shadow".into()),
                layout: vec![
                    layouts.shadow_view.clone(),
                    layouts.empty.clone(),
                    layouts.draw.clone(),
                ],
                vertex: VertexState {
                    shader: shaders.shadow.clone(),
                    ..Default::default()
                },
                fragment: None,
                primitive: PrimitiveState {
                    cull_mode: None,
                    unclipped_depth: true,
                    ..Default::default()
                },
                depth_stencil: Some(DepthStencilState {
                    format: CORE_3D_DEPTH_FORMAT,
                    depth_write_enabled: Some(true),
                    depth_compare: Some(CompareFunction::GreaterEqual),
                    stencil: StencilState::default(),
                    bias: DepthBiasState::default(),
                }),
                multisample: MultisampleState::default(),
                ..Default::default()
            })
        });
    commands.insert_resource(GrassShadowPipeline(shadow));
}

/// Draws a camera's LOD buckets: all three in the main pass, as many as the
/// shadow mode allows in shadow passes.
pub(crate) struct DrawGrassBuckets<const SHADOW: bool>;

impl<P: PhaseItem, const SHADOW: bool> RenderCommand<P> for DrawGrassBuckets<SHADOW> {
    type Param = (SRes<GrassShared>, SRes<GrassSettings>);
    type ViewQuery = ();
    /// The camera whose blades these are (the phase item is the camera).
    type ItemQuery = (Read<GrassViewBuffers>, Read<GrassViewBindGroups>);

    fn render<'w>(
        _item: &P,
        _view: ROQueryItem<'w, '_, Self::ViewQuery>,
        camera: Option<ROQueryItem<'w, '_, Self::ItemQuery>>,
        (shared, settings): SystemParamItem<'w, '_, Self::Param>,
        pass: &mut TrackedRenderPass<'w>,
    ) -> RenderCommandResult {
        let Some((buffers, groups)) = camera else {
            return RenderCommandResult::Skip;
        };
        let Some(draw) = groups.draw.as_ref() else {
            return RenderCommandResult::Skip;
        };
        pass.set_bind_group(2, draw, &[]);
        pass.set_index_buffer(shared.into_inner().indices.slice(..), IndexFormat::Uint32);
        let buckets = if SHADOW { settings.shadow_buckets() } else { 3 };
        for bucket in 0..buckets {
            pass.draw_indexed_indirect(&buffers.draw_args, bucket * DRAW_ARGS_SIZE);
        }
        RenderCommandResult::Success
    }
}

/// Binds the light view's uniform for the shadow pass.
pub(crate) struct SetGrassShadowView<const I: usize>;

impl<P: PhaseItem, const I: usize> RenderCommand<P> for SetGrassShadowView<I> {
    type Param = SRes<GrassSharedBindGroups>;
    type ViewQuery = Read<ViewUniformOffset>;
    type ItemQuery = ();

    fn render<'w>(
        _item: &P,
        offset: ROQueryItem<'w, '_, Self::ViewQuery>,
        _entity: Option<ROQueryItem<'w, '_, Self::ItemQuery>>,
        groups: SystemParamItem<'w, '_, Self::Param>,
        pass: &mut TrackedRenderPass<'w>,
    ) -> RenderCommandResult {
        let groups = groups.into_inner();
        let (Some(view), Some(empty)) = (groups.shadow_view.as_ref(), groups.empty.as_ref()) else {
            return RenderCommandResult::Skip;
        };
        pass.set_bind_group(I, view, &[offset.offset]);
        pass.set_bind_group(I + 1, empty, &[]);
        RenderCommandResult::Success
    }
}

pub(crate) type DrawGrassMain = (
    SetItemPipeline,
    SetMeshViewBindGroup<0>,
    SetMeshViewBindingArrayBindGroup<1>,
    DrawGrassBuckets<false>,
);

pub(crate) type DrawGrassShadow = (
    SetItemPipeline,
    SetGrassShadowView<0>,
    DrawGrassBuckets<true>,
);

/// What `queue_grass` reads from each view.
type CameraItems = (
    Entity,
    &'static MainEntity,
    &'static ExtractedView,
    Option<&'static ViewLightEntities>,
    Has<NoGrass>,
    Option<&'static GrassViewBindGroups>,
);

/// Puts the grass item into the opaque phase of every camera and into the
/// shadow phase of every directional-light view those cameras own.
#[allow(clippy::too_many_arguments)]
pub(crate) fn queue_grass(
    pipeline_cache: Res<PipelineCache>,
    main_pipeline: Res<GrassMainPipeline>,
    mut main_pipelines: ResMut<SpecializedRenderPipelines<GrassMainPipeline>>,
    shadow_pipeline: Res<GrassShadowPipeline>,
    opaque_draw_functions: Res<DrawFunctions<Opaque3d>>,
    shadow_draw_functions: Res<DrawFunctions<Shadow>>,
    mut opaque_phases: ResMut<ViewBinnedRenderPhases<Opaque3d>>,
    mut shadow_phases: ResMut<ViewBinnedRenderPhases<Shadow>>,
    view_key_cache: Res<ViewKeyCache>,
    views: Query<CameraItems>,
    light_views: Query<(&ExtractedView, &LightEntity)>,
    shared: Res<GrassShared>,
    settings: Res<GrassSettings>,
) {
    let draw_main = opaque_draw_functions.read().id::<DrawGrassMain>();
    let draw_shadow = shadow_draw_functions.read().id::<DrawGrassShadow>();
    let has_grass = shared.chunk_count > 0;

    for (entity, &main_entity, view, lights, no_grass, groups) in &views {
        let Some(opaque) = opaque_phases.get_mut(&view.retained_view_entity) else {
            continue;
        };
        opaque.remove(main_entity);
        let ready = groups.is_some_and(|groups| groups.draw.is_some());
        let draws = has_grass && ready && !no_grass;

        if draws && let Some(&view_key) = view_key_cache.get(&view.retained_view_entity) {
            let key = view_key
                | MeshPipelineKey::from_primitive_topology_and_strip_index(
                    PrimitiveTopology::TriangleList,
                    None,
                );
            let pipeline = main_pipelines.specialize(&pipeline_cache, &main_pipeline, key);
            opaque.add(
                Opaque3dBatchSetKey {
                    draw_function: draw_main,
                    pipeline,
                    material_bind_group_index: None,
                    lightmap_slab: None,
                    slabs: MeshSlabs::default(),
                },
                Opaque3dBinKey {
                    asset_id: AssetId::<Mesh>::default().untyped(),
                },
                (entity, main_entity),
                InputUniformIndex::default(),
                BinnedRenderPhaseType::NonMesh,
            );
        }

        let Some(lights) = lights else {
            continue;
        };
        for &light_view in &lights.lights {
            let Ok((light_view, light)) = light_views.get(light_view) else {
                continue;
            };
            let Some(shadow) = shadow_phases.get_mut(&light_view.retained_view_entity) else {
                continue;
            };
            shadow.remove(main_entity);
            let casts = matches!(light, LightEntity::Directional { cascade_index, .. }
                if *cascade_index < settings.shadow_cascades());
            if let (true, true, Some(pipeline)) = (draws, casts, shadow_pipeline.0) {
                shadow.add(
                    ShadowBatchSetKey {
                        pipeline,
                        draw_function: draw_shadow,
                        material_bind_group_index: None,
                        slabs: MeshSlabs::default(),
                    },
                    ShadowBinKey {
                        asset_id: AssetId::<Mesh>::default().untyped(),
                    },
                    (entity, main_entity),
                    InputUniformIndex::default(),
                    BinnedRenderPhaseType::NonMesh,
                );
            }
        }
    }
}
