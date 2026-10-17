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
mod tests {
    use bevy::asset::RenderAssetUsages;
    use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

    use super::*;

    fn image(width: u32, height: u32, texel: [u8; 4]) -> Image {
        Image::new_fill(
            Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            &texel,
            TextureFormat::Rgba8Unorm,
            RenderAssetUsages::default(),
        )
    }

    #[test]
    fn an_image_is_read_once_until_it_changes() {
        let mut cache = PackedImages::default();
        let id = AssetId::<Image>::default();
        let first = cache.mask(id, &image(4, 4, [255, 0, 0, 255]));
        let again = cache.mask(id, &image(4, 4, [0, 0, 0, 255]));
        assert_eq!(cache.reads, 1, "the second use must not read the texels");
        assert!(Arc::ptr_eq(&first, &again), "and shares the same words");

        // A changed image is read again, and only that one.
        let other = AssetId::<Image>::Uuid {
            uuid: bevy::asset::uuid::Uuid::from_u128(7),
        };
        cache.mask(other, &image(2, 2, [0, 0, 0, 0]));
        assert_eq!(cache.reads, 2);
        cache.forget(&HashSet::from([id]));
        let changed = cache.mask(id, &image(4, 4, [0, 255, 0, 255]));
        assert_eq!(cache.reads, 3);
        assert_ne!(*changed, *first, "the new texels are in it");
        cache.mask(other, &image(2, 2, [0, 0, 0, 0]));
        assert_eq!(cache.reads, 3, "the other image stayed cached");

        // Heightmaps are cached apart from masks of the same image.
        cache.heights(id, &image(4, 4, [128, 0, 0, 255]));
        assert_eq!(cache.reads, 4);
        cache.heights(id, &image(4, 4, [128, 0, 0, 255]));
        assert_eq!(cache.reads, 4);

        // What fields no longer use is let go.
        cache.retain(&[other], &[]);
        cache.mask(id, &image(4, 4, [0, 255, 0, 255]));
        assert_eq!(cache.reads, 5, "an image dropped from use is read again");
    }

    #[test]
    fn a_mask_packs_two_texels_to_a_word_and_pads_odd_sizes() {
        let words = pack_mask(&image(3, 1, [255, 128, 0, 255]));
        // Three texels: one full word, and a half word.
        assert_eq!(words.len() as u32, mask_words(3));
        assert_eq!(words[0], 255 | (128 << 8) | (255 << 16) | (128 << 24));
        assert_eq!(words[1], 255 | (128 << 8));
    }

    #[test]
    fn joining_one_image_copies_nothing() {
        let a = Arc::new(vec![1u32, 2, 3]);
        let b = Arc::new(vec![4u32, 5]);
        assert!(Arc::ptr_eq(&join(std::slice::from_ref(&a)), &a));
        assert_eq!(*join(&[a, b]), vec![1, 2, 3, 4, 5]);
        assert!(join::<u32>(&[]).is_empty());
    }
}
