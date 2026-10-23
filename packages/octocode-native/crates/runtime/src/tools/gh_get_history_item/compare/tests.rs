//! Comparison identity and the batch-aligned commit pages.
use super::compare_identity;
use crate::tools::gh_get_history_item::window::page_starting_at;
use serde_json::json;

#[test]
fn compare_identity_expands_permalink_abbreviations() {
    let raw = json!({
        "permalink_url": "https://github.com/a/b/compare/abc1234...def5678",
        "base_commit": {"sha": "abc1234aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"},
        "commits": [{"sha": "def5678bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"}]
    });
    let (base, head) = compare_identity(&raw, "main", "feature");
    assert!(base.starts_with("abc1234"));
    assert!(head.starts_with("def5678"));
}

/// GitHub's permalink spells both sides `owner:abbrev`; the echo
/// must keep the caller's bare-SHA form on both sides, and a base that is
/// both base_commit and merge_base_commit still expands.
#[test]
fn compare_identity_echoes_the_requested_form_without_owner_prefix() {
    let base_sha = "af20f667fd2536c9502f69d99fe6bdedfcc839cb";
    let head_sha = "c265e3f9413161c900cbf4aa70d451b8e6b3920a";
    let raw = json!({
        "permalink_url": "https://github.com/o/r/compare/o:af20f66...o:c265e3f",
        "base_commit": {"sha": base_sha},
        "merge_base_commit": {"sha": base_sha},
        "commits": [{"sha": head_sha}]
    });
    assert_eq!(
        compare_identity(&raw, base_sha, head_sha),
        (base_sha.to_owned(), head_sha.to_owned())
    );
    assert_eq!(
        compare_identity(&raw, "af20f66", "c265e3f"),
        (base_sha.to_owned(), head_sha.to_owned())
    );
    // A caller-written fork prefix is kept.
    assert_eq!(
        compare_identity(&raw, base_sha, "fork:c265e3f").1,
        format!("fork:{head_sha}")
    );
}

/// A list lead starts right after the shown items and every later item
/// falls on exactly one of its pages: the comparison's unseen commits
/// (batch-sized pages) and an issue's unshown comments (one provider
/// batch at most) alike.
#[test]
fn a_page_starting_at_an_item_reaches_every_later_item_once() {
    for (seen, batch) in [(100, 5), (100, 3), (97, 5), (30, 4), (7, 1), (5, 5)] {
        let (page, size) = page_starting_at(seen, batch);
        assert!((1..=batch).contains(&size), "{seen}/{batch}: {size}");
        assert_eq!(size * (page - 1), seen, "{seen}/{batch}: {size} {page}");
    }
    assert_eq!(page_starting_at(100, 5), (21, 5));
    assert_eq!(page_starting_at(12, 100), (2, 12));
    assert_eq!(page_starting_at(97, 100), (2, 97));
    assert_eq!(page_starting_at(190, 100), (3, 95));
    assert_eq!(page_starting_at(200, 100), (3, 100));
}
