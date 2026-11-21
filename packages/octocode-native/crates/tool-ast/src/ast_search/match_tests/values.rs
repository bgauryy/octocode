//! Complete selected match values and intentional structural expansions.

use super::*;

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
    let query = json!({"operation":"match","path":root.0.join("block.rs"),
        "rule":{"pattern":"fn $N() { $$$B }"}});
    let out = run(&root.0, query.clone()).expect("lean block");
    // A complete result with a clipped row carries no call: the row says
    // it is clipped, and `includeCaptures` returns it whole.
    assert!(out.get("next").is_none(), "{out}");
    assert!(out.get("related").is_none(), "{out}");
    let lean = out["files"][0]["matches"][0]["value"]
        .as_str()
        .expect("value");
    assert!(lean.ends_with('…'), "{out}");
    let mut next = query;
    next["includeCaptures"] = json!(true);
    let next = &next;
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
