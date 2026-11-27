use super::*;

#[test]
fn image_capture_keeps_binary_bytes_and_final_origin() {
    let bytes = vec![0x89, b'P', b'N', b'G', 0, 0xff, 0xfe];
    let fake = Fake::default()
        .status("https://example.org/image", 302, Some("/actual.png"))
        .bytes(
            "https://example.org/actual.png",
            Some("image/png"),
            bytes.clone(),
        );
    let (captured, source) =
        select_image(&fake, parse("https://example.org/image").unwrap()).unwrap();
    assert_eq!(captured, bytes);
    assert_eq!(source["url"], "https://example.org/actual.png");
    assert_eq!(fake.requested.borrow().len(), 2);
}

#[test]
fn an_image_source_never_follows_embedded_images_or_unsupported_media() {
    for (media, body) in [
        ("text/html", b"<img src='/nested.png'>".as_slice()),
        ("image/webp", b"RIFFWEBP".as_slice()),
    ] {
        let fake = Fake::default().bytes("https://example.org/image", Some(media), body.to_vec());
        let error = select_image(&fake, parse("https://example.org/image").unwrap()).unwrap_err();
        assert_eq!(error.code, ErrorCode::ClassificationContextUnavailable);
        assert_eq!(fake.requested.borrow().len(), 1);
    }
}

#[test]
fn image_fragments_are_rejected_at_origin_and_after_redirect() {
    let fake = Fake::default()
        .status(
            "https://example.org/image",
            302,
            Some("/actual.png#xywh=0,0,1,1"),
        )
        .bytes(
            "https://example.org/actual.png",
            Some("image/png"),
            vec![1, 2, 3],
        );
    assert_eq!(
        select_image(&fake, parse("https://example.org/image#crop").unwrap())
            .unwrap_err()
            .code,
        ErrorCode::InvalidClassificationContext
    );
    assert!(fake.requested.borrow().is_empty());
    assert_eq!(
        select_image(&fake, parse("https://example.org/image").unwrap())
            .unwrap_err()
            .code,
        ErrorCode::InvalidClassificationContext
    );
}
