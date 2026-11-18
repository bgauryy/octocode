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
            frame: None,
        }
    );
}

#[test]
fn fit_pads_short_jpeg_into_png() {
    let jpeg = include_bytes!("../../../../../runtime/tests/fixtures/clasify/short.jpg").to_vec();
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
