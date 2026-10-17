//! Grass quality: one resource, [`GrassSettings`], with a preset per hardware
//! tier, built the way styles and wind are: a `const` preset plus struct
//! update.

use bevy::ecs::reflect::ReflectResource;
use bevy::ecs::resource::Resource;
use bevy::reflect::Reflect;
use bevy::reflect::std_traits::ReflectDefault;

use crate::api::GrassCamera;

/// How much grass is drawn, how far, and at what cost.
///
/// Every setting can change at any time, from a settings menu for instance:
/// the next frame draws with the new values. Raising `instance_capacity` or
/// `max_interactors` also reallocates each camera's buffers, once.
///
/// Start from a tier and change what you want, as with
/// [`GrassStyle`](crate::GrassStyle) and [`Wind`](crate::Wind):
///
/// ```no_run
/// # use bevy::prelude::*;
/// # use bevy_grass::prelude::*;
/// // High density, but a longer reach for a camera high above the ground.
/// # let mut app = App::new();
/// app.add_plugins(GrassPlugin {
///     settings: GrassSettings {
///         radius: 140.0,
///         ..GrassSettings::HIGH
///     },
/// });
///
/// // And later, from the graphics menu:
/// fn apply(mut grass: ResMut<GrassSettings>) {
///     *grass = GrassSettings::LOW;
/// }
/// ```
///
/// Values outside the documented ranges are clamped, with one warning that
/// names the setting.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Reflect)]
#[reflect(Resource, Default, Debug, PartialEq)]
pub struct GrassSettings {
    /// How far grass reaches from a perspective camera, in metres; at
    /// least 1. LOD 1 starts at 20% of it and LOD 2 at 50%. Orthographic
    /// cameras draw what they see and ignore it.
    pub radius: f32,
    /// Multiplier on every style's density, 0 (no grass) and up; 1 draws
    /// styles as they are written.
    pub density_scale: f32,
    /// Minimum on-screen width of a blade, in pixels, 0 (no minimum) and up.
    /// Blades narrower than a pixel shimmer; raise this for cameras far from
    /// the ground. A pixel width depends on the camera, not on the grass, so
    /// it lives here and, per camera, in [`GrassCamera`](crate::GrassCamera).
    pub min_width_px: f32,
    /// Which blades cast shadows.
    pub shadows: GrassShadows,
    /// The level of detail orthographic cameras draw everything at, 0 (most
    /// detail, 7 rows) to 2; every blade is the same size on their screen, so
    /// distance cannot pick it.
    ///
    /// LOD 1 (3 rows) at DEMO ARPG GAME's camera, where a 1 m blade is about 64 px
    /// tall, could not be told from LOD 0 at 1:1 and drew 9% faster, so only
    /// Ultra pays for 7 rows.
    pub orthographic_lod: u32,
    /// Visible blades each camera's buffers hold, 2 to
    /// [`GrassSettings::MAX_INSTANCE_CAPACITY`]. Blades beyond it are dropped
    /// with a warning. Costs 32 bytes of GPU memory per blade, per camera.
    pub instance_capacity: u32,
    /// Most [`GrassInteractor`](crate::GrassInteractor)s a camera stamps a
    /// frame, nearest to the camera first, 1 to
    /// [`GrassSettings::MAX_INTERACTORS`]. A game with a horde wants more,
    /// and 512 costs about the same as 64.
    pub max_interactors: u32,
}

impl GrassSettings {
    /// Most blades a camera can hold: the buffers must fit in a storage
    /// binding, which is 128 MiB on the GPUs Bevy supports by default.
    pub const MAX_INSTANCE_CAPACITY: u32 = 4_000_000;
    /// Most interactors a camera can stamp.
    pub const MAX_INTERACTORS: u32 = 65_536;

    /// Steam Deck and integrated GPUs.
    pub const LOW: Self = Self {
        radius: 40.0,
        density_scale: 0.6,
        min_width_px: 1.0,
        shadows: GrassShadows::Off,
        orthographic_lod: 1,
        instance_capacity: 80_000,
        max_interactors: 64,
    };
    /// GTX 1660 / RX 5600 class.
    pub const MEDIUM: Self = Self {
        radius: 70.0,
        density_scale: 0.8,
        min_width_px: 1.0,
        shadows: GrassShadows::Near,
        orthographic_lod: 1,
        instance_capacity: 250_000,
        max_interactors: 64,
    };
    /// RTX 3060 / RX 6700 class. The default.
    pub const HIGH: Self = Self {
        radius: 100.0,
        density_scale: 1.0,
        min_width_px: 1.0,
        shadows: GrassShadows::Near,
        orthographic_lod: 1,
        instance_capacity: 600_000,
        max_interactors: 64,
    };
    /// RTX 4070 class and up.
    pub const ULTRA: Self = Self {
        radius: 150.0,
        density_scale: 1.0,
        min_width_px: 1.0,
        shadows: GrassShadows::Full,
        orthographic_lod: 0,
        instance_capacity: 1_200_000,
        max_interactors: 64,
    };

    /// These settings with every value brought into its documented range.
    #[must_use]
    pub fn sanitized(self) -> Self {
        // A NaN is not in any range; it falls back to the default.
        let finite = |value: f32, fallback: f32| if value.is_finite() { value } else { fallback };
        Self {
            radius: finite(self.radius, Self::HIGH.radius).max(1.0),
            density_scale: finite(self.density_scale, Self::HIGH.density_scale).max(0.0),
            min_width_px: finite(self.min_width_px, Self::HIGH.min_width_px).max(0.0),
            orthographic_lod: self.orthographic_lod.min(2),
            instance_capacity: self.instance_capacity.clamp(2, Self::MAX_INSTANCE_CAPACITY),
            max_interactors: self.max_interactors.clamp(1, Self::MAX_INTERACTORS),
            ..self
        }
    }

    /// These settings as one camera draws with them: its [`GrassCamera`]
    /// overrides, where it has any, over this resource.
    #[must_use]
    pub fn for_camera(&self, camera: Option<&GrassCamera>) -> Self {
        let Some(camera) = camera else {
            return *self;
        };
        Self {
            radius: camera.radius.unwrap_or(self.radius),
            density_scale: camera.density_scale.unwrap_or(self.density_scale),
            min_width_px: camera.min_width_px.unwrap_or(self.min_width_px),
            shadows: camera.shadows.unwrap_or(self.shadows),
            ..*self
        }
        .sanitized()
    }

    /// What `sanitized` changed, one line per setting, for the warning.
    pub(crate) fn problems(&self) -> Vec<String> {
        let fixed = self.sanitized();
        let mut problems = Vec::new();
        let mut check = |name: &str, was: String, now: String| {
            if was != now {
                problems.push(format!("{name} {was} is out of range, using {now}"));
            }
        };
        check("radius", self.radius.to_string(), fixed.radius.to_string());
        check(
            "density_scale",
            self.density_scale.to_string(),
            fixed.density_scale.to_string(),
        );
        check(
            "min_width_px",
            self.min_width_px.to_string(),
            fixed.min_width_px.to_string(),
        );
        check(
            "orthographic_lod",
            self.orthographic_lod.to_string(),
            fixed.orthographic_lod.to_string(),
        );
        check(
            "instance_capacity",
            self.instance_capacity.to_string(),
            fixed.instance_capacity.to_string(),
        );
        check(
            "max_interactors",
            self.max_interactors.to_string(),
            fixed.max_interactors.to_string(),
        );
        problems
    }
}

impl Default for GrassSettings {
    fn default() -> Self {
        Self::HIGH
    }
}

impl From<GrassQuality> for GrassSettings {
    fn from(quality: GrassQuality) -> Self {
        quality.settings()
    }
}

/// A hardware tier, for a graphics menu that offers "Low / Medium / High /
/// Ultra". [`GrassSettings`] is what the plugin uses; `GrassSettings::from`
/// turns a tier into one, and `GrassSettings::LOW` and the others are the
/// same presets without the enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Reflect)]
#[reflect(Default, Debug, PartialEq)]
pub enum GrassQuality {
    /// Steam Deck and integrated GPUs.
    Low,
    /// GTX 1660 / RX 5600 class.
    Medium,
    /// RTX 3060 / RX 6700 class.
    #[default]
    High,
    /// RTX 4070 class and up.
    Ultra,
}

impl GrassQuality {
    /// The tier's [`GrassSettings`].
    pub const fn settings(self) -> GrassSettings {
        match self {
            Self::Low => GrassSettings::LOW,
            Self::Medium => GrassSettings::MEDIUM,
            Self::High => GrassSettings::HIGH,
            Self::Ultra => GrassSettings::ULTRA,
        }
    }

    /// Visible blades the tier is budgeted for.
    pub const fn instance_capacity(self) -> u32 {
        self.settings().instance_capacity
    }

    /// How far grass reaches from a perspective camera, in metres.
    pub const fn radius(self) -> f32 {
        self.settings().radius
    }

    /// Which blades cast shadows.
    pub const fn shadows(self) -> GrassShadows {
        self.settings().shadows
    }

    /// Multiplier on every style's density.
    pub const fn density_scale(self) -> f32 {
        self.settings().density_scale
    }

    /// The level of detail orthographic cameras draw everything at.
    pub const fn orthographic_lod(self) -> u32 {
        self.settings().orthographic_lod
    }
}

/// Which blades cast shadows from directional lights. Grass always
/// receives shadows; casting is the expensive part.
///
/// A field of [`GrassSettings`]: change it at any time, from a settings menu
/// for instance, and the next frame draws with the new mode.
///
/// ```no_run
/// # use bevy::prelude::*;
/// # use bevy_grass::prelude::*;
/// fn toggle_grass_shadows(mut grass: ResMut<GrassSettings>) {
///     grass.shadows = if grass.shadows == GrassShadows::Off {
///         GrassShadows::Near
///     } else {
///         GrassShadows::Off
///     };
/// }
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Reflect)]
#[reflect(Default, Debug, PartialEq)]
pub enum GrassShadows {
    /// No grass shadows.
    Off,
    /// Full-detail blades (LOD 0) into the first shadow cascade only: the
    /// shadows a viewer can resolve, at a fraction of the cost.
    #[default]
    Near,
    /// LOD 0 and LOD 1 blades into the first two cascades.
    Full,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_tiers_keep_the_numbers_they_always_had() {
        // (radius, density scale, shadows, orthographic LOD, capacity)
        let expected = [
            (GrassQuality::Low, 40.0, 0.6, GrassShadows::Off, 1, 80_000),
            (
                GrassQuality::Medium,
                70.0,
                0.8,
                GrassShadows::Near,
                1,
                250_000,
            ),
            (
                GrassQuality::High,
                100.0,
                1.0,
                GrassShadows::Near,
                1,
                600_000,
            ),
            (
                GrassQuality::Ultra,
                150.0,
                1.0,
                GrassShadows::Full,
                0,
                1_200_000,
            ),
        ];
        for (tier, radius, density, shadows, lod, capacity) in expected {
            let settings = GrassSettings::from(tier);
            assert_eq!(settings.radius, radius, "{tier:?}");
            assert_eq!(settings.density_scale, density, "{tier:?}");
            assert_eq!(settings.min_width_px, 1.0, "{tier:?}");
            assert_eq!(settings.shadows, shadows, "{tier:?}");
            assert_eq!(settings.orthographic_lod, lod, "{tier:?}");
            assert_eq!(settings.instance_capacity, capacity, "{tier:?}");
            assert_eq!(settings.max_interactors, 64, "{tier:?}");
            // The convenience accessors agree.
            assert_eq!(tier.radius(), radius);
            assert_eq!(tier.instance_capacity(), capacity);
            assert_eq!(tier.shadows(), shadows);
        }
        assert_eq!(GrassSettings::default(), GrassSettings::HIGH);
        assert_eq!(GrassQuality::default().settings(), GrassSettings::default());
    }

    #[test]
    fn a_tier_can_be_overridden_one_knob_at_a_time_even_in_a_const() {
        // The point of the type: High, but a longer reach, no `Option`s.
        const TALL_CAMERA: GrassSettings = GrassSettings {
            radius: 140.0,
            ..GrassSettings::HIGH
        };
        assert_eq!(TALL_CAMERA.radius, 140.0);
        assert_eq!(
            TALL_CAMERA.instance_capacity,
            GrassSettings::HIGH.instance_capacity
        );
        assert_eq!(TALL_CAMERA.shadows, GrassSettings::HIGH.shadows);
        assert_ne!(TALL_CAMERA, GrassSettings::ULTRA);
    }

    #[test]
    fn the_settings_are_a_reflected_resource() {
        use bevy::app::App;
        use bevy::ecs::reflect::AppTypeRegistry;
        use std::any::TypeId;

        let mut app = App::new();
        app.register_type::<GrassSettings>()
            .register_type::<GrassShadows>();
        let registry = app.world().resource::<AppTypeRegistry>().read();
        // Inspectors and scenes find the resource through these.
        let id = TypeId::of::<GrassSettings>();
        assert!(registry.get_type_data::<ReflectResource>(id).is_some());
        assert!(registry.get_type_data::<ReflectDefault>(id).is_some());
        assert!(
            registry
                .get_type_data::<ReflectDefault>(TypeId::of::<GrassShadows>())
                .is_some()
        );
    }

    #[test]
    fn a_camera_overrides_only_what_it_names() {
        let settings = GrassSettings::HIGH;
        // No component, or one that names nothing: the resource, exactly.
        assert_eq!(settings.for_camera(None), settings);
        assert_eq!(settings.for_camera(Some(&GrassCamera::default())), settings);

        // A minimap at Low reach next to a main camera at High.
        let minimap = GrassCamera {
            radius: Some(30.0),
            density_scale: Some(0.4),
            min_width_px: Some(3.0),
            shadows: Some(GrassShadows::Off),
        };
        let seen = settings.for_camera(Some(&minimap));
        assert_eq!(seen.radius, 30.0);
        assert_eq!(seen.density_scale, 0.4);
        assert_eq!(seen.min_width_px, 3.0);
        assert_eq!(seen.shadows, GrassShadows::Off);
        // Everything it did not name is the resource's, capacity included.
        assert_eq!(seen.instance_capacity, settings.instance_capacity);
        assert_eq!(seen.max_interactors, settings.max_interactors);
        assert_eq!(seen.orthographic_lod, settings.orthographic_lod);

        // One override leaves the rest following the resource as it changes.
        let radius_only = GrassCamera {
            radius: Some(30.0),
            ..GrassCamera::default()
        };
        let changed = GrassSettings {
            density_scale: 0.5,
            shadows: GrassShadows::Full,
            ..settings
        };
        let seen = changed.for_camera(Some(&radius_only));
        assert_eq!((seen.radius, seen.density_scale), (30.0, 0.5));
        assert_eq!(seen.shadows, GrassShadows::Full);

        // An override is clamped like the resource.
        let bad = GrassCamera {
            radius: Some(-1.0),
            density_scale: Some(f32::NAN),
            ..GrassCamera::default()
        };
        let seen = settings.for_camera(Some(&bad));
        assert_eq!(seen.radius, 1.0);
        assert_eq!(seen.density_scale, settings.density_scale);
    }

    #[test]
    fn the_camera_component_is_reflected() {
        use bevy::app::App;
        use bevy::ecs::reflect::{AppTypeRegistry, ReflectComponent};
        use std::any::TypeId;

        let mut app = App::new();
        app.register_type::<GrassCamera>();
        let registry = app.world().resource::<AppTypeRegistry>().read();
        let id = TypeId::of::<GrassCamera>();
        assert!(registry.get_type_data::<ReflectComponent>(id).is_some());
        assert!(registry.get_type_data::<ReflectDefault>(id).is_some());
    }

    #[test]
    fn every_tier_is_in_range() {
        for tier in [
            GrassSettings::LOW,
            GrassSettings::MEDIUM,
            GrassSettings::HIGH,
            GrassSettings::ULTRA,
        ] {
            assert_eq!(tier.sanitized(), tier);
            assert!(tier.problems().is_empty());
        }
    }

    #[test]
    fn out_of_range_values_are_clamped_and_named() {
        let bad = GrassSettings {
            radius: -5.0,
            density_scale: f32::NAN,
            min_width_px: -1.0,
            orthographic_lod: 9,
            instance_capacity: u32::MAX,
            max_interactors: 0,
            ..GrassSettings::HIGH
        };
        let fixed = bad.sanitized();
        assert_eq!(fixed.radius, 1.0);
        assert_eq!(fixed.density_scale, GrassSettings::HIGH.density_scale);
        assert_eq!(fixed.min_width_px, 0.0);
        assert_eq!(fixed.orthographic_lod, 2);
        assert_eq!(
            fixed.instance_capacity,
            GrassSettings::MAX_INSTANCE_CAPACITY
        );
        assert_eq!(fixed.max_interactors, 1);
        assert_eq!(fixed.sanitized(), fixed, "clamping is idempotent");

        let problems = bad.problems().join("; ");
        for name in [
            "radius",
            "density_scale",
            "min_width_px",
            "orthographic_lod",
            "instance_capacity",
            "max_interactors",
        ] {
            assert!(problems.contains(name), "no mention of {name}: {problems}");
        }
        // The largest allowed capacity fits the default storage binding.
        let bytes = u64::from(GrassSettings::MAX_INSTANCE_CAPACITY) * 32;
        assert!(bytes <= 128 * 1024 * 1024);
    }
}
