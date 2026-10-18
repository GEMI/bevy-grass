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
