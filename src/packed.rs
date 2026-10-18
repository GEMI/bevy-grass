//! Density masks and heightmaps as the GPU reads them, kept per image.
//!
//! Reading a 4096 by 4096 mask texel by texel takes milliseconds, so it is
//! done once per image and again only when that image changes: moving a field
//! or changing a style, which happens far more often, must not pay for it.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use bevy::asset::AssetId;
use bevy::color::LinearRgba;
use bevy::ecs::prelude::*;
use bevy::image::Image;

/// The packed form of every image a field uses as a mask or a heightmap.
#[derive(Resource, Debug, Default)]
pub(crate) struct PackedImages {
    masks: HashMap<AssetId<Image>, Arc<Vec<u32>>>,
    heights: HashMap<AssetId<Image>, Arc<Vec<f32>>>,
    /// Images read texel by texel so far. A test can tell whether a change
    /// cost a read.
    pub reads: u64,
}

impl PackedImages {
    /// `image` as a mask: R8G8 texels, two per word, low half first. The
    /// words are padded so a mask starts on a whole word.
    pub(crate) fn mask(&mut self, id: AssetId<Image>, image: &Image) -> Arc<Vec<u32>> {
        if let Some(packed) = self.masks.get(&id) {
            return packed.clone();
        }
        self.reads += 1;
        let packed = Arc::new(pack_mask(image));
        self.masks.insert(id, packed.clone());
        packed
    }

    /// `image` as a heightmap: its red channel, one normalized height per
    /// texel.
    pub(crate) fn heights(&mut self, id: AssetId<Image>, image: &Image) -> Arc<Vec<f32>> {
        if let Some(packed) = self.heights.get(&id) {
            return packed.clone();
        }
        self.reads += 1;
        let packed = Arc::new(read_heights(image));
        self.heights.insert(id, packed.clone());
        packed
    }

    /// Drops what was read from images that changed, so the next use reads
    /// them again.
    pub(crate) fn forget(&mut self, changed: &HashSet<AssetId<Image>>) {
        self.masks.retain(|id, _| !changed.contains(id));
        self.heights.retain(|id, _| !changed.contains(id));
    }

    /// Keeps only what fields still use.
    pub(crate) fn retain(&mut self, masks: &[AssetId<Image>], heights: &[AssetId<Image>]) {
        self.masks.retain(|id, _| masks.contains(id));
        self.heights.retain(|id, _| heights.contains(id));
    }
}

/// Words in a mask of `texels` texels.
pub(crate) fn mask_words(texels: u32) -> u32 {
    texels.div_ceil(2)
}

/// A mask as R8G8 texels, two per word, low half first.
///
/// Whatever the image's colour space, the red and green are the values
/// stored. (An sRGB image read as a colour would be gamma-curved.)
fn pack_mask(image: &Image) -> Vec<u32> {
    let size = image.size();
    let mut words = Vec::with_capacity(mask_words(size.x * size.y) as usize);
    let mut half = None;
    for y in 0..size.y {
        for x in 0..size.x {
            let texel = image.get_color_at(x, y).map_or(0xffff, |color| {
                let c = LinearRgba::from(color);
                let r = (c.red.clamp(0.0, 1.0) * 255.0).round() as u32;
                let g = (c.green.clamp(0.0, 1.0) * 255.0).round() as u32;
                r | (g << 8)
            });
            match half.take() {
                None => half = Some(texel),
                Some(low) => words.push(low | (texel << 16)),
            }
        }
    }
    if let Some(low) = half {
        words.push(low);
    }
    words
}

/// A heightmap's red channel, normalized.
fn read_heights(image: &Image) -> Vec<f32> {
    let size = image.size();
    let mut heights = Vec::with_capacity((size.x * size.y) as usize);
    for y in 0..size.y {
        for x in 0..size.x {
            heights.push(
                image
                    .get_color_at(x, y)
                    .map_or(0.0, |c| LinearRgba::from(c).red),
            );
        }
    }
    heights
}

/// Joins packed images end to end. One image needs no copy at all.
pub(crate) fn join<T: Copy>(parts: &[Arc<Vec<T>>]) -> Arc<Vec<T>> {
    match parts {
        [] => Arc::default(),
        [only] => only.clone(),
        many => Arc::new(many.iter().flat_map(|part| part.iter().copied()).collect()),
    }
}

#[cfg(test)]
mod tests;
