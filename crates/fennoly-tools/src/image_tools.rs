//! Image compression, resizing and thumbnails.

use std::io::Cursor;

use image::codecs::jpeg::JpegEncoder;
use image::imageops::FilterType;
use image::{DynamicImage, ImageDecoder, ImageReader, Rgb, RgbImage};

use crate::ToolError;

/// MIME types the image tools can read.
pub const SUPPORTED_MIME: &[&str] = &[
    "image/jpeg",
    "image/png",
    "image/gif",
    "image/webp",
    "image/bmp",
];

pub fn is_supported(mime: &str) -> bool {
    SUPPORTED_MIME.contains(&mime)
}

#[derive(Debug, Clone, Copy)]
pub struct CompressOptions {
    /// JPEG quality, 1 to 100.
    pub quality: u8,
    /// Longest edge in pixels; the image is only ever made smaller.
    pub max_dimension: Option<u32>,
}

const MAX_SIDE: u32 = 16_384;
const MAX_DECODE_BYTES: u64 = 400 * 1024 * 1024;

/// Decodes an image and applies its EXIF orientation, so phone photos
/// come out the right way up.
pub fn decode(bytes: &[u8]) -> Result<DynamicImage, ToolError> {
    let mut reader = ImageReader::new(Cursor::new(bytes)).with_guessed_format()?;
    // A small file can claim enormous dimensions (a "decompression bomb").
    // These limits still fit 100-megapixel photos.
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_SIDE);
    limits.max_image_height = Some(MAX_SIDE);
    limits.max_alloc = Some(MAX_DECODE_BYTES);
    reader.limits(limits);
    let mut decoder = reader.into_decoder()?;
    // Checked here too: not every format's decoder enforces `max_alloc`
    // before it allocates the pixel buffer.
    if decoder.total_bytes() > MAX_DECODE_BYTES {
        return Err(ToolError::Image(image::ImageError::Limits(image::error::LimitError::from_kind(
            image::error::LimitErrorKind::InsufficientMemory,
        ))));
    }
    let orientation = decoder.orientation()?;
    let mut img = DynamicImage::from_decoder(decoder)?;
    img.apply_orientation(orientation);
    Ok(img)
}

/// Flattens transparency onto white and returns an RGB image ready for JPEG.
fn to_rgb_on_white(img: &DynamicImage) -> RgbImage {
    let rgba = img.to_rgba8();
    let mut out = RgbImage::new(rgba.width(), rgba.height());
    for (x, y, p) in rgba.enumerate_pixels() {
        let a = p[3] as u32;
        let blend = |c: u8| ((c as u32 * a + 255 * (255 - a)) / 255) as u8;
        out.put_pixel(x, y, Rgb([blend(p[0]), blend(p[1]), blend(p[2])]));
    }
    out
}

fn shrink(img: DynamicImage, max: u32) -> DynamicImage {
    if img.width() > max || img.height() > max {
        img.resize(max, max, FilterType::Lanczos3)
    } else {
        img
    }
}

/// Re-encodes an image as JPEG with the given quality and size limit.
pub fn compress(bytes: &[u8], opts: CompressOptions) -> Result<Vec<u8>, ToolError> {
    let mut img = decode(bytes)?;
    if let Some(max) = opts.max_dimension {
        img = shrink(img, max.max(16));
    }
    let rgb = to_rgb_on_white(&img);
    let mut out = Vec::new();
    let quality = opts.quality.clamp(1, 100);
    JpegEncoder::new_with_quality(&mut out, quality).encode_image(&rgb)?;
    Ok(out)
}

/// Makes a JPEG thumbnail whose longest edge is `size` pixels.
pub fn thumbnail(bytes: &[u8], size: u32) -> Result<Vec<u8>, ToolError> {
    let img = decode(bytes)?;
    let small = img.thumbnail(size, size);
    let rgb = to_rgb_on_white(&small);
    let mut out = Vec::new();
    JpegEncoder::new_with_quality(&mut out, 78).encode_image(&rgb)?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageFormat, Rgba, RgbaImage};

    fn sample_png(w: u32, h: u32) -> Vec<u8> {
        let img = RgbaImage::from_fn(w, h, |x, y| {
            Rgba([(x * 7 % 256) as u8, (y * 13 % 256) as u8, ((x + y) % 256) as u8, 255])
        });
        let mut out = Vec::new();
        DynamicImage::ImageRgba8(img)
            .write_to(&mut Cursor::new(&mut out), ImageFormat::Png)
            .unwrap();
        out
    }

    /// A PNG whose header claims `w` x `h` pixels (the pixel data is not
    /// there: decoding must stop at the header).
    fn bomb_png(w: u32, h: u32) -> Vec<u8> {
        fn crc32(data: &[u8]) -> u32 {
            let mut crc = 0xFFFF_FFFFu32;
            for &b in data {
                crc ^= u32::from(b);
                for _ in 0..8 {
                    crc = if crc & 1 != 0 { (crc >> 1) ^ 0xEDB8_8320 } else { crc >> 1 };
                }
            }
            !crc
        }
        let mut png = sample_png(1, 1);
        // IHDR data starts after the 8-byte signature, 4-byte length and "IHDR".
        png[16..20].copy_from_slice(&w.to_be_bytes());
        png[20..24].copy_from_slice(&h.to_be_bytes());
        let crc = crc32(&png[12..29]);
        png[29..33].copy_from_slice(&crc.to_be_bytes());
        png
    }

    #[test]
    fn decompression_bombs_are_refused() {
        // Refused because of the limits, before any pixel data is read.
        for (w, h) in [(100_000, 100_000), (20_000, 2), (16_000, 16_000)] {
            let err = decode(&bomb_png(w, h)).err().map(|e| e.to_string().to_lowercase()).unwrap_or_default();
            assert!(err.contains("limit"), "{w}x{h}: {err}");
        }
        assert!(thumbnail(&bomb_png(20_000, 2), 320).is_err());
        assert!(decode(&sample_png(64, 48)).is_ok());
    }

    #[test]
    fn lower_quality_gives_smaller_files() {
        let png = sample_png(400, 300);
        let high = compress(&png, CompressOptions { quality: 95, max_dimension: None }).unwrap();
        let low = compress(&png, CompressOptions { quality: 30, max_dimension: None }).unwrap();
        assert!(low.len() < high.len(), "{} !< {}", low.len(), high.len());
    }

    #[test]
    fn resize_respects_max_dimension() {
        let png = sample_png(800, 400);
        let out = compress(&png, CompressOptions { quality: 80, max_dimension: Some(200) }).unwrap();
        let img = decode(&out).unwrap();
        assert_eq!((img.width(), img.height()), (200, 100));
    }

    #[test]
    fn never_upscales() {
        let png = sample_png(100, 50);
        let out = compress(&png, CompressOptions { quality: 80, max_dimension: Some(1000) }).unwrap();
        let img = decode(&out).unwrap();
        assert_eq!((img.width(), img.height()), (100, 50));
    }

    #[test]
    fn thumbnail_fits_box() {
        let png = sample_png(1000, 500);
        let img = decode(&thumbnail(&png, 320).unwrap()).unwrap();
        assert!(img.width() <= 320 && img.height() <= 320);
    }
}
