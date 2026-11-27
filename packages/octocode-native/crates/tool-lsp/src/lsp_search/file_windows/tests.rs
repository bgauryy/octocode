use super::*;

/// Windows cover the list once, in order; a later window is admitted
/// only under its own digest, and a page past the last never is.
#[test]
fn windows_cover_the_list_once_and_pin_its_digest() {
    let list = FileWindows::new((0..25).collect::<Vec<_>>(), 10, "d".into());
    assert_eq!(list.pages(), 3);
    let covered = (1..=list.pages())
        .flat_map(|page| list.items()[list.range(page)].to_vec())
        .collect::<Vec<_>>();
    assert_eq!(covered, (0..25).collect::<Vec<_>>());
    assert_eq!(list.page_of(19), 2);
    assert!(list.admits(2, Some("d")) && !list.admits(2, Some("other")));
    assert!(!list.admits(4, None));
    assert!(list.more_after(2) && !list.more_after(3));
    assert_eq!(list.into_window(3), (20..25).collect::<Vec<_>>());
    let empty = FileWindows::<u8>::new(Vec::new(), 10, "e".into());
    assert!(empty.admits(1, None) && !empty.admits(2, None));
}
