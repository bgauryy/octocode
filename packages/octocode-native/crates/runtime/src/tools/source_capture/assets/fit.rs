//! Fitting an image into the provider window: a side above `max` scales the
//! whole image down by area averaging (aspect kept, never cropped), then a
//! side below `min` is centered on a transparent PNG canvas.
use super::invalid;
use crate::id::clasify_policy;
use crate::tools::judgment::ClassificationError;
use octocode_contract_types::ErrorCode;
use std::io::{Cursor, Write as _};
use zune_jpeg::zune_core::{colorspace::ColorSpace, options::DecoderOptions};

/// PNG IHDR color type for 8-bit RGBA.
const RGBA: u8 = 6;

/// Returns the fitted size and its PNG bytes: each side ends within
/// `min..=max`, except a side the aspect ratio shrinks below `min`, which
/// padding then lifts.
pub(super) fn fit(
    bytes: &[u8],
    format: &str,
    width: usize,
    height: usize,
    min: usize,
    max: usize,
) -> Result<(usize, usize, Vec<u8>), ClassificationError> {
    let mut rgba = match format {
        "png" => decode_png(bytes, width, height)?,
        _ => decode_jpeg(bytes, width, height)?,
    };
    let (mut width, mut height) = (width, height);
    if width > max || height > max {
        let scale = (max as f64 / width as f64).min(max as f64 / height as f64);
        let side = |length: usize| ((length as f64 * scale).round() as usize).clamp(1, max);
        let (scaled_width, scaled_height) = (side(width), side(height));
        rgba = downscale(&rgba, width, height, scaled_width, scaled_height);
        (width, height) = (scaled_width, scaled_height);
    }
    let (canvas_width, canvas_height) = (width.max(min), height.max(min));
    let canvas = if (canvas_width, canvas_height) == (width, height) {
        rgba
    } else {
        let (left, top) = ((canvas_width - width) / 2, (canvas_height - height) / 2);
        let mut canvas = vec![0u8; canvas_width * canvas_height * 4];
        for (row, line) in rgba.chunks_exact(width * 4).enumerate() {
            let start = ((top + row) * canvas_width + left) * 4;
            canvas[start..start + line.len()].copy_from_slice(line);
        }
        canvas
    };
    Ok((
        canvas_width,
        canvas_height,
        encode_png(&canvas, canvas_width, canvas_height, RGBA)?,
    ))
}

/// Each output index's source taps on one axis: the exact fractional
/// coverage of the source pixels under it, normalized to sum to one.
fn taps(input: usize, output: usize) -> Vec<Vec<(usize, f32)>> {
    let scale = input as f64 / output as f64;
    (0..output)
        .map(|index| {
            let (start, end) = (index as f64 * scale, (index + 1) as f64 * scale);
            (start.floor() as usize..(end.ceil() as usize).min(input))
                .filter_map(|source| {
                    let cover = end.min(source as f64 + 1.0) - start.max(source as f64);
                    (cover > 0.0).then_some((source, (cover / scale) as f32))
                })
                .collect()
        })
        .collect()
}

/// Area-averaged downscale with alpha-weighted color, so transparent pixels
/// never tint their neighbours. One output row at a time keeps memory at a
/// few rows beyond the source and result.
fn downscale(
    rgba: &[u8],
    width: usize,
    height: usize,
    out_width: usize,
    out_height: usize,
) -> Vec<u8> {
    let columns = taps(width, out_width);
    let premultiplied_row = |y: usize| -> Vec<[f32; 4]> {
        let row = &rgba[y * width * 4..][..width * 4];
        columns
            .iter()
            .map(|taps| {
                taps.iter().fold([0f32; 4], |mut sum, &(x, weight)| {
                    let pixel = &row[x * 4..][..4];
                    let alpha = f32::from(pixel[3]) / 255.0;
                    for channel in 0..3 {
                        sum[channel] += f32::from(pixel[channel]) * alpha * weight;
                    }
                    sum[3] += f32::from(pixel[3]) * weight;
                    sum
                })
            })
            .collect()
    };
    let mut out = Vec::with_capacity(out_width * out_height * 4);
    for rows in taps(height, out_height) {
        let mut sum = vec![[0f32; 4]; out_width];
        for (y, weight) in rows {
            for (total, value) in sum.iter_mut().zip(premultiplied_row(y)) {
                for channel in 0..4 {
                    total[channel] += value[channel] * weight;
                }
            }
        }
        for [red, green, blue, alpha] in sum {
            let color = |value: f32| {
                if alpha > 0.0 {
                    (value * 255.0 / alpha).round().clamp(0.0, 255.0) as u8
                } else {
                    0
                }
            };
            out.extend([
                color(red),
                color(green),
                color(blue),
                alpha.round().clamp(0.0, 255.0) as u8,
            ]);
        }
    }
    out
}

fn options() -> DecoderOptions {
    DecoderOptions::default()
        .set_max_width(clasify_policy::MAX_SOURCE_IMAGE_DIMENSION)
        .set_max_height(clasify_policy::MAX_SOURCE_IMAGE_DIMENSION)
}

fn decode_png(bytes: &[u8], width: usize, height: usize) -> Result<Vec<u8>, ClassificationError> {
    let undecodable = || invalid("The PNG image could not be decoded.");
    let options = options()
        .png_set_strip_to_8bit(true)
        .png_set_add_alpha_channel(true);
    let mut decoder = zune_png::PngDecoder::new_with_options(Cursor::new(bytes), options);
    let pixels = decoder.decode_raw().map_err(|_| undecodable())?;
    if decoder.dimensions() != Some((width, height)) {
        return Err(undecodable());
    }
    let rgba: Vec<u8> = match decoder.colorspace() {
        Some(ColorSpace::RGBA) => pixels,
        Some(ColorSpace::LumaA) => pixels
            .as_chunks::<2>()
            .0
            .iter()
            .flat_map(|&[g, a]| [g, g, g, a])
            .collect(),
        _ => return Err(undecodable()),
    };
    if rgba.len() != width * height * 4 {
        return Err(undecodable());
    }
    Ok(rgba)
}

fn decode_jpeg(bytes: &[u8], width: usize, height: usize) -> Result<Vec<u8>, ClassificationError> {
    let options = options().jpeg_set_out_colorspace(ColorSpace::RGBA);
    let mut decoder = zune_jpeg::JpegDecoder::new_with_options(Cursor::new(bytes), options);
    let rgba = decoder
        .decode()
        .map_err(|_| invalid("The JPEG image could not be decoded."))?;
    if rgba.len() != width * height * 4 {
        return Err(invalid("The JPEG image could not be decoded."));
    }
    Ok(rgba)
}

/// Writes an 8-bit, non-interlaced PNG with unfiltered scanlines.
fn encode_png(
    pixels: &[u8],
    width: usize,
    height: usize,
    color: u8,
) -> Result<Vec<u8>, ClassificationError> {
    let failed = || {
        ClassificationError::new(
            ErrorCode::ClassificationContextFailed,
            "The image could not be padded to the minimum size.",
            "Pad the image to the minimum size yourself and retry.",
        )
    };
    let stride = pixels.len() / height;
    let mut deflate = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    for line in pixels.chunks_exact(stride) {
        deflate.write_all(&[0]).map_err(|_| failed())?;
        deflate.write_all(line).map_err(|_| failed())?;
    }
    let mut header = Vec::with_capacity(13);
    header.extend_from_slice(&u32::try_from(width).map_err(|_| failed())?.to_be_bytes());
    header.extend_from_slice(&u32::try_from(height).map_err(|_| failed())?.to_be_bytes());
    header.extend_from_slice(&[8, color, 0, 0, 0]);
    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    for (kind, data) in [
        (b"IHDR", header),
        (b"IDAT", deflate.finish().map_err(|_| failed())?),
        (b"IEND", Vec::new()),
    ] {
        out.extend_from_slice(
            &u32::try_from(data.len())
                .map_err(|_| failed())?
                .to_be_bytes(),
        );
        let mut crc = crc32fast::Hasher::new();
        crc.update(kind);
        crc.update(&data);
        out.extend_from_slice(kind);
        out.extend_from_slice(&data);
        out.extend_from_slice(&crc.finalize().to_be_bytes());
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::super::{Fitted, fit as admit};
    use super::*;

    /// PNG IHDR color types used by the fixtures below.
    const RGB: u8 = 2;
    const GRAY_ALPHA: u8 = 4;

    fn encoded(color: u8, width: usize, height: usize, pixel: &[u8]) -> Vec<u8> {
        encode_png(&pixel.repeat(width * height), width, height, color).unwrap()
    }

    fn pixels(bytes: &[u8]) -> (usize, usize, Vec<u8>) {
        let mut decoder = zune_png::PngDecoder::new(Cursor::new(bytes));
        let rgba = decoder.decode_raw().unwrap();
        assert_eq!(decoder.colorspace(), Some(ColorSpace::RGBA));
        let (width, height) = decoder.dimensions().unwrap();
        (width, height, rgba)
    }

    fn at(rgba: &[u8], width: usize, x: usize, y: usize) -> &[u8] {
        &rgba[(y * width + x) * 4..][..4]
    }

    #[test]
    fn fit_pads_short_axes_with_centered_transparency() {
        for (width, height, canvas) in [
            (500, 104, (500, 280)),
            (104, 500, (280, 500)),
            (16, 8, (280, 280)),
            (279, 281, (280, 281)),
        ] {
            let source = encoded(RGB, width, height, &[10, 20, 30]);
            let fitted = admit(source).unwrap();
            assert_eq!(
                (fitted.format, fitted.width, fitted.height, fitted.source),
                ("png", canvas.0, canvas.1, Some((width, height)))
            );
            let (decoded_width, decoded_height, rgba) = pixels(&fitted.bytes);
            assert_eq!((decoded_width, decoded_height), canvas);
            let (left, top) = ((canvas.0 - width) / 2, (canvas.1 - height) / 2);
            assert_eq!(at(&rgba, canvas.0, left, top), [10, 20, 30, 255]);
            assert_eq!(
                at(&rgba, canvas.0, left + width - 1, top + height - 1),
                [10, 20, 30, 255]
            );
            if left > 0 || top > 0 {
                assert_eq!(at(&rgba, canvas.0, 0, 0), [0, 0, 0, 0], "{width}x{height}");
            }
            assert_eq!(
                at(&rgba, canvas.0, canvas.0 - 1, canvas.1 - 1),
                [0, 0, 0, 0],
                "{width}x{height}"
            );
        }
    }

    #[test]
    fn fit_keeps_source_alpha_and_gray_and_admits_window_images_unchanged() {
        let fitted = admit(encoded(GRAY_ALPHA, 8, 8, &[200, 50])).unwrap();
        let (width, _, rgba) = pixels(&fitted.bytes);
        assert_eq!(at(&rgba, width, 140, 140), [200, 200, 200, 50]);
        let exact = encoded(RGB, 280, 1568, &[1, 2, 3]);
        assert_eq!(
            admit(exact.clone()).unwrap(),
            Fitted {
                format: "png",
                width: 280,
                height: 1568,
                bytes: exact,
                source: None,
            }
        );
    }

    #[test]
    fn fit_pads_short_jpeg_into_png() {
        let jpeg = include_bytes!("../../../../tests/fixtures/clasify/short.jpg").to_vec();
        let fitted = admit(jpeg).unwrap();
        assert_eq!(
            (fitted.format, fitted.width, fitted.height, fitted.source),
            ("png", 280, 280, Some((16, 8)))
        );
        let (_, _, rgba) = pixels(&fitted.bytes);
        assert_eq!(at(&rgba, 280, 140, 140)[3], 255);
        assert_eq!(at(&rgba, 280, 0, 0), [0, 0, 0, 0]);
    }

    #[test]
    fn fit_scales_large_images_into_the_window_keeping_aspect() {
        for (width, height, fitted_size) in [
            (1600, 900, (1568, 882)),
            (900, 1600, (882, 1568)),
            (3000, 3000, (1568, 1568)),
        ] {
            let fitted = admit(encoded(RGB, width, height, &[10, 20, 30])).unwrap();
            assert_eq!(
                (fitted.width, fitted.height, fitted.source),
                (fitted_size.0, fitted_size.1, Some((width, height)))
            );
            let (_, _, rgba) = pixels(&fitted.bytes);
            assert!(
                rgba.as_chunks::<4>()
                    .0
                    .iter()
                    .all(|pixel| *pixel == [10, 20, 30, 255]),
                "{width}x{height}"
            );
        }
    }

    #[test]
    fn fit_scales_then_pads_an_extreme_aspect() {
        let fitted = admit(encoded(RGB, 4000, 100, &[10, 20, 30])).unwrap();
        assert_eq!((fitted.width, fitted.height), (1568, 280));
        let (_, _, rgba) = pixels(&fitted.bytes);
        assert_eq!(at(&rgba, 1568, 0, 0), [0, 0, 0, 0]);
        assert_eq!(at(&rgba, 1568, 784, 140), [10, 20, 30, 255]);
    }

    #[test]
    fn downscale_averages_area_and_weights_color_by_alpha() {
        let gray = downscale(&[0, 0, 0, 255, 255, 255, 255, 255], 2, 1, 1, 1);
        assert_eq!(gray, [128, 128, 128, 255]);
        let red = downscale(&[255, 0, 0, 255, 0, 0, 0, 0], 2, 1, 1, 1);
        assert_eq!(red, [255, 0, 0, 128]);
        // Three source pixels into two: the middle one splits evenly.
        let split = downscale(
            &[0, 0, 0, 255, 90, 90, 90, 255, 180, 180, 180, 255],
            3,
            1,
            2,
            1,
        );
        assert_eq!(split, [30, 30, 30, 255, 150, 150, 150, 255]);
    }
}
