//! Whether this GPU can draw grass, decided once when the renderer starts.
//!
//! Grass needs compute shaders, indirect dispatches and draws, storage
//! buffers in the vertex shader, and more storage buffers in one shader stage
//! than WebGPU promises. On a device short of any of them, creating the grass
//! pipelines is a wgpu validation error, and Bevy's renderer stops at the
//! first one. WebGL2 builds are such devices, and so is any browser that
//! offers fewer storage buffers per stage than grass binds. Checking first
//! turns that into one warning and a scene without grass.
//!
//! The storage buffers needed are counted from the bind group layouts the
//! pipelines are made from, not written down, so the check follows any change
//! to the layouts.

use core::fmt;

use bevy::ecs::prelude::*;
use bevy::ecs::reflect::ReflectResource;
use bevy::reflect::Reflect;
use bevy::reflect::std_traits::ReflectDefault;
use bevy::render::render_resource::{
    BindGroupLayoutDescriptor, BindingType, BufferBindingType, DownlevelFlags, ShaderStages,
};
use bevy::render::settings::WgpuLimits;

use super::compute;
use super::resources::GrassLayouts;
use super::view::MAX_INSTANCE_BYTES;

/// Whether this GPU can draw grass, decided once when the renderer starts.
///
/// A device that cannot draw grass gets one warning and no grass; the rest of
/// the scene draws as usual. WebGL2 builds are such devices, and so are
/// browsers that offer fewer storage buffers per shader stage than grass
/// binds. Read it to tell the player, or to hide grass settings that would
/// change nothing:
///
/// ```no_run
/// # use bevy::prelude::*;
/// # use bevy_grass::prelude::*;
/// fn grass_menu(support: Res<GrassSupport>) {
///     if !support.is_supported() {
///         info!("no grass on this device: {}", *support);
///     }
/// }
/// ```
///
/// It is [`GrassSupport::Unknown`] until the renderer has started, and in an
/// app without one.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq, Default, Reflect)]
#[reflect(Resource, Default, Debug, PartialEq)]
pub enum GrassSupport {
    /// Not decided yet: the renderer has not started, or there is none.
    #[default]
    Unknown,
    /// Grass draws.
    Supported,
    /// The device has no compute shaders, which generate the blades. WebGL2
    /// has none; build for the browser with Bevy's `webgpu` feature.
    NoComputeShaders,
    /// The device cannot take dispatch or draw arguments from a buffer, which
    /// is how the blades the GPU generated are drawn.
    NoIndirectExecution,
    /// The device cannot read storage buffers in a vertex shader, which is
    /// where the blades are read from.
    NoVertexStorage,
    /// The device allows fewer storage buffers in one shader stage than a
    /// grass pipeline binds.
    TooFewStorageBuffers {
        /// Storage buffers the busiest grass pipeline binds in one stage.
        needed: u32,
        /// `max_storage_buffers_per_shader_stage` of the device.
        limit: u32,
    },
    /// The device's storage bindings are smaller than a camera's blade
    /// buffer can be at `GrassSettings::MAX_INSTANCE_CAPACITY`.
    StorageBindingTooSmall {
        /// Bytes of the largest blade buffer.
        needed: u64,
        /// `max_storage_buffer_binding_size` of the device.
        limit: u64,
    },
}

impl GrassSupport {
    /// Whether grass draws on this device.
    ///
    /// False while it is still [`GrassSupport::Unknown`].
    #[must_use]
    pub fn is_supported(self) -> bool {
        self == Self::Supported
    }
}

impl fmt::Display for GrassSupport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unknown => f.write_str("not decided yet"),
            Self::Supported => f.write_str("grass draws on this device"),
            Self::NoComputeShaders => f.write_str(
                "the device has no compute shaders (WebGL2 has none; build for the browser \
                 with Bevy's `webgpu` feature)",
            ),
            Self::NoIndirectExecution => {
                f.write_str("the device has no indirect dispatches or draws")
            }
            Self::NoVertexStorage => {
                f.write_str("the device cannot read storage buffers in a vertex shader")
            }
            Self::TooFewStorageBuffers { needed, limit } => write!(
                f,
                "the device allows {limit} storage buffers per shader stage and grass binds \
                 {needed}"
            ),
            Self::StorageBindingTooSmall { needed, limit } => write!(
                f,
                "the device's storage bindings hold {limit} bytes and a camera's blades can \
                 need {needed}"
            ),
        }
    }
}

/// Decides whether a device with these capabilities and limits can run the
/// pipelines made from `layouts`.
pub(crate) fn check(
    flags: DownlevelFlags,
    limits: &WgpuLimits,
    layouts: &GrassLayouts,
) -> GrassSupport {
    if !flags.contains(DownlevelFlags::COMPUTE_SHADERS) {
        return GrassSupport::NoComputeShaders;
    }
    if !flags.contains(DownlevelFlags::INDIRECT_EXECUTION) {
        return GrassSupport::NoIndirectExecution;
    }
    if !flags.contains(DownlevelFlags::VERTEX_STORAGE) {
        return GrassSupport::NoVertexStorage;
    }
    let needed = storage_buffers_needed(layouts);
    let limit = limits.max_storage_buffers_per_shader_stage;
    if needed > limit {
        return GrassSupport::TooFewStorageBuffers { needed, limit };
    }
    let limit = limits.max_storage_buffer_binding_size;
    if MAX_INSTANCE_BYTES > limit {
        return GrassSupport::StorageBindingTooSmall {
            needed: MAX_INSTANCE_BYTES,
            limit,
        };
    }
    GrassSupport::Supported
}

/// The most storage buffers any grass pipeline binds in one shader stage.
///
/// The draws count only the grass group: Bevy's own view groups are the same
/// for every mesh, and Bevy fits them to the device itself.
pub(crate) fn storage_buffers_needed(layouts: &GrassLayouts) -> u32 {
    let compute = compute::ENTRY_POINTS
        .iter()
        .map(|entry| storage_buffers(&layouts.compute(entry), ShaderStages::COMPUTE));
    let draw = [ShaderStages::VERTEX, ShaderStages::FRAGMENT]
        .map(|stage| storage_buffers(core::slice::from_ref(&layouts.draw), stage));
    compute.chain(draw).max().unwrap_or(0)
}

/// Storage buffers that `stage` sees in a pipeline made of `layouts`.
fn storage_buffers(layouts: &[BindGroupLayoutDescriptor], stage: ShaderStages) -> u32 {
    let count = layouts
        .iter()
        .flat_map(|layout| &layout.entries)
        .filter(|entry| {
            entry.visibility.contains(stage)
                && matches!(
                    entry.ty,
                    BindingType::Buffer {
                        ty: BufferBindingType::Storage { .. },
                        ..
                    }
                )
        })
        .count();
    count as u32
}

/// Run condition for the render world's grass systems: only once the device
/// is known to draw grass.
pub(crate) fn supported(support: Option<Res<GrassSupport>>) -> bool {
    support.is_some_and(|support| support.is_supported())
}

#[cfg(test)]
mod tests;
