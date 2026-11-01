//! Complete selected match values and intentional structural expansions.

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
        json!([{"startLine":1,"endLine":5},{"startLine":9,"endLine":12}]),
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
        json!([{"startLine":1,"endLine":11}]),
        "{out}"
    );
}

#[test]
fn captured_values_preserve_long_source_without_a_display_cap() {
    for size in [40, 600, 50_000] {
        let root = Fixture::new();
        let selected = format!("call(\"{}END_OF_VALUE\")", "é😀界".repeat(size));
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
        assert!(
            out["next"]
                .as_object()
                .is_none_or(|next| next.keys().all(|name| !name.starts_with("expandValues"))),
            "complete values need no size-driven expansion"
        );
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
    // Optional work on a complete result: a related lead, never a required
    // continuation, so the row carries no `next` (and needs no coverage
    // note).
    assert!(out.get("next").is_none(), "{out}");
    let call = out["related"]
        .as_array()
        .and_then(|calls| {
            calls
                .iter()
                .find(|call| call["input"]["queries"][0]["includeCaptures"] == true)
        })
        .unwrap_or_else(|| panic!("related expansion: {out}"));
    assert_eq!(call["kind"], "related", "{out}");
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

#[test]
fn complete_match_values_preserve_comments_newlines_and_indentation() {
    let root = Fixture::new();
    let selected = "call(\n    // retain this comment\n    \"value\"\n)";
    root.write("comments.rs", format!("fn main() {{ {selected}; }}\n"));
    for captures in [false, true] {
        let out = run(
            &root.0,
            json!({"operation":"match","path":root.0.join("comments.rs"),
            "rule":{"pattern":"call($$$A)"},"includeCaptures":captures}),
        )
        .expect("match");
        assert_eq!(out["files"][0]["matches"][0]["value"], selected, "{out}");
    }
}
