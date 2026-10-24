# Fields, terrain and visibility

A `GrassField` is a rectangle of grass centred on its `Transform`, lying in
the XZ plane. Its size is `size` times the transform's X and Z scale, so a
scaled field is bigger, like a scaled `Plane3d`. It turns with the
transform's rotation about Y, and its mask and heightmap turn with it. Grass
grows upright, so a tilt is left out, with one warning: for a slope, use a
heightmap or a mesh.

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

## Grow grass on a mesh

For terrain that is a mesh, put the field on the terrain's own entity and
give it `GrassGround::mesh`. The field then lies where the mesh is drawn,
turn and scale included:

```rust
fn setup(
    mut commands: Commands,
    assets: Res<AssetServer>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut styles: ResMut<Assets<GrassStyle>>,
) {
    let terrain: Handle<Mesh> =
        assets.load(GltfAssetLabel::Primitive { mesh: 0, primitive: 0 }.from_asset("terrain.glb"));
    commands.spawn((
        Mesh3d(terrain.clone()),
        MeshMaterial3d(materials.add(Color::srgb(0.2, 0.15, 0.09))),
        GrassField {
            size: Vec2::splat(256.0), // the mesh's extent in X and Z
            style: styles.add(GrassStyle::MEADOW),
            ground: GrassGround::mesh(terrain),
            ..default()
        },
        Transform::from_rotation(Quat::from_rotation_y(0.5)),
    ));
}
```

The mesh is baked once, on the CPU, into a heightmap of one height per half
metre (`GrassGround::Mesh { mesh, cell_size }` picks another cell size, at
most 4096 cells per side), and from then on it is drawn exactly like a
heightmap: there is no cost per frame. It is baked again when the mesh asset,
the field's size or the cell size changes, not when the field moves. The
field waits for the mesh to load, so it never appears at the wrong height.

- **Where blades grow.** On the highest surface over each point, so the
  underside of an overhang or a bridge gets none. Nowhere the mesh does not
  reach: a field larger than its mesh stops at the mesh's edge, within half
  a cell.
- **Scale.** Like the mesh, the ground follows the transform's scale,
  including Y. Blade heights do not.
- **What can be baked.** Triangle lists and strips with `Float32x3`
  positions, kept on the CPU (`RenderAssetUsages::MAIN_WORLD`, which loaded
  and built meshes have by default). A mesh that cannot be baked draws no
  grass and logs one warning.
- **Several meshes.** One field per mesh; a terrain cut into tiles gets a
  field on each tile.

[`examples/mesh_terrain.rs`](../examples/mesh_terrain.rs) builds rolling
hills and grows a meadow on them.


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

A field can be spawned at any time, not only at `Startup`. Behind a loading
screen, spawn it when the level is ready, with a state's `OnEnter` rather
than a system that checks a flag every frame, or spawn it with
`Visibility::Hidden` and show it when the screen lifts:

```rust
app.add_systems(OnEnter(GameState::Playing), spawn_grass);
```

Layers 0 to 31 are supported. Every other 3D camera draws its own grass, with
its own culling and level of detail; [`GrassCamera`](settings.md#draw-differently-on-one-camera)
changes how much.
