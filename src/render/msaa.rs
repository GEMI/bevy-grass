//! A camera's MSAA sample count, kept to what the GPU supports.
//!
//! Bevy makes a camera's textures and every pipeline that draws into them
//! with the camera's [`Msaa`] sample count. A count the device does not
//! support is a wgpu validation error, and Bevy's renderer stops at it: the
//! window goes black, grass or not. WebGPU supports only 1 and 4 samples, so
//! `Msaa::Sample2` and `Msaa::Sample8`, which most native GPUs take, stop
//! every browser. Checking the count before the frame is extracted turns that
//! into one warning and the most samples the device has below it.

use bevy::ecs::prelude::*;
use bevy::log::warn;
use bevy::render::render_resource::TextureFormat;
use bevy::render::renderer::RenderAdapter;
use bevy::render::view::Msaa;

/// The sample counts a 3D camera's textures support on this device: a bit
/// per count, 1, 2, 4 and 8 from the lowest bit up.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct MsaaSupport {
    counts: u8,
}

impl Default for MsaaSupport {
    /// Every count, until the renderer has started: nothing is changed
    /// before the device is known.
    fn default() -> Self {
        Self { counts: 0b1111 }
    }
}

impl MsaaSupport {
    /// The sample counts `Msaa` offers.
    const SAMPLES: [u32; 4] = [1, 2, 4, 8];

    /// The formats of a 3D camera's colour target, without HDR and with it,
    /// and of its depth (`CORE_3D_DEPTH_FORMAT`); a count must suit all three.
    const FORMATS: [TextureFormat; 3] = [
        TextureFormat::Rgba8UnormSrgb,
        TextureFormat::Rgba16Float,
        TextureFormat::Depth32Float,
    ];

    /// What `adapter` supports.
    pub(crate) fn of(adapter: &RenderAdapter) -> Self {
        Self::from_fn(|samples| {
            Self::FORMATS.iter().all(|format| {
                adapter
                    .get_texture_format_features(*format)
                    .flags
                    .sample_count_supported(samples)
            })
        })
    }

    fn from_fn(supported: impl Fn(u32) -> bool) -> Self {
        let mut counts = 0;
        for (bit, samples) in Self::SAMPLES.into_iter().enumerate() {
            if supported(samples) {
                counts |= 1 << bit;
            }
        }
        Self { counts }
    }

    fn supports(self, samples: u32) -> bool {
        Self::SAMPLES
            .iter()
            .position(|count| *count == samples)
            .is_some_and(|bit| self.counts & (1 << bit) != 0)
    }

    /// The most samples, at most `samples`, this device supports. One
    /// sample is no multisampling, which every device has.
    fn fallback(self, samples: u32) -> u32 {
        Self::SAMPLES
            .into_iter()
            .rev()
            .find(|count| *count <= samples && self.supports(*count))
            .unwrap_or(1)
    }

    /// The supported counts, for a warning: "1x and 4x".
    fn list(self) -> String {
        let counts: Vec<String> = Self::SAMPLES
            .into_iter()
            .filter(|samples| self.supports(*samples))
            .map(|samples| format!("{samples}x"))
            .collect();
        counts.join(" and ")
    }
}

/// Lowers a camera's [`Msaa`] that this device does not support to the most
/// samples below it that it does, with one warning per change.
pub(crate) fn fit_msaa(
    support: Res<MsaaSupport>,
    mut cameras: Query<(Entity, &mut Msaa), Changed<Msaa>>,
) {
    for (entity, mut msaa) in &mut cameras {
        let samples = msaa.samples();
        if support.supports(samples) {
            continue;
        }
        let fallback = support.fallback(samples);
        warn!(
            "bevy_grass: the camera {entity} asks for {samples}x MSAA, which this GPU does not \
             support (it supports {}), and Bevy stops rendering at an unsupported sample count; \
             using {fallback}x. Pick one of the supported counts for this device.",
            support.list()
        );
        *msaa = Msaa::from_samples(fallback);
    }
}

#[cfg(test)]
mod tests {
    use super::MsaaSupport;

    /// What WebGPU guarantees, and all a browser offers.
    fn webgpu() -> MsaaSupport {
        MsaaSupport::from_fn(|samples| samples == 1 || samples == 4)
    }

    #[test]
    fn falls_back_to_the_most_samples_below_that_the_device_has() {
        let support = webgpu();
        assert!(support.supports(1) && support.supports(4));
        assert!(!support.supports(2) && !support.supports(8));
        assert_eq!(support.fallback(8), 4);
        assert_eq!(support.fallback(2), 1);
        assert_eq!(support.list(), "1x and 4x");
    }

    #[test]
    fn leaves_every_count_alone_until_the_device_is_known() {
        let unknown = MsaaSupport::default();
        assert!(MsaaSupport::SAMPLES.iter().all(|n| unknown.supports(*n)));
        let none = MsaaSupport::from_fn(|_| false);
        assert_eq!(none.fallback(8), 1);
    }
}
