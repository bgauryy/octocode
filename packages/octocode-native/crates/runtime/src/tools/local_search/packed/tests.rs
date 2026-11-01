use super::*;

fn hit(line: u32, value: &str) -> TextSearchMatch {
    TextSearchMatch {
        deferred: None,
        line,
        last_line: line.is_multiple_of(5).then_some(line + 2),
        column: line * 3,
        byte_offset: line * 5,
        value: value.to_owned(),
        value_digest: TextSearchMatch::digest_of(value),
        count: line.is_multiple_of(2).then_some(line),
        rank: Some(line % 4),
    }
}

fn assert_same(left: &TextSearchResult, right: &TextSearchResult) {
    assert_eq!(left.files.len(), right.files.len());
    for (a, b) in left.files.iter().zip(&right.files) {
        assert_eq!(
            (&a.path, a.match_count, &a.term_counts, a.source),
            (&b.path, b.match_count, &b.term_counts, b.source)
        );
        assert_eq!(a.matches.len(), b.matches.len());
        for (x, y) in a.matches.iter().zip(&b.matches) {
            assert_eq!(
                (
                    x.line,
                    x.last_line,
                    x.column,
                    x.byte_offset,
                    &x.value,
                    x.value_digest,
                    x.count,
                    x.rank,
                ),
                (
                    y.line,
                    y.last_line,
                    y.column,
                    y.byte_offset,
                    &y.value,
                    y.value_digest,
                    y.count,
                    y.rank,
                )
            );
        }
    }
    assert_eq!(left.stats.match_count, right.stats.match_count);
    assert_eq!(left.stats.cap_reasons, right.stats.cap_reasons);
}

/// Every field of every file comes back exactly, multibyte text,
/// empty values, times either side of the epoch and absent ones
/// included, and the packed bytes are smaller than the values.
#[test]
fn a_packed_scan_unpacks_to_the_same_scan() {
    let line = "fn needle(x: u32) -> u32 { x + 1 } // 中文 ✓".repeat(3);
    let files = (0..200u32)
        .map(|n| TextSearchFile {
            term_counts: (0..n % 4).map(|term| n + term).collect(),
            path: format!("/repo/src/module_{n}/file.rs"),
            match_count: n,
            matches: (0..n % 7)
                .map(|l| hit(l, if l == 1 { "" } else { &line }))
                .collect(),
            source: match n % 3 {
                0 => None,
                1 => Some(SearchedSource {
                    size: u64::from(n) << 40,
                    modified: Some(SystemTime::UNIX_EPOCH + Duration::new(1_700_000_000, n)),
                    digest: [n as u8; 32],
                }),
                _ => Some(SearchedSource {
                    size: 0,
                    modified: SystemTime::UNIX_EPOCH.checked_sub(Duration::new(5, 7)),
                    digest: [7; 32],
                }),
            },
        })
        .collect::<Vec<_>>();
    let scan = TextSearchResult {
        files,
        stats: TextSearchStats {
            match_count: Some(600),
            cap_reasons: vec![octocode_engine::types::CapReason::MaxFileSize],
            ..Default::default()
        },
    };
    let packed = Packed::pack(&scan, usize::MAX, usize::MAX).expect("packed");
    assert_same(&packed.unpack().expect("unpacked"), &scan);
    let values = scan
        .files
        .iter()
        .flat_map(|file| &file.matches)
        .map(|hit| hit.value.len())
        .sum::<usize>();
    assert!(
        packed.weight() * 5 < values,
        "{} vs {values}",
        packed.weight()
    );
    // Past the unpacked bound nothing is packed.
    assert!(Packed::pack(&scan, packed.raw_len - 1, usize::MAX).is_none());
    assert!(Packed::pack(&scan, packed.raw_len, usize::MAX).is_some());
    // Past the packed bound nothing is packed either.
    assert!(Packed::pack(&scan, usize::MAX, packed.weight() - 1).is_none());
    assert!(Packed::pack(&scan, usize::MAX, packed.weight()).is_some());
}

#[test]
fn compact_classes_store_an_oversized_scan_and_preserve_all_duplicate_relations() {
    let mut scan = TextSearchResult {
        files: vec![TextSearchFile {
            term_counts: Vec::new(),
            path: "/dense.txt".into(),
            match_count: 150_000,
            matches: (0..150_000)
                .map(|n| {
                    let mut hit = hit(n, &format!("needle {n} {}", "x".repeat(200)));
                    hit.value_digest = <sha2::Sha256 as sha2::Digest>::digest(
                        (n - u32::from(n % 10 == 9)).to_le_bytes(),
                    )
                    .into();
                    hit
                })
                .collect(),
            source: None,
        }],
        stats: Default::default(),
    };
    let cap = super::super::manifest::MAX_BYTES;
    assert!(
        Packed::pack(&scan, 64 * 1024 * 1024, cap).is_none(),
        "old payload exceeds cap"
    );
    for hit in &mut scan.files[0].matches {
        hit.deferred = Some(300);
        hit.value.clear();
    }
    let packed =
        Packed::pack(&scan, 64 * 1024 * 1024, cap).expect("compact payload within unchanged cap");
    let unpacked = packed.unpack().expect("unpack");
    assert_eq!(unpacked.files[0].matches.len(), 150_000);
    for (index, hit) in unpacked.files[0].matches.iter().enumerate() {
        assert_eq!(hit.line as usize, index);
        assert_eq!(hit.deferred, Some(300));
        assert!(hit.value.is_empty());
        assert_eq!(
            hit.value_digest[..8],
            ((index / 10 * 9 + (index % 10).min(8)) as u64).to_le_bytes()
        );
    }
    assert!(Packed::pack_checked(&scan, usize::MAX, usize::MAX, &|| true).is_none());
    assert!(packed.unpack_checked(&|| true).is_none());
}

#[test]
fn an_empty_scan_packs() {
    let scan = TextSearchResult {
        files: Vec::new(),
        stats: TextSearchStats::default(),
    };
    let packed = Packed::pack(&scan, 0, usize::MAX).expect("packed");
    assert!(packed.unpack().expect("unpacked").files.is_empty());
}
