//! Clipped match values and the reads a match offers: every cut stays
//! reachable by a read or an expansion.

use super::*;

#[test]
fn a_match_offers_a_read_of_the_top_files_hits() {
    let root = Fixture::new();
    root.write(
        "a.rs",
        "fn a() {\n    hit(1);\n}\n\n\n\n\n\n\n\n\nfn b() { hit(2); }\n",
    );
    let out = run(
        &root.0,
        json!({"operation":"match","path":root.0,"parser":"rust","rule":{"pattern":"hit($A)"}}),
    )
    .expect("match");
    let read = crate::tools::test_support::checks::related_call(&out, "localFetch", |_| true);
    assert_eq!(read["tool"], "localFetch", "{out}");
    let row = &read["input"]["queries"][0];
    assert_eq!(
        row["ranges"],
        json!([{"startLine":1,"endLine":5},{"startLine":9,"endLine":15}]),
        "{out}"
    );
    assert!(
        row["path"]
            .as_str()
            .is_some_and(|path| path.ends_with("a.rs") && std::path::Path::new(path).is_absolute()),
        "{out}"
    );
    crate::contracts::validate_query("localFetch", row.clone()).expect("a valid localFetch row");
}

/// A multi-line match shows only its header in the row, so the top read
/// covers the whole match (with context), not a window around its first line.
#[test]
fn a_multi_line_match_read_covers_the_whole_match() {
    let root = Fixture::new();
    let body = (1..=8)
        .map(|n| format!("    step({n});\n"))
        .collect::<String>();
    root.write("a.rs", format!("// head\nfn run() {{\n{body}}}\n"));
    let out = run(
        &root.0,
        json!({"operation":"match","path":root.0,"parser":"rust","rule":{"pattern":"fn run() { $$$BODY }"}}),
    )
    .expect("match");
    let row = &crate::tools::test_support::checks::related_call(&out, "localFetch", |_| true)["input"]
        ["queries"][0];
    assert_eq!(
        row["ranges"],
        json!([{"startLine":1,"endLine":14}]),
        "{out}"
    );
}

#[test]
fn captured_values_preserve_long_source_without_a_display_cap() {
    for size in [40, 600, 20_000] {
        let root = Fixture::new();
        let selected = format!("call({})", "y".repeat(size));
        root.write("long.rs", format!("fn main() {{ {selected}; }}\n"));
        let out = run(
            &root.0,
            json!({"operation":"match","path":root.0.join("long.rs"),
            "rule":{"pattern":"call($A)"},"includeCaptures":true}),
        )
        .expect("complete value");
        let row = &out["files"][0]["matches"][0];
        assert_eq!(row["value"], selected, "size {size}: {out}");
        assert!(row.get("clipped").is_none());
        assert!(value_reads(&out).is_empty());
    }
}

#[test]
fn block_expansion_reaches_the_complete_body_and_captures() {
    let root = Fixture::new();
    let body = (0..600)
        .map(|i| format!("    let v{i} = {i};\n"))
        .collect::<String>();
    let selected = format!("fn big() {{\n{body}}}");
    root.write("block.rs", format!("{selected}\n"));
    let out = run(
        &root.0,
        json!({"operation":"match","path":root.0.join("block.rs"),
        "rule":{"pattern":"fn $N() { $$$B }"}}),
    )
    .expect("lean block");
    let call = &out["next"]["expandCaptures"];
    assert_eq!(call["kind"], "continue", "{out}");
    let next = &call["input"]["queries"][0];
    assert_eq!(next["includeCaptures"], true);
    let full = run(&root.0, next.clone()).expect("complete body");
    assert_eq!(full["files"][0]["matches"][0]["value"], selected);
    assert_eq!(
        full["files"][0]["matches"][0]["metavarRanges"]["B"]
            .as_array()
            .expect("captures")
            .len(),
        600
    );
    assert!(only_read_left(&full), "{full}");
}
