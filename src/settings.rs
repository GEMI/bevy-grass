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
/// [`GrassStyle`](crate::GrassStyle) and [`GrassWind`](crate::GrassWind):
///
/// ```no_run
/// # use bevy::prelude::*;
/// # use bevy_grass::prelude::*;
/// // High density, but a longer reach for a camera high above the ground.
/// # let mut app = App::new();
/// app.add_plugins(GrassPlugins.set(GrassPlugin {
///     settings: GrassSettings {
///         radius: 140.0,
///         ..GrassSettings::HIGH
///     },
/// }));
///
/// // And later, from the graphics menu:
/// fn apply(mut grass: ResMut<GrassSettings>) {
///     *grass = GrassSettings::LOW;
/// }
/// ```
///
/// The tiers set the first seven settings. Every other setting (the distances
/// where the levels of detail begin, how blades thin out, the blade mesh, the
/// limits of a view, how grass is trampled) is the same in all of them, and
/// its default is the number the crate was designed and measured with: change
/// one when your game is not that game.
///
/// ```no_run
/// # use bevy::prelude::*;
/// # use bevy_grass::prelude::*;
/// # let mut app = App::new();
/// app.insert_resource(GrassSettings {
///     // Full detail twice as far, for a camera that rarely looks far.
///     lod1_start: 0.4,
///     lod2_start: 0.7,
///     // Trails in deep snow last six seconds, and a vehicle flattens five metres.
///     trample_recovery: 6.0,
///     max_interactor_radius: 5.0,
///     ..GrassSettings::HIGH
/// });
/// ```
///
/// Values outside the documented ranges are clamped, with one warning that
/// names the setting.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Reflect)]
#[reflect(Resource, Default, Debug, PartialEq)]
pub struct GrassSettings {
    /// How far grass reaches from a perspective camera, in metres; at
    /// least 1. The distances below that shape the thinning and the levels
    /// of detail ([`lod1_start`](Self::lod1_start),
    /// [`lod2_start`](Self::lod2_start), [`fade_start`](Self::fade_start))
    /// are shares of it. Orthographic cameras draw what they see and ignore
    /// it.
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
    /// detail) to 2; every blade is the same size on their screen, so
    /// distance cannot pick it. How many rows each level has is
    /// [`blade_rows`](Self::blade_rows).
    ///
    /// LOD 1 (3 rows by default) at DEMO ARPG GAME's camera, where a 1 m blade is about 64 px
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

    // The blade mesh.
    /// Rows in a blade of each level of detail, LOD 0 (nearest) to LOD 2,
    /// 1 to 31 each. A row is two vertices, and every blade also has a tip
    /// vertex: 7 rows is 15 vertices and 13 triangles, 1 row one triangle.
    /// More rows make a blade bend in a smoother curve (the benefit of tall
    /// grass in close-ups); fewer cost less to draw, for stylized or
    /// low-poly grass. Changing it makes the plugin build the blade meshes
    /// again, once.
    pub blade_rows: [u32; 3],

    // How density and detail fall off with distance (perspective cameras).
    /// Where the first level of detail (LOD 1, 3 rows) begins, as a share of
    /// [`radius`](Self::radius), 0 to 1. Up to here every blade is drawn at
    /// full density and full detail (LOD 0, 7 rows); a camera close to the
    /// ground, or a game of small grass seen up close, can push it out.
    pub lod1_start: f32,
    /// Where LOD 2 (1 row) begins, as a share of the radius, from
    /// `lod1_start` to 1.
    pub lod2_start: f32,
    /// Where the last fade begins, as a share of the radius, from
    /// `lod2_start` to 1. From here the grass thins out to nothing at the
    /// radius, so its edge never shows as a line; at 1 it ends abruptly.
    pub fade_start: f32,
    /// Width of the dithered band around each level-of-detail boundary, in
    /// metres; at least 0.01. Inside it the level changes blade by blade, so
    /// the boundary dissolves into noise instead of a ring: a wider band is
    /// softer, a narrower one saves a few LOD 0 blades.
    pub lod_blend: f32,
    /// The share of the blades drawn where LOD 2 begins, 0 to 1. Density
    /// falls in a straight line from 100% at `lod1_start` to this at
    /// `lod2_start`.
    pub lod2_density: f32,
    /// The share of the blades drawn where the last fade begins, 0 to 1,
    /// reached in a straight line from `lod2_density` at `lod2_start`.
    pub fade_density: f32,
    /// A further share of the blades kept from the start of LOD 1 on, 0 to
    /// 1, on top of the thinning above; the blades that stay widen to cover
    /// the gaps. GPUs shade thin blades poorly, so halving the mid-range
    /// cost 16% in the benchmark and looked the same; raise it toward 1 for
    /// a lusher mid-distance at a price.
    pub mid_range_density: f32,
    /// Most that blades widen as thinning leaves fewer of them, as a
    /// multiple of their own width, 1 (never) to 3.2. Widening keeps the
    /// coverage the same while the blades are fewer.
    pub max_widening: f32,
    /// How far a blade turning edge-on to the camera is rotated toward it,
    /// 0 (never; blades thin out to lines when seen side-on) to 1 (it
    /// always faces the camera, which flattens the grass).
    pub turn_to_camera: f32,

    // Limits of the view.
    /// Half the side of the square around an orthographic camera in which
    /// blades are stored, in metres, 8 to
    /// [`GrassSettings::MAX_ORTHOGRAPHIC_EXTENT`]. A blade further from the
    /// camera than this (horizontally, and vertically down to a floor of
    /// 256 m) is misplaced, so a strategy game that sees a kilometre wants
    /// it raised. Blade positions take 16 bits, so a bigger extent is
    /// coarser: 256 m gives 8 mm, 4,096 m gives 12 cm. Perspective cameras
    /// use their radius instead.
    pub orthographic_extent: f32,
    /// Slices of visible chunks each camera generates in a frame, 1 to
    /// [`GrassSettings::MAX_CHUNK_SLICES`]. A chunk is one slice for every
    /// 512 blades it may hold, so a very dense style or a long radius needs
    /// more; the rest of the grass is dropped with a warning. Costs 16 bytes
    /// of GPU memory per slice, per camera. The bench peaks at 739 at Ultra.
    pub max_chunk_slices: u32,
    /// Frames between the checks that read the GPU's counters back for
    /// [`GrassStats`](crate::GrassStats) and the overflow warnings; 0 never
    /// reads them (a shipped game that does not show the numbers). A debug
    /// overlay wants a small number.
    pub readback_interval: u32,

    // Trampling by [`GrassInteractor`](crate::GrassInteractor)s.
    /// Seconds of game time a fully flattened blade takes to spring back, at
    /// least 0.01. Raise it for deep snow or mud, lower it for stiff stalks.
    pub trample_recovery: f32,
    /// Side of one cell of a camera's trample window, in metres, 0.05 to 2.
    /// The window is 256 cells across, so 0.25 m gives a 64 m window; a
    /// bigger cell reaches further, a smaller one makes narrower trails.
    pub trample_cell_size: f32,
    /// Largest [`GrassInteractor`](crate::GrassInteractor) radius stamped,
    /// in metres, 0.1 to [`GrassSettings::MAX_INTERACTOR_RADIUS`]. A bigger
    /// radius is cut to it. The cost of an interactor grows with the square
    /// of its radius.
    pub max_interactor_radius: f32,
}

impl GrassSettings {
    /// Most blades a camera can hold: the buffers must fit in a storage
    /// binding, which is 128 MiB on the GPUs Bevy supports by default.
    pub const MAX_INSTANCE_CAPACITY: u32 = 4_000_000;
    /// Most interactors a camera can stamp.
    pub const MAX_INTERACTORS: u32 = 65_536;
    /// Most chunk slices a camera can generate: one compute dispatch holds at
    /// most 65,535 workgroups.
    pub const MAX_CHUNK_SLICES: u32 = 65_535;
    /// Largest `orthographic_extent`, in metres.
    pub const MAX_ORTHOGRAPHIC_EXTENT: f32 = 4096.0;
    /// Largest `max_interactor_radius`, in metres.
    pub const MAX_INTERACTOR_RADIUS: f32 = 16.0;
    /// Most rows a blade of any level of detail can have: the vertices of a
    /// blade must fit in the 64 that separate one level from the next.
    pub const MAX_BLADE_ROWS: u32 = 31;
    /// Largest `max_widening`: a blade's width is stored up to four times
    /// its style's, and varies by up to 25% on its own.
    pub const MAX_WIDENING: f32 = 3.2;

    /// Steam Deck and integrated GPUs.
    pub const LOW: Self = Self {
        radius: 40.0,
        density_scale: 0.6,
        min_width_px: 1.0,
        shadows: GrassShadows::Off,
        orthographic_lod: 1,
        instance_capacity: 80_000,
        max_interactors: 64,
        ..Self::HIGH
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
        ..Self::HIGH
    };
    /// RTX 3060 / RX 6700 class. The default. The tiers differ in the first
    /// seven settings only; the rest are the same for all of them.
    pub const HIGH: Self = Self {
        radius: 100.0,
        density_scale: 1.0,
        min_width_px: 1.0,
        shadows: GrassShadows::Near,
        orthographic_lod: 1,
        instance_capacity: 600_000,
        max_interactors: 64,
        blade_rows: [7, 3, 1],
        lod1_start: 0.2,
        lod2_start: 0.5,
        fade_start: 0.9,
        lod_blend: 4.0,
        lod2_density: 0.5,
        fade_density: 0.15,
        mid_range_density: 0.5,
        max_widening: 2.5,
        turn_to_camera: 0.4,
        orthographic_extent: 256.0,
        max_chunk_slices: 8192,
        readback_interval: 120,
        trample_recovery: 1.5,
        trample_cell_size: 0.25,
        max_interactor_radius: 2.0,
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
        ..Self::HIGH
    };
    /// A 240 Hz / competitive-game tier: the shortest reach, half density and
    /// no grass shadow casting. It still receives shadows. Measure it with
    /// `tools/bench_240.py`; the numbers are deliberately conservative and
    /// may be tuned after those runs.
    pub const COMPETITIVE: Self = Self {
        radius: 40.0,
        density_scale: 0.5,
        min_width_px: 1.0,
        shadows: GrassShadows::Off,
        orthographic_lod: 1,
        instance_capacity: 100_000,
        max_interactors: 64,
        ..Self::HIGH
    };

    /// These settings with every value brought into its documented range.
    #[must_use]
    pub fn sanitized(self) -> Self {
        // A NaN is not in any range; it falls back to the default.
        let finite = |value: f32, fallback: f32| if value.is_finite() { value } else { fallback };
        let high = Self::HIGH;
        // The distances keep their order: LOD 1, then LOD 2, then the fade.
        let lod1_start = finite(self.lod1_start, high.lod1_start).clamp(0.0, 1.0);
        let lod2_start = finite(self.lod2_start, high.lod2_start).clamp(lod1_start, 1.0);
        let fade_start = finite(self.fade_start, high.fade_start).clamp(lod2_start, 1.0);
        Self {
            radius: finite(self.radius, high.radius).max(1.0),
            density_scale: finite(self.density_scale, high.density_scale).max(0.0),
            min_width_px: finite(self.min_width_px, high.min_width_px).max(0.0),
            orthographic_lod: self.orthographic_lod.min(2),
            instance_capacity: self.instance_capacity.clamp(2, Self::MAX_INSTANCE_CAPACITY),
            max_interactors: self.max_interactors.clamp(1, Self::MAX_INTERACTORS),
            blade_rows: self
                .blade_rows
                .map(|rows| rows.clamp(1, Self::MAX_BLADE_ROWS)),
            lod1_start,
            lod2_start,
            fade_start,
            lod_blend: finite(self.lod_blend, high.lod_blend).max(0.01),
            lod2_density: finite(self.lod2_density, high.lod2_density).clamp(0.0, 1.0),
            fade_density: finite(self.fade_density, high.fade_density).clamp(0.0, 1.0),
            mid_range_density: finite(self.mid_range_density, high.mid_range_density)
                .clamp(0.0, 1.0),
            max_widening: finite(self.max_widening, high.max_widening)
                .clamp(1.0, Self::MAX_WIDENING),
            turn_to_camera: finite(self.turn_to_camera, high.turn_to_camera).clamp(0.0, 1.0),
            orthographic_extent: finite(self.orthographic_extent, high.orthographic_extent)
                .clamp(8.0, Self::MAX_ORTHOGRAPHIC_EXTENT),
            max_chunk_slices: self.max_chunk_slices.clamp(1, Self::MAX_CHUNK_SLICES),
            readback_interval: self.readback_interval,
            trample_recovery: finite(self.trample_recovery, high.trample_recovery).max(0.01),
            trample_cell_size: finite(self.trample_cell_size, high.trample_cell_size)
                .clamp(0.05, 2.0),
            max_interactor_radius: finite(self.max_interactor_radius, high.max_interactor_radius)
                .clamp(0.1, Self::MAX_INTERACTOR_RADIUS),
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
        macro_rules! check_all {
            ($($field:ident),* $(,)?) => {$(
                check(
                    stringify!($field),
                    self.$field.to_string(),
                    fixed.$field.to_string(),
                );
            )*};
        }
        check_all!(
            radius,
            density_scale,
            min_width_px,
            orthographic_lod,
            instance_capacity,
            max_interactors,
            lod1_start,
            lod2_start,
            fade_start,
            lod_blend,
            lod2_density,
            fade_density,
            mid_range_density,
            max_widening,
            turn_to_camera,
            orthographic_extent,
            max_chunk_slices,
            trample_recovery,
            trample_cell_size,
            max_interactor_radius,
        );
        check(
            "blade_rows",
            format!("{:?}", self.blade_rows),
            format!("{:?}", fixed.blade_rows),
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

/// A hardware tier, for a graphics menu that offers "Competitive / Low /
/// Medium / High / Ultra". [`GrassSettings`] is what the plugin uses;
/// `GrassSettings::from` turns a tier into one, and `GrassSettings::LOW` and
/// the others are the same presets without the enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Reflect)]
#[reflect(Default, Debug, PartialEq)]
pub enum GrassQuality {
    /// 240 Hz and competitive games: 40 m of half-density grass, no grass
    /// shadow casting. The cheapest tier.
    Competitive,
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
            Self::Competitive => GrassSettings::COMPETITIVE,
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
            (
                GrassQuality::Competitive,
                40.0,
                0.5,
                GrassShadows::Off,
                1,
                100_000,
            ),
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
            GrassSettings::COMPETITIVE,
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

    #[test]
    fn the_new_knobs_default_to_the_numbers_that_were_once_fixed() {
        // Every tier shares them, and they are what the shaders and the
        // golden images were made with: changing one changes every picture.
        for tier in [
            GrassSettings::COMPETITIVE,
            GrassSettings::LOW,
            GrassSettings::MEDIUM,
            GrassSettings::HIGH,
            GrassSettings::ULTRA,
        ] {
            assert_eq!(tier.blade_rows, [7, 3, 1]);
            assert_eq!((tier.lod1_start, tier.lod2_start), (0.2, 0.5));
            assert_eq!(tier.fade_start, 0.9);
            assert_eq!(tier.lod_blend, 4.0);
            assert_eq!((tier.lod2_density, tier.fade_density), (0.5, 0.15));
            assert_eq!(tier.mid_range_density, 0.5);
            assert_eq!(tier.max_widening, 2.5);
            assert_eq!(tier.turn_to_camera, 0.4);
            assert_eq!(tier.orthographic_extent, 256.0);
            assert_eq!(tier.max_chunk_slices, 8192);
            assert_eq!(tier.readback_interval, 120);
            assert_eq!(tier.trample_recovery, 1.5);
            assert_eq!(tier.trample_cell_size, 0.25);
            assert_eq!(tier.max_interactor_radius, 2.0);
        }
    }

    #[test]
    fn the_new_knobs_are_clamped_and_named_like_the_others() {
        let bad = GrassSettings {
            blade_rows: [0, 99, 5],
            lod_blend: 0.0,
            lod2_density: 7.0,
            fade_density: -1.0,
            mid_range_density: f32::NAN,
            max_widening: 50.0,
            turn_to_camera: -2.0,
            orthographic_extent: 1.0e9,
            max_chunk_slices: 0,
            trample_recovery: 0.0,
            trample_cell_size: 0.0,
            max_interactor_radius: 1000.0,
            ..GrassSettings::HIGH
        };
        let fixed = bad.sanitized();
        assert_eq!(fixed.blade_rows, [1, GrassSettings::MAX_BLADE_ROWS, 5]);
        assert_eq!(fixed.lod_blend, 0.01);
        assert_eq!((fixed.lod2_density, fixed.fade_density), (1.0, 0.0));
        assert_eq!(fixed.mid_range_density, 0.5, "NaN: the default");
        assert_eq!(fixed.max_widening, GrassSettings::MAX_WIDENING);
        assert_eq!(fixed.turn_to_camera, 0.0);
        assert_eq!(
            fixed.orthographic_extent,
            GrassSettings::MAX_ORTHOGRAPHIC_EXTENT
        );
        assert_eq!(fixed.max_chunk_slices, 1);
        assert_eq!(fixed.trample_recovery, 0.01);
        assert_eq!(fixed.trample_cell_size, 0.05);
        assert_eq!(
            fixed.max_interactor_radius,
            GrassSettings::MAX_INTERACTOR_RADIUS
        );
        assert_eq!(fixed.sanitized(), fixed, "clamping is idempotent");

        let problems = bad.problems().join("; ");
        for name in [
            "blade_rows",
            "lod_blend",
            "lod2_density",
            "fade_density",
            "mid_range_density",
            "max_widening",
            "turn_to_camera",
            "orthographic_extent",
            "max_chunk_slices",
            "trample_recovery",
            "trample_cell_size",
            "max_interactor_radius",
        ] {
            assert!(problems.contains(name), "no mention of {name}: {problems}");
        }

        // The largest limits are ones the GPU can honour: a dispatch holds
        // 65,535 workgroups, and the widest blade fits its stored width.
        assert_eq!(GrassSettings::MAX_CHUNK_SLICES, 65_535);
        const { assert!(1.25 * GrassSettings::MAX_WIDENING <= 4.0) };
        // Sixteen bits of position over the widest extent is still centimetres.
        const { assert!(2.0 * GrassSettings::MAX_ORTHOGRAPHIC_EXTENT / 65_535.0 < 0.15) };
        // A zero readback interval means never, and is valid.
        let never = GrassSettings {
            readback_interval: 0,
            ..GrassSettings::HIGH
        };
        assert_eq!(never.sanitized(), never);
        assert!(never.problems().is_empty());
    }

    #[test]
    fn the_distances_keep_their_order() {
        // LOD 1, then LOD 2, then the fade, each within the radius.
        let backwards = GrassSettings {
            lod1_start: 0.8,
            lod2_start: 0.3,
            fade_start: 0.1,
            ..GrassSettings::HIGH
        };
        let fixed = backwards.sanitized();
        assert_eq!(
            (fixed.lod1_start, fixed.lod2_start, fixed.fade_start),
            (0.8, 0.8, 0.8)
        );
        assert!(backwards.problems().join(";").contains("lod2_start"));
        let beyond = GrassSettings {
            lod1_start: 4.0,
            ..GrassSettings::HIGH
        };
        assert_eq!(beyond.sanitized().lod1_start, 1.0);
        assert_eq!(beyond.sanitized().fade_start, 1.0, "the fade follows");
        // A camera sees them through its own radius.
        let near = GrassSettings {
            radius: 50.0,
            ..GrassSettings::HIGH
        };
        let seen = near.for_camera(Some(&GrassCamera {
            radius: Some(30.0),
            ..GrassCamera::default()
        }));
        assert_eq!(seen.lod1_start, 0.2, "shares of whichever radius is used");
    }
}
