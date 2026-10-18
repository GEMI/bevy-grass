//! Which directional light grass follows, and whether that light's shadow
//! setup suits blades.
//!
//! Grass casts its shadows for one directional light and treats one as the
//! sun in the deferred G-buffer. A scene can have several (a sun and a moon),
//! so the choice is explicit: the light marked [`GrassSun`], or else the
//! first shadow-casting one. "First" is by entity, so it does not change from
//! run to run.

use bevy::ecs::change_detection::DetectChanges;
use bevy::ecs::prelude::*;
use bevy::light::{CascadeShadowConfig, DirectionalLight, DirectionalLightShadowMap};
use bevy::log::warn;
use bevy::math::Vec3;
use bevy::transform::components::GlobalTransform;

use crate::api::GrassSun;
use crate::fields::FieldTables;
use crate::{GrassSettings, GrassShadows};

/// A directional light, as far as choosing one goes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct SunCandidate {
    pub entity: Entity,
    /// Whether it casts shadows (`shadow_maps_enabled`).
    pub casts: bool,
    /// Whether it carries [`GrassSun`].
    pub marked: bool,
    /// The direction it shines.
    pub shines: Vec3,
}

impl SunCandidate {
    /// Where the light comes in "first": by entity index, which for lights
    /// spawned one after another is the order they were spawned in. (An
    /// `Entity` itself orders the other way round in Bevy 0.20.)
    fn order(&self) -> (u32, Entity) {
        (self.entity.index_u32(), self.entity)
    }
}

/// The light grass follows among `lights`, which must be sorted by `order`:
/// the marked one if there is one (even without shadows: the mark is an
/// instruction), else the first that casts shadows.
fn grass_light(lights: &[SunCandidate]) -> Option<&SunCandidate> {
    lights
        .iter()
        .find(|light| light.marked)
        .or_else(|| lights.iter().find(|light| light.casts))
}

/// The direction the light grass casts shadows for shines, if it casts
/// shadows at all, and the direction of the light the deferred G-buffer
/// treats as the sun: that same light, or else the first light there is.
pub(crate) fn pick_sun(
    lights: impl IntoIterator<Item = SunCandidate>,
) -> (Option<Vec3>, Option<Vec3>) {
    let mut lights: Vec<_> = lights.into_iter().collect();
    lights.sort_by_key(SunCandidate::order);
    let chosen = grass_light(&lights);
    let shadow_light = chosen.filter(|light| light.casts).map(|light| light.shines);
    let sun = chosen.or_else(|| lights.first()).map(|light| light.shines);
    (shadow_light, sun)
}

/// What is likely wrong with the choice of light, as sentences for the log.
/// `shadows` is whether grass casts shadows at all.
pub(crate) fn sun_problems(lights: &[SunCandidate], shadows: bool) -> Vec<String> {
    let mut lights = lights.to_vec();
    lights.sort_by_key(SunCandidate::order);
    let mut problems = Vec::new();
    let marked: Vec<_> = lights.iter().filter(|light| light.marked).collect();
    if marked.len() > 1 {
        problems.push(format!(
            "{} lights carry GrassSun; grass follows the one spawned first, {}. \
             Remove the marker from the others.",
            marked.len(),
            marked[0].entity
        ));
    }
    if !shadows {
        return problems;
    }
    if let Some(light) = marked.first()
        && !light.casts
    {
        problems.push(format!(
            "the GrassSun light {} has shadow_maps_enabled off, so grass casts no shadows. \
             Turn them on for that light, or set GrassSettings::shadows to Off.",
            light.entity
        ));
    }
    let casters = lights.iter().filter(|light| light.casts).count();
    if marked.is_empty() && casters > 1 {
        problems.push(format!(
            "{casters} directional lights cast shadows and none carries GrassSun; grass shadows \
             follow the one spawned first, {}. Add GrassSun to the light that should \
             decide, usually the sun.",
            lights
                .iter()
                .find(|light| light.casts)
                .map_or(Entity::PLACEHOLDER, |l| l.entity)
        ));
    }
    problems
}

/// How wide one texel of a cascade's shadow map is, in metres, for a cascade
/// whose far bound is `far_bound` metres from the camera in a map of `size`
/// texels. About 1.5 times the far bound over the size: measured on a
/// 60 m cascade in a 2048 map, which gave 4 to 5 cm.
pub(crate) fn texel_size(far_bound: f32, size: usize) -> f32 {
    1.5 * far_bound / size.max(1) as f32
}

/// A warning when blades are narrower than a texel of the first cascade,
/// which is where grass casts its shadows: a blade then shadows or not
/// depending on which side of a texel centre it sways to, and blade shadows
/// blink.
pub(crate) fn cascade_problem(far_bound: f32, size: usize, narrowest_blade: f32) -> Option<String> {
    let texel = texel_size(far_bound, size);
    (texel > narrowest_blade).then(|| {
        // The deepest first cascade whose texel is as wide as the blade.
        let deepest = narrowest_blade * size as f32 / 1.5;
        format!(
            "the first shadow cascade reaches {far_bound} m, so a shadow texel is about {:.1} cm \
             across, wider than the narrowest blades ({:.1} cm); their shadows will blink as they \
             sway. Make the first cascade at most {deepest:.0} m deep (3 cascades with \
             first_cascade_far_bound: 12.0 is a good start), or use a larger \
             DirectionalLightShadowMap.",
            texel * 100.0,
            narrowest_blade * 100.0,
        )
    })
}

/// What `check_shadow_setup` has said so far, so each problem is said once.
#[derive(Resource, Debug, Default)]
pub(crate) struct ShadowSetupReport {
    pub said: Vec<String>,
}

impl ShadowSetupReport {
    /// Records `problems` and returns the ones not said before.
    fn new_problems(&mut self, problems: Vec<String>) -> Vec<String> {
        let fresh: Vec<_> = problems
            .into_iter()
            .filter(|problem| !self.said.contains(problem))
            .collect();
        self.said.extend(fresh.iter().cloned());
        fresh
    }
}

/// Watches the lights and the fields, and says what is likely to make grass
/// shadows wrong, once per distinct problem.
#[expect(clippy::type_complexity, reason = "Bevy system queries are verbose")]
pub(crate) fn check_shadow_setup(
    settings: Res<GrassSettings>,
    tables: Res<FieldTables>,
    map: Option<Res<DirectionalLightShadowMap>>,
    lights: Query<(
        Entity,
        &DirectionalLight,
        &GlobalTransform,
        Option<&CascadeShadowConfig>,
        Has<GrassSun>,
    )>,
    changed: Query<
        (),
        Or<(
            Changed<DirectionalLight>,
            Changed<CascadeShadowConfig>,
            Added<GrassSun>,
        )>,
    >,
    mut removed: RemovedComponents<GrassSun>,
    mut report: ResMut<ShadowSetupReport>,
) {
    let removed = removed.read().count() > 0;
    let map_changed = map.as_ref().is_some_and(DetectChanges::is_changed);
    if !(settings.is_changed()
        || tables.is_changed()
        || map_changed
        || removed
        || !changed.is_empty())
    {
        return;
    }
    let shadows = settings.shadows != GrassShadows::Off;
    let candidates: Vec<_> = lights
        .iter()
        .map(|(entity, light, transform, _, marked)| SunCandidate {
            entity,
            casts: light.shadow_maps_enabled,
            marked,
            shines: transform.forward().as_vec3(),
        })
        .collect();
    let mut problems = sun_problems(&candidates, shadows);

    // The first cascade of the light grass follows, against the blades there are.
    let mut sorted = candidates.clone();
    sorted.sort_by_key(SunCandidate::order);
    if shadows
        && let Some(chosen) = grass_light(&sorted).filter(|light| light.casts)
        && let Ok((_, _, _, Some(cascades), _)) = lights.get(chosen.entity)
        && let Some(&first) = cascades.bounds.first()
        && let Some(narrowest) = tables
            .fields
            .iter()
            .map(|entry| entry.shape.z)
            .filter(|width| *width > 0.0)
            .min_by(f32::total_cmp)
    {
        let size = map.as_ref().map_or(2048, |map| map.size);
        problems.extend(cascade_problem(first, size, narrowest));
    }

    for problem in report.new_problems(problems) {
        warn!("bevy_grass: {problem}");
    }
}

#[cfg(test)]
mod tests;
