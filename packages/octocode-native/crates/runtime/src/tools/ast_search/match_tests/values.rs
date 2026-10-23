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
        json!({"operation":"match","mainGoal":"test","reasoning":"test","path":root.0,"language":"rust","pattern":"hit($A)"}),
    )
    .expect("match");
    let read = &out["next"]["read"];
    assert_eq!(read["tool"], "localFetch", "{out}");
    let row = &read["query"]["queries"][0];
    assert_eq!(row["ranges"], json!(["1-5", "9-15"]), "{out}");
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
        json!({"operation":"match","mainGoal":"test","reasoning":"test","path":root.0,"language":"rust","pattern":"fn run() { $$$BODY }"}),
    )
    .expect("match");
    let row = &out["next"]["read"]["query"]["queries"][0];
    assert_eq!(row["ranges"], json!(["1-14"]), "{out}");
}

#[test]
fn match_content_length_bounds_each_match_value() {
    let root = Fixture::new();
    let source = root.0.join("long.rs");
    let args = (0..200)
        .map(|i| format!("arg{i}"))
        .collect::<Vec<_>>()
        .join(", ");
    std::fs::write(&source, format!("fn m() {{ call({args}); }}\n")).expect("source");
    let value_len = |length: Option<u32>| {
        let mut query = json!({"operation":"match","mainGoal":"test","reasoning":"test","path":source,"pattern":"call($$$A)"});
        if let Some(length) = length {
            query["matchContentLength"] = json!(length);
        }
        let out = run(&root.0, query).expect("match");
        lean_row(&out["files"][0]["matches"][0]).2.chars().count()
    };
    assert_eq!(value_len(Some(40)), 40);
    assert!(value_len(Some(5_000)) > 300);
    assert_eq!(value_len(None), 500);
}

/// A value cut at `matchContentLength` is display clipping, not a coverage
/// gap: every match is listed, so the row is not partial. The cut is never
/// silent: the shown text ends in `…`, an object row says `truncated`, and
/// `next.expandValues` reads exactly the clipped rows' lines whole.
#[test]
fn clipped_values_do_not_mark_the_row_partial() {
    let root = Fixture::new();
    let source = root.0.join("long.rs");
    let args = (0..200)
        .map(|i| format!("arg{i}"))
        .collect::<Vec<_>>()
        .join(", ");
    std::fs::write(
        &source,
        format!("fn n() {{ call(x); }}\nfn m() {{\n    call({args});\n}}\n"),
    )
    .expect("source");
    let out = run(
        &root.0,
        json!({"operation":"match","path":source,"pattern":"call($$$A)","captureText":true}),
    )
    .expect("match");
    assert!(out.get("isPartial").is_none(), "{out}");
    assert!(out.get("partialReasons").is_none(), "{out}");
    let rows = out["files"][0]["matches"].as_array().expect("rows");
    assert_eq!(rows.len(), 2, "{out}");
    assert!(rows[0].get("clipped").is_none(), "{out}");
    assert_eq!(rows[1]["clipped"], true, "{out}");
    assert!(
        rows[1]["value"].as_str().is_some_and(|v| v.ends_with('…')),
        "{out}"
    );
    let reads = value_reads(&out);
    assert_eq!(reads.len(), 1, "{out}");
    assert_eq!(reads[0]["ranges"], json!(["3-3"]), "{out}");
    assert!(
        reads[0]["path"]
            .as_str()
            .is_some_and(|path| path.ends_with("long.rs")),
        "{out}"
    );
    // The lean view hides the `$$$A` capture as well: one expandCaptures
    // re-run returns captures and whole values; it is still not partial.
    let lean = run(
        &root.0,
        json!({"operation":"match","path":source,"pattern":"call($$$A)"}),
    )
    .expect("lean match");
    assert!(lean.get("isPartial").is_none(), "{lean}");
    let expand = &lean["next"]["expandCaptures"]["query"]["queries"][0];
    assert_eq!(expand["captureText"], true, "{lean}");
    let full = format!("call({args})");
    assert_eq!(expand["matchContentLength"], full.chars().count(), "{lean}");
    let expanded = run(&root.0, expand.clone()).expect("expanded");
    assert_eq!(expanded["files"][0]["matches"][1]["value"], full.as_str());
    assert!(only_read_left(&expanded), "{expanded}");
}

/// Clipped rows across files and past the read-range limit each stay
/// reachable: one `expandValues` row per file and per range batch.
#[test]
fn clipped_values_read_per_file_in_range_batches() {
    let root = Fixture::new();
    let long = format!("call({});", "y".repeat(80));
    let many = (0..30)
        .map(|i| {
            if i % 2 == 0 {
                long.clone()
            } else {
                format!("// {i}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    root.write("a.rs", format!("fn a() {{\n{many}\n}}\n"));
    root.write("b.rs", format!("fn b() {{\n{long}\n}}\n"));
    let out = run(
        &root.0,
        json!({"operation":"match","path":root.0,"language":"rust","pattern":"call($A)","captureText":true,"matchContentLength":20}),
    )
    .expect("match");
    assert!(out.get("isPartial").is_none(), "{out}");
    let reads = value_reads(&out);
    let ranges = reads
        .iter()
        .flat_map(|row| row["ranges"].as_array().cloned().unwrap_or_default())
        .collect::<Vec<_>>();
    // 15 clipped calls in a.rs (lines 2, 4, … 30) and one in b.rs (line 2).
    assert_eq!(ranges.len(), 16, "{out}");
    let max = crate::contracts::query_limits::local_fetch::RANGES_MAX_ITEMS;
    assert!(
        reads
            .iter()
            .all(|row| row["ranges"].as_array().map_or(0, Vec::len) <= max),
        "{out}"
    );
    assert!(reads.len() >= 3, "{out}");
}

/// A value longer than the `matchContentLength` maximum still reads whole:
/// the lead is a line read, not a longer re-run that would cut again.
#[test]
fn values_past_the_content_maximum_read_their_lines() {
    let root = Fixture::new();
    let source = root.0.join("huge.rs");
    // Few long arguments keep the value past the content maximum while the
    // matcher's work (one node per argument) stays far inside its deadline
    // on a loaded machine.
    let args = (0..400)
        .map(|i| format!("argument_with_a_long_descriptive_name_{i:04}"))
        .collect::<Vec<_>>()
        .join(", ");
    std::fs::write(&source, format!("fn m() {{\n    call({args});\n}}\n")).expect("source");
    let out = run(
        &root.0,
        json!({"operation":"match","path":source,"pattern":"call($$$A)","captureText":true}),
    )
    .expect("match");
    assert!(out.get("isPartial").is_none(), "{out}");
    assert_eq!(value_reads(&out)[0]["ranges"], json!(["2-2"]), "{out}");
    let codes = out["diagnostics"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|d| d["code"].as_str())
        .collect::<Vec<_>>();
    assert!(!codes.contains(&"structural.match.valueClipped"), "{out}");
}

/// A cut header row (a whole block) expands its captures and whole text in
/// one re-run; a lean cut without hidden captures reads its lines.
#[test]
fn block_and_lean_cuts_expand_to_the_whole_text() {
    let root = Fixture::new();
    let source = root.0.join("long.rs");
    std::fs::write(&source, "fn m() { call(1); }\nfn n() { call(x); }\n").expect("source");
    let lean = run(
        &root.0,
        json!({"operation":"match","path":source,"pattern":"fn n() { call(x); }","matchContentLength":8}),
    )
    .expect("lean cut");
    assert!(lean.get("isPartial").is_none(), "{lean}");
    assert!(lean["next"].get("expandCaptures").is_none(), "{lean}");
    assert_eq!(value_reads(&lean)[0]["ranges"], json!(["2-2"]), "{lean}");
    let block = root.0.join("block.rs");
    let body = (0..120)
        .map(|i| format!("    let v{i} = {i};"))
        .collect::<Vec<_>>()
        .join("\n");
    std::fs::write(&block, format!("fn big() {{\n{body}\n}}\n")).expect("block");
    let out = run(
        &root.0,
        json!({"operation":"match","mainGoal":"test","reasoning":"test","path":block,"pattern":"fn $N() { $$$B }"}),
    )
    .expect("block match");
    assert!(value_reads(&out).is_empty(), "{out}");
    let expand = out["next"]["expandCaptures"]["query"]["queries"][0].clone();
    assert_eq!(expand["captureText"], true, "{out}");
    let expanded = run(&root.0, expand).expect("expanded block");
    let value = expanded["files"][0]["matches"][0]["value"]
        .as_str()
        .expect("value");
    assert_eq!(value, format!("fn big() {{\n{body}\n}}"));
    assert!(only_read_left(&expanded), "{expanded}");
}

/// A clipped value that spans more lines than a hit read shows (a whole
/// file's match) is not offered for a whole read: `expandValues` would put
/// the file on the page. Shorter clipped values in the same file still are.
#[test]
fn a_clipped_whole_file_value_is_not_offered_for_reading() {
    let root = Fixture::new();
    let source = root.0.join("long.rs");
    let body = (0..500)
        .map(|i| format!("    call_{i}(argument_{i});"))
        .collect::<Vec<_>>()
        .join("\n");
    std::fs::write(
        &source,
        format!(
            "fn m() {{\n{body}\n}}\nfn n() {{ call({}); }}\n",
            "y".repeat(600)
        ),
    )
    .expect("source");
    let out = run(
        &root.0,
        json!({"operation":"match","path":source,"rule":{"kind":"function_item"},"captureText":true}),
    )
    .expect("match");
    assert_eq!(out["matchCount"], 2, "{out}");
    let reads = value_reads(&out);
    assert_eq!(reads.len(), 1, "{out}");
    assert_eq!(reads[0]["ranges"], json!(["503-503"]), "{out}");
}

/// Clipped values in more files than one localFetch call reads are never
/// dropped: `expandValues` batches the first files, and a per-call warning
/// names each file and line range left out of it, with the clipped value too
/// long for a hit read.
#[test]
fn clipped_values_past_one_read_batch_are_named() {
    let root = Fixture::new();
    let long = format!("call({});", "y".repeat(80));
    let files = crate::response::result::rows_per_call(crate::id::ToolId::LocalFetch) + 2;
    for index in 0..files {
        root.write(&format!("f{index}.rs"), format!("fn f() {{\n{long}\n}}\n"));
    }
    // One call spanning 500+ lines: too long for a hit read.
    let args = (0..500)
        .map(|i| format!("    argument_{i},"))
        .collect::<Vec<_>>()
        .join("\n");
    root.write("z.rs", format!("fn z() {{\ncall(\n{args}\n);\n}}\n"));
    let out = run(
        &root.0,
        json!({"operation":"match","path":root.0,"language":"rust","rule":{"kind":"call_expression"},
            "captureText":true,"matchContentLength":20}),
    )
    .expect("match");
    let reads = value_reads(&out);
    assert_eq!(
        reads.len(),
        crate::response::result::rows_per_call(crate::id::ToolId::LocalFetch),
        "{out}"
    );
    let read = reads
        .iter()
        .filter_map(|row| row["path"].as_str())
        .map(|path| path.rsplit('/').next().unwrap_or(path).to_owned())
        .collect::<Vec<_>>();
    let warnings = out["warnings"].as_array().expect("warnings");
    assert_eq!(warnings.len(), 1, "{out}");
    let warning = warnings[0].as_str().expect("text");
    // Every file with a clipped value is read or named, never both.
    for index in 0..files {
        let name = format!("f{index}.rs");
        let named = warning.contains(&format!("{name} 2-2"));
        assert!(read.contains(&name) != named, "{name}: {out}");
    }
    assert!(warning.contains("z.rs 2-503"), "{out}");
}
