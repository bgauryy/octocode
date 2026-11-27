use super::*;

fn header(width: u32, height: u32) -> Vec<u8> {
    let mut bytes = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
    bytes.extend_from_slice(&width.to_be_bytes());
    bytes.extend_from_slice(&height.to_be_bytes());
    bytes.resize(33, 0);
    bytes
}

#[test]
fn dimensions_reject_unknown_truncated_zero_and_excessive_headers() {
    for bytes in [
        vec![],
        vec![0xff, 0xd8, 0xff],
        b"GIF89a".to_vec(),
        header(0, 1),
        header(1, 0),
    ] {
        assert_eq!(
            dimensions(&bytes).unwrap_err().code,
            ErrorCode::InvalidClassificationContext
        );
    }
    for bytes in [header(8193, 1), header(1, 8193), header(8193, 8193)] {
        assert_eq!(
            dimensions(&bytes).unwrap_err().code,
            ErrorCode::ClassificationContextTooLarge
        );
    }
    assert_eq!(
        dimensions(&header(1568, 1568)).unwrap(),
        ("png", 1568, 1568)
    );
}

#[test]
fn fit_rejects_short_images_whose_pixels_do_not_decode() {
    assert_eq!(
        fit(header(500, 104)).unwrap_err().code,
        ErrorCode::InvalidClassificationContext
    );
}

#[test]
fn fit_rejects_large_images_whose_pixels_do_not_decode() {
    assert_eq!(
        fit(header(1600, 900)).unwrap_err().code,
        ErrorCode::InvalidClassificationContext
    );
}

#[test]
fn dimensions_accept_every_side_up_to_the_source_maximum() {
    for (width, height) in [(1, 1), (280, 1568), (1569, 280), (8192, 8192)] {
        assert_eq!(
            dimensions(&header(width, height)).unwrap(),
            ("png", width as usize, height as usize)
        );
    }
}

#[test]
fn dimensions_reject_sides_beyond_the_source_maximum() {
    for (width, height) in [(usize::MAX, 2), (2, usize::MAX), (8193, 8192)] {
        assert_eq!(
            validate_dimensions(width, height).unwrap_err().code,
            ErrorCode::ClassificationContextTooLarge
        );
    }
}
