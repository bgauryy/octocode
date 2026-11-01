use super::*;

#[test]
fn decoded_identity_is_not_raw_identity() {
    let text = "a😀\r\n";
    let wide: Vec<_> = [0xff, 0xfe]
        .into_iter()
        .chain(text.encode_utf16().flat_map(u16::to_le_bytes))
        .collect();
    let mut budget = budget();
    let utf8 = AdmittedSource::from_bytes(text.as_bytes(), "file", &mut budget).unwrap();
    let utf16 = AdmittedSource::from_bytes(&wide, "file", &mut budget).unwrap();
    let templates = SourceTemplates::new(
        text,
        Producer::new("test", "1"),
        AnalysisOperation::Declarations,
    )
    .unwrap();
    let a = templates.bind(&utf8, &mut budget).unwrap();
    let b = templates.bind(&utf16, &mut budget).unwrap();
    let one = a.project(ObservedSpan::root(1, 5), &mut budget).unwrap();
    let two = b.project(ObservedSpan::root(1, 5), &mut budget).unwrap();
    assert_ne!(one.analysis_unit_id, two.analysis_unit_id);
    assert!(
        matches!(&one.segments[0].branches[0], OriginBranch::Copied(origin) if origin.raw_range == ByteRange::new(1,5))
    );
    assert!(
        matches!(&two.segments[0].branches[0], OriginBranch::Transcoded(origin) if origin.raw_range == ByteRange::new(4,8))
    );
    assert!(b.project(ObservedSpan::root(2, 2), &mut budget).is_err());
}

#[test]
fn bom_empty_anchors_and_scalar_boundaries_are_exact() {
    for (bytes, raw) in [
        (vec![], 0),
        (vec![0xef, 0xbb, 0xbf], 3),
        (vec![0xff, 0xfe], 2),
        (vec![0xfe, 0xff], 2),
    ] {
        let source = AdmittedSource::from_bytes(&bytes, "empty", &mut budget()).unwrap();
        let projection = source.project(ByteRange::new(0, 0)).unwrap();
        assert!(
            matches!(projection,OriginBranch::Copied(ref v)|OriginBranch::Transcoded(ref v) if v.raw_range==ByteRange::new(raw,raw))
        );
    }
    for bytes in [
        vec![0xff, 0xfe, 0, 0xd8],
        vec![0xfe, 0xff, 0xd8, 0],
        vec![0xc0, 0xaf],
        vec![0xff, 0xfe, 0x41],
    ] {
        assert!(matches!(
            AdmittedSource::from_bytes(&bytes, "bad", &mut budget()),
            Err(SourceError::Refused(_))
        ));
    }
}

#[test]
#[expect(
    clippy::print_stderr,
    reason = "explicit resource-profile test emits its measured accounting receipt"
)]
fn utf16_checkpoint_index_is_bounded_for_eight_mib_input() {
    let text = "x".repeat(8 << 20);
    let raw: Vec<_> = [0xff, 0xfe]
        .into_iter()
        .chain(text.encode_utf16().flat_map(u16::to_le_bytes))
        .collect();
    let mut budget = budget();
    let source = AdmittedSource::from_bytes(&raw, "large", &mut budget).unwrap();
    eprintln!(
        "8MiB decoded UTF16: owned={} charged={} work={}",
        source.owned_bytes(),
        budget.usage.bytes,
        budget.usage.work
    );
    assert!(source.owned_bytes() < 26 << 20);
    assert!(budget.usage.work < 40_000);
    for at in [0, 255, 256, 257, 8 << 20] {
        assert!(
            matches!(source.project(ByteRange::new(at,at)).unwrap(),OriginBranch::Transcoded(v) if v.raw_range==ByteRange::new(2+at*2,2+at*2))
        );
    }
}

proptest::proptest! {
    #[test]
    fn utf16_all_scalar_spans_round_trip(chars in proptest::collection::vec(proptest::sample::select(vec!['a','é','中','😀','\r','\n']),0..600)) {
        let text: String=chars.iter().collect();
        let raw: Vec<_>=[0xfe,0xff].into_iter().chain(text.encode_utf16().flat_map(u16::to_be_bytes)).collect();
        let source=AdmittedSource::from_bytes(&raw,"property",&mut budget()).unwrap();
        let mut raw_at=2u32;
        for (at,ch) in text.char_indices() {
            let end=at+ch.len_utf8();
            let expected=ByteRange::new(raw_at,raw_at+ch.len_utf16() as u32*2);
            proptest::prop_assert!(matches!(source.project(ByteRange::new(at as u32,end as u32)).unwrap(),OriginBranch::Transcoded(v) if v.raw_range==expected));
            for interior in at+1..end { proptest::prop_assert!(source.project(ByteRange::new(interior as u32,interior as u32)).is_err()); }
            raw_at=expected.end_byte;
        }
    }
}

/// One admission names its bytes and its text by digest: the text digest is
/// the bytes' own when the text is those bytes, and the text's hash when
/// decoding changed them (BOM-marked UTF-16).
#[test]
fn admitted_digests_name_the_bytes_and_the_text() {
    let utf16: Vec<u8> = [0xff, 0xfe]
        .into_iter()
        .chain("fn é() {}\n".encode_utf16().flat_map(u16::to_le_bytes))
        .collect();
    for bytes in [
        b"fn plain() {}\n".to_vec(),
        "\u{feff}fn bom() {}\n".as_bytes().to_vec(),
        utf16,
    ] {
        let admitted = AdmittedSource::from_bytes(&bytes, "a.rs", &mut budget()).unwrap();
        assert_eq!(
            admitted.source_digest(),
            Some(crate::digest::sha256(&bytes).as_str())
        );
        assert_eq!(
            admitted.text_digest(),
            crate::digest::sha256(admitted.text().as_bytes())
        );
    }
    let text = AdmittedSource::from_text("x");
    assert_eq!(text.source_digest(), None);
    assert_eq!(text.text_digest(), crate::digest::sha256(b"x"));
}
