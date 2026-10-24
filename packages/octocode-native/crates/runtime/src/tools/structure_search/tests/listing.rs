use super::*;

#[test]
fn typed_size_and_date_bounds_are_inclusive_and_do_not_match_directories() {
    let root = Fixture::new();
    root.write("small.rs", "a");
    root.write("exact.rs", "ab");
    root.write("large.rs", "abc");
    std::fs::create_dir(root.0.join("dir")).expect("dir");
    let instant = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_792_843_200);
    for name in ["small.rs", "exact.rs", "large.rs"] {
        std::fs::File::options()
            .write(true)
            .open(root.0.join(name))
            .expect("file")
            .set_times(
                std::fs::FileTimes::new()
                    .set_modified(instant)
                    .set_accessed(instant),
            )
            .expect("times");
    }
    let iso = crate::civil_date::iso8601_millis(1_792_843_200_000);
    for extra in [
        json!({"sizeBytes":{"min":2,"max":2}}),
        json!({"modifiedAt":{"from":iso,"to":iso},"sizeBytes":{"min":2,"max":2}}),
        json!({"accessedAt":{"from":iso,"to":iso},"sizeBytes":{"min":2,"max":2}}),
    ] {
        let mut query = json!({"operation":"files","path":root.0});
        query
            .as_object_mut()
            .expect("row")
            .extend(extra.as_object().expect("extra").clone());
        let out = run(&root.0, query).expect("bounded listing");
        let entries = listed(&out);
        assert_eq!(entries.len(), 1, "{out}");
        assert!(
            entries[0]["path"]
                .as_str()
                .expect("path")
                .ends_with("/exact.rs"),
            "{out}"
        );
        assert_eq!(entries[0]["size"], 2);
    }
}

#[test]
fn directory_entries_never_claim_content_size() {
    let root = Fixture::new();
    root.write("a.rs", "12345");
    std::fs::create_dir(root.0.join("empty")).expect("dir");
    let out = run(
        &root.0,
        json!({"operation":"files","path":root.0,"sort":{"by":"sizeBytes","order":"desc"}}),
    )
    .expect("listing");
    let entries = listed(&out);
    assert_eq!(entries.len(), 2, "{out}");
    assert_eq!(entries[0]["size"], 5, "{out}");
    assert_eq!(entries[1]["type"], "directory", "{out}");
    assert!(entries[1].get("size").is_none(), "{out}");
}
