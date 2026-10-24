# Styles

A `GrassStyle` says what a field looks like. It is a `GrassBlade` (the
blade's shape and colours) plus what belongs to the whole field: density,
variation, the ground tint, toon shading and other plants. It is an asset, so
many fields can share one. Start from one of the [presets](presets.md) and
change what you need.

## Make your own style

The blade is one level in:

```rust
fn setup(mut commands: Commands, mut styles: ResMut<Assets<GrassStyle>>) {
    let style = GrassStyle {
        blade: GrassBlade {
            height: Vec2::new(0.6, 1.2),
            tip_color: Color::srgb(0.75, 0.70, 0.35),
            stiffness: 0.6,
            ..GrassStyle::TALL_PRAIRIE.blade
        },
        ..GrassStyle::TALL_PRAIRIE
    }
    .with_spacing(0.15); // blades about 15 cm apart

    commands.spawn((
        GrassField {
            size: Vec2::new(120.0, 80.0),
            style: styles.add(style),
            ..default()
        },
        Transform::from_xyz(40.0, 2.0, -10.0),
    ));
}
```

`GrassSpecies` and `GrassStyle` are not `Copy`, so to reuse a style held in a
variable after `..style`, write `..style.clone()` (`GrassBlade` is `Copy`).

## Change a style at runtime

Changing a style asset (`styles.get_mut(&field.style)`) changes every field
that uses it on the next frame, and it is cheap enough to do every frame, for
a season or day/night blend or wet grass:

```rust
fn season(time: Res<Time>, mut styles: ResMut<Assets<GrassStyle>>, field: Single<&GrassField>) {
    let t = (time.elapsed_secs() * 0.1).sin() * 0.5 + 0.5;
    if let Some(mut style) = styles.get_mut(&field.style) {
        style.blade.tip_color = Color::srgb(0.34 + 0.3 * t, 0.56 - 0.1 * t, 0.14);
    }
}
```

What a change costs is in proportion to what it touches. A mask or heightmap
is read from its image once, and again only when that image changes (a 4096 by
4096 mask takes milliseconds to read, once, not per frame). A style change
rewrites only the entries of the fields that use that style, and the chunks,
masks and heightmaps are neither rebuilt nor uploaded again. Moving a field
sideways rewrites only its entry. A change no field shows costs nothing.
`GrassStats::tables` counts it: `images_read`, `entries_rewritten`, `rebuilds`
and the expensive `layout_rebuilds`.

## Change one field without a style of its own

"This meadow, a bit drier" for one area does not need another asset.
`GrassFieldOverrides` on a field tints its blades, scales their height and
thins or thickens them, while the style stays shared:

```rust
commands.spawn((
    GrassField { size: Vec2::splat(32.0), style: meadow.clone(), ..default() },
    GrassFieldOverrides {
        tint: Some(Color::srgb(1.3, 1.1, 0.7)), // multiplies root and tip colours
        height_scale: 0.6,
        density_scale: 0.5,
    },
));
```

The overrides are applied when the field's entries are built, so they cost
nothing per frame, and changing one rewrites that field's entries only.

## How tall the grass grows

Blade height is set in four places, each for a different reach. Pick the
narrowest one that does what you need:

| To change the height... | Set | What it reaches |
| --- | --- | --- |
| of a look, wherever it is used | `blade.height` of the style (and of each species) | Every field that shares the style |
| of one field | `GrassFieldOverrides::height_scale` | That field; the style stays shared |
| in places within a field | the density mask's green channel | Each texel of the mask |
| where the game cuts it, while it runs | `GrassEdit::mow`, or a `GrassBrush` with `GrassEditOp::Mow` | The grass under the edit, until restored or regrown |

[Presets](presets.md#sizes) lists each preset's blade heights and its
tallest plant. Units 2 m tall stand well above `MEADOW` (0.6 m at most); to
keep grass under 0.4 m there as well, scale the field rather than editing the
preset:

```rust
commands.spawn((
    GrassField::new(Vec2::splat(800.0), GrassStyle::MEADOW, &mut styles),
    GrassFieldOverrides { height_scale: 0.4 / 0.6, ..default() },
));
```

A preset such as `GrassStyle::MEADOW` is a constant, and `GrassField::new`
adds a copy of it; changing that copy in `Assets<GrassStyle>` changes the
fields made from it and never the constant.

## Mix in flowers, seed heads and other plants

Any style can carry other plants, each a `GrassSpecies`: a `GrassBlade` of its
own, its share of the blades, and how much it grows in patches:

```rust
let field = GrassStyle::SUMMER_TALL.with_species(vec![
    GrassSpecies::new(GrassBlade::SEED_HEADS, 0.10),
    GrassSpecies::new(GrassBlade::WHITE_FLOWERS, 0.03).clustered(0.8),
    GrassSpecies::new(
        GrassBlade { head_color: Color::srgb(0.70, 0.45, 0.95), ..GrassBlade::WHITE_FLOWERS },
        0.02,
    )
    .clustered(0.8),
]);
let wild = GrassStyle {
    blade: GrassBlade {
        droop: 0.25, // tall blades arch over
        ..field.blade
    },
    variation: GrassVariation {
        tufts: 0.3,        // gather blades into tufts
        bare_share: 0.05,  // 5% of the ground bare
        patch_amount: 0.4, // colour drifts in patches several metres across
        dry_tips: 0.2,
        ..field.variation
    },
    ..field
};
```

`GrassStyle::SUMMER_WILD`, `AUTUMN_WILD` and `WILDFLOWER_MEADOW` are ready-made
examples.

### Build a mixed preset

A mix is not made by adding two whole presets together. `FOREST_FLOOR`, for
example, starts from `FOREST_MOSS` and then mixes in fern, leaf-litter, clover
and broad-leaf blades as species (see
[`src/presets/forest.rs`](../src/presets/forest.rs)). Each species has its own
`GrassBlade`, so its shape, height and colours can differ from the field's
grass and from every other species:

```rust
let mixed = GrassStyle {
    blade: GrassBlade::MOSS,
    density: 140.0,
    species: vec![
        GrassSpecies { share: 0.20, clustering: 0.5, blade: GrassBlade::FERN },
        GrassSpecies {
            share: 0.10,
            clustering: 0.4,
            blade: GrassBlade {
                height: Vec2::new(0.12, 0.30),
                width: 0.07,
                tip_color: Color::srgb(0.45, 0.22, 0.08),
                ..GrassBlade::BROAD_LEAF
            },
        },
        GrassSpecies {
            share: 0.04,
            clustering: 0.8,
            blade: GrassBlade {
                head_color: Color::srgb(0.80, 0.30, 0.55),
                ..GrassBlade::WHITE_FLOWERS
            },
        },
    ]
    .into(),
    ..GrassStyle::FOREST_MOSS
};
```

The same mix as a `.grass.ron` file, with no Rust code at all:

```ron
(
    base: "FOREST_MOSS",
    density: 140.0,
    species: [
        (share: 0.20, clustering: 0.5, style: (base: "FOREST_FERNS")),
        (share: 0.10, clustering: 0.4, style: (
            base: "BROAD_LEAF",
            height: (0.12, 0.30),
            width: 0.07,
            tip_color: (0.45, 0.22, 0.08),
        )),
        (share: 0.04, clustering: 0.8, style: (
            base: "WHITE_FLOWERS",
            head_color: (0.80, 0.30, 0.55),
        )),
    ],
)
```

### What a species can and cannot set

A species sets only its blade, `share` and `clustering`. Density, ground tint,
patch colour, dry tips, dead share, tufts and bare patches belong to the whole
field: the compiler does not let you set them on a species, and a
`.grass.ron` file that tries gets an error that names them.

So a species cannot have its own density: a fern with `share: 0.2` in a 200
blades per m² field is still about 40 ferns per m². `clustering` changes where
they grow, not how many. That is why `FOREST_FLOOR` lowers the moss density to
140 instead of keeping `FOREST_MOSS`'s 220.

A style mixes at most 16 entries, its own blades included. The style's own
blades take whatever share is left; if the species shares add up to more than
1, they are scaled down and the own blades disappear.

## Load styles from files, and edit them live

Every preset ships as a file in
[`assets/grass_styles`](../assets/grass_styles). Copy one into your game's
assets and load it like any asset:

```rust
commands.spawn(GrassField {
    size: Vec2::splat(100.0),
    style: assets.load("grass/my_meadow.grass.ron"),
    ..default()
});
```

Every preset is also an asset that ships inside the crate, so a game can load
one without copying anything (the names are the lower-case preset names):

```rust
style: assets.load("embedded://bevy_grass/styles/summer_tall.grass.ron"),
```

Every field is optional, and a missing one comes from `base`, a preset name.
A variant lists only what differs. Colours are sRGB, written as a tuple,
`(0.70, 0.72, 0.74)`, or as the hex string a colour picker copies,
`"#b3b8bd"`. The file is flat: the blade's settings and the field's sit side
by side, as in the tables below. A species lists its blade settings only:

```ron
(
    base: "SUMMER_TALL",
    tip_color: "#b3b8bd",
    tufts: 0.3,
    species: [
        (share: 0.10, style: (base: "SEED_HEADS")),
        (share: 0.03, clustering: 0.8, style: (
            base: "WHITE_FLOWERS",
            head_color: (0.75, 0.55, 0.95),
        )),
    ],
)
```

With Bevy's `file_watcher` feature, saving the file changes the grass in the
running game. A broken file logs what is wrong (an unknown preset name lists
the valid ones) and leaves the grass as it was. To make a file a named preset
in this crate instead, see [Adding a preset](presets.md#adding-a-preset-to-the-crate).

## Reference

Units are metres unless stated.

**`style.blade`** (`GrassBlade`; also what a `GrassSpecies` has):

| Setting | Meaning |
| --- | --- |
| `height: Vec2` | Shortest and tallest blade |
| `width` | Blade width at the root; each blade varies ±25% |
| `lean` | How far the tip leans at rest, as a share of the height |
| `stiffness` | Resistance to wind, 0 (soft) to 1 (stiff) |
| `root_color`, `tip_color` | Colours along the blade |
| `root_occlusion` | How much darker the roots are, 0–1 |
| `translucency` | How much sunlight shows through a backlit blade, 0–1 |
| `specular` | Rough sheen on the upper half of the blade, 0–1 |
| `twist` | Twist along the blade, in turns at the tip; each blade its own way |
| `droop` | How far blades arch over and hang, 0–1; tall blades the most |
| `fold` | How far blades fold along the midrib, 0–1 |
| `head_length`, `head_width`, `head_color` | A seed head or flower at the top: its share of the length, width and colour. It turns toward the camera |

**The field's settings:**

| Setting | Meaning |
| --- | --- |
| `density` | Blades per m² at full density (see `spacing()`) |
| `ground_tint` (`GroundTint`: `color`, `amount`) | Blend the lowest third of each blade toward the ground's colour, 0–1. In files: `ground_color`, `ground_tint` |
| `toon_bands` | Flat toon shading with this many bands; 0 is physically based |
| `variation.patch_color`, `patch_amount`, `patch_size` | Large patches where blades drift toward another colour |
| `variation.dry_tips` | How far the tops of blades dry toward straw, 0–1 |
| `variation.dead_share` | Share of blades that are dead and straw-coloured |
| `variation.tufts`, `tuft_spacing` | How strongly blades gather into tufts (0–1), and the distance between tufts. The blade count stays the same |
| `variation.bare_share`, `bare_patch_size` | Share of the ground left bare, in patches whose edges thin out, and their size |
| `species` | Other plants among the blades, a list of `GrassSpecies` (up to 15) |

Every variation setting (`twist` through `bare_share`, and `species`) is off
at 0 or empty, and costs nothing while no field uses it.

| Method | What it does |
| --- | --- |
| `spacing()` | Average distance between neighbouring blades at full density (`1 / √density`) |
| `with_spacing(metres)`, `with_density(per_m2)` | The same style, denser or sparser |
| `with_species(list)` | The same style with these plants mixed in |
| `preset(name)`, `presets()` | A preset by its constant's name, or all of them with their names |
| `from_ron(text)`, `to_ron()` | Read or write the `.grass.ron` format |

**`GrassSpecies`:**

| Item | What it does |
| --- | --- |
| `share` | Share of the field's blades, 0–1, and exact. If the shares add up to more than 1, they are scaled down in proportion and the style's own blades disappear |
| `clustering` | 0: scattered evenly. 1: grows in patches about a metre across |
| `blade` | What its blades look like: a `GrassBlade`. In a file, `style:` with the blade settings |
| `GrassSpecies::new(blade, share)` | A scattered species (`const`) |
| `.clustered(amount)` | The same species, growing in patches |
