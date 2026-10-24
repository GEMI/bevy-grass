//! The page's tuning panel: every setting a game can give grass, read and
//! written through reflection.
//!
//! Every public type of `bevy_grass` derives `Reflect`, so the panel is built
//! from the types themselves: [`publish_tunables`] walks each one into JSON
//! the page turns into controls, and [`apply_tuning`] writes a control's value
//! back by its reflect path (`blade.height.y`, `species[1].share`). A field
//! added to the crate shows up on the page without a change here; only its
//! range is the page's to choose. The same walk writes each value as the Rust
//! a game would paste, as the changes from the preset it started from.

use std::fmt::Write as _;
use std::str::FromStr;

use bevy::prelude::*;
use bevy::reflect::enums::{DynamicEnum, DynamicVariant, VariantInfo};
use bevy::reflect::std_traits::ReflectDefault;
use bevy::reflect::tuple::DynamicTuple;
use bevy::reflect::{ReflectMut, ReflectPath, ReflectRef, TypeInfo, TypeRegistry};
use bevy_grass::prelude::*;

use crate::{Meadow, QUALITIES};

/// What the page can tune, in the order it shows them.
pub(crate) const TARGETS: [&str; 10] = [
    "settings", "style", "wind", "far", "field", "regrowth", "walker", "camera", "brush", "debug",
];

/// The wind presets, for the wind's base and the page's list.
pub(crate) const WINDS: [(&str, GrassWind); 4] = [
    ("CALM", GrassWind::CALM),
    ("BREEZE", GrassWind::BREEZE),
    ("GUSTY", GrassWind::GUSTY),
    ("STORM", GrassWind::STORM),
];

/// The plants a species can start from.
pub(crate) const BLADES: [(&str, GrassBlade); 13] = [
    ("SEED_HEADS", GrassBlade::SEED_HEADS),
    ("WHITE_FLOWERS", GrassBlade::WHITE_FLOWERS),
    ("YELLOW_FLOWERS", GrassBlade::YELLOW_FLOWERS),
    ("CLOVER", GrassBlade::CLOVER),
    ("BROAD_LEAF", GrassBlade::BROAD_LEAF),
    ("MOSS", GrassBlade::MOSS),
    ("FERN", GrassBlade::FERN),
    ("BRACKEN", GrassBlade::BRACKEN),
    ("LEAF_LITTER", GrassBlade::LEAF_LITTER),
    ("COTTON_GRASS", GrassBlade::COTTON_GRASS),
    ("CATTAIL", GrassBlade::CATTAIL),
    ("CORNFLOWER", GrassBlade::CORNFLOWER),
    ("POPPY", GrassBlade::POPPY),
];

/// The debug view the demo starts with: hidden until the page turns it on.
pub(crate) fn start_debug() -> GrassDebug {
    GrassDebug {
        enabled: false,
        ..default()
    }
}

/// The brush the left mouse button paints with.
#[derive(Resource, Debug, Clone, Copy)]
pub(crate) struct Brush(pub(crate) GrassBrush);

impl Default for Brush {
    fn default() -> Self {
        Self(GrassBrush::new(GrassEditOp::Density(0.0), 2.0))
    }
}

/// A change the page asked for.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(
    not(target_arch = "wasm32"),
    expect(
        dead_code,
        reason = "the page makes them, and natively there is no page"
    )
)]
pub(crate) enum TuneEdit {
    /// One value, by its reflect path in a target; an enum takes a variant's
    /// name, and `"Some:150"` also sets the variant's first field.
    Set {
        target: String,
        path: String,
        value: String,
    },
    /// A target back to what it started from: the tier, the preset, the
    /// wind preset or its default.
    Reset(String),
    /// The wind preset called this.
    Wind(String),
    /// A species growing the plant called this, a tenth of the blades.
    AddSpecies(String),
    RemoveSpecies(usize),
    /// A style read from a `.grass.ron` file.
    Style(Box<GrassStyle>),
    /// The meadow's grass back as it grew, undoing the brush.
    RestoreField,
}

/// The page's changes and what it reads back.
#[derive(Resource, Debug, Default)]
pub(crate) struct Tuning {
    /// Changes asked for since the last frame.
    pub(crate) edits: Vec<TuneEdit>,
    /// Counts changes the page did not make control by control (a tier, a
    /// preset, a reset), after which it reads every value again.
    pub(crate) revision: u32,
    /// The wind preset the wind started from.
    pub(crate) wind: &'static str,
    /// Something changed since `json` was written.
    pub(crate) dirty: bool,
    /// Every target as JSON, for the page.
    pub(crate) json: String,
}

impl Tuning {
    /// Marks a change the page has to read back.
    pub(crate) fn bump(&mut self) {
        self.revision += 1;
        self.dirty = true;
    }
}

/// Applies the page's changes, in the order it made them.
pub(crate) fn apply_tuning(world: &mut World) {
    if world.resource::<Tuning>().edits.is_empty() {
        return;
    }
    let edits = std::mem::take(&mut world.resource_mut::<Tuning>().edits);
    let registry = world.resource::<AppTypeRegistry>().clone();
    let registry = registry.read();
    let mut outside = false;
    for edit in edits {
        let result = match edit {
            TuneEdit::Set {
                target,
                path,
                value,
            } => with_target_mut(world, &target, |root| {
                let field = if path.is_empty() {
                    root
                } else {
                    path.as_str()
                        .reflect_element_mut(root)
                        .map_err(|e| e.to_string())?
                };
                set_value(field, &value, &registry)
            })
            .unwrap_or_else(|| Err(format!("no target {target}")))
            .map(|()| {
                if target == "style" {
                    far_follows_style(world);
                }
            })
            .map_err(|e| format!("{target}.{path} = {value}: {e}")),
            TuneEdit::Reset(target) => {
                outside = true;
                reset(world, &target)
            }
            TuneEdit::Wind(name) => {
                outside = true;
                WINDS
                    .iter()
                    .find(|(preset, _)| *preset == name)
                    .map(|(preset, wind)| {
                        *world.resource_mut::<GrassWind>() = *wind;
                        world.resource_mut::<Tuning>().wind = preset;
                    })
                    .ok_or_else(|| format!("no wind {name}"))
            }
            TuneEdit::AddSpecies(name) => {
                outside = true;
                BLADES
                    .iter()
                    .find(|(preset, _)| *preset == name)
                    .ok_or_else(|| format!("no plant {name}"))
                    .and_then(|(_, blade)| {
                        edit_style(world, |style| {
                            style.species.to_mut().push(GrassSpecies::new(*blade, 0.1));
                        })
                    })
            }
            TuneEdit::RemoveSpecies(at) => {
                outside = true;
                edit_style(world, |style| {
                    if at < style.species.len() {
                        style.species.to_mut().remove(at);
                    }
                })
            }
            TuneEdit::Style(new) => {
                outside = true;
                edit_style(world, |style| *style = *new)
            }
            TuneEdit::RestoreField => {
                let half = crate::MEADOW * 0.5;
                world.write_message(GrassEdit::restore(GrassBrushShape::rect(
                    Vec2::ZERO,
                    Vec2::splat(half),
                    0.0,
                )));
                Ok(())
            }
        };
        if let Err(message) = result {
            warn!("tuning: {message}");
        }
    }
    let mut tuning = world.resource_mut::<Tuning>();
    if outside {
        tuning.bump();
    } else {
        tuning.dirty = true;
    }
}

/// Rewrites the page's JSON after a change.
pub(crate) fn publish_tunables(world: &mut World) {
    if !world.resource::<Tuning>().dirty || world.get_resource::<Meadow>().is_none() {
        return;
    }
    let mut json = std::mem::take(&mut world.resource_mut::<Tuning>().json);
    json.clear();
    write_tunables(world, &mut json);
    let mut tuning = world.resource_mut::<Tuning>();
    tuning.json = json;
    tuning.dirty = false;
}

fn write_tunables(world: &World, json: &mut String) {
    let tuning = world.resource::<Tuning>();
    let _ = write!(json, "{{\"revision\":{},\"targets\":[", tuning.revision);
    for (at, name) in TARGETS.iter().enumerate() {
        let Some(value) = target(world, name) else {
            continue;
        };
        if at > 0 {
            json.push(',');
        }
        let _ = write!(json, "{{\"name\":\"{name}\",\"value\":");
        write_node(value, json);
        let mut rust = String::new();
        let (base, expression) = base(world, name);
        write_rust_changes(value, base.as_deref(), &expression, 0, &mut rust);
        json.push_str(",\"rust\":");
        write_string(&rust, json);
        if *name == "style"
            && let Some(style) = value.try_downcast_ref::<GrassStyle>()
        {
            json.push_str(",\"ron\":");
            write_string(&style.to_ron(), json);
        }
        json.push('}');
    }
    json.push_str("]}");
}

/// The meadow's style, changed by `change`.
fn edit_style(world: &mut World, change: impl FnOnce(&mut GrassStyle)) -> Result<(), String> {
    let handle = world.resource::<Meadow>().style.clone();
    {
        let mut styles = world.resource_mut::<Assets<GrassStyle>>();
        let mut style = styles.get_mut(&handle).ok_or("the style is not loaded")?;
        change(&mut style);
    }
    far_follows_style(world);
    Ok(())
}

/// The far grass takes on the colour of the style's blades, as at a preset.
fn far_follows_style(world: &mut World) {
    let meadow = world.resource::<Meadow>();
    let (style, ground) = (meadow.style.clone(), meadow.ground.clone());
    let Some(color) = world
        .resource::<Assets<GrassStyle>>()
        .get(&style)
        .map(|style| FarGrass::new(style).color)
    else {
        return;
    };
    if let Some(mut material) = world
        .resource_mut::<Assets<FarGrassMaterial>>()
        .get_mut(&ground)
    {
        material.extension.color = color;
    }
}

fn reset(world: &mut World, name: &str) -> Result<(), String> {
    let meadow = world.resource::<Meadow>();
    let (quality, preset) = (meadow.quality, meadow.preset);
    let style = crate::preset(preset).map(|(_, style)| style);
    match name {
        "settings" => *world.resource_mut::<GrassSettings>() = QUALITIES[quality].1,
        "style" => {
            let style = style.ok_or("no preset")?;
            edit_style(world, |current| *current = style)?;
        }
        "far" => {
            let ground = world.resource::<Meadow>().ground.clone();
            let style = style.ok_or("no preset")?;
            if let Some(mut material) = world
                .resource_mut::<Assets<FarGrassMaterial>>()
                .get_mut(&ground)
            {
                material.extension = crate::far_grass(&style);
            }
        }
        "wind" => {
            let wind = world.resource::<Tuning>().wind;
            let wind = WINDS.iter().find(|(preset, _)| *preset == wind);
            *world.resource_mut::<GrassWind>() = wind.map_or_else(GrassWind::default, |w| w.1);
        }
        // Where the demo starts differs from the type's default.
        "debug" => *world.resource_mut::<GrassDebug>() = start_debug(),
        "brush" => *world.resource_mut::<Brush>() = Brush::default(),
        _ => {
            let registry = world.resource::<AppTypeRegistry>().clone();
            let registry = registry.read();
            with_target_mut(world, name, |value| {
                let type_id = value
                    .get_represented_type_info()
                    .ok_or("no type information")?
                    .type_id();
                let default = default_of(type_id, &registry)?;
                value
                    .try_apply(default.as_partial_reflect())
                    .map_err(|e| e.to_string())
            })
            .unwrap_or_else(|| Err(format!("no target {name}")))?;
        }
    }
    Ok(())
}

/// The value a target is compared with for its Rust, and how to write it.
fn base(world: &World, name: &str) -> (Option<Box<dyn PartialReflect>>, String) {
    let meadow = world.resource::<Meadow>();
    match name {
        "settings" => {
            let (tier, settings) = QUALITIES[meadow.quality];
            (Some(Box::new(settings)), format!("GrassSettings::{tier}"))
        }
        "style" => (
            crate::preset(meadow.preset).map(|(_, style)| Box::new(style) as _),
            format!("GrassStyle::{}", meadow.preset),
        ),
        "wind" => {
            let wind = world.resource::<Tuning>().wind;
            (
                WINDS
                    .iter()
                    .find(|(preset, _)| *preset == wind)
                    .map(|(_, wind)| Box::new(*wind) as _),
                format!("GrassWind::{wind}"),
            )
        }
        "far" => (None, String::new()),
        _ => {
            let registry = world.resource::<AppTypeRegistry>().read();
            let default = target(world, name)
                .and_then(PartialReflect::get_represented_type_info)
                .and_then(|info| default_of(info.type_id(), &registry).ok())
                .map(<dyn Reflect>::into_partial_reflect);
            (default, "default()".to_owned())
        }
    }
}

/// The value the page reads as `name`.
fn target<'w>(world: &'w World, name: &str) -> Option<&'w dyn PartialReflect> {
    let meadow = world.get_resource::<Meadow>()?;
    match name {
        "settings" => world
            .get_resource::<GrassSettings>()
            .map(|value| value as _),
        "wind" => world.get_resource::<GrassWind>().map(|value| value as _),
        "debug" => world.get_resource::<GrassDebug>().map(|value| value as _),
        "brush" => world.get_resource::<Brush>().map(|brush| &brush.0 as _),
        "style" => world
            .resource::<Assets<GrassStyle>>()
            .get(&meadow.style)
            .map(|value| value as _),
        "far" => world
            .resource::<Assets<FarGrassMaterial>>()
            .get(&meadow.ground)
            .map(|material| &material.extension as _),
        "field" => world
            .get::<GrassFieldOverrides>(meadow.field)
            .map(|value| value as _),
        "regrowth" => world
            .get::<GrassRegrowth>(meadow.field)
            .map(|value| value as _),
        "walker" => world
            .get::<GrassInteractor>(meadow.walker)
            .map(|value| value as _),
        "camera" => world
            .get::<GrassCamera>(meadow.camera)
            .map(|value| value as _),
        _ => None,
    }
}

/// Runs `change` on the value the page writes as `name`; `None` if there is
/// none.
fn with_target_mut<R>(
    world: &mut World,
    name: &str,
    change: impl FnOnce(&mut dyn PartialReflect) -> R,
) -> Option<R> {
    let meadow = world.get_resource::<Meadow>()?;
    let (style, ground) = (meadow.style.clone(), meadow.ground.clone());
    let (field, walker, camera) = (meadow.field, meadow.walker, meadow.camera);
    match name {
        "settings" => Some(change(&mut *world.resource_mut::<GrassSettings>())),
        "wind" => Some(change(&mut *world.resource_mut::<GrassWind>())),
        "debug" => Some(change(&mut *world.resource_mut::<GrassDebug>())),
        "brush" => Some(change(&mut world.resource_mut::<Brush>().0)),
        "style" => {
            let mut styles = world.resource_mut::<Assets<GrassStyle>>();
            let mut style = styles.get_mut(&style)?;
            Some(change(&mut *style))
        }
        "far" => {
            let mut materials = world.resource_mut::<Assets<FarGrassMaterial>>();
            let mut material = materials.get_mut(&ground)?;
            Some(change(&mut material.extension))
        }
        "field" => Some(change(&mut *world.get_mut::<GrassFieldOverrides>(field)?)),
        "regrowth" => Some(change(&mut *world.get_mut::<GrassRegrowth>(field)?)),
        "walker" => Some(change(&mut *world.get_mut::<GrassInteractor>(walker)?)),
        "camera" => Some(change(&mut *world.get_mut::<GrassCamera>(camera)?)),
        _ => None,
    }
}

/// Sets one value from the page's text: a number, `true` or `false`, a
/// `#rrggbb` colour, or an enum's variant.
fn set_value(
    value: &mut dyn PartialReflect,
    text: &str,
    registry: &TypeRegistry,
) -> Result<(), String> {
    if let Some(color) = value.try_downcast_mut::<Color>() {
        *color = Srgba::hex(text).map_err(|e| e.to_string())?.into();
        return Ok(());
    }
    if let Some(number) = value.try_downcast_mut::<f32>() {
        *number = parse(text)?;
        return Ok(());
    }
    if let Some(number) = value.try_downcast_mut::<u32>() {
        *number = parse(text)?;
        return Ok(());
    }
    if let Some(number) = value.try_downcast_mut::<usize>() {
        *number = parse(text)?;
        return Ok(());
    }
    if let Some(flag) = value.try_downcast_mut::<bool>() {
        *flag = parse(text)?;
        return Ok(());
    }
    let (variant, first) = text
        .split_once(':')
        .map_or((text, None), |(variant, first)| (variant, Some(first)));
    let ReflectMut::Enum(current) = value.reflect_mut() else {
        return Err(format!("cannot set a {}", value.reflect_type_path()));
    };
    if current.variant_name() != variant {
        let info = current
            .get_represented_enum_info()
            .ok_or("an enum without type information")?;
        let fields = match info.variant(variant) {
            Some(VariantInfo::Unit(_)) => DynamicVariant::Unit,
            Some(VariantInfo::Tuple(tuple)) => {
                let mut fields = DynamicTuple::default();
                for field in tuple.iter() {
                    fields.insert_boxed(
                        default_of(field.type_id(), registry)?.into_partial_reflect(),
                    );
                }
                DynamicVariant::Tuple(fields)
            }
            Some(VariantInfo::Struct(_)) => return Err(format!("{variant} has named fields")),
            None => return Err(format!("no variant {variant}")),
        };
        value
            .try_apply(&DynamicEnum::new(variant, fields))
            .map_err(|e| e.to_string())?;
    }
    if let Some(first) = first {
        let ReflectMut::Enum(current) = value.reflect_mut() else {
            unreachable!("the value was an enum a moment ago");
        };
        let field = current.field_at_mut(0).ok_or("no field to set")?;
        set_value(field, first, registry)?;
    }
    Ok(())
}

fn parse<T: FromStr>(text: &str) -> Result<T, String> {
    text.trim()
        .parse()
        .map_err(|_| format!("cannot read {text:?}"))
}

/// A new value of the type `type_id`, from its `Default`.
fn default_of(
    type_id: std::any::TypeId,
    registry: &TypeRegistry,
) -> Result<Box<dyn Reflect>, String> {
    registry
        .get_type_data::<ReflectDefault>(type_id)
        .map(ReflectDefault::default)
        .ok_or_else(|| "a type without a default".to_owned())
}

/// Writes `value` as the page's JSON: `{"t": kind, "v": value}`, with a
/// struct's fields in `f` (each with its name and, when the demo is built
/// with docs, its doc comment), a list's items in `items` and an enum's
/// variants in `variants`.
pub(crate) fn write_node(value: &dyn PartialReflect, json: &mut String) {
    if let Some(color) = value.try_downcast_ref::<Color>() {
        let _ = write!(
            json,
            "{{\"t\":\"color\",\"v\":\"{}\"}}",
            color.to_srgba().to_hex()
        );
        return;
    }
    if let Some(number) = value.try_downcast_ref::<f32>() {
        json.push_str("{\"t\":\"f32\",\"v\":");
        write_f32(*number, json);
        json.push('}');
        return;
    }
    if let Some(duration) = value.try_downcast_ref::<std::time::Duration>() {
        let _ = write!(
            json,
            "{{\"t\":\"ms\",\"v\":{:.3}}}",
            duration.as_secs_f64() * 1000.0
        );
        return;
    }
    if let Some(number) = integer(value) {
        let _ = write!(json, "{{\"t\":\"int\",\"v\":{number}}}");
        return;
    }
    if let Some(flag) = value.try_downcast_ref::<bool>() {
        let _ = write!(json, "{{\"t\":\"bool\",\"v\":{flag}}}");
        return;
    }
    match value.reflect_ref() {
        ReflectRef::Struct(fields) => {
            let _ = write!(
                json,
                "{{\"t\":\"struct\",\"ty\":\"{}\",\"f\":[",
                type_ident(value)
            );
            let info = match value.get_represented_type_info() {
                Some(TypeInfo::Struct(info)) => Some(info),
                _ => None,
            };
            for at in 0..fields.field_len() {
                let (Some(name), Some(field)) = (fields.name_at(at), fields.field_at(at)) else {
                    continue;
                };
                if at > 0 {
                    json.push(',');
                }
                let _ = write!(json, "{{\"n\":\"{name}\"");
                if let Some(docs) = info.and_then(|info| field_docs(info, name)) {
                    json.push_str(",\"d\":");
                    write_string(docs, json);
                }
                json.push_str(",\"v\":");
                write_node(field, json);
                json.push('}');
            }
            json.push_str("]}");
        }
        ReflectRef::List(list) => write_items(list.iter(), false, json),
        ReflectRef::Array(array) => write_items(array.iter(), true, json),
        ReflectRef::Enum(current) => {
            let _ = write!(
                json,
                "{{\"t\":\"enum\",\"ty\":\"{}\",\"v\":\"{}\",\"variants\":[",
                type_ident(value),
                current.variant_name()
            );
            if let Some(info) = current.get_represented_enum_info() {
                for (at, name) in info.variant_names().iter().enumerate() {
                    if at > 0 {
                        json.push(',');
                    }
                    let _ = write!(json, "\"{name}\"");
                }
            }
            json.push_str("],\"f\":[");
            for at in 0..current.field_len() {
                if at > 0 {
                    json.push(',');
                }
                let _ = write!(json, "{{\"n\":\"{at}\",\"v\":");
                if let Some(field) = current.field_at(at) {
                    write_node(field, json);
                }
                json.push('}');
            }
            json.push_str("]}");
        }
        _ => json.push_str("{\"t\":\"opaque\"}"),
    }
}

fn write_items<'a>(
    items: impl Iterator<Item = &'a dyn PartialReflect>,
    fixed: bool,
    json: &mut String,
) {
    let _ = write!(json, "{{\"t\":\"list\",\"fixed\":{fixed},\"items\":[");
    for (at, item) in items.enumerate() {
        if at > 0 {
            json.push(',');
        }
        write_node(item, json);
    }
    json.push_str("]}");
}

#[cfg(feature = "docs")]
fn field_docs(info: &bevy::reflect::structs::StructInfo, name: &str) -> Option<&'static str> {
    info.field(name).and_then(bevy::reflect::NamedField::docs)
}

#[cfg(not(feature = "docs"))]
fn field_docs(_: &bevy::reflect::structs::StructInfo, _: &str) -> Option<&'static str> {
    None
}

fn integer(value: &dyn PartialReflect) -> Option<u64> {
    value
        .try_downcast_ref::<u32>()
        .map(|n| u64::from(*n))
        .or_else(|| value.try_downcast_ref::<u64>().copied())
        .or_else(|| value.try_downcast_ref::<usize>().map(|n| *n as u64))
}

fn type_ident(value: &dyn PartialReflect) -> &str {
    value
        .get_represented_type_info()
        .and_then(|info| info.type_path_table().ident())
        .unwrap_or_else(|| value.reflect_short_type_path())
}

/// A number JSON can hold: JSON has no infinity or NaN.
fn write_f32(number: f32, json: &mut String) {
    if number.is_finite() {
        let _ = write!(json, "{number}");
    } else {
        json.push_str("null");
    }
}

pub(crate) fn write_string(text: &str, json: &mut String) {
    json.push('"');
    for c in text.chars() {
        match c {
            '"' => json.push_str("\\\""),
            '\\' => json.push_str("\\\\"),
            '\n' => json.push_str("\\n"),
            c if u32::from(c) < 0x20 => {
                let _ = write!(json, "\\u{:04x}", u32::from(c));
            }
            c => json.push(c),
        }
    }
    json.push('"');
}

/// Writes `value` as a Rust expression: only the fields that differ from
/// `base`, followed by `..{expression}`, so a tuned style reads as the
/// changes from its preset.
fn write_rust_changes(
    value: &dyn PartialReflect,
    base: Option<&dyn PartialReflect>,
    expression: &str,
    depth: usize,
    rust: &mut String,
) {
    let (ReflectRef::Struct(fields), Some(ReflectRef::Struct(base_fields))) =
        (value.reflect_ref(), base.map(PartialReflect::reflect_ref))
    else {
        write_rust(value, depth, rust);
        return;
    };
    if special(value) {
        write_rust(value, depth, rust);
        return;
    }
    let changed: Vec<usize> = (0..fields.field_len())
        .filter(|at| {
            let (Some(field), Some(base)) = (fields.field_at(*at), base_fields.field_at(*at))
            else {
                return true;
            };
            field.reflect_partial_eq(base) != Some(true)
        })
        .collect();
    if changed.is_empty() {
        rust.push_str(expression);
        return;
    }
    let indent = "    ".repeat(depth + 1);
    let _ = writeln!(rust, "{} {{", type_ident(value));
    for at in changed {
        let (Some(name), Some(field)) = (fields.name_at(at), fields.field_at(at)) else {
            continue;
        };
        let _ = write!(rust, "{indent}{name}: ");
        write_rust_changes(
            field,
            base_fields.field_at(at),
            &format!("{expression}.{name}"),
            depth + 1,
            rust,
        );
        rust.push_str(",\n");
    }
    let _ = write!(rust, "{indent}..{expression}\n{}}}", "    ".repeat(depth));
}

/// A struct written with a constructor rather than its fields.
fn special(value: &dyn PartialReflect) -> bool {
    value.try_downcast_ref::<Vec2>().is_some() || value.try_downcast_ref::<Vec3>().is_some()
}

/// Writes `value` as a Rust expression, every field of it.
fn write_rust(value: &dyn PartialReflect, depth: usize, rust: &mut String) {
    if let Some(color) = value.try_downcast_ref::<Color>() {
        let c = color.to_srgba();
        let _ = write!(
            rust,
            "Color::srgb({}, {}, {})",
            round(c.red),
            round(c.green),
            round(c.blue)
        );
        return;
    }
    if let Some(number) = value.try_downcast_ref::<f32>() {
        let _ = write!(rust, "{number:?}");
        return;
    }
    if let Some(v) = value.try_downcast_ref::<Vec2>() {
        let _ = write!(rust, "Vec2::new({:?}, {:?})", v.x, v.y);
        return;
    }
    if let Some(v) = value.try_downcast_ref::<Vec3>() {
        let _ = write!(rust, "Vec3::new({:?}, {:?}, {:?})", v.x, v.y, v.z);
        return;
    }
    if let Some(number) = integer(value) {
        let _ = write!(rust, "{number}");
        return;
    }
    if let Some(flag) = value.try_downcast_ref::<bool>() {
        let _ = write!(rust, "{flag}");
        return;
    }
    let indent = "    ".repeat(depth + 1);
    match value.reflect_ref() {
        ReflectRef::Struct(fields) => {
            let _ = writeln!(rust, "{} {{", type_ident(value));
            for at in 0..fields.field_len() {
                let (Some(name), Some(field)) = (fields.name_at(at), fields.field_at(at)) else {
                    continue;
                };
                let _ = write!(rust, "{indent}{name}: ");
                write_rust(field, depth + 1, rust);
                rust.push_str(",\n");
            }
            let _ = write!(rust, "{}}}", "    ".repeat(depth));
        }
        ReflectRef::Array(array) => {
            rust.push('[');
            for (at, item) in array.iter().enumerate() {
                if at > 0 {
                    rust.push_str(", ");
                }
                write_rust(item, depth, rust);
            }
            rust.push(']');
        }
        ReflectRef::List(list) => {
            // A `Cow` of a slice, such as the species, takes a `Vec`.
            rust.push_str("vec![\n");
            for item in list.iter() {
                rust.push_str(&indent);
                write_rust(item, depth + 1, rust);
                rust.push_str(",\n");
            }
            let _ = write!(rust, "{}].into()", "    ".repeat(depth));
        }
        ReflectRef::Enum(current) => {
            let name = type_ident(value);
            if name == "Option" {
                rust.push_str(current.variant_name());
            } else {
                let _ = write!(rust, "{name}::{}", current.variant_name());
            }
            if current.field_len() > 0 {
                rust.push('(');
                for at in 0..current.field_len() {
                    if at > 0 {
                        rust.push_str(", ");
                    }
                    if let Some(field) = current.field_at(at) {
                        write_rust(field, depth, rust);
                    }
                }
                rust.push(')');
            }
        }
        _ => rust.push_str("default()"),
    }
}

/// A colour channel to three places, as a person would write it.
fn round(channel: f32) -> f32 {
    (channel * 1000.0).round() / 1000.0
}

#[cfg(test)]
mod tests;
