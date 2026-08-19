use std::fs::File;
use std::io::BufReader;
use std::path::Path;
use std::sync::Once;

use fast_image_resize::images::Image as FirImage;
use fast_image_resize::pixels::PixelType;
use fast_image_resize::Resizer;
use image::{ImageBuffer, Rgba};

use super::request::DecodedImage;

type RgbaImage = ImageBuffer<Rgba<u8>, Vec<u8>>;

static REGISTER_HEIC: Once = Once::new();

// must run once before any decode, so image::ImageReader can open heic/heif files via libheif
pub fn ensure_heic_registered() {
    REGISTER_HEIC.call_once(|| {
        libheif_rs::integration::image::register_all_decoding_hooks();
    });
}

pub fn decode_and_resize(path: &Path, target_long_edge: u32) -> anyhow::Result<DecodedImage> {
    ensure_heic_registered();

    let is_jpeg = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case("jpg") || e.eq_ignore_ascii_case("jpeg"))
        .unwrap_or(false);

    let mut rgba: RgbaImage = if is_jpeg {
        match decode_jpeg_scaled(path, target_long_edge) {
            Ok(img) => img,
            Err(_) => decode_via_image_crate(path)?,
        }
    } else {
        decode_via_image_crate(path)?
    };

    if let Some(orientation) = read_exif_orientation(path) {
        rgba = apply_orientation(rgba, orientation);
    }

    let (src_w, src_h) = rgba.dimensions();
    let (dst_w, dst_h) = scaled_dims(src_w, src_h, target_long_edge);

    if (dst_w, dst_h) == (src_w, src_h) {
        return Ok(DecodedImage {
            width: src_w,
            height: src_h,
            rgba: rgba.into_raw(),
        });
    }

    let mut src_image = FirImage::new(src_w, src_h, PixelType::U8x4);
    src_image.buffer_mut().copy_from_slice(rgba.as_raw());

    let mut dst_image = FirImage::new(dst_w, dst_h, PixelType::U8x4);
    let mut resizer = Resizer::new();
    resizer.resize(&src_image, &mut dst_image, None)?;

    Ok(DecodedImage {
        width: dst_w,
        height: dst_h,
        rgba: dst_image.buffer().to_vec(),
    })
}

fn decode_via_image_crate(path: &Path) -> anyhow::Result<RgbaImage> {
    let img = image::ImageReader::open(path)?
        .with_guessed_format()?
        .decode()?;
    Ok(img.to_rgba8())
}

fn decode_jpeg_scaled(path: &Path, target_long_edge: u32) -> anyhow::Result<RgbaImage> {
    let bytes = std::fs::read(path)?;
    let mut decompressor = turbojpeg::Decompressor::new()?;
    let header = decompressor.read_header(&bytes)?;
    let long_edge = header.width.max(header.height);

    let factor = if target_long_edge == 0 || long_edge <= target_long_edge as usize {
        turbojpeg::ScalingFactor::ONE
    } else {
        turbojpeg::Decompressor::supported_scaling_factors()
            .into_iter()
            .filter(|f| f.scale(long_edge) >= target_long_edge as usize)
            .min_by_key(|f| f.scale(long_edge))
            .unwrap_or(turbojpeg::ScalingFactor::ONE)
    };
    decompressor.set_scaling_factor(factor)?;

    let scaled_w = factor.scale(header.width);
    let scaled_h = factor.scale(header.height);
    let mut pixels = vec![0u8; scaled_w * scaled_h * 4];
    let output = turbojpeg::Image {
        pixels: pixels.as_mut_slice(),
        width: scaled_w,
        pitch: scaled_w * 4,
        height: scaled_h,
        format: turbojpeg::PixelFormat::RGBA,
    };
    decompressor.decompress(&bytes, output)?;

    RgbaImage::from_raw(scaled_w as u32, scaled_h as u32, pixels)
        .ok_or_else(|| anyhow::anyhow!("failed to assemble decoded JPEG buffer"))
}

fn scaled_dims(width: u32, height: u32, target_long_edge: u32) -> (u32, u32) {
    let long_edge = width.max(height);
    if long_edge <= target_long_edge || target_long_edge == 0 {
        return (width, height);
    }
    let scale = target_long_edge as f64 / long_edge as f64;
    let new_w = ((width as f64) * scale).round().max(1.0) as u32;
    let new_h = ((height as f64) * scale).round().max(1.0) as u32;
    (new_w, new_h)
}

fn read_exif_orientation(path: &Path) -> Option<u32> {
    let file = File::open(path).ok()?;
    let mut reader = BufReader::new(file);
    let exif = exif::Reader::new().read_from_container(&mut reader).ok()?;
    let field = exif.get_field(exif::Tag::Orientation, exif::In::PRIMARY)?;
    field.value.get_uint(0)
}

fn apply_orientation(img: RgbaImage, orientation: u32) -> RgbaImage {
    match orientation {
        2 => image::imageops::flip_horizontal(&img),
        3 => image::imageops::rotate180(&img),
        4 => image::imageops::flip_vertical(&img),
        5 => image::imageops::flip_horizontal(&image::imageops::rotate90(&img)),
        6 => image::imageops::rotate90(&img),
        7 => image::imageops::flip_horizontal(&image::imageops::rotate270(&img)),
        8 => image::imageops::rotate270(&img),
        _ => img,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scaled_dims_never_upscales() {
        assert_eq!(scaled_dims(400, 300, 2000), (400, 300));
    }

    #[test]
    fn scaled_dims_preserves_aspect_ratio() {
        let (w, h) = scaled_dims(4000, 3000, 2000);
        assert_eq!(w, 2000);
        assert_eq!(h, 1500);
    }
}
