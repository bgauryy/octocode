use super::*;

#[test]
fn totals_lead_the_state_and_digests_follow() {
    let carried = format!("{:016x}{:016x}{:016x}", 7, 9, 0xabcd);
    let listed = Listed::of(Some(&carried), 2);
    assert_eq!(listed.totals(), Some(&[7, 9][..]));
    assert_eq!(listed.state().as_deref(), Some(carried.as_str()));
    assert!(Listed::of(Some(&carried), 1).totals().is_none());
    assert!(Listed::of(None, 2).totals().is_none());
    assert!(Listed::of(None, 2).state().is_none());
}

fn item(path: &str) -> octocode_github::CodeSearchItem {
    serde_json::from_value(serde_json::json!({
        "name": path, "path": path, "sha": "1", "html_url": "https://x",
        "repository": {"full_name": "A/b", "html_url": "https://x", "url": "https://x"}
    }))
    .expect("item")
}

/// A part's state keeps the earlier pages' files apart from the files
/// this page showed: reading the page again, those are `Shown` (not
/// repeats), and once the page is whole its state lists every file.
#[test]
fn a_part_state_separates_this_page_from_earlier_pages() {
    let mut first = Listed::of(None, 1);
    assert_eq!(first.insert(&item("old.rs")), Seen::New);
    let earlier = first.state().expect("state");
    let mut page = Listed::of(Some(&earlier), 1);
    for path in ["a.rs", "λ/🦀.rs", "c.rs"] {
        assert_eq!(page.insert(&item(path)), Seen::New);
    }
    assert_eq!(page.insert(&item("old.rs")), Seen::Repeat);
    let within = page.state_within([("a", "b", "a.rs"), ("a", "b", "λ/🦀.rs")]);
    assert!(within.starts_with(&earlier), "{within}");

    let mut again = Listed::of(Some(&within), 1);
    assert!(again.was_shown("A/B", "λ/🦀.rs") && !again.was_shown("a/b", "c.rs"));
    assert_eq!(again.insert(&item("a.rs")), Seen::Shown);
    assert_eq!(again.insert(&item("λ/🦀.rs")), Seen::Shown);
    assert_eq!(again.insert(&item("c.rs")), Seen::New);
    assert_eq!(again.insert(&item("old.rs")), Seen::Repeat);
    let whole = Listed::of(again.state().as_deref(), 1);
    for path in ["old.rs", "a.rs", "λ/🦀.rs", "c.rs"] {
        assert!(!whole.was_shown("a/b", path), "a whole page has no part");
    }
    let mut next = whole;
    assert_eq!(next.insert(&item("c.rs")), Seen::Repeat);
}
