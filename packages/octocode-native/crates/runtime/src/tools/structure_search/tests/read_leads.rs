//! Successful listing identities are enough to form a read; no generic
//! file-ranking advice or size-dependent read suggestion is published.
use super::*;

#[test]
fn successful_listing_views_keep_all_selected_entries_without_read_advice() {
    let root = Fixture::new();
    let names = [
        "a_types.generated.rs",
        "main.rs",
        "package.json",
        "README.md",
    ];
    for name in names {
        std::fs::write(root.0.join(name), "source evidence\n").expect("source");
    }
    for operation in ["tree", "files"] {
        for view in ["paths", "details"] {
            let out = run(
                &root.0,
                json!({"operation": operation, "view": view, "path": root.0}),
            )
            .expect("listing");
            let rows = out["files"].to_string();
            for name in names {
                assert!(rows.contains(name), "{operation}/{view}: {out}");
            }
            assert!(
                related(&out, "localFetch").is_null(),
                "{operation}/{view}: {out}"
            );
            assert!(out.get("hints").is_none(), "{operation}/{view}: {out}");
        }
    }
}
