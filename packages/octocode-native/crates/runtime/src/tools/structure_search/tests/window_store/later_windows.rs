use super::*;

/// The pages of a window served from the kept listing are proved like any
/// stored walk's: an entry created in a walked directory, or a shown row edited,
/// restarts the page after it.
#[test]
fn a_later_windows_pages_restart_on_an_edit() {
    for (operation, extra) in ranked() {
        for (view, edit) in [("paths", "new.rs"), ("details", "")] {
            let root = fixture();
            let mut row = listing(&root, operation, &extra);
            row["view"] = json!(view);
            with_window(WINDOW_ROWS, || {
                let first = run_budget(&root.0, row.clone(), 40).expect("first page");
                let mut next = next_query(&first).expect("a next page");
                while next.get("page").is_some() {
                    let page = run_budget(&root.0, next, 40).expect("page");
                    next = next_query(&page).expect("a next window");
                }
                let window = run_budget(&root.0, next, 40).expect("second window");
                let page = next_query(&window).expect("its next page");
                assert!(page.get("page").is_some(), "{window}");
                std::thread::sleep(std::time::Duration::from_millis(20));
                if edit.is_empty() {
                    for dir in 0..6 {
                        for file in 0..8 {
                            root.write(&format!("d{dir}/f{file}.rs"), "edited, and longer\n");
                        }
                    }
                } else {
                    root.write(edit, "x\n");
                }
                let stale = run_budget(&root.0, page, 40).expect("page after the edit");
                assert_eq!(
                    stale["errorCode"], "staleSnapshot",
                    "{operation} {view}: {stale}"
                );
            });
        }
    }
}

/// A `testCode` scope's windows read the same whichever way they were served,
/// the entries it left out so far included.
#[test]
fn a_test_code_listings_kept_windows_read_as_walked_ones() {
    let mut all = ranked();
    all.push(traversal());
    for (operation, extra) in all {
        if operation == "tree" {
            continue;
        }
        let root = fixture();
        for dir in 0..6 {
            root.write(
                &format!("d{dir}/tests/case_test.rs"),
                "#[test]\nfn t() {}\n",
            );
        }
        for scope in ["exclude", "only"] {
            let mut row = listing(&root, operation, &extra);
            row["testCode"] = json!(scope);
            with_window(WINDOW_ROWS, || {
                let kept = follow(&root, row.clone(), &NeverCancel, &|_| {});
                let walked = follow(&root, row.clone(), &NeverCancel, &|next| {
                    if let Some(snapshot) = next["snapshot"].as_str() {
                        crate::tools::structure_search::windows::forget(snapshot);
                    }
                });
                assert_eq!(kept, walked, "{operation} {extra} {scope}");
            });
        }
    }
}
