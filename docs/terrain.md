# Fields, terrain and visibility

A `GrassField` is a rectangle of grass centred on its `Transform`, lying in
the XZ plane. Its size is `size` times the transform's X and Z scale, so a
scaled field is bigger, like a scaled `Plane3d`. Rotation is not supported
yet: a rotated field is drawn unrotated, with one warning.

## Grow grass on a heightmap, with a density mask

```rust
fn setup(
    mut commands: Commands,
    assets: Res<AssetServer>,
    mut styles: ResMut<Assets<GrassStyle>>,
) {
    // Masks and heightmaps are data, not colour: load them as linear.
    let linear = |path: &'static str| {
        assets
            .load_builder()
            .with_settings(|settings: &mut ImageLoaderSettings| settings.is_srgb = false)
            .load(path)
    };
    commands.spawn(GrassField {
        size: Vec2::splat(512.0),
        style: styles.add(GrassStyle::SUMMER_TALL),
        ground: GrassGround::Heightmap {
            image: linear("terrain/height.png"),
            height_scale: 40.0,
        },
        density_map: Some(linear("terrain/grass_mask.png")),
    });
}
```

Both images cover the whole field, and their first row is the field's −Z edge.
In the heightmap, red 0–1 spans `height_scale` metres above the field's
height. In the mask, red is density (0 bare, 1 full) and green multiplies the
blade height. The images are read on the CPU when the field changes, so keep
their CPU copy (Bevy's default for loaded images).
`ImageLoaderSettings` is `bevy::image::ImageLoaderSettings`.

## Keep the grass colour past the last blade

Beyond the grass radius no blades are drawn. `FarGrassMaterial` is a
`StandardMaterial` for your terrain that fades to the grass colour with
distance, with the same gusts rolling across it, so there is no seam:

```rust
fn ground(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<FarGrassMaterial>>,
) {
    commands.spawn((
        Mesh3d(meshes.add(Plane3d::default().mesh().size(1000.0, 1000.0))),
        MeshMaterial3d(materials.add(FarGrassMaterial {
            base: StandardMaterial::from_color(Color::srgb(0.20, 0.15, 0.09)),
            extension: FarGrass::new(&GrassStyle::MEADOW),
        })),
    ));
}
```

A custom terrain shader can call the same WESL function instead:
`bevy_grass::shaders::far::grass_far_color`.

| Item | What it does |
| --- | --- |
| `FarGrassMaterial` | `ExtendedMaterial<StandardMaterial, FarGrass>` for terrain, forward and deferred |
| `FarGrassPlugin` | Registers the material. Part of `GrassPlugins` |
| `FarGrass` | `color`, `amount` (0–1), `fade: Option<Vec2>` (where the tint starts and is full; `None` follows `GrassSettings::radius`), `sheen` (gusts brighten it, 0–1), `area: Option<Rect>`. Its wind follows `GrassWindFrame`, the same frame the blades read |
| `FarGrass::new(&style)` | Colour from the style; the fade follows the grass radius, also when it changes |
| `.with_fade(Vec2)` | A fade of your own, whatever the radius |
| `.with_area(rect)` | Only inside this world-XZ rectangle |
| `FarGrassUniform` | The material's GPU data (for custom materials) |
| `grass_far_color` (WESL) | `bevy_grass::shaders::far::grass_far_color`, for your own terrain shader |

## Hide a field, or leave a camera out

Fields and cameras follow Bevy's usual rules for what is drawn where:

```rust
// Hide a field, or a whole level it is a child of, and show it again. Nothing
// is rebuilt either way.
commands.entity(field).insert(Visibility::Hidden);

// Split-screen, a minimap, a mirror: a camera draws the fields it shares a
// render layer with. A field or camera without `RenderLayers` is on layer 0.
commands.spawn((Camera3d::default(), RenderLayers::layer(1))); // a minimap
commands.spawn((GrassField { size: Vec2::splat(64.0), style, ..default() },
                RenderLayers::from_layers(&[0, 1])));          // seen by both

// Or keep a camera from drawing any grass at all.
commands.spawn((Camera3d::default(), NoGrass));
```

Layers 0 to 31 are supported. Every other 3D camera draws its own grass, with
its own culling and level of detail; [`GrassCamera`](settings.md#draw-differently-on-one-camera)
changes how much.
