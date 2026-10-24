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

use crate::api::GrassStyle;
use crate::render::resources::WindUniform;
use crate::wind::{GrassWindFrame, GrassWindSystems};
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
/// [`GrassWindFrame`], so changing either changes the far grass too. It needs
/// [`FarGrassPlugin`], which `GrassPlugins` includes. A camera that overrides
/// the radius with [`GrassCamera`](crate::GrassCamera) does not move the fade:
/// it belongs to the ground, not to a view.
///
/// A terrain shader of your own can call the same WESL function,
/// `bevy_grass::shaders::far::grass_far_color`; [`FarGrass::uniform`] gives
/// its material the data.
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
    /// The wind, kept equal to the [`GrassWindFrame`] resource by the plugin.
    #[reflect(ignore)]
    wind: GrassWindFrame,
    /// [`GrassSettings::radius`], kept equal to the resource by the plugin.
    #[reflect(ignore)]
    radius: f32,
    #[texture(101)]
    #[sampler(102)]
    gusts: Handle<Image>,
}

impl FarGrass {
    /// Far grass for fields of `style`, fading in where the blades thin out
    /// (see [`FarGrass::fade`]).
    #[must_use]
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
            wind: GrassWindFrame::default(),
            radius: GrassQuality::default().radius(),
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

    /// The gust texture `grass_far_color` reads, the same noise the blades
    /// bend by, for a terrain material of your own (see
    /// [`FarGrass::uniform`]).
    pub const GUSTS: Handle<Image> = GUST_TEXTURE;

    /// This far grass as the `FarGrass` uniform of `far.wesl`, with the wind
    /// and the grass radius of this frame, for a terrain material of your
    /// own.
    ///
    /// [`FarGrassMaterial`] keeps its far grass up to date by itself. A game
    /// with its own terrain material gives it a `FarGrassUniform` and the
    /// [`FarGrass::GUSTS`] texture, rewrites the uniform each frame (the
    /// gusts scroll), and calls `grass_far_color` from its shader:
    ///
    /// ```no_run
    /// # use bevy::prelude::*;
    /// # use bevy::render::render_resource::AsBindGroup;
    /// # use bevy::shader::ShaderRef;
    /// # use bevy_grass::prelude::*;
    /// # use bevy_grass::FarGrassUniform;
    /// #[derive(Asset, AsBindGroup, TypePath, Debug, Clone)]
    /// struct Terrain {
    ///     #[uniform(0)]
    ///     far: FarGrassUniform,
    ///     #[texture(1)]
    ///     #[sampler(2)]
    ///     gusts: Handle<Image>,
    /// }
    ///
    /// impl Material for Terrain {
    ///     fn fragment_shader() -> ShaderRef {
    ///         // Imports `bevy_grass::shaders::far::{FarGrass, grass_far_color}`.
    ///         "shaders/terrain.wesl".into()
    ///     }
    /// }
    ///
    /// /// What the terrain's far grass looks like.
    /// #[derive(Resource)]
    /// struct TerrainGrass(FarGrass);
    ///
    /// fn follow_the_grass(
    ///     grass: Res<TerrainGrass>,
    ///     wind: Res<GrassWindFrame>,
    ///     settings: Res<GrassSettings>,
    ///     mut terrains: ResMut<Assets<Terrain>>,
    /// ) {
    ///     let far = grass.0.uniform(&wind, &settings);
    ///     for (_, terrain) in terrains.iter_mut() {
    ///         terrain.far = far;
    ///     }
    /// }
    /// # let _ = FarGrass::GUSTS;
    /// ```
    ///
    /// Put the system after `GrassWindSystems::Sample`, so the far grass and
    /// the blades read the same wind.
    #[must_use]
    pub fn uniform(&self, wind: &GrassWindFrame, settings: &GrassSettings) -> FarGrassUniform {
        Self {
            wind: *wind,
            radius: settings.sanitized().radius,
            ..self.clone()
        }
        .packed()
    }

    /// The uniform, with the wind and radius this far grass holds.
    fn packed(&self) -> FarGrassUniform {
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
            wind: self.wind.into(),
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

/// Mirrors `FarGrass` in `far.wesl`: what [`FarGrass::uniform`] gives a
/// terrain material of your own.
#[derive(Debug, Clone, Copy, PartialEq, ShaderType)]
pub struct FarGrassUniform {
    color: Vec4,
    fade: Vec4,
    area: Vec4,
    wind: WindUniform,
}

impl AsBindGroupShaderType<FarGrassUniform> for FarGrass {
    fn as_bind_group_shader_type(&self, _images: &RenderAssets<GpuImage>) -> FarGrassUniform {
        self.packed()
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
    let size = crate::shader_model::GUST_SIZE;
    let mut image = Image::new(
        Extent3d {
            width: size,
            height: size,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        crate::shader_model::gust_noise(size),
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

/// Keeps every far-grass material's wind equal to the [`GrassWindFrame`]
/// resource and its fade equal to [`GrassSettings::radius`].
fn sync_with_grass(
    wind: Res<GrassWindFrame>,
    settings: Res<GrassSettings>,
    mut materials: ResMut<Assets<FarGrassMaterial>>,
) {
    let radius = settings.sanitized().radius;
    let stale: Vec<_> = materials
        .iter()
        .filter(|(_, material)| {
            let far = &material.extension;
            far.wind != *wind || far.radius != radius
        })
        .map(|(id, _)| id)
        .collect();
    for id in stale {
        if let Some(mut material) = materials.get_mut(id) {
            material.extension.wind = *wind;
            material.extension.radius = radius;
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
        app.init_resource::<GrassWindFrame>()
            .add_plugins(bevy::pbr::MaterialPlugin::<FarGrassMaterial>::default())
            .add_systems(PreStartup, add_gust_texture)
            .add_systems(Update, sync_with_grass.after(GrassWindSystems::Sample));
    }
}

#[cfg(test)]
mod tests;
