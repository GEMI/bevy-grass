//! The far field: grass colour on the ground beyond the grass radius, where
//! no blades are drawn (`shaders/far.wesl`).

use bevy::asset::{Asset, Assets, Handle, RenderAssetUsages, uuid_handle};
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

use crate::GrassQuality;
use crate::api::{GrassStyle, Wind};

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
///             extension: FarGrass::new(&GrassStyle::MEADOW, GrassQuality::High),
///         })),
///     ));
/// }
/// ```
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
    /// in metres. [`FarGrass::new`] matches the quality tier's grass radius:
    /// the tint grows as the blades thin out.
    pub fade: Vec2,
    /// How much gusts brighten the far grass, 0 (not at all) to 1.
    pub sheen: f32,
    /// Where grass grows, in world X and Z. `None`: everywhere the material
    /// is.
    pub area: Option<Rect>,
    /// The wind, kept equal to the [`Wind`] resource by the plugin.
    #[reflect(ignore)]
    wind: Wind,
    #[texture(101)]
    #[sampler(102)]
    gusts: Handle<Image>,
}

impl FarGrass {
    /// Far grass for fields of `style`, fading in where the `quality` tier's
    /// blades thin out.
    pub fn new(style: &GrassStyle, quality: GrassQuality) -> Self {
        let radius = quality.radius();
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
            fade: Vec2::new(radius * 0.5, radius * 0.95),
            sheen: 0.3,
            area: None,
            wind: Wind::default(),
            gusts: GUST_TEXTURE,
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
        Self::new(&GrassStyle::default(), GrassQuality::default())
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
        FarGrassUniform {
            color: color.extend(self.amount.clamp(0.0, 1.0)),
            fade: Vec4::new(
                self.fade.x,
                self.fade.y.max(self.fade.x + 0.01),
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
            strength: Vec4::new(self.wind.strength, 0.0, 0.0, 0.0),
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

/// Keeps every far-grass material's wind equal to the [`Wind`] resource.
pub(crate) fn sync_wind(wind: Res<Wind>, mut materials: ResMut<Assets<FarGrassMaterial>>) {
    let stale: Vec<_> = materials
        .iter()
        .filter(|(_, material)| !same_wind(&material.extension.wind, &wind))
        .map(|(id, _)| id)
        .collect();
    for id in stale {
        if let Some(mut material) = materials.get_mut(id) {
            material.extension.wind = wind.clone();
        }
    }
}

fn same_wind(a: &Wind, b: &Wind) -> bool {
    a.direction == b.direction
        && a.strength == b.strength
        && a.gust_scale == b.gust_scale
        && a.gust_speed == b.gust_speed
}
