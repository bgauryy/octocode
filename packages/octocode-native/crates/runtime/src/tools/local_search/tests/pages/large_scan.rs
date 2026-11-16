//! Complete file pagination across a large matched scan.
use super::*;

/// A search matching more files than any one collection can hold in a
/// page chain's scan still pages every matched file exactly once, and the
/// first page counts them all (`pagination.total` is files matched, not a cap).
#[test]
fn every_matched_file_of_a_large_scan_is_paged_exactly_once() {
    let root = tempfile::tempdir().expect("fixture");
    let (dirs, per_dir) = (120, 100);
    for dir in 0..dirs {
        let parent = root.path().join(format!("d{dir:03}"));
        fs::create_dir(&parent).expect("dir");
        for file in 0..per_dir {
            fs::write(parent.join(format!("{file:03}.txt")), "needle\n").expect("file");
        }
    }
    let (walk, pages) = walk_search(
        root.path(),
        serde_json::json!({"path":root.path(),"matchString":["needle"],"view":"files","sort":{"by":"path","order":"asc"}}),
        None,
    );
    let expected = (0..dirs)
        .flat_map(|dir| (0..per_dir).map(move |file| format!("d{dir:03}/{file:03}.txt")))
        .collect::<std::collections::BTreeSet<_>>();
    let delivered = walk
        .rows
        .iter()
        .map(|row| {
            std::path::Path::new(row)
                .strip_prefix(root.path())
                .map_or_else(|_| row.clone(), |rel| rel.to_string_lossy().into_owned())
        })
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(walk.rows.len(), expected.len(), "each file once");
    assert_eq!(delivered, expected);
    assert_eq!(
        pages[0]["pagination"]["total"],
        expected.len(),
        "{}",
        pages[0]["pagination"]
    );
    assert!(
        pages.iter().all(
            |page| page["warnings"].as_array().is_none_or(|warnings| warnings
                .iter()
                .all(|warning| !warning.as_str().unwrap_or("").contains("no page reaches")))
        ),
        "no page discloses unreachable files"
    );
}
