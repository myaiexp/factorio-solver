// Crops a Factorio icon file down to its leading full-resolution square.

use image::{DynamicImage, GenericImageView};

/// Leftmost `icon_size` square. Handles both observed layouts: most icon
/// files are a mipmap strip (full-resolution icon followed by shrinking mip
/// levels laid out to the right), so cropping the leading `icon_size`
/// columns strips the tail; on the few plain `icon_size`x`icon_size` files
/// it's a no-op. An image smaller than `icon_size` in either dimension is
/// returned whole rather than cropped, so a corrupt/unexpected file never
/// panics or produces a zero-size image.
pub fn crop_icon(img: DynamicImage, icon_size: u32) -> DynamicImage {
    let (width, height) = img.dimensions();
    if width < icon_size || height < icon_size {
        return img;
    }
    img.crop_imm(0, 0, icon_size, icon_size)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    #[test]
    fn crops_mipmap_strip_to_leading_square() {
        let img = DynamicImage::new_rgba8(120, 64); // 153 of 156 real icons
        assert_eq!(crop_icon(img, 64).dimensions(), (64, 64));
    }

    #[test]
    fn passes_through_plain_square_icon() {
        let img = DynamicImage::new_rgba8(64, 64); // the other 3
        assert_eq!(crop_icon(img, 64).dimensions(), (64, 64));
    }

    #[test]
    fn undersized_image_is_used_whole_not_cropped() {
        let img = DynamicImage::new_rgba8(32, 32);
        assert_eq!(crop_icon(img, 64).dimensions(), (32, 32));
    }

    #[test]
    fn exact_size_image_is_returned_unchanged() {
        let img = DynamicImage::new_rgba8(64, 64);
        assert_eq!(crop_icon(img, 64).dimensions(), (64, 64));
    }

    #[test]
    fn crop_preserves_the_leftmost_pixels() {
        let mut buf = image::RgbaImage::new(8, 4);
        for y in 0..4 {
            for x in 0..8u32 {
                let color =
                    if x < 4 { Rgba([255, 0, 0, 255]) } else { Rgba([0, 0, 255, 255]) };
                buf.put_pixel(x, y, color);
            }
        }
        let cropped = crop_icon(DynamicImage::ImageRgba8(buf), 4).to_rgba8();
        assert_eq!(cropped.dimensions(), (4, 4));
        for pixel in cropped.pixels() {
            assert_eq!(*pixel, Rgba([255, 0, 0, 255]));
        }
    }
}
