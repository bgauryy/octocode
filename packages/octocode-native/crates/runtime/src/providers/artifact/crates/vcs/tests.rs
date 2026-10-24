use super::{ArchiveEntry, MAX_ENTRY_BYTES, crate_vcs, test_archive};
use crate::providers::artifact::http::RegistryClient;

/// One file's bytes from a whole gzip-compressed tar archive, fed to
/// the streaming reader as one chunk.
fn archive_entry(archive: &[u8], wanted: &str) -> Option<Vec<u8>> {
    let mut reader = ArchiveEntry::new(wanted);
    let _ = reader.feed(archive);
    reader.finish()
}

#[test]
fn reads_the_vcs_info_entry_wherever_it_sits() {
    let info =
        br#"{"git":{"sha1":"EA6D652A102DEE3F22B490DB70545B7F66A23FB7"},"path_in_vcs":"tokio"}"#;
    let first = test_archive(&[
        ("tokio-1.40.0/.cargo_vcs_info.json", info, b'0'),
        ("tokio-1.40.0/Cargo.toml", b"[package]", b'0'),
    ]);
    let last = test_archive(&[
        ("tokio-1.40.0/Cargo.toml", &[b'x'; 700], b'0'),
        ("tokio-1.40.0/.cargo_vcs_info.json", info, b'0'),
    ]);
    for bytes in [first, last] {
        assert_eq!(
            archive_entry(&bytes, "tokio-1.40.0/.cargo_vcs_info.json").as_deref(),
            Some(&info[..])
        );
    }
    // A GNU long name names the entry that follows it.
    let long = "x".repeat(120);
    let wanted = format!("{long}/.cargo_vcs_info.json");
    let bytes = test_archive(&[
        ("././@LongLink", format!("{wanted}\0").as_bytes(), b'L'),
        ("truncated", info, b'0'),
    ]);
    assert_eq!(archive_entry(&bytes, &wanted).as_deref(), Some(&info[..]));
    assert_eq!(archive_entry(&bytes, "absent"), None);
    assert_eq!(archive_entry(b"not gzip", "absent"), None);
}

#[test]
fn an_oversized_wanted_or_long_name_entry_is_never_buffered() {
    let big = vec![b'x'; MAX_ENTRY_BYTES as usize + 1];
    let bytes = test_archive(&[("c-1/.cargo_vcs_info.json", &big, b'0')]);
    assert_eq!(archive_entry(&bytes, "c-1/.cargo_vcs_info.json"), None);
    let bytes = test_archive(&[("././@LongLink", &big, b'L'), ("x", b"{}", b'0')]);
    assert_eq!(archive_entry(&bytes, "x"), None);
}

/// A published crate's VCS info is read from its archive once.
#[tokio::test]
async fn crate_vcs_info_is_downloaded_once() {
    let name = "vcs-once";
    let info = br#"{"git":{"sha1":"b6a77c4413f902523646be0d7f5520631df53ff6"},"path_in_vcs":"x"}"#;
    let entry = format!("{name}-1.0.0/.cargo_vcs_info.json");
    let http = crate::providers::artifact::test_http::SequenceHttp::new(vec![test_archive(&[(
        entry.as_str(),
        &info[..],
        b'0',
    )])]);
    let budget = crate::providers::artifact::test_http::test_budget();
    let cache = crate::providers::artifact::ArtifactCache::new(None);
    let client = RegistryClient {
        http: &http,
        budget: &budget,
        cache: Some(&cache),
        tags: None,
    };
    let expected = Some((
        "b6a77c4413f902523646be0d7f5520631df53ff6".to_owned(),
        Some("x".to_owned()),
    ));
    assert_eq!(crate_vcs(name, "1.0.0", &client).await, expected);
    assert_eq!(crate_vcs(name, "1.0.0", &client).await, expected);
    assert_eq!(http.calls(), 1);
}

/// The archive streams in and the download stops at the VCS entry:
/// cargo packs it first, so a large crate is read for its first pieces
/// only; an entry packed last is still found.
#[tokio::test]
async fn crate_vcs_info_stops_the_download_at_its_entry() {
    let info = br#"{"git":{"sha1":"b6a77c4413f902523646be0d7f5520631df53ff6"},"path_in_vcs":""}"#;
    // Incompressible filler: the compressed archive stays megabytes.
    let mut state = 0x9e37_79b9_7f4a_7c15_u64;
    let filler = (0..2 * 1024 * 1024)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state as u8
        })
        .collect::<Vec<_>>();
    let budget = crate::providers::artifact::test_http::test_budget();
    for (name, first) in [("streamed-first", true), ("streamed-last", false)] {
        let entry = format!("{name}-1.0.0/.cargo_vcs_info.json");
        let source = format!("{name}-1.0.0/src/lib.rs");
        let entries: Vec<(&str, &[u8], u8)> = if first {
            vec![
                (entry.as_str(), &info[..], b'0'),
                (source.as_str(), &filler, b'0'),
            ]
        } else {
            vec![
                (source.as_str(), &filler, b'0'),
                (entry.as_str(), &info[..], b'0'),
            ]
        };
        let http =
            crate::providers::artifact::test_http::ChunkedHttp::new(test_archive(&entries), 4096);
        let client = RegistryClient::uncached(&http, &budget);
        assert_eq!(
            crate_vcs(name, "1.0.0", &client).await,
            Some(("b6a77c4413f902523646be0d7f5520631df53ff6".to_owned(), None))
        );
        if first {
            assert!(
                http.fed() <= 2,
                "read {} of {} pieces",
                http.fed(),
                http.pieces()
            );
        } else {
            assert!(
                http.fed() + 1 >= http.pieces(),
                "read {} of {}",
                http.fed(),
                http.pieces()
            );
        }
    }
}
