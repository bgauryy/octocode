use super::*;

#[test]
fn bom_utf16_decodes_both_orders_without_guessing_or_losing_coordinates() {
    let text = "α😀\r\nnext\n\u{feff}inside";
    for little in [true, false] {
        let mut bytes = if little {
            vec![0xff, 0xfe]
        } else {
            vec![0xfe, 0xff]
        };
        for unit in text.encode_utf16() {
            bytes.extend(if little {
                unit.to_le_bytes()
            } else {
                unit.to_be_bytes()
            });
        }
        assert_eq!(sniff_complete(&bytes), BinaryVerdict::Text);
        assert_eq!(decode_unicode(&bytes).as_deref(), Some(text));
        assert_eq!(decode(&bytes).text, text);
        assert!(decode(&bytes).warning.is_some());
    }
    assert_eq!(sniff_complete(b"a\0b\0"), BinaryVerdict::Binary);
    for bytes in [
        vec![0xff, 0xfe, 0x61],
        vec![0xff, 0xfe, 0, 0xd8],
        vec![0xfe, 0xff, 0xdc, 0],
        vec![0xff, 0xfe, 0, 0],
    ] {
        assert_eq!(sniff_complete(&bytes), BinaryVerdict::Binary);
    }
}

/// The one admission: binary is refused whatever the decoding; the
/// lenient decoding keeps legacy 8-bit text with a warning, the strict
/// one refuses it; both strip a leading UTF-8 BOM and decode BOM-marked
/// UTF-16 with its warning.
#[test]
fn admit_refuses_binary_and_decodes_by_the_requested_decoding() {
    for decoding in [Decoding::Lenient, Decoding::Unicode] {
        assert_eq!(
            admit(b"fn a() {}\0", decoding).unwrap_err(),
            Refusal::Binary
        );
        assert_eq!(
            admit(b"\xef\xbb\xbfa\n", decoding).unwrap().text,
            "a\n",
            "{decoding:?}"
        );
        let utf16 = [0xff, 0xfe, b'h', 0, b'i', 0];
        let admitted = admit(&utf16, decoding).unwrap();
        assert_eq!(admitted.text, "hi");
        assert_eq!(admitted.warning, Some(UTF16_WARNING));
    }
    let latin = admit(b"caf\xe9\n", Decoding::Lenient).unwrap();
    assert_eq!(latin.text, "caf\u{e9}\n");
    assert!(latin.warning.is_some());
    assert_eq!(
        admit(b"caf\xe9\n", Decoding::Unicode).unwrap_err(),
        Refusal::NotUnicode
    );
    assert!(
        admit(b"plain\n", Decoding::Unicode)
            .unwrap()
            .warning
            .is_none()
    );
    assert!(matches!(
        admit(b"plain\n", Decoding::Unicode).unwrap().text,
        Cow::Borrowed(_)
    ));
}

#[test]
fn utf16_sniff_boundary_does_not_split_a_surrogate_pair() {
    let mut bytes = vec![0xff, 0xfe];
    for unit in "a"
        .repeat((SNIFF_BYTES - 4) / 2)
        .encode_utf16()
        .chain("😀".encode_utf16())
    {
        bytes.extend(unit.to_le_bytes());
    }
    assert_eq!(sniff(&bytes[..SNIFF_BYTES]), BinaryVerdict::Text);
    assert_eq!(sniff(&bytes), BinaryVerdict::Text);
    assert_eq!(sniff_complete(&bytes), BinaryVerdict::Text);
    bytes.extend([0, 0]);
    assert_eq!(sniff_complete(&bytes), BinaryVerdict::Binary);
}

#[test]
fn a_file_past_the_limit_or_not_regular_is_refused() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("big.rs");
    std::fs::write(&path, "x".repeat(65)).expect("write");
    assert!(matches!(
        read_source(&path, 64),
        Err(ReadError::TooLarge(65))
    ));
    assert_eq!(read_source(&path, 65).expect("read").bytes.len(), 65);
    assert!(matches!(
        read_source(dir.path(), 1024),
        Err(ReadError::NotRegular)
    ));
    assert!(matches!(
        read_source(&dir.path().join("missing"), 1024),
        Err(ReadError::Io(_))
    ));
}

/// One bounded read serves every caller: it continues after the bytes
/// already read, refuses a source past the limit with the length it
/// read, and stops at the first failed check between chunks.
#[test]
fn a_bounded_read_continues_a_prefix_and_stops_at_its_limit_or_a_check() {
    let source = b"0123456789".repeat(10_000);
    let read = read_source_from(&source[4..], source[..4].to_vec(), 100_000, || Ok(()))
        .expect("within the limit");
    assert_eq!(read, source);
    assert!(matches!(
        read_source_from(&source[..], Vec::new(), 99_999, || Ok(())),
        Err(ReadError::TooLarge(100_000))
    ));
    let mut checks = 0;
    let stopped = read_source_from(&source[..], Vec::new(), 100_000, || {
        checks += 1;
        if checks > 1 {
            Err(std::io::Error::new(
                std::io::ErrorKind::Interrupted,
                "cancelled",
            ))
        } else {
            Ok(())
        }
    });
    assert!(
        matches!(&stopped, Err(ReadError::Io(error)) if error.kind() == std::io::ErrorKind::Interrupted),
        "{stopped:?}"
    );
}

#[cfg(unix)]
#[test]
fn a_read_follows_a_symlink_and_a_walk_open_refuses_it() {
    let dir = tempfile::tempdir().expect("tempdir");
    let target = dir.path().join("target.rs");
    std::fs::write(&target, "fn a() {}").expect("write");
    let link = dir.path().join("link.rs");
    std::os::unix::fs::symlink(&target, &link).expect("link");
    assert_eq!(read_source(&link, 1024).expect("read").bytes, b"fn a() {}");
    assert!(open_regular(&link).is_err());
}

#[test]
fn binary_is_a_nul_or_dense_controls_and_never_an_encoding() {
    assert_eq!(sniff(b"fn a() {}\0"), BinaryVerdict::Binary);
    assert_eq!(
        sniff(b"\x89PNG\r\n\x1a\n\x02\x03\x04"),
        BinaryVerdict::Binary
    );
    assert_eq!(sniff("caf\u{e9}\n".as_bytes()), BinaryVerdict::Text);
    assert_eq!(sniff(b"caf\xe9\n"), BinaryVerdict::Text);
    assert_eq!(sniff(b"\x1b[31mred\x1b[0m\n"), BinaryVerdict::Text);
    assert_eq!(sniff(b""), BinaryVerdict::Text);
    // A NUL past the sniffed bytes does not make text binary.
    let mut late = vec![b'a'; SNIFF_BYTES];
    late.push(0);
    assert_eq!(sniff(&late), BinaryVerdict::Text);
}

#[test]
fn a_char_straddling_the_sniff_boundary_and_stray_bytes_are_text() {
    let mut data = vec![b'a'; SNIFF_BYTES - 2];
    data.extend_from_slice("\u{1f600}".as_bytes());
    data.extend(std::iter::repeat_n(b'b', 256));
    assert_eq!(sniff(&data), BinaryVerdict::Text);
    let mut stray = vec![b'a'; 100];
    stray[50] = 0xff;
    assert_eq!(sniff(&stray), BinaryVerdict::Text);
    let mut controls = vec![b'a'; 100];
    controls[..10].copy_from_slice(&[1, 2, 3, 4, 5, 6, 7, 8, 14, 15]);
    assert_eq!(sniff(&controls), BinaryVerdict::Binary);
}

#[test]
fn decode_keeps_utf8_and_falls_back_to_latin1_or_replacement() {
    assert_eq!(decode(b"ok").warning, None);
    let latin = decode(b"caf\xe9");
    assert_eq!(latin.text, "caf\u{e9}");
    assert!(latin.warning.is_some_and(|w| w.contains("Latin-1")));
    let mixed = decode(b"\xc3\xa9\xff");
    assert_eq!(mixed.text, "\u{e9}\u{fffd}");
}

/// A leading UTF-8 BOM is not text in any decoding: valid UTF-8 stays
/// borrowed, and the fallbacks neither keep U+FEFF nor show it as
/// Latin-1 `ï»¿`. A later U+FEFF is text.
#[test]
fn decode_drops_a_leading_bom_in_every_decoding() {
    let utf8 = decode(b"\xef\xbb\xbfalpha\n\xef\xbb\xbfbeta");
    assert_eq!(utf8.text, "alpha\n\u{feff}beta");
    assert!(matches!(utf8.text, Cow::Borrowed(_)));
    assert_eq!(decode(b"\xef\xbb\xbf\xc3\xa9\xff").text, "\u{e9}\u{fffd}");
    assert_eq!(decode(b"\xef\xbb\xbfcaf\xe9").text, "caf\u{e9}");
}

/// Bytes inside a file keep a leading U+FEFF: only a file's first
/// bytes can hold its byte-order mark.
#[test]
fn interior_bytes_keep_a_leading_feff_as_text() {
    let window = Decoded::interior(b"\xef\xbb\xbfgamma\n");
    assert_eq!(window.text, "\u{feff}gamma\n");
    assert!(window.warning.is_none());
    assert_eq!(
        Decoded::interior(b"\xef\xbb\xbfcaf\xe9").text,
        "\u{feff}caf\u{fffd}"
    );
}
