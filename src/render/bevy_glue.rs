//! Every touch point with Bevy's render internals: pipeline specialization,
//! phase items in `Opaque3d`, `Opaque3dPrepass`, `Opaque3dDeferred` and
//! `Shadow`, and the render commands that draw them. A Bevy upgrade should
//! need changes here and little else.
//!
//! Grass is one `NonMesh` binned phase item per camera, drawn with three
//! `draw_indexed_indirect` calls (one per bucket) whatever the number of
//! fields. The item *is* the camera's render entity, which carries that
//! camera's instance buffers, so the same item queued into the camera's
//! prepass and shadow views draws the camera's blades there too.
//!
//! Binned phases keep items between frames, so each frame the item is
//! removed and re-added: that keeps it correct when a camera gains `NoGrass`,
//! its MSAA or prepasses change, or the grass goes away, at the cost of a few
//! map operations.

use bevy::asset::{AssetId, Handle, RenderAssetUsages};
use bevy::core_pipeline::core_3d::{
    CORE_3D_DEPTH_FORMAT, Opaque3d, Opaque3dBatchSetKey, Opaque3dBinKey,
};
use bevy::core_pipeline::deferred::Opaque3dDeferred;
use bevy::core_pipeline::prepass::{
    DeferredPrepass, Opaque3dPrepass, OpaqueNoLightmap3dBatchSetKey, OpaqueNoLightmap3dBinKey,
    prepass_target_descriptors,
};
use bevy::ecs::prelude::*;
use bevy::ecs::query::ROQueryItem;
use bevy::ecs::system::lifetimeless::{Read, SRes};
use bevy::ecs::system::{SystemParam, SystemParamItem};
use bevy::material::OpaqueRendererMethod;
use bevy::mesh::{Mesh, MeshVertexBufferLayoutRef, PrimitiveTopology};
use bevy::pbr::{
    DefaultOpaqueRendererMethod, LightEntity, MeshPipeline, MeshPipelineKey, PrepassPipeline,
    SetMeshViewBindGroup, SetMeshViewBindingArrayBindGroup, SetPrepassViewBindGroup,
    SetPrepassViewEmptyBindGroup, Shadow, ShadowBatchSetKey, ShadowBinKey, ViewKeyCache,
    ViewKeyPrepassCache, ViewLightEntities,
};
use bevy::reflect::{PartialReflect, ReflectRef};
use bevy::render::mesh::MeshVertexBufferLayouts;
use bevy::render::mesh::allocator::MeshSlabs;
use bevy::render::render_phase::{
    BinnedPhaseItem, BinnedRenderPhase, BinnedRenderPhaseType, DrawFunctionId, DrawFunctions,
    InputUniformIndex, PhaseItem, RenderCommand, RenderCommandResult, SetItemPipeline,
    TrackedRenderPass, ViewBinnedRenderPhases,
};
use bevy::render::render_resource::{
    BindGroupLayoutDescriptor, CachedRenderPipelineId, CompareFunction, DepthBiasState,
    DepthStencilState, FragmentState, IndexFormat, MultisampleState, PipelineCache, PrimitiveState,
    RenderPipelineDescriptor, SpecializedMeshPipeline, SpecializedRenderPipeline,
    SpecializedRenderPipelines, StencilState, VertexState,
};
use bevy::render::renderer::RenderDevice;
use bevy::render::settings::WgpuFeatures;
use bevy::render::sync_world::MainEntity;
use bevy::render::view::{ExtractedView, ViewUniformOffset};
use bevy::shader::{Shader, ShaderDefVal};

use super::resources::{
    ExtractedGrass, GrassLayouts, GrassShaders, GrassShared, GrassSharedBindGroups,
};
use super::view::{DRAW_ARGS_SIZE, GrassViewBindGroups, GrassViewBuffers};
use crate::api::{GrassCamera, NoGrass};
use crate::fields::LookFeatures;

/// A grass draw pipeline's key: the view's, plus the optional look terms
/// some field uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct GrassPipelineKey {
    pub mesh: MeshPipelineKey,
    pub features: LookFeatures,
}

/// The main-pass pipeline: Bevy's mesh pipeline, so the view bindings and
/// shader definitions match what its PBR functions expect, with the vertex
/// buffers removed and the mesh bind group replaced by the grass one.
#[derive(Resource, Clone)]
pub(crate) struct GrassMainPipeline {
    mesh_pipeline: MeshPipeline,
    shader: Handle<Shader>,
    grass_layout: BindGroupLayoutDescriptor,
    /// The layout of a mesh with positions and normals, which is what
    /// `MeshPipeline::specialize` needs to produce a lit pipeline. No vertex
    /// buffer is bound; the grass shader builds vertices from indices.
    vertex_layout: MeshVertexBufferLayoutRef,
}

// Not a `SpecializedMeshPipeline`: Bevy's cache for those assumes a vertex
// buffer, and grass has none.
impl SpecializedRenderPipeline for GrassMainPipeline {
    type Key = GrassPipelineKey;

    fn specialize(&self, key: Self::Key) -> RenderPipelineDescriptor {
        let mut descriptor = self
            .mesh_pipeline
            .specialize(key.mesh, &self.vertex_layout)
            .expect("the template layout has every attribute the mesh pipeline requires");
        descriptor.label = Some("grass_main".into());
        descriptor.vertex.shader = self.shader.clone();
        descriptor.vertex.buffers.clear();
        descriptor
            .vertex
            .shader_defs
            .extend(key.features.shader_defs().map(ShaderDefVal::from));
        if let Some(fragment) = descriptor.fragment.as_mut() {
            fragment.shader = self.shader.clone();
            fragment.entry_point = Some("fragment".into());
            fragment
                .shader_defs
                .extend(key.features.shader_defs().map(ShaderDefVal::from));
        }
        // Keep the view (0) and binding-array (1) groups; grass data is 2.
        descriptor.layout.truncate(2);
        descriptor.layout.push(self.grass_layout.clone());
        descriptor.primitive.cull_mode = None;
        // Grass writes and tests depth itself, also when the view has a
        // prepass that already drew it: the depths are equal, so the lit pass
        // still draws. Without a prepass, a depth-only grass pass ahead of the
        // lit one was measured and made the frame slower (see the plan's
        // status section), so grass never asks for one.
        if let Some(depth) = descriptor.depth_stencil.as_mut() {
            depth.depth_write_enabled = Some(true);
            depth.depth_compare = Some(CompareFunction::GreaterEqual);
        }
        descriptor
    }
}

/// The prepass and deferred pipeline: depth, and normals, motion vectors and
/// the G-buffer when the view has them. Specialized by the view's prepass
/// key, as Bevy's own prepass pipeline is.
#[derive(Resource, Clone)]
pub(crate) struct GrassPrepassPipeline {
    view_layout_motion_vectors: BindGroupLayoutDescriptor,
    view_layout_no_motion_vectors: BindGroupLayoutDescriptor,
    empty_layout: BindGroupLayoutDescriptor,
    grass_layout: BindGroupLayoutDescriptor,
    shader: Handle<Shader>,
}

impl SpecializedRenderPipeline for GrassPrepassPipeline {
    type Key = GrassPipelineKey;

    fn specialize(
        &self,
        GrassPipelineKey {
            mesh: key,
            features,
        }: Self::Key,
    ) -> RenderPipelineDescriptor {
        let normals = key.contains(MeshPipelineKey::NORMAL_PREPASS);
        let motion = key.contains(MeshPipelineKey::MOTION_VECTOR_PREPASS);
        let deferred = key.contains(MeshPipelineKey::DEFERRED_PREPASS);
        let mut defs: Vec<ShaderDefVal> = vec![
            "PREPASS_PIPELINE".into(),
            // Bevy's PBR modules name a material group; grass has none.
            ShaderDefVal::UInt("MATERIAL_BIND_GROUP".into(), 3),
        ];
        // Toon shading needs the lights, which a G-buffer pass has not got.
        defs.extend(
            features
                .shader_defs()
                .filter(|def| *def != "GRASS_TOON")
                .map(ShaderDefVal::from),
        );
        for (on, name) in [
            (
                key.contains(MeshPipelineKey::DEPTH_PREPASS),
                "DEPTH_PREPASS",
            ),
            (normals, "NORMAL_PREPASS"),
            (motion, "MOTION_VECTOR_PREPASS"),
            (deferred, "DEFERRED_PREPASS"),
            (normals || deferred, "NORMAL_PREPASS_OR_DEFERRED_PREPASS"),
            (
                motion || deferred,
                "MOTION_VECTOR_PREPASS_OR_DEFERRED_PREPASS",
            ),
        ] {
            if on {
                defs.push(name.into());
            }
        }
        let mut targets = prepass_target_descriptors(normals, motion, deferred);
        if targets.iter().all(Option::is_none) {
            // Depth only: no fragment shader at all.
            targets.clear();
        } else {
            defs.push("PREPASS_FRAGMENT".into());
        }
        let fragment = (!targets.is_empty()).then(|| FragmentState {
            shader: self.shader.clone(),
            shader_defs: defs.clone(),
            targets,
            ..Default::default()
        });
        RenderPipelineDescriptor {
            label: Some(
                if deferred {
                    "grass_deferred"
                } else {
                    "grass_prepass"
                }
                .into(),
            ),
            layout: vec![
                if motion {
                    self.view_layout_motion_vectors.clone()
                } else {
                    self.view_layout_no_motion_vectors.clone()
                },
                self.empty_layout.clone(),
                self.grass_layout.clone(),
            ],
            vertex: VertexState {
                shader: self.shader.clone(),
                shader_defs: defs,
                ..Default::default()
            },
            fragment,
            primitive: PrimitiveState {
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: Some(DepthStencilState {
                format: CORE_3D_DEPTH_FORMAT,
                depth_write_enabled: Some(true),
                depth_compare: Some(CompareFunction::GreaterEqual),
                stencil: StencilState::default(),
                bias: DepthBiasState::default(),
            }),
            multisample: MultisampleState {
                count: key.msaa_samples(),
                ..Default::default()
            },
            ..Default::default()
        }
    }
}

/// The shadow pipeline: depth only, for directional-light views.
/// Specialized by the terms that move blades (`LookFeatures::geometry`), so
/// shadows have the blades' shape.
#[derive(Resource, Clone)]
pub(crate) struct GrassShadowPipeline {
    shadow_view_layout: BindGroupLayoutDescriptor,
    empty_layout: BindGroupLayoutDescriptor,
    grass_layout: BindGroupLayoutDescriptor,
    shader: Handle<Shader>,
    /// Directional shadow maps need geometry behind the near plane clamped
    /// rather than clipped, as Bevy's own shadow pipelines do. Without the
    /// GPU feature grass casts no shadow rather than a wrong one.
    supported: bool,
}

impl SpecializedRenderPipeline for GrassShadowPipeline {
    type Key = LookFeatures;

    fn specialize(&self, features: LookFeatures) -> RenderPipelineDescriptor {
        RenderPipelineDescriptor {
            label: Some("grass_shadow".into()),
            layout: vec![
                self.shadow_view_layout.clone(),
                self.empty_layout.clone(),
                self.grass_layout.clone(),
            ],
            vertex: VertexState {
                shader: self.shader.clone(),
                shader_defs: features.shader_defs().map(ShaderDefVal::from).collect(),
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
        }
    }
}

pub(crate) fn init_pipelines(
    mut commands: Commands,
    mesh_pipeline: Res<MeshPipeline>,
    prepass_pipeline: Res<PrepassPipeline>,
    shaders: Res<GrassShaders>,
    layouts: Res<GrassLayouts>,
    mut vertex_layouts: ResMut<MeshVertexBufferLayouts>,
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
    // Bevy's own prepass layouts, so its prepass view bind groups fit.
    commands.insert_resource(GrassPrepassPipeline {
        view_layout_motion_vectors: prepass_pipeline.view_layout_motion_vectors.clone(),
        view_layout_no_motion_vectors: prepass_pipeline.view_layout_no_motion_vectors.clone(),
        empty_layout: prepass_pipeline.empty_layout.clone(),
        grass_layout: layouts.draw.clone(),
        shader: shaders.prepass.clone(),
    });

    commands.insert_resource(GrassShadowPipeline {
        shadow_view_layout: layouts.shadow_view.clone(),
        empty_layout: layouts.empty.clone(),
        grass_layout: layouts.draw.clone(),
        shader: shaders.shadow.clone(),
        supported: device.features().contains(WgpuFeatures::DEPTH_CLIP_CONTROL),
    });
}

/// Draws a camera's LOD buckets: all three in the main pass and prepasses,
/// as many as the shadow mode allows in shadow passes.
pub(crate) struct DrawGrassBuckets<const SHADOW: bool>;

impl<P: PhaseItem, const SHADOW: bool> RenderCommand<P> for DrawGrassBuckets<SHADOW> {
    type Param = (SRes<GrassShared>, SRes<ExtractedGrass>);
    type ViewQuery = ();
    /// The camera whose blades these are (the phase item is the camera).
    type ItemQuery = (
        Read<GrassViewBuffers>,
        Read<GrassViewBindGroups>,
        Option<Read<GrassCamera>>,
    );

    fn render<'w>(
        _item: &P,
        _view: ROQueryItem<'w, '_, Self::ViewQuery>,
        camera: Option<ROQueryItem<'w, '_, Self::ItemQuery>>,
        (shared, extracted): SystemParamItem<'w, '_, Self::Param>,
        pass: &mut TrackedRenderPass<'w>,
    ) -> RenderCommandResult {
        let Some((buffers, groups, grass_camera)) = camera else {
            return RenderCommandResult::Skip;
        };
        let Some(draw) = groups.draw.as_ref() else {
            return RenderCommandResult::Skip;
        };
        pass.set_bind_group(2, draw, &[]);
        pass.set_index_buffer(shared.into_inner().indices.slice(..), IndexFormat::Uint32);
        let buckets = if SHADOW {
            extracted
                .into_inner()
                .settings
                .for_camera(grass_camera)
                .shadows
                .buckets()
        } else {
            3
        };
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

/// Draws grass into the prepass and the deferred G-buffer, with Bevy's
/// prepass view bindings.
pub(crate) type DrawGrassPrepass = (
    SetItemPipeline,
    SetPrepassViewBindGroup<0>,
    SetPrepassViewEmptyBindGroup<1>,
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
    Has<DeferredPrepass>,
    Option<&'static GrassViewBindGroups>,
    Option<&'static GrassCamera>,
);

/// Whether opaque materials render deferred by default. Grass follows the
/// default, like a `StandardMaterial` left at `OpaqueRendererMethod::Auto`.
fn deferred_by_default(method: &DefaultOpaqueRendererMethod) -> bool {
    // The method is private to Bevy; reflection reads it.
    match method.reflect_ref() {
        ReflectRef::TupleStruct(method) => method
            .field(0)
            .and_then(<dyn PartialReflect>::try_downcast_ref::<OpaqueRendererMethod>)
            .is_some_and(|method| *method == OpaqueRendererMethod::Deferred),
        _ => false,
    }
}

/// The pipelines `queue_grass` specializes.
#[derive(SystemParam)]
pub(crate) struct GrassPipelines<'w> {
    cache: Res<'w, PipelineCache>,
    main: Res<'w, GrassMainPipeline>,
    main_specialized: ResMut<'w, SpecializedRenderPipelines<GrassMainPipeline>>,
    prepass: Res<'w, GrassPrepassPipeline>,
    prepass_specialized: ResMut<'w, SpecializedRenderPipelines<GrassPrepassPipeline>>,
    shadow: Res<'w, GrassShadowPipeline>,
    shadow_specialized: ResMut<'w, SpecializedRenderPipelines<GrassShadowPipeline>>,
    view_keys: Res<'w, ViewKeyCache>,
    prepass_view_keys: Res<'w, ViewKeyPrepassCache>,
    renderer_method: Res<'w, DefaultOpaqueRendererMethod>,
}

/// The phases `queue_grass` fills, and their draw functions.
#[derive(SystemParam)]
pub(crate) struct GrassPhases<'w> {
    opaque: ResMut<'w, ViewBinnedRenderPhases<Opaque3d>>,
    prepass: ResMut<'w, ViewBinnedRenderPhases<Opaque3dPrepass>>,
    deferred: ResMut<'w, ViewBinnedRenderPhases<Opaque3dDeferred>>,
    shadow: ResMut<'w, ViewBinnedRenderPhases<Shadow>>,
    opaque_draws: Res<'w, DrawFunctions<Opaque3d>>,
    prepass_draws: Res<'w, DrawFunctions<Opaque3dPrepass>>,
    deferred_draws: Res<'w, DrawFunctions<Opaque3dDeferred>>,
    shadow_draws: Res<'w, DrawFunctions<Shadow>>,
}

/// Adds the grass item to a prepass or deferred phase.
fn add_prepass_item<P>(
    phase: &mut BinnedRenderPhase<P>,
    pipeline: CachedRenderPipelineId,
    draw_function: DrawFunctionId,
    item: (Entity, MainEntity),
) where
    P: BinnedPhaseItem<
            BatchSetKey = OpaqueNoLightmap3dBatchSetKey,
            BinKey = OpaqueNoLightmap3dBinKey,
        >,
{
    phase.add(
        OpaqueNoLightmap3dBatchSetKey {
            pipeline,
            draw_function,
            material_bind_group_index: None,
            slabs: MeshSlabs::default(),
        },
        OpaqueNoLightmap3dBinKey {
            asset_id: AssetId::<Mesh>::default().untyped(),
        },
        item,
        InputUniformIndex::default(),
        BinnedRenderPhaseType::NonMesh,
    );
}

/// Puts the grass item into the opaque phase of every camera (or, for a
/// deferred camera, its deferred phase), into its prepass when it has one,
/// and into the shadow phase of every directional-light view it owns.
// One flat pass over the cameras; split up it would only be harder to follow.
#[expect(
    clippy::too_many_lines,
    reason = "one flat pass over the cameras; splitting it would only be harder to follow"
)]
pub(crate) fn queue_grass(
    mut pipelines: GrassPipelines,
    mut phases: GrassPhases,
    views: Query<CameraItems>,
    light_views: Query<(&ExtractedView, &LightEntity)>,
    shared: Res<GrassShared>,
    extracted: Res<ExtractedGrass>,
) {
    let draw_main = phases.opaque_draws.read().id::<DrawGrassMain>();
    let draw_prepass = phases.prepass_draws.read().id::<DrawGrassPrepass>();
    let draw_deferred = phases.deferred_draws.read().id::<DrawGrassPrepass>();
    let draw_shadow = phases.shadow_draws.read().id::<DrawGrassShadow>();
    let has_grass = shared.chunk_count > 0;
    let features = extracted.tables.features;
    let with_features = |mesh| GrassPipelineKey { mesh, features };
    let deferred_default = deferred_by_default(&pipelines.renderer_method);
    let triangles = MeshPipelineKey::from_primitive_topology_and_strip_index(
        PrimitiveTopology::TriangleList,
        None,
    );

    for (entity, &main_entity, view, lights, no_grass, deferred_prepass, groups, grass_camera) in
        &views
    {
        let retained = view.retained_view_entity;
        let item = (entity, main_entity);
        let ready = groups.is_some_and(|groups| groups.draw.is_some());
        let draws = has_grass && ready && !no_grass;
        let prepass_key = pipelines
            .prepass_view_keys
            .get(&retained)
            .map(|key| *key | triangles);

        // A deferred camera draws grass into its G-buffer and nowhere else.
        let mut deferred = false;
        if let Some(phase) = phases.deferred.get_mut(&retained) {
            phase.remove(main_entity);
            if let (true, true, true, Some(mesh_key)) =
                (draws, deferred_prepass, deferred_default, prepass_key)
            {
                let pipeline = pipelines.prepass_specialized.specialize(
                    &pipelines.cache,
                    &pipelines.prepass,
                    with_features(mesh_key | MeshPipelineKey::DEFERRED_PREPASS),
                );
                add_prepass_item(phase, pipeline, draw_deferred, item);
                deferred = true;
            }
        }

        // A forward camera with a prepass draws grass there too, so SSAO,
        // TAA and anything else reading the prepass sees it.
        if let Some(phase) = phases.prepass.get_mut(&retained) {
            phase.remove(main_entity);
            if let (true, false, Some(mesh_key)) = (draws, deferred, prepass_key) {
                let pipeline = pipelines.prepass_specialized.specialize(
                    &pipelines.cache,
                    &pipelines.prepass,
                    with_features(mesh_key),
                );
                add_prepass_item(phase, pipeline, draw_prepass, item);
            }
        }

        let Some(opaque) = phases.opaque.get_mut(&retained) else {
            continue;
        };
        opaque.remove(main_entity);
        if draws
            && !deferred
            && let Some(&view_key) = pipelines.view_keys.get(&retained)
        {
            let pipeline = pipelines.main_specialized.specialize(
                &pipelines.cache,
                &pipelines.main,
                with_features(view_key | triangles),
            );
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
                item,
                InputUniformIndex::default(),
                BinnedRenderPhaseType::NonMesh,
            );
        }

        if let Some(lights) = lights {
            let pipeline = (draws && pipelines.shadow.supported).then(|| {
                pipelines.shadow_specialized.specialize(
                    &pipelines.cache,
                    &pipelines.shadow,
                    features.geometry(),
                )
            });
            queue_shadows(
                &mut phases.shadow,
                lights,
                &light_views,
                item,
                pipeline,
                draw_shadow,
                extracted
                    .settings
                    .for_camera(grass_camera)
                    .shadows
                    .cascades(),
            );
        }
    }
}

/// Puts the grass item into the shadow phase of every directional-light
/// cascade the shadow mode draws, and takes it out of the others. `pipeline`
/// is `None` when the camera draws no grass.
fn queue_shadows(
    phases: &mut ViewBinnedRenderPhases<Shadow>,
    lights: &ViewLightEntities,
    light_views: &Query<(&ExtractedView, &LightEntity)>,
    item: (Entity, MainEntity),
    pipeline: Option<CachedRenderPipelineId>,
    draw_function: DrawFunctionId,
    cascades: usize,
) {
    for &light_view in &lights.lights {
        let Ok((light_view, light)) = light_views.get(light_view) else {
            continue;
        };
        let Some(shadow) = phases.get_mut(&light_view.retained_view_entity) else {
            continue;
        };
        shadow.remove(item.1);
        let casts = matches!(light, LightEntity::Directional { cascade_index, .. }
            if *cascade_index < cascades);
        if let (true, Some(pipeline)) = (casts, pipeline) {
            shadow.add(
                ShadowBatchSetKey {
                    pipeline,
                    draw_function,
                    material_bind_group_index: None,
                    slabs: MeshSlabs::default(),
                },
                ShadowBinKey {
                    asset_id: AssetId::<Mesh>::default().untyped(),
                },
                item,
                InputUniformIndex::default(),
                BinnedRenderPhaseType::NonMesh,
            );
        }
    }
}
