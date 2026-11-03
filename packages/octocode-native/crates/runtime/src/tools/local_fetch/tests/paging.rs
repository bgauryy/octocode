use super::*;

#[test]
fn default_read_budget_walk_reconstructs_every_source_byte() {
    let fixture = Fixture::new();
    let source = "let x = 1;\n".repeat(6_000);
    let file = fixture.write("short-lines.rs", &source);
    let paths = Paths(fixture.0.clone());
    for budget in [1500, 10000, 50000] {
        let mut query = q(&file);
        let mut joined = String::new();
        let mut pages = 0;
        loop {
            let page = execute_local_fetch(&query, &paths, &Safe, &NeverCancel, Some(budget), None);
            assert_eq!(page.error_code, None, "{page:?}");
            joined.push_str(page.content.as_deref().expect("content"));
            pages += 1;
            assert!(pages < 100);
            let Some(next) = page.next.and_then(|next| next.continue_window) else {
                break;
            };
            assert!(local_fetch_query::snapshot(&next.query).is_some());
            query = next.query;
        }
        assert_eq!(joined, source);
        assert!(pages > 1);
    }
}

#[test]
fn utf8_byte_progress_reconstructs_the_complete_line() {
    let fixture = Fixture::new();
    let source = "a😀b語".repeat(2000);
    let path = fixture.write("unicode.txt", &source);
    let paths = Paths(fixture.0.clone());
    let mut query = q(&path);
    let mut joined = String::new();
    loop {
        let page = execute_local_fetch(&query, &paths, &Safe, &NeverCancel, Some(100), None);
        assert_eq!(page.pagination.as_ref().expect("page").offset, joined.len());
        joined.push_str(page.content.as_deref().expect("content"));
        let Some(next) = page.next.and_then(|next| next.continue_window) else {
            break;
        };
        query = next.query;
    }
    assert_eq!(joined, source);
}

#[test]
fn a_mid_character_byte_offset_snaps_to_the_character_start() {
    let t = Fixture::new();
    let p = t.0.join("mb.txt");
    fs::write(&p, "abc語def\n").expect("fixture");
    let paths = Paths(t.0.clone());
    let r = fetch(
        &qj(
            &p,
            serde_json::json!({"readProgress":{"unit":"bytes","offset":4}}),
        ),
        &paths,
    );
    assert_eq!(r.error_code, None, "{r:?}");
    assert_eq!(r.content.as_deref(), Some("語def\n"), "{r:?}");
    assert!(
        r.warnings
            .iter()
            .any(|warning| warning.contains("starts at its first byte, 3")),
        "{:?}",
        r.warnings
    );
}
