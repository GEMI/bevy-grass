//! The far field: grass colour on the ground beyond the grass radius, where
//! no blades are drawn (`shaders/far.wesl`).

use bevy::app::{App, Plugin, PreStartup, Update};
use bevy::asset::{Asset, Assets, Handle, RenderAssetUsages, embedded_asset, uuid_handle};
use bevy::color::{Color, ColorToComponents, LinearRgba};
use bevy::ecs::prelude::*;
use bevy::image::{Image, ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor};
use bevy::math::{Rect, Vec2, Vec4};
use bevy::pbr::{ExtendedMaterial, MaterialExtension, StandardMaterial};
use bevy::reflect::Reflect;
use bevy::render::render_asset::RenderAssets;
use bevy::render::render_resource::{
    AsBindGroup, AsBindGroupShaderType, Extent3d, ShaderType, TextureDimension, TextureFormat,
};
use bevy::render::texture::GpuImage;
use bevy::shader::ShaderRef;

use crate::api::{GrassStyle, GrassWind};
use crate::clock::GrassClock;
use crate::{GrassQuality, GrassSettings};

/// A terrain material whose colour turns to grass with distance from the
/// camera, so the ground beyond the grass radius, where no blades are drawn,
/// still reads as grass, and there is no seam where the blades end. Gusts
/// roll across it with the wind.
///
/// ```no_run
/// # use bevy::prelude::*;
/// # use bevy_grass::prelude::*;
/// fn ground(
///     mut commands: Commands,
///     mut meshes: ResMut<Assets<Mesh>>,
///     mut materials: ResMut<Assets<FarGrassMaterial>>,
/// ) {
///     commands.spawn((
///         Mesh3d(meshes.add(Plane3d::default().mesh().size(500.0, 500.0))),
///         MeshMaterial3d(materials.add(FarGrassMaterial {
///             base: StandardMaterial::from_color(Color::srgb(0.2, 0.15, 0.09)),
///             extension: FarGrass::new(&GrassStyle::MEADOW),
///         })),
///     ));
/// }
/// ```
///
/// The fade follows [`GrassSettings::radius`], and the gusts follow
/// [`GrassWind`], so changing either changes the far grass too. It needs
/// [`FarGrassPlugin`], which `GrassPlugins` includes. A camera that overrides
/// the radius with [`GrassCamera`](crate::GrassCamera) does not move the fade:
/// it belongs to the ground, not to a view.
///
/// Custom terrain shaders can call the same WESL function,
/// `bevy_grass::shaders::far::grass_far_color`.
pub type FarGrassMaterial = ExtendedMaterial<StandardMaterial, FarGrass>;

/// The far-grass part of [`FarGrassMaterial`].
#[derive(Asset, AsBindGroup, Reflect, Debug, Clone)]
#[uniform(100, FarGrassUniform)]
pub struct FarGrass {
    /// The grass's colour seen from afar. [`FarGrass::new`] takes it from a
    /// style.
    pub color: Color,
    /// How fully the far ground takes on that colour, 0 (not at all) to 1.
    pub amount: f32,
    /// Distance from the camera where the tint starts and where it is full,
    /// in metres. `None` (what [`FarGrass::new`] gives) follows
    /// [`GrassSettings::radius`], starting at half of it and full at 95%,
    /// where the blades have thinned out. Set it to override.
    pub fade: Option<Vec2>,
    /// How much gusts brighten the far grass, 0 (not at all) to 1.
    pub sheen: f32,
    /// Where grass grows, in world X and Z. `None`: everywhere the material
    /// is.
    pub area: Option<Rect>,
    /// The wind, kept equal to the [`GrassWind`] resource by the plugin.
    #[reflect(ignore)]
    wind: GrassWind,
    /// [`GrassSettings::radius`], kept equal to the resource by the plugin.
    #[reflect(ignore)]
    radius: f32,
    /// How far the gusts have scrolled, kept equal to the blades' by the
    /// plugin.
    #[reflect(ignore)]
    gust_offset: Vec2,
    #[texture(101)]
    #[sampler(102)]
    gusts: Handle<Image>,
}

impl FarGrass {
    /// Far grass for fields of `style`, fading in where the blades thin out
    /// (see [`FarGrass::fade`]).
    pub fn new(style: &GrassStyle) -> Self {
        let root = LinearRgba::from(style.blade.root_color);
        let tip = LinearRgba::from(style.blade.tip_color);
        // Seen from afar, the upper part of the blades hides the rest. Lit,
        // grass is about half as bright as flat ground of its colour: blades
        // shade each other and catch the sun edge-on, while the ground faces
        // it. Measured in the `meadow` example, in linear light: the last
        // drawn grass averaged 0.52 of the ground tinted without the factor.
        let color = (root.to_vec4().lerp(tip.to_vec4(), 0.8) * 0.5).truncate();
        Self {
            color: LinearRgba::rgb(color.x, color.y, color.z).into(),
            amount: 1.0,
            fade: None,
            sheen: 0.3,
            area: None,
            wind: GrassWind::default(),
            radius: GrassQuality::default().radius(),
            gust_offset: Vec2::ZERO,
            gusts: GUST_TEXTURE,
        }
    }

    /// The same far grass, fully tinted from `fade.y` metres away and
    /// starting at `fade.x`, whatever the grass radius is.
    #[must_use]
    pub fn with_fade(self, fade: Vec2) -> Self {
        Self {
            fade: Some(fade),
            ..self
        }
    }

    /// The same far grass, only inside `area` (world X and Z): usually the
    /// field's rectangle.
    #[must_use]
    pub fn with_area(self, area: Rect) -> Self {
        Self {
            area: Some(area),
            ..self
        }
    }
}

impl Default for FarGrass {
    fn default() -> Self {
        Self::new(&GrassStyle::default())
    }
}

impl MaterialExtension for FarGrass {
    fn fragment_shader() -> ShaderRef {
        FAR_GROUND_SHADER.into()
    }

    fn deferred_fragment_shader() -> ShaderRef {
        FAR_GROUND_SHADER.into()
    }
}

/// The material's shader, embedded by `GrassPlugin`.
const FAR_GROUND_SHADER: &str = "embedded://bevy_grass/shaders/far_ground.wesl";

/// Mirrors `FarGrass` in `far.wesl`.
#[derive(Debug, Clone, Copy, ShaderType)]
pub struct FarGrassUniform {
    color: Vec4,
    fade: Vec4,
    area: Vec4,
    wind: Vec4,
    strength: Vec4,
}

impl AsBindGroupShaderType<FarGrassUniform> for FarGrass {
    fn as_bind_group_shader_type(&self, _images: &RenderAssets<GpuImage>) -> FarGrassUniform {
        let color = LinearRgba::from(self.color).to_vec4().truncate();
        let area = self.area.unwrap_or_default();
        let fade = self
            .fade
            .unwrap_or_else(|| Vec2::new(self.radius * 0.5, self.radius * 0.95));
        FarGrassUniform {
            color: color.extend(self.amount.clamp(0.0, 1.0)),
            fade: Vec4::new(
                fade.x,
                fade.y.max(fade.x + 0.01),
                self.sheen,
                if self.area.is_some() { 1.0 } else { 0.0 },
            ),
            area: Vec4::new(area.min.x, area.min.y, area.max.x, area.max.y),
            wind: Vec4::new(
                self.wind.direction.x,
                self.wind.direction.y,
                self.wind.gust_scale,
                self.wind.gust_speed,
            ),
            strength: Vec4::new(
                self.wind.strength,
                self.gust_offset.x,
                self.gust_offset.y,
                0.0,
            ),
        }
    }
}

/// The gust texture as an image asset, for `FarGrass`. The same noise as
/// the one the blades bend by.
pub(crate) const GUST_TEXTURE: Handle<Image> = uuid_handle!("8d0b6c2e-3f4a-4e57-9a1d-6b2c7e9f0a13");

/// Adds the gust image, if the app has images.
pub(crate) fn add_gust_texture(images: Option<ResMut<Assets<Image>>>) {
    let Some(mut images) = images else {
        return;
    };
    let size = crate::render::resources::GUST_SIZE;
    let mut image = Image::new(
        Extent3d {
            width: size,
            height: size,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        crate::render::resources::gust_noise(size),
        TextureFormat::R8Unorm,
        RenderAssetUsages::RENDER_WORLD,
    );
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::Repeat,
        address_mode_v: ImageAddressMode::Repeat,
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        ..Default::default()
    });
    let _ = images.insert(&GUST_TEXTURE, image);
}

/// Keeps every far-grass material's wind equal to the [`GrassWind`] resource
/// and its fade equal to [`GrassSettings::radius`].
fn sync_with_grass(
    wind: Res<GrassWind>,
    settings: Res<GrassSettings>,
    clock: Res<GrassClock>,
    mut materials: ResMut<Assets<FarGrassMaterial>>,
) {
    let radius = settings.sanitized().radius;
    let stale: Vec<_> = materials
        .iter()
        .filter(|(_, material)| {
            let far = &material.extension;
            far.wind != *wind || far.radius != radius || far.gust_offset != clock.gust_offset
        })
        .map(|(id, _)| id)
        .collect();
    for id in stale {
        if let Some(mut material) = materials.get_mut(id) {
            material.extension.wind = *wind;
            material.extension.radius = radius;
            material.extension.gust_offset = clock.gust_offset;
        }
    }
}

/// Adds [`FarGrassMaterial`]: the terrain material that carries the grass's
/// colour past the last blade. Part of `GrassPlugins`; leave it out when the
/// game has its own ground shader.
#[derive(Debug, Default, Clone, Copy)]
pub struct FarGrassPlugin;

impl Plugin for FarGrassPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "shaders/far_ground.wesl");
        app.init_resource::<GrassWind>()
            .init_resource::<GrassClock>()
            .add_plugins(bevy::pbr::MaterialPlugin::<FarGrassMaterial>::default())
            .add_systems(PreStartup, add_gust_texture)
            .add_systems(Update, sync_with_grass.after(crate::clock::tick));
    }
}
