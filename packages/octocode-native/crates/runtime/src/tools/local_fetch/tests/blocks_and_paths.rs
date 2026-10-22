use super::*;

fn checked_streamed_page(query: &LocalFetchQuery, paths: &Paths) -> LocalFetchResult {
    let mut rows = crate::contracts::prepare_many_and_validate(
        "localFetch",
        serde_json::json!({"queries":[query]}),
    )
    .expect("canonical input contract");
    let query_json = rows.remove(0);
    let admitted = serde_json::from_value(query_json.clone()).expect("admitted query");
    let result = fetch(&admitted, paths);
    let mut row =
        serde_json::json!({"index":0,"data":serde_json::to_value(&result).expect("output")});
    if let Some(status) = result.status.row_status() {
        row["status"] = serde_json::json!(status);
    }
    crate::response::extras::prune_row(&mut row, crate::id::ToolId::LocalFetch, &query_json);
    crate::response::rows::minimize_row(&mut row, crate::id::ToolId::LocalFetch, &query_json);
    let mut envelope = serde_json::json!({"results":[row]});
    crate::response::numbered::number_read_rows(crate::id::ToolId::LocalFetch, &mut envelope);
    crate::contracts::validate_output("localFetch", &envelope).expect("canonical output contract");
    result
}

#[test]
fn streamed_oversized_line_pages_its_bytes_and_later_selected_lines() {
    let fixture = Fixture::new();
    let path = fixture.0.join("large.log");
    let longest = "x".repeat(4 * 1024 * 1024);
    let suffix = "suffix past the streamed window\n";
    let selected_tail = "selected tail\n";
    // Streamed sources are those over the in-memory ceiling: the lines
    // after the oversized one only take the file just past it.
    let outside = (super::super::acquire::MAX_SOURCE_BYTES as usize).saturating_sub(longest.len())
        / "outside\n".len()
        + 1_000;
    fixture.write(
        "large.log",
        format!(
            "{longest}{suffix}{selected_tail}{}",
            "outside\n".repeat(outside)
        ),
    );
    let paths = Paths(fixture.0.clone());
    let mut query = qj(
        &path,
        serde_json::json!({"ranges":["1-2"], "unit":"bytes", "length":50_000}),
    );
    let view = format!("{longest}{suffix}{selected_tail}");
    let mut collected = String::new();
    let mut pages = 0usize;
    loop {
        let result = checked_streamed_page(&query, &paths);
        assert_eq!(result.status, ReadStatus::Success, "{result:?}");
        let content = result.content.as_deref().expect("page content");
        let page = result.pagination.as_ref().expect("pagination");
        if page.unit == WindowUnit::Bytes {
            assert_eq!(page.offset, collected.len(), "pages are contiguous");
            assert_eq!(page.total_bytes, view.len(), "the range's own size");
        }
        assert!(result.terminal_limit.is_none(), "{result:?}");
        collected.push_str(content);
        pages += 1;
        assert!(pages < 300, "paging must terminate");
        let Some(next) = result.next.and_then(|next| next.continue_window) else {
            break;
        };
        let wire = serde_json::to_value(next).expect("executable continuation");
        query = serde_json::from_value(wire["query"]["queries"][0].clone()).expect("copied query");
        assert!(query.snapshot.is_some(), "source identity persists");
        if collected.len() < longest.len() {
            assert_eq!(
                query.unit,
                Some(WindowUnit::Bytes),
                "byte cursor stays in bytes"
            );
        }
    }
    assert!(pages > 1);
    assert_eq!(collected, view);
    assert!(!collected.contains("outside"));
}

/// A streamed multi-range page stops inside the first span that needs
/// several window pages: it serves every span before it and that span's
/// first page, then continues that span alone (`continueWindow`) and reads
/// the ranges after it once (`readOtherRanges`, L2-F15). Following them
/// reaches every requested line exactly once.
#[test]
fn a_streamed_multi_range_read_serves_what_fits_and_continues_the_rest() {
    let fixture = Fixture::new();
    let source = (1..=1_500_000)
        .map(|n| format!("line {n}\n"))
        .collect::<String>();
    let path = fixture.write("large.log", &source);
    let paths = Paths(fixture.0.clone());
    let first = checked_streamed_page(
        &qj(
            &path,
            serde_json::json!({"ranges":["100-150","150000-157500","300000-300010"]}),
        ),
        &paths,
    );
    assert_eq!(first.status, ReadStatus::Success, "{first:?}");
    assert_eq!(first.is_partial, Some(true), "{first:?}");
    let mut seen = std::collections::BTreeMap::<usize, usize>::new();
    let mut count = |content: &str| {
        for line in content.lines() {
            if let Some(n) = line.strip_prefix("line ").and_then(|n| n.parse().ok()) {
                *seen.entry(n).or_default() += 1;
            }
        }
    };
    let content = first.content.as_deref().expect("content");
    assert!(content.contains("line 100\nline 101\n") && content.contains("line 150000\n"));
    count(content);
    let next = first.next.expect("continuations");
    assert!(next.read_bounded_lines.is_none(), "{next:?}");
    let others = next
        .read_other_ranges
        .expect("the ranges after the cut")
        .query;
    assert_eq!(
        serde_json::json!(others.ranges),
        serde_json::json!(["300000-300010"])
    );
    let mut page = next.continue_window;
    let mut pages = 1;
    while let Some(call) = page {
        let wire = serde_json::to_value(call).expect("executable continuation");
        let query: LocalFetchQuery =
            serde_json::from_value(wire["query"]["queries"][0].clone()).expect("copied query");
        assert_eq!(
            serde_json::json!(query.ranges),
            serde_json::json!(["150000-157500"])
        );
        let result = checked_streamed_page(&query, &paths);
        assert_eq!(result.status, ReadStatus::Success, "{result:?}");
        count(result.content.as_deref().expect("page content"));
        page = result.next.and_then(|next| next.continue_window);
        pages += 1;
        assert!(pages < 100, "paging must terminate");
    }
    assert!(pages > 1);
    let rest = checked_streamed_page(&others, &paths);
    assert_eq!(rest.status, ReadStatus::Success, "{rest:?}");
    assert!(rest.next.is_none(), "{rest:?}");
    count(rest.content.as_deref().expect("content"));
    let requested = (100..=150)
        .chain(150_000..=157_500)
        .chain(300_000..=300_010);
    for n in requested {
        assert_eq!(seen.remove(&n), Some(1), "line {n}");
    }
    assert!(seen.is_empty(), "unrequested lines: {seen:?}");
}

#[test]
fn streamed_byte_pages_reject_source_changes_and_whole_source_byte_reads() {
    let fixture = Fixture::new();
    let line = "x".repeat(32 * 1024);
    let source = format!("{line}\nselected tail\n{}", "outside\n".repeat(1_400_000));
    let path = fixture.write("large.log", &source);
    let paths = Paths(fixture.0.clone());
    let first = checked_streamed_page(&qj(&path, serde_json::json!({"ranges":["1-2"]})), &paths);
    assert_eq!(first.terminal_limit, None);
    let next = first
        .next
        .expect("next")
        .continue_window
        .expect("byte page");
    assert_eq!(next.query.unit, Some(WindowUnit::Bytes));
    assert_eq!(local_fetch_query::offset(&next.query), Some(16_384));
    let second = checked_streamed_page(&next.query, &paths);
    assert_eq!(second.status, ReadStatus::Success, "{:?}", second.error);
    assert_eq!(second.content.as_deref(), Some(&line[16_384..]));
    fixture.write("large.log", source.replacen('x', "y", 1));
    let stale = checked_streamed_page(&next.query, &paths);
    assert_eq!(stale.error_code.as_deref(), Some("staleSnapshot"));
    let unbounded = checked_streamed_page(&qj(&path, serde_json::json!({"unit":"bytes"})), &paths);
    assert_eq!(
        unbounded.error_code.as_deref(),
        Some("largeSourceWindowOnly")
    );
}

#[test]
fn a_streamed_clipped_private_key_body_stays_redacted_across_windows() {
    let fixture = Fixture::new();
    let path = fixture.write(
        "large.log",
        format!(
            "-----BEGIN PRIVATE KEY-----\n{}\n-----END PRIVATE KEY-----\nselected tail\n{}",
            "S".repeat(4 * 1024 * 1024 + 17),
            "outside\n".repeat(900_000)
        ),
    );
    let paths = Paths(fixture.0.clone());
    let mut query = qj(&path, serde_json::json!({"ranges":["2-4"]}));
    let mut collected = String::new();
    for _ in 0..4 {
        let result = checked_streamed_page(&query, &paths);
        assert_eq!(result.status, ReadStatus::Success, "{:?}", result.error);
        assert_eq!(result.terminal_limit, None);
        collected.push_str(result.content.as_deref().expect("content"));
        if let Some(next) = result.next.and_then(|next| next.continue_window) {
            query = next.query;
        } else {
            assert!(collected.ends_with("selected tail\n"));
            assert!(!collected.contains("SSSS"));
            assert!(!collected.contains("BEGIN"));
            assert!(!collected.contains("outside"));
            assert!(collected.contains("[REDACTED"));
            return;
        }
    }
    panic!("redacted windows must terminate");
}

const PY_TWO: &str = "import os\n\ndef first(a):\n    x = 1\n    return a\n\n\ndef second(b):\n    if b:\n        return first(1)\n    return 2\n";

#[test]
fn an_oversized_block_continues_through_the_declaration_end() {
    let t = Fixture::new();
    let p = t.0.join("m.py");
    let body: String = (0..450).map(|i| format!("    x{i} = {i}\n")).collect();
    // def at line 1, 450 body lines (2-451), return at 452.
    fs::write(&p, format!("def big():\n{body}    return 0\n")).expect("fixture");
    let paths = Paths(t.0.clone());
    let ranged = qj(&p, serde_json::json!({"ranges": ["10-12"], "block": true}));
    let r = fetch(&ranged, &paths);
    assert_eq!(
        r.source_line_ranges,
        vec![LineRange {
            start: 10,
            end: 409
        }]
    );
    let rest = r
        .next
        .and_then(|next| next.continue_block)
        .expect("next.continueBlock");
    assert_eq!(
        rest.query
            .ranges
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>(),
        vec!["410-452".to_owned()],
        "{rest:?}"
    );
    assert!(
        !local_fetch_query::block(&rest.query),
        "a block re-read would widen back: {rest:?}"
    );
    let r = fetch(&rest.query, &paths);
    assert_eq!(
        r.source_line_ranges,
        vec![LineRange {
            start: 410,
            end: 452
        }]
    );
    // A match inside an oversized requested declaration keeps its context
    // window; the declaration's unread rest is a page of this read.
    let matched = qj(
        &p,
        serde_json::json!({"matchString": "x200 = 200", "contextLines": 1, "block": true}),
    );
    let r = fetch(&matched, &paths);
    assert_eq!(
        r.source_line_ranges,
        vec![LineRange {
            start: 201,
            end: 203
        }]
    );
    let whole = r
        .next
        .and_then(|next| next.continue_block)
        .expect("next.continueBlock");
    let ranges: Vec<&str> = whole
        .query
        .ranges
        .iter()
        .map(|range| range.as_str())
        .collect();
    assert_eq!(ranges, ["1-200", "204-452"], "{whole:?}");
    assert!(whole.query.match_string.is_none() && !local_fetch_query::block(&whole.query));
}

#[test]
fn a_long_matched_line_marks_both_cuts_and_offers_the_whole_line() {
    let t = Fixture::new();
    let p = t.0.join("min.js");
    let line = format!("{}needleFn(){}", "a".repeat(3000), "b".repeat(3000));
    fs::write(&p, format!("head\n{line}\ntail\n")).expect("fixture");
    let paths = Paths(t.0.clone());
    let req = qj(&p, serde_json::json!({"matchString": "needleFn"}));
    let r = fetch(&req, &paths);
    let content = r.content.as_deref().expect("content");
    assert!(
        content.starts_with("... [2800 bytes omitted] ...\n"),
        "{content:.80}"
    );
    assert!(
        content.ends_with("\n... [2802 bytes omitted] ...\n"),
        "{content:.80}"
    );
    let wire = serde_json::to_value(&r).expect("serializable");
    crate::contracts::validate_output(
        "localFetch",
        &serde_json::json!({"results":[{"index":0,"data":wire}]}),
    )
    .expect("a row with next.wholeLines satisfies the output contract");
    let whole = r
        .next
        .and_then(|next| next.whole_lines)
        .expect("next.wholeLines");
    assert_eq!(whole.query.context_lines, Some(0), "{whole:?}");
    let r = fetch(&whole.query, &paths);
    assert_eq!(r.content, Some(format!("{line}\n")), "{:?}", r.warnings);
}

/// `regex:"rust"` is the Rust regex engine, the one `localSearch` runs: what it
/// refuses (backreferences, lookaround) is `invalidPattern` with the parser's
/// reason, and `next.repair` retries the same read as `regex:"pcre2"`, the
/// engine that owns them. It never needs a worker or an ECMAScript fallback.
#[test]
fn rust_regex_refuses_backreferences_and_lookaround_and_repairs_to_pcre2() {
    let t = Fixture::new();
    let p = t.0.join("regex.txt");
    fs::write(&p, "needle one\nhello aa\n").expect("fixture");
    let paths = Paths(t.0.clone());
    for (pattern, reason) in [
        (r"(\w)\1", "backreferences are not supported"),
        ("(?<=needle )one", "look-around"),
    ] {
        let result = fetch(
            &qj(
                &p,
                serde_json::json!({"matchString": pattern, "regex": "rust"}),
            ),
            &paths,
        );
        assert_eq!(
            result.error_code.as_deref(),
            Some("invalidPattern"),
            "{pattern}: {result:?}"
        );
        let error = result.error.clone().unwrap_or_default();
        assert!(
            error.starts_with(&format!(
                "Invalid regex pattern: matchString \"{}\": ",
                pattern.replace('\\', "\\\\")
            )) && error.contains(reason),
            "{error}"
        );
        let repaired = result
            .next
            .as_ref()
            .and_then(|next| next.repair.as_ref())
            .expect("repair lead")
            .query
            .clone();
        assert_eq!(
            repaired.regex,
            Some(octocode_contract_types::ReadRegex::Pcre2)
        );
        assert_eq!(
            local_fetch_query::match_strings(&repaired),
            vec![pattern],
            "kept as written"
        );
        let fixed = fetch(&repaired, &paths);
        assert_eq!(fixed.error_code, None, "{pattern}: {fixed:?}");
        assert!(fixed.content.is_some_and(|c| !c.is_empty()), "{pattern}");
    }
}

/// The verdict is the pattern's, never the file's size: a pattern that
/// compiles matches a file past 64 KiB, and one that does not fails there
/// with the very error it gives on a small file.
#[test]
fn rust_regex_verdict_does_not_depend_on_file_size() {
    let t = Fixture::new();
    let small = t.0.join("small.txt");
    let big = t.0.join("big.txt");
    let filler = "word xx yy\n";
    fs::write(&small, "tail aa\n").expect("fixture");
    fs::write(&big, format!("{}tail aa\n", filler.repeat(7_000))).expect("fixture");
    assert!(fs::metadata(&big).expect("meta").len() > 64 * 1024);
    let paths = Paths(t.0.clone());
    let run = |file: &Path, pattern: &str| {
        fetch(
            &qj(
                file,
                serde_json::json!({"matchString": pattern, "regex": "rust", "contextLines": 0}),
            ),
            &paths,
        )
    };
    for file in [&small, &big] {
        let found = run(file, "t[a]il");
        assert_eq!(found.error_code, None, "{found:?}");
        assert!(found.content.is_some_and(|c| c.contains("tail aa")));
    }
    let on_small = run(&small, r"(\w)\1");
    let on_big = run(&big, r"(\w)\1");
    assert_eq!(on_small.error_code.as_deref(), Some("invalidPattern"));
    assert_eq!(on_big.error, on_small.error);
}

/// A whole-file read through one oversized (minified) line byte-chunks only
/// that line: once it ends, paging resumes by lines and every later line
/// keeps its number. The walk stays lossless.
#[test]
fn full_content_resumes_numbered_lines_after_one_oversized_line() {
    let t = Fixture::new();
    let paths = Paths(t.0.clone());
    let file = t.0.join("strings.ts");
    let mut source = String::new();
    for i in 1..=1_000 {
        source.push_str(&format!("export const before_{i} = {i};\n"));
    }
    source.push_str(&format!("const table = \"{}\";\n", "x".repeat(27_500)));
    for i in 1..=300 {
        source.push_str(&format!("export const after_{i} = {i};\n"));
    }
    fs::write(&file, &source).expect("fixture");
    let mut query = q(&file);
    query.full_content = Some(true);
    // The standard view: every line carries its source number (`N\t`).
    query.minify = Some(MinifyMode::Standard);
    let mut pages = Vec::new();
    loop {
        let page = execute_local_fetch(&query, &paths, &Safe, &NeverCancel, Some(20_000), None);
        assert_eq!(page.error_code, None, "{page:?}");
        let next = page.next.clone().and_then(|next| next.continue_window);
        pages.push(page);
        match next {
            Some(next) => query = next.query,
            None => break,
        }
        assert!(pages.len() < 40, "runaway paging");
    }
    // Every page after the long line is a numbered line page.
    let long_line = 1_001;
    let mut numbered_after = Vec::new();
    let mut text = String::new();
    for page in &pages {
        let content = page.content.clone().unwrap_or_default();
        let unit = page.pagination.as_ref().map(|p| p.unit);
        if unit == Some(WindowUnit::Lines) {
            for line in content.lines() {
                let (number, _) = line.split_once('\t').expect("numbered line");
                let number: usize = number.parse().expect("number");
                if number > long_line {
                    numbered_after.push(number);
                }
            }
        }
        text.push_str(&content);
    }
    assert_eq!(numbered_after, (1_002..=1_301).collect::<Vec<_>>());
    // Pages join into the numbered view exactly once: no gap, no repeat.
    let view = source
        .lines()
        .enumerate()
        .map(|(index, line)| format!("{}\t{line}\n", index + 1))
        .collect::<String>();
    // The numbered view ends without the source's final newline.
    let (text, view) = (text.trim_end_matches('\n'), view.trim_end_matches('\n'));
    let at = text
        .bytes()
        .zip(view.bytes())
        .position(|(a, b)| a != b)
        .unwrap_or(text.len().min(view.len()));
    assert!(
        text == view,
        "lossless walk diverges at byte {at} ({} vs {} bytes): got {:?} want {:?}",
        text.len(),
        view.len(),
        &text[at.saturating_sub(40)..(at + 80).min(text.len())],
        &view[at.saturating_sub(40)..(at + 80).min(view.len())]
    );
}

/// A directory is `notAFile` and leads to its tree.
#[test]
fn directory_path_is_not_a_file_with_tree_lead() {
    let t = Fixture::new();
    fs::create_dir_all(t.0.join("src")).expect("dir");
    let mut req = test_local_fetch_query();
    req.path = "src".parse().expect("path");
    for result in [
        fetch(&req, &workspace_policy(&t.0)),
        fetch(&q(&t.0.join("src")), &Paths(t.0.clone())),
    ] {
        assert_eq!(result.error_code.as_deref(), Some("notAFile"), "{result:?}");
        let next = serde_json::to_value(result.next.as_ref().expect("next")).expect("json");
        assert_eq!(next["viewTree"]["tool"], "structureSearch", "{next}");
        assert_eq!(
            next["viewTree"]["query"]["queries"][0]["operation"], "tree",
            "{next}"
        );
    }
}

/// A path the policy refuses keeps the policy's shared code, as every local
/// tool reports it: a NUL byte anywhere is malformed input (not a file to
/// look for), and a withheld path is denied.
#[test]
fn a_policy_refusal_keeps_the_shared_code() {
    let t = Fixture::new();
    fs::write(t.0.join("kv.json"), "{}\n").expect("file");
    fs::write(t.0.join(".env"), "A=1\n").expect("env");
    for (path, code) in [
        ("kv\u{0}.json", "invalidInput"),
        ("kv.json\u{0}", "invalidInput"),
        (".env", crate::policy::PATH_POLICY_DENIED),
    ] {
        let mut req = test_local_fetch_query();
        req.path = path.parse().expect("path");
        let result = fetch(&req, &workspace_policy(&t.0));
        assert_eq!(result.error_code.as_deref(), Some(code), "{result:?}");
    }
}

/// A missing file leads to a listing by its stem from the workspace root:
/// a file moved to another tree (or a path that names the wrong package)
/// is found at any depth, not only under the closest existing directory.
#[test]
fn missing_file_leads_to_files_by_stem() {
    let t = Fixture::new();
    fs::create_dir_all(t.0.join("src/a")).expect("dirs");
    fs::create_dir_all(t.0.join("pkg/docs")).expect("dirs");
    fs::write(t.0.join("pkg/docs/blok.ts"), "x\n").expect("moved");
    let mut req = test_local_fetch_query();
    req.path = "src/a/blok.rs".parse().expect("path");
    let result = fetch(&req, &workspace_policy(&t.0));
    assert_eq!(
        result.error_code.as_deref(),
        Some("pathNotFound"),
        "{result:?}"
    );
    let next = serde_json::to_value(result.next.as_ref().expect("next")).expect("json");
    let listing = &next["findFile"]["query"]["queries"][0];
    assert_eq!(listing["operation"], "files", "{next}");
    assert_eq!(listing["path"], ".", "{next}");
    assert_eq!(listing["include"], serde_json::json!(["blok.*"]), "{next}");
    // The lead runs and lists the moved file.
    let (paths, security) = crate::tools::test_support::simple_policy(&t.0);
    let listed = crate::tools::structure_search::execute_structure(
        &serde_json::from_value(listing.clone()).expect("structureSearch row"),
        &paths,
        &security,
        &crate::execution::cancel::NeverCancel,
        None,
    )
    .expect("listing");
    assert!(
        listed["files"]
            .as_array()
            .is_some_and(|dirs| dirs.iter().any(|dir| {
                dir["dir"] == "pkg/docs"
                    && dir["files"].as_array().is_some_and(|names| {
                        names
                            .iter()
                            .any(|name| name.as_str().is_some_and(|n| n.starts_with("blok.ts")))
                    })
            })),
        "{listed}"
    );
}

/// The lead keeps the whole stem as a glob, so a short name (`x.md`)
/// lists only `x.*`, never every name that contains an `x`.
#[test]
fn missing_file_lead_never_widens_to_names_containing_the_stem() {
    let t = Fixture::new();
    fs::create_dir_all(t.0.join("docs")).expect("dirs");
    for name in ["x.txt", "box.md", "index.md"] {
        fs::write(t.0.join("docs").join(name), "x\n").expect("fixture");
    }
    let mut req = test_local_fetch_query();
    req.path = "docs/x.md".parse().expect("path");
    let result = fetch(&req, &workspace_policy(&t.0));
    let next = serde_json::to_value(result.next.as_ref().expect("next")).expect("json");
    let listing = &next["findFile"]["query"]["queries"][0];
    assert_eq!(listing["include"], serde_json::json!(["x.*"]), "{next}");
    let globs = crate::policy::include::include_globs(&["x.*".to_owned()]);
    assert_eq!(globs, vec!["x.*".to_owned()], "a glob is not widened");
}

/// A matchString miss leads to a localSearch of the file's directory.
#[test]
fn match_miss_leads_to_directory_search() {
    let t = Fixture::new();
    let p = t.0.join("a.rs");
    fs::write(&p, "fn a() {}\n").expect("fixture");
    let result = fetch(
        &qj(&p, serde_json::json!({"matchString": "nowhere_here"})),
        &Paths(t.0.clone()),
    );
    let next = serde_json::to_value(result.next.as_ref().expect("next")).expect("json");
    let search = &next["findText"];
    assert_eq!(search["tool"], "localSearch", "{next}");
    assert_eq!(
        search["query"]["queries"][0]["matchString"], "nowhere_here",
        "{next}"
    );
    assert_eq!(
        search["query"]["queries"][0]["path"],
        t.0.to_string_lossy().as_ref(),
        "{next}"
    );
}

/// A `ranges` match read that missed while the file has hits outside its
/// ranges says so and leads to the same read without `ranges`, not to a
/// search of the directory.
#[test]
fn a_ranged_miss_with_hits_elsewhere_leads_to_the_unranged_read() {
    let t = Fixture::new();
    let p = t.0.join("a.rs");
    fs::write(&p, "fn a() {}\nfn b() {}\nlet needle = 1;\n").expect("fixture");
    let result = fetch(
        &qj(
            &p,
            serde_json::json!({"matchString": "needle", "ranges": ["1-2"]}),
        ),
        &Paths(t.0.clone()),
    );
    assert_eq!(result.selected_match_count, Some(0), "{result:?}");
    assert_eq!(
        result.warnings,
        vec!["1 line matches outside the requested ranges.".to_owned()]
    );
    let next = serde_json::to_value(result.next.as_ref().expect("next")).expect("json");
    let read = &next["read"];
    assert_eq!(read["tool"], "localFetch", "{next}");
    assert_eq!(
        read["query"]["queries"][0]["matchString"], "needle",
        "{next}"
    );
    assert!(
        read["query"]["queries"][0].get("ranges").is_none(),
        "{next}"
    );
    assert!(next.get("findText").is_none(), "{next}");
}

/// The miss lead searches the directory for exactly what the read looked
/// for: the regex dialect, the case rule and `wholeWord` carry over, so the
/// lead never matches more than the read would have.
#[test]
fn a_match_miss_lead_keeps_the_reads_dialect_case_and_whole_word() {
    let t = Fixture::new();
    let p = t.0.join("a.rs");
    fs::write(&p, "fn a() {}\n").expect("fixture");
    let result = fetch(
        &qj(
            &p,
            serde_json::json!({
                "matchString": "(?<=x)nowhere",
                "regex": "pcre2",
                "caseMode": "insensitive",
                "wholeWord": true
            }),
        ),
        &Paths(t.0.clone()),
    );
    let next = serde_json::to_value(result.next.as_ref().expect("next")).expect("json");
    let row = &next["findText"]["query"]["queries"][0];
    assert_eq!(row["matchString"], "(?<=x)nowhere", "{next}");
    assert_eq!(row["regex"], "pcre2", "{next}");
    assert_eq!(row["caseMode"], "insensitive", "{next}");
    assert_eq!(row["wholeWord"], true, "{next}");
}

/// Several literal patterns that all missed: the lead searches every one of
/// them, each escaped, so `a.b` does not widen to any character.
#[test]
fn a_match_miss_lead_searches_every_literal_pattern_escaped() {
    let t = Fixture::new();
    let p = t.0.join("a.rs");
    fs::write(&p, "fn a() {}\n").expect("fixture");
    let result = fetch(
        &qj(
            &p,
            serde_json::json!({"matchString": ["no.where", "Elsewhere"]}),
        ),
        &Paths(t.0.clone()),
    );
    let next = serde_json::to_value(result.next.as_ref().expect("next")).expect("json");
    let row = &next["findText"]["query"]["queries"][0];
    assert_eq!(
        row["matchString"], "(?i:no\\.where)|(?:Elsewhere)",
        "{next}"
    );
    assert_eq!(row["regex"], "rust", "{next}");
    assert_eq!(row["caseMode"], "sensitive", "{next}");
}

/// A symbols outline names each multi-line declaration's line span.
#[test]
fn symbols_view_heads_carry_line_ranges() {
    let t = Fixture::new();
    let p = t.0.join("m.py");
    fs::write(&p, PY_TWO).expect("fixture");
    let content = fetch(
        &qj(&p, serde_json::json!({"minify": "symbols"})),
        &Paths(t.0.clone()),
    )
    .content
    .expect("content");
    assert!(content.contains("3-5\tdef first(a):"), "{content}");
    assert!(content.contains("8-11\tdef second(b):"), "{content}");
    assert!(content.contains("1\timport os"), "{content}");
}

/// A declaration that starts with annotations or decorators carries its
/// span on its head row (the class, method or field line); the collapsed
/// annotation row above it stays a plain row, in Java as in TypeScript.
#[test]
fn symbols_view_spans_sit_on_the_head_row_below_annotations() {
    let t = Fixture::new();
    let paths = Paths(t.0.clone());
    let outline = |name: &str, text: &str| {
        let p = t.0.join(name);
        fs::write(&p, text).expect("fixture");
        fetch(&qj(&p, serde_json::json!({"minify": "symbols"})), &paths)
            .content
            .expect("content")
    };
    let java = outline(
        "Ann.java",
        "package x;\n\n@Entity\n@Table(name = \"t\")\npublic class Ann {\n    @Id\n    @GeneratedValue\n    private Long id;\n\n    @Override\n    @Deprecated\n    public String toString() {\n        return \"\";\n    }\n}\n",
    );
    for row in [
        "\n3\t@Entity",
        "\n5-15\tpublic class Ann {",
        "\n6\t    @Id",
        "\n8\t    private Long id;",
        "\n10\t    @Override",
        "\n12-14\t    public String toString() {",
    ] {
        assert!(java.contains(row), "{row:?} in {java}");
    }
    let ts = outline(
        "dec.ts",
        "@Injectable()\n@Component({\n  selector: 'x',\n})\nexport class A {\n  @Output()\n  @HostBinding('class.x')\n  changed = 1;\n  method(): void {\n    call();\n  }\n}\n",
    );
    for row in [
        "1\t@Injectable()",
        "\n5-12\texport class A {",
        "\n6\t  @Output()",
        "\n8\t  changed = 1;",
        "\n9-11\t  method(): void {",
    ] {
        assert!(ts.contains(row), "{row:?} in {ts}");
    }
    // Every row shows once: no line both inside a span's head and its own row.
    for text in [&java, &ts] {
        let mut lines: Vec<&str> = text
            .lines()
            .filter_map(|row| row.split_once('\t')?.0.split('-').next())
            .collect();
        let all = lines.len();
        lines.dedup();
        assert_eq!(lines.len(), all, "{text}");
    }
}

/// With a declaration hit present, a call-site hit keeps its window
/// instead of widening to its caller; the widened declaration is named.
#[test]
fn block_reports_widened_declarations() {
    let t = Fixture::new();
    let p = t.0.join("m.py");
    fs::write(&p, PY_TWO).expect("fixture");
    let result = fetch(
        &qj(
            &p,
            serde_json::json!({"matchString": "first", "block": true, "contextLines": 0}),
        ),
        &Paths(t.0.clone()),
    );
    let body = serde_json::to_value(&result).expect("json");
    assert_eq!(
        body["blocks"],
        serde_json::json!([{"symbolName": "first", "line": 3, "endLine": 5}]),
        "{body}"
    );
    let content = result.content.expect("content");
    // The call on line 10 shows alone, not `second` (8-11) whole.
    assert!(content.contains("def first(a):"), "{content}");
    assert!(!content.contains("def second(b):"), "{content}");
    assert!(content.contains("return first(1)"), "{content}");
}

/// A long-line source's default window is bounded by bytes, with the
/// full window one continuation away.
#[test]
fn minified_default_window_is_byte_bounded() {
    let t = Fixture::new();
    let p = t.0.join("lodash.min.js");
    let text: String = (1..=140)
        .map(|n| {
            let tag = if n == 126 { "HIT" } else { "abc" };
            format!("var v{n:03}={tag};{}\n", "x".repeat(505))
        })
        .collect();
    fs::write(&p, &text).expect("fixture");
    let result = fetch(
        &qj(&p, serde_json::json!({"matchString": "HIT"})),
        &Paths(t.0.clone()),
    );
    let content = result.content.clone().expect("content");
    assert!(content.len() <= 2600, "{} bytes", content.len());
    assert!(content.contains("var v126=HIT;"), "{content}");
    let next = serde_json::to_value(result.next.as_ref().expect("next")).expect("json");
    let window = crate::id::DEFAULT_READ_CONTEXT_LINES;
    assert_eq!(
        next["expandContext"]["query"]["queries"][0]["contextLines"], window,
        "{next}"
    );
    // Short-line sources keep the full default window and no lead.
    let short = t.0.join("a.txt");
    fs::write(&short, numbered(40).replace("l20\n", "l20 HIT\n")).expect("fixture");
    let plain = fetch(
        &qj(&short, serde_json::json!({"matchString": "HIT"})),
        &Paths(t.0.clone()),
    );
    assert!(
        plain
            .next
            .as_ref()
            .is_none_or(|next| next.expand_context.is_none()),
        "{plain:?}"
    );
    assert_eq!(
        plain.content.expect("content").lines().count(),
        2 * window + 1
    );
}

/// A `unit:"lines"` walk over a file holding one line longer
/// than a page reaches every source byte exactly once. Line pages continue
/// at the next line; the oversized line is served as byte pages that each
/// cite it as `sourceLineRanges: [{line, endLine}]` (the wire names),
/// and line paging resumes on the following line.
#[test]
fn line_walk_over_an_oversized_line_covers_every_byte_once() {
    let t = Fixture::new();
    let paths = Paths(t.0.clone());
    let file = t.0.join("bundle.js");
    let mut source = String::new();
    for i in 1..=1_164 {
        source.push_str(&format!("var line_{i} = {i};\n"));
    }
    let long_line = 1_165;
    source.push_str(&format!("    CSS: '{}',\n", "x".repeat(42_830)));
    for i in 1..=11 {
        source.push_str(&format!("tail_{i}();\n"));
    }
    let total_lines = long_line + 11;
    fs::write(&file, &source).expect("fixture");
    let mut query = qj(&file, serde_json::json!({"unit": "lines", "length": 400}));
    let (mut text, mut last_line, mut byte_pages, mut pages) = (String::new(), 0, 0, 0);
    loop {
        let page = fetch(&query, &paths);
        assert_eq!(page.error_code, None, "{page:?}");
        pages += 1;
        assert!(pages < 40, "runaway paging");
        let wire = serde_json::to_value(&page).expect("serializable");
        let content = page.content.clone().unwrap_or_default();
        let unit = page.pagination.as_ref().map(|p| p.unit);
        if unit == Some(WindowUnit::Bytes) {
            byte_pages += 1;
            assert_eq!(
                wire["sourceLineRanges"],
                serde_json::json!([{"line": long_line, "endLine": long_line}]),
                "a byte page cites the oversized line it serves"
            );
            last_line = long_line;
        } else {
            let ranges = &page.source_line_ranges;
            let first = ranges.first().map_or(0, |r| r.start);
            assert_eq!(
                first,
                last_line + 1,
                "line page {pages} must continue gap-free"
            );
            assert!(
                ranges.windows(2).all(|w| w[1].start == w[0].end + 1),
                "{ranges:?}"
            );
            last_line = ranges.last().map_or(last_line, |r| r.end);
        }
        text.push_str(&content);
        match page.next.and_then(|next| next.continue_window) {
            Some(next) => query = next.query,
            None => break,
        }
    }
    assert!(byte_pages >= 3, "the 42 KB line spans several byte pages");
    assert_eq!(last_line, total_lines);
    assert!(text == source, "pages join into the source exactly once");
}

#[test]
fn selected_line_walk_keeps_byte_fallback_and_every_source_byte() {
    use crate::tools::selection::{Decision, JudgeCall, SelectionJudge};
    use serde_json::Value;

    use std::time::Duration;

    struct SourceOrderJudge;
    impl SelectionJudge for SourceOrderJudge {
        fn identity(&self) -> Value {
            serde_json::json!({"backend":"source-order-test"})
        }
        fn select(&self, call: JudgeCall<'_>) -> crate::tools::selection::Judgments {
            Ok(call
                .admitted
                .iter()
                .map(|&ordinal| (ordinal, Decision::Selected))
                .collect())
        }
    }

    let t = Fixture::new();
    let paths = Paths(t.0.clone());
    let file = t.0.join("selected-giant.txt");
    let mut source = (1..=1_164)
        .map(|line| format!("var line_{line} = {line};\n"))
        .collect::<String>();
    source.push_str(&format!("    CSS: '{}',\n", "x".repeat(42_830)));
    source.push_str("last line without newline");
    fs::write(&file, &source).expect("fixture");
    let executor = tokio::runtime::Runtime::new().expect("runtime");
    let selection = crate::tools::test_support::selection_fake::runtime(
        SourceOrderJudge,
        executor.handle().clone(),
        Duration::from_secs(10),
    );
    let mut query = qj(&file, serde_json::json!({"unit":"lines","length":400}));
    query.clasify_context = Some(
        serde_json::from_value(serde_json::json!({"classifiers":["find CSS"]})).expect("context"),
    );
    let (mut text, mut units, mut pages) = (String::new(), Vec::new(), 0);
    loop {
        let page = execute_local_fetch(&query, &paths, &Safe, &NeverCancel, None, Some(&selection));
        assert_eq!(page.error_code, None, "{page:?}");
        assert_eq!(
            page.selection.as_ref().map(|receipt| receipt.status),
            Some(octocode_contract_types::SelectionStatus::Complete)
        );
        pages += 1;
        assert!(pages < 40, "runaway paging");
        units.push(page.pagination.as_ref().map(|pagination| pagination.unit));
        text.push_str(page.content.as_deref().unwrap_or_default());
        match page.next.and_then(|next| next.continue_window) {
            Some(next) => query = next.query,
            None => break,
        }
    }
    assert!(
        units
            .windows(2)
            .any(|pair| pair == [Some(WindowUnit::Lines), Some(WindowUnit::Bytes)])
    );
    assert!(
        units
            .windows(2)
            .any(|pair| pair == [Some(WindowUnit::Bytes), Some(WindowUnit::Lines)])
    );
    assert_eq!(
        text, source,
        "selection and mixed units preserve every source byte"
    );
}

/// A regex read with more matches than the old 10,000 in-process cap
/// selects every matching line (never a silent prefix of them).
#[test]
fn regex_read_selects_every_match_past_ten_thousand() {
    use octocode_contract_types::ReadRegex;
    let t = Fixture::new();
    let p = t.0.join("many.txt");
    let source: String = (1..=12_000).map(|i| format!("hit {i}\n")).collect();
    fs::write(&p, &source).expect("fixture");
    for engine in [ReadRegex::Rust, ReadRegex::Pcre2] {
        let mut request = q(&p);
        request.match_string = Some(MatchString::String(
            "hit [0-9]+".parse().expect("match string"),
        ));
        request.regex = Some(engine);
        request.context_lines = Some(0);
        let result = fetch(&request, &Paths(t.0.clone()));
        assert_eq!(result.error, None, "{engine:?}");
        assert_eq!(result.selected_match_count, Some(12_000), "{engine:?}");
        assert!(
            result.warnings.iter().all(|w| !w.contains("match limit")),
            "{engine:?}: {:?}",
            result.warnings
        );
    }
}

/// A regex read that reaches the in-process match limit discloses it:
/// isPartial, a warning naming the limit, and a localSearch lead that pages
/// every match of the same pattern in the same file.
#[test]
fn regex_read_past_the_match_limit_is_partial_with_a_search_lead() {
    use octocode_contract_types::ReadRegex;
    let t = Fixture::new();
    let p = t.0.join("dense.txt");
    let mut source = String::new();
    for _ in 0..(extraction::MAX_REGEX_MATCHES / 1_000 + 1) {
        source.push_str(&"x".repeat(1_000));
        source.push('\n');
    }
    fs::write(&p, &source).expect("fixture");
    for engine in [ReadRegex::Rust, ReadRegex::Pcre2] {
        let mut request = q(&p);
        request.match_string = Some(MatchString::String("x".parse().expect("match string")));
        request.regex = Some(engine);
        request.context_lines = Some(0);
        let result = fetch(&request, &Paths(t.0.clone()));
        assert_eq!(result.error, None, "{engine:?}");
        assert_eq!(result.is_partial, Some(true), "{engine:?}");
        assert_eq!(
            result.partial_reasons,
            vec![super::types::PartialReason::RegexMatchLimit],
            "{engine:?}"
        );
        assert!(
            result.warnings.iter().any(|w| w.contains("match limit")),
            "{engine:?}: {:?}",
            result.warnings
        );
        let lead = result
            .next
            .as_ref()
            .and_then(|next| next.text_search.clone())
            .expect("findText lead");
        assert_eq!(lead["tool"], "localSearch", "{lead}");
        let row = &lead["query"]["queries"][0];
        assert_eq!(row["matchString"], "x", "{lead}");
        assert_eq!(row["path"], p.to_string_lossy().as_ref(), "{lead}");
    }
}

/// When the read pages, the tool emits the findText lead beside the
/// page continuation (the response moves it to `hints.findText`).
#[test]
fn paged_regex_read_past_the_match_limit_keeps_its_search_lead() {
    use octocode_contract_types::ReadRegex;
    let t = Fixture::new();
    let p = t.0.join("hits.txt");
    let source: String = (1..=extraction::MAX_REGEX_MATCHES + 5)
        .map(|n| format!("hit {n}\n"))
        .collect();
    fs::write(&p, &source).expect("fixture");
    let mut request = q(&p);
    request.match_string = Some(MatchString::String("hit".parse().expect("match string")));
    request.regex = Some(ReadRegex::Rust);
    let result = execute_local_fetch(
        &request,
        &Paths(t.0.clone()),
        &Safe,
        &NeverCancel,
        None,
        None,
    );
    assert!(
        result.warnings.iter().any(|w| w.contains("match limit")),
        "{:?}",
        result.warnings
    );
    let next = result.next.as_ref().expect("next");
    assert!(next.continue_window.is_some(), "the read pages: {next:?}");
    let lead = next
        .text_search
        .as_ref()
        .expect("findText lead on a paged read");
    assert_eq!(lead["tool"], "localSearch", "{lead}");
}

/// A matchString read of a JSON file names each hit line's section: a
/// window that hides the section header still says `devDependencies.vite`.
/// Other files carry no `matchKeys`.
#[test]
fn json_match_reads_name_each_hit_line_key_path() {
    let t = Fixture::new();
    let mut text = String::from("{\n  \"dependencies\": {\n");
    for n in 0..40 {
        text.push_str(&format!("    \"dep{n}\": \"1\",\n"));
    }
    text.push_str("    \"vite\": \"^7\"\n  },\n  \"devDependencies\": {\n");
    for n in 0..40 {
        text.push_str(&format!("    \"dev{n}\": \"1\",\n"));
    }
    text.push_str("    \"vite\": \"^7\"\n  }\n}\n");
    let json = t.0.join("package.json");
    fs::write(&json, &text).expect("fixture");
    let paths = Paths(t.0.clone());
    let req = qj(
        &json,
        serde_json::json!({"matchString": "\"vite\"", "contextLines": 0}),
    );
    let wire = serde_json::to_value(fetch(&req, &paths)).expect("serialize");
    assert_eq!(
        wire["matchKeys"],
        serde_json::json!({"43": "dependencies.vite", "86": "devDependencies.vite"}),
        "{wire}"
    );
    let plain = t.0.join("notes.txt");
    fs::write(&plain, &text).expect("fixture");
    let wire = serde_json::to_value(fetch(
        &qj(&plain, serde_json::json!({"matchString": "\"vite\""})),
        &paths,
    ))
    .expect("serialize");
    assert!(wire.get("matchKeys").is_none(), "{wire}");
}

/// A matchString read of a TOML file names each hit's table and key, with
/// the same `matchKeys` map (`[target.'cfg(…)'.dependencies]`).
#[test]
fn toml_match_reads_name_each_hit_line_key_path() {
    let t = Fixture::new();
    let toml = t.0.join("Cargo.toml");
    fs::write(
        &toml,
        "[package]\nname = \"demo\"\n\n[dependencies]\nbacktrace = \"0.3\"\n\n[target.'cfg(unix)'.dev-dependencies]\nbacktrace = \"0.3\"\n",
    )
    .expect("fixture");
    let paths = Paths(t.0.clone());
    let wire = serde_json::to_value(fetch(
        &qj(
            &toml,
            serde_json::json!({"matchString": "backtrace", "contextLines": 0}),
        ),
        &paths,
    ))
    .expect("serialize");
    assert_eq!(
        wire["matchKeys"],
        serde_json::json!({"5": "dependencies.backtrace", "8": "target.cfg(unix).dev-dependencies.backtrace"}),
        "{wire}"
    );
}

/// A list item that does not compile as a regex is named by index and text
/// (`"def post("` in a `regex:"rust"` list), on both regex engines.
#[test]
fn invalid_regex_items_are_named_by_index() {
    let t = Fixture::new();
    let p = t.0.join("a.py");
    fs::write(&p, "def post(self):\n    pass\n").expect("fixture");
    let paths = Paths(t.0.clone());
    for engine in ["rust", "pcre2"] {
        let list = fetch(
            &qj(
                &p,
                serde_json::json!({"matchString": ["pass", "def post("], "regex": engine}),
            ),
            &paths,
        );
        assert_eq!(
            list.error_code.as_deref(),
            Some("invalidPattern"),
            "{list:?}"
        );
        // The repair keeps the item that compiles and escapes the one that
        // does not, in the same engine.
        let repair = &list.next.as_ref().expect("repair lead").repair;
        let repaired = repair.as_ref().expect("repair").query.clone();
        assert_eq!(
            local_fetch_query::match_strings(&repaired),
            vec!["pass", "def post\\("],
            "{engine}"
        );
        let fixed = fetch(&repaired, &paths);
        assert_eq!(fixed.error_code, None, "{engine}: {fixed:?}");
        let error = list.error.unwrap_or_default();
        assert!(
            error.starts_with("Invalid regex pattern: matchString[1] \"def post(\": "),
            "{engine}: {error}"
        );
        let one = fetch(
            &qj(
                &p,
                serde_json::json!({"matchString": "def post(", "regex": engine}),
            ),
            &paths,
        );
        let error = one.error.unwrap_or_default();
        assert!(
            error.starts_with("Invalid regex pattern: matchString \"def post(\": "),
            "{engine}: {error}"
        );
    }
    assert_eq!(
        crate::id::ToolId::LocalFetch
            .output()
            .error_hint("invalidPattern"),
        Some(crate::response::output::NO_HINT)
    );
}

/// `ranges` with `matchString` is a region grep: only hits inside the
/// ranges count, their windows stay inside them, and ranges wholly past the
/// file end are an error, never a whole-file grep.
#[test]
fn ranges_with_match_string_grep_inside_the_ranges_only() {
    let t = Fixture::new();
    let p = t.0.join("a.txt");
    let text = (1..=30)
        .map(|n| {
            if n % 10 == 5 {
                format!("hit {n}\n")
            } else {
                format!("l{n}\n")
            }
        })
        .collect::<String>();
    fs::write(&p, text).expect("fixture");
    let paths = Paths(t.0.clone());
    let mut req = q(&p);
    req.match_string = Some(MatchString::String("hit".parse().expect("match string")));
    req.context_lines = Some(3);
    req.ranges = vec!["10-20".parse().expect("range")];
    let r = fetch(&req, &paths);
    assert_eq!(r.matched_lines, vec![15], "{r:?}");
    assert_eq!(
        r.source_line_ranges,
        vec![LineRange { start: 12, end: 18 }],
        "{r:?}"
    );
    req.ranges = vec!["90-100".parse().expect("range")];
    let past = fetch(&req, &paths);
    assert!(
        past.error
            .as_deref()
            .is_some_and(|e| e.contains("past the file end")),
        "{past:?}"
    );
}

/// Ranges that all start past the file end repair to the same read of the
/// file's last window, as wide as the widest request; a range that reaches
/// into the file is served and carries no repair.
#[test]
fn ranges_past_the_file_end_lead_to_the_last_window() {
    let t = Fixture::new();
    let p = t.write("short.txt", "l1\nl2\nl3\nl4\nl5\n");
    let paths = Paths(t.0.clone());
    let mut req = q(&p);
    req.ranges = vec![
        "90-92".parse().expect("range"),
        "95-100".parse().expect("range"),
    ];
    let past = fetch(&req, &paths);
    assert!(past.error.is_some(), "{past:?}");
    let repair = past
        .next
        .as_ref()
        .and_then(|next| next.repair.as_ref())
        .expect("repair lead");
    assert_eq!(
        local_fetch_query::line_ranges(&repair.query),
        vec![LineRange { start: 1, end: 5 }],
        "{past:?}"
    );
    let fixed = fetch(&repair.query, &paths);
    assert!(fixed.error.is_none(), "{fixed:?}");
    req.ranges = vec![
        "3-4".parse().expect("range"),
        "9-10".parse().expect("range"),
    ];
    assert!(fetch(&req, &paths).error.is_none());
    req.ranges = vec!["9-10".parse().expect("range")];
    let narrow = fetch(&req, &paths);
    assert_eq!(
        narrow
            .next
            .as_ref()
            .and_then(|next| next.repair.as_ref())
            .map(|repair| local_fetch_query::line_ranges(&repair.query)),
        Some(vec![LineRange { start: 4, end: 5 }]),
        "{narrow:?}"
    );
}

#[test]
fn match_ranges_multiline_hits_must_fit_inside_one_requested_region() {
    let fixture = Fixture::new();
    let path = fixture.write("multiline.txt", "alpha\nbeta\ngamma\n");
    let paths = Paths(fixture.0.clone());
    for dialect in ["literal", "rust", "pcre2"] {
        for context in [0, 3] {
            let query = qj(
                &path,
                serde_json::json!({
                    "matchString":"alpha\nbeta", "regex":dialect,
                    "ranges":["1-1"], "contextLines":context
                }),
            );
            let result = fetch(&query, &paths);
            assert_eq!(result.content.as_deref(), Some(""), "{dialect}: {result:?}");
            assert!(result.matched_lines.is_empty(), "{dialect}: {result:?}");
            assert!(
                result.source_line_ranges.is_empty(),
                "{dialect}: {result:?}"
            );
        }
        let query = qj(
            &path,
            serde_json::json!({
                "matchString":["alpha\nbeta", "gamma"], "regex":dialect,
                "ranges":["1-1", "3-3"], "contextLines":0
            }),
        );
        let result = fetch(&query, &paths);
        assert_eq!(
            result.content.as_deref(),
            Some("gamma\n"),
            "{dialect}: {result:?}"
        );
        assert_eq!(result.matched_lines, [3]);
        assert_eq!(result.source_line_ranges, [LineRange { start: 3, end: 3 }]);
    }
}

#[test]
fn match_ranges_multiline_hits_keep_exact_region_end_and_explicit_blocks() {
    let fixture = Fixture::new();
    let source = "fn scope() {\n    alpha();\n    beta();\n}\n";
    let path = fixture.write("multiline.rs", source);
    let paths = Paths(fixture.0.clone());
    let mut query = qj(
        &path,
        serde_json::json!({
            "matchString":"    alpha();\n    beta();\n",
            "ranges":["2-3"], "contextLines":0
        }),
    );
    let result = fetch(&query, &paths);
    assert_eq!(
        result.content.as_deref(),
        Some("    alpha();\n    beta();\n")
    );
    assert_eq!(result.matched_lines, [2, 3]);
    assert_eq!(result.source_line_ranges, [LineRange { start: 2, end: 3 }]);
    query.block = Some(true);
    let expanded = fetch(&query, &paths);
    assert_eq!(expanded.content.as_deref(), Some(source));
    assert_eq!(
        expanded.source_line_ranges,
        [LineRange { start: 1, end: 4 }]
    );
}

#[test]
fn match_ranges_keep_implicit_declaration_expansion_inside_requested_lines() {
    let fixture = Fixture::new();
    let path = fixture.write("scope.rs", "fn scope() {\n    let needle = 1;\n}\n");
    let paths = Paths(fixture.0.clone());
    let mut query = qj(
        &path,
        serde_json::json!({"matchString":"needle", "ranges":["2-2"], "contextLines":1}),
    );
    let result = fetch(&query, &paths);
    assert_eq!(result.content.as_deref(), Some("    let needle = 1;\n"));
    assert_eq!(result.source_line_ranges, [LineRange { start: 2, end: 2 }]);
    assert_eq!(result.match_ranges, [LineRange { start: 2, end: 2 }]);

    query.block = Some(true);
    let explicit = fetch(&query, &paths);
    assert_eq!(
        explicit.content.as_deref(),
        Some("fn scope() {\n    let needle = 1;\n}\n")
    );
}

#[test]
fn match_ranges_keep_long_line_byte_context_inside_requested_lines() {
    let fixture = Fixture::new();
    let source = format!(
        "outside_before\nneedle{}needle\noutside_after\n",
        "x".repeat(3_000)
    );
    let path = fixture.write("long-line.txt", &source);
    let paths = Paths(fixture.0.clone());
    let query = qj(
        &path,
        serde_json::json!({"matchString":"needle", "ranges":["2-2"]}),
    );
    let result = fetch(&query, &paths);
    let content = result.content.as_deref().expect("bounded match content");
    assert!(!content.contains("outside_before"), "{content}");
    assert!(!content.contains("outside_after"), "{content}");
    assert_eq!(content.matches("needle").count(), 2);
    assert_eq!(result.source_line_ranges, [LineRange { start: 2, end: 2 }]);
    assert_eq!(result.match_ranges, [LineRange { start: 2, end: 2 }]);
}

/// A path-only read of a minified (one huge line) file pages by bytes;
/// every continuation carries the unit of the page it continues, so walking
/// `next.continueWindow` verbatim reaches the end byte-identical.
#[test]
fn path_only_walk_of_a_minified_file_is_byte_identical() {
    let t = Fixture::new();
    let paths = Paths(t.0.clone());
    let file = t.0.join("min.js");
    let source: String = (0..40_000)
        .map(|i| format!("needle{i} "))
        .collect::<String>()
        .chars()
        .take(300_000)
        .collect();
    fs::write(&file, &source).expect("fixture");
    let mut query = q(&file);
    let (mut text, mut pages) = (String::new(), 0);
    loop {
        let page = fetch(&query, &paths);
        assert_eq!(page.error_code, None, "{page:?}");
        assert!(!page.out_of_range, "continuation left the view: {page:?}");
        text.push_str(&page.content.clone().unwrap_or_default());
        pages += 1;
        assert!(pages < 40, "runaway paging");
        match page.next.and_then(|next| next.continue_window) {
            Some(next) => query = next.query,
            None => break,
        }
    }
    assert!(pages >= 18, "{pages} pages");
    assert!(text == source, "pages join into the source exactly once");
}

/// A file whose one line takes many byte pages (a minified bundle or a
/// one-line JSON dump) names the faster routes on its first page: a
/// `matchString` window around the part that matters, and the largest page
/// `length`. Later pages, a few pages, and line pages do not repeat it.
#[test]
fn a_many_page_long_line_names_the_faster_routes_on_its_first_page() {
    let t = Fixture::new();
    let paths = Paths(t.0.clone());
    let long = t.0.join("data.bin.txt");
    fs::write(&long, "needle0 ".repeat(40_000)).expect("fixture");
    let first = fetch(&q(&long), &paths);
    assert_eq!(first.error_code, None, "{first:?}");
    let note = first.warnings.join("\n");
    assert!(note.contains("20 byte pages"), "{note}");
    assert!(note.contains("matchString"), "{note}");
    assert!(note.contains("50000"), "{note}");
    assert!(!note.contains("first 1 lines"), "{note}");
    let second = fetch(
        &first
            .next
            .and_then(|next| next.continue_window)
            .expect("next page")
            .query,
        &paths,
    );
    assert!(second.warnings.is_empty(), "{:?}", second.warnings);

    let few = t.0.join("few.txt");
    fs::write(&few, "needle0 ".repeat(5_000)).expect("fixture");
    let few = fetch(&q(&few), &paths);
    assert!(
        !few.warnings.join("\n").contains("matchString"),
        "{:?}",
        few.warnings
    );

    let lines = t.0.join("lines.txt");
    fs::write(&lines, numbered(200_000)).expect("fixture");
    let lines = fetch(&q(&lines), &paths);
    assert!(
        !lines.warnings.join("\n").contains("matchString"),
        "{:?}",
        lines.warnings
    );
}

/// `contextLines` widens matchString hits; a read without matchString is
/// refused by the contract, the rule's one owner, before any file is read.
#[test]
fn context_lines_without_match_string_is_refused_by_the_contract() {
    let refused = crate::contracts::prepare_many_and_validate(
        "localFetch",
        serde_json::json!({"queries":[{"path":"a.rs","ranges":["1-1"],"contextLines":5}]}),
    )
    .expect_err("contextLines without matchString");
    assert!(
        format!("{refused:?}").contains("contextLines"),
        "{refused:?}"
    );
}

/// The row names the file it read: `.` and `..` segments of the requested
/// path are resolved in the echoed `path`, the file's canonical path
/// relative to the workspace.
#[test]
fn the_row_path_is_the_normal_form_of_the_requested_one() {
    let t = Fixture::new();
    fs::create_dir_all(t.0.join("sub")).expect("dir");
    let p = t.0.join("sub").join("a.rs");
    fs::write(&p, "fn a() {}\n").expect("fixture");
    let dotted =
        t.0.join("sub")
            .join("..")
            .join(".")
            .join("sub")
            .join("a.rs");
    let result = fetch(&qj(&dotted, serde_json::json!({})), &Paths(t.0.clone()));
    assert_eq!(result.status, ReadStatus::Success, "{result:?}");
    assert_eq!(result.path, "sub/a.rs", "{result:?}");
}

/// A `..` after a symlink leaves the link's target, not the link: the row
/// names the file the read resolved, never a lexical fold of the request.
#[cfg(unix)]
#[test]
fn the_row_path_follows_a_symlink_before_its_parent_segment() {
    let t = Fixture::new();
    let root = fs::canonicalize(&t.0).expect("canonical");
    fs::create_dir_all(root.join("other").join("inner")).expect("dir");
    fs::write(root.join("other").join("a.rs"), "fn other() {}\n").expect("fixture");
    fs::write(root.join("a.rs"), "fn lexical() {}\n").expect("fixture");
    std::os::unix::fs::symlink(root.join("other").join("inner"), root.join("link"))
        .expect("symlink");
    let result = fetch(
        &qj(Path::new("link/../a.rs"), serde_json::json!({})),
        &Paths(root.clone()),
    );
    assert_eq!(
        result.content.as_deref(),
        Some("fn other() {}\n"),
        "{result:?}"
    );
    assert_eq!(result.path, "other/a.rs", "{result:?}");
}

/// Whole-file and streamed reads through a symlink name the requested path
/// in the row and in every continuation, and the first page names the
/// file it resolved to; a path with no symlink names its canonical file.
#[cfg(unix)]
#[test]
fn reads_through_a_symlink_keep_the_requested_path() {
    let t = Fixture::new();
    let root = fs::canonicalize(&t.0).expect("canonical");
    fs::create_dir_all(root.join("other")).expect("dir");
    fs::write(root.join("other").join("a.rs"), "fn a() {}\n").expect("fixture");
    // Past the whole-file guard, the source is served from streamed windows.
    let line = format!("{}\n", "x".repeat(99));
    let lines = usize::try_from(acquire::MAX_SOURCE_BYTES).expect("size") / line.len() + 10;
    fs::write(root.join("other").join("big.txt"), line.repeat(lines)).expect("fixture");
    std::os::unix::fs::symlink(root.join("other"), root.join("link")).expect("symlink");
    std::os::unix::fs::symlink(root.join("other").join("a.rs"), root.join("file-link.rs"))
        .expect("symlink");
    let policy = workspace_policy(&root);
    for (requested, target, streamed) in [
        ("link/a.rs", Some("other/a.rs"), false),
        ("link/big.txt", Some("other/big.txt"), true),
        ("file-link.rs", Some("other/a.rs"), false),
        ("other/./a.rs", None, false),
    ] {
        let mut req = test_local_fetch_query();
        req.path = requested.parse().expect("path");
        let result = fetch(&req, &policy);
        assert_eq!(
            result.status,
            ReadStatus::Success,
            "{requested}: {:?}",
            result.error
        );
        assert_eq!(
            result
                .warnings
                .iter()
                .any(|w| w.starts_with("Large source")),
            streamed,
            "{requested}: {:?}",
            result.warnings
        );
        let disclosed = result
            .warnings
            .iter()
            .find_map(|w| w.split_once(" resolves through a symlink to "))
            .map(|(_, to)| to.trim_end_matches('.').to_owned());
        assert_eq!(disclosed.as_deref(), target, "{requested}");
        let named = if target.is_some() {
            requested
        } else {
            "other/a.rs"
        };
        assert_eq!(result.path, named, "{requested}");
        // A read that continues the file names the file its row names.
        let mut next = result.next.clone().unwrap_or_default();
        let reads = next
            .reads_mut()
            .map(|read| read.query.path.to_string())
            .collect::<Vec<_>>();
        assert_eq!(!reads.is_empty(), streamed, "{requested}: {reads:?}");
        assert!(
            reads.iter().all(|path| *path == result.path),
            "{requested}: {reads:?}"
        );
        // The next page does not repeat where the link led.
        if let Some(read) = result
            .next
            .as_ref()
            .and_then(|next| next.continue_window.as_ref())
        {
            let page = fetch(&read.query, &policy);
            assert!(
                !page
                    .warnings
                    .iter()
                    .any(|w| w.contains("resolves through a symlink")),
                "{requested}: {:?}",
                page.warnings
            );
        }
    }
}

/// On a case-insensitive file system a path that differs from the file's
/// name only in letter case reads the file under its own name: no
/// component is a symlink, so no symlink is named.
#[test]
fn a_case_only_difference_is_not_a_symlink() {
    let t = Fixture::new();
    let root = fs::canonicalize(&t.0).expect("canonical");
    fs::create_dir_all(root.join("src")).expect("dir");
    fs::write(root.join("src/Readme.md"), "# readme\n").expect("fixture");
    if !root.join("SRC/README.MD").exists() {
        return; // case-sensitive: the other spelling names no file
    }
    let policy = workspace_policy(&root);
    let mut req = test_local_fetch_query();
    req.path = "SRC/README.MD".parse().expect("path");
    let result = fetch(&req, &policy);
    assert_eq!(result.status, ReadStatus::Success, "{:?}", result.error);
    assert!(
        !result.warnings.iter().any(|w| w.contains("symlink")),
        "{:?}",
        result.warnings
    );
    assert_eq!(result.path, "src/Readme.md", "{result:?}");
}

/// A missing file leads to its parent's tree, where a mistyped name's
/// siblings show, beside the listing that finds a moved file and the read
/// of the one sibling its mistyped name is close to.
#[test]
fn a_missing_file_leads_to_its_parents_tree() {
    let t = Fixture::new();
    fs::create_dir_all(t.0.join("src")).expect("dir");
    fs::write(t.0.join("src/executor.rs"), "fn a() {}\n").expect("fixture");
    let policy = workspace_policy(&t.0);
    let r = fetch(
        &qj(&t.0.join("src/exector.rs"), serde_json::json!({})),
        &policy,
    );
    assert_eq!(r.error_code.as_deref(), Some("pathNotFound"), "{r:?}");
    let next = serde_json::to_value(r.next.as_ref().expect("next")).expect("json");
    let tree = &next["viewTree"]["query"]["queries"][0];
    assert_eq!(tree["operation"], "tree", "{next}");
    assert_eq!(tree["path"], "src", "{next}");
    assert!(next.get("findFile").is_some(), "{next}");
    assert!(!r.path.is_empty(), "an error row names its path");
    let read = &r
        .next
        .as_ref()
        .expect("next")
        .read
        .as_ref()
        .expect("read")
        .query;
    assert_eq!(
        read.path.as_str(),
        t.0.join("src/executor.rs").to_string_lossy(),
        "{next}"
    );
    assert_eq!(fetch(read, &policy).status, ReadStatus::Success);
}

/// A mistyped name close to a file the path policy withholds names no
/// read of it: recovery never reveals a withheld file.
#[test]
fn a_missing_file_close_to_a_withheld_one_offers_no_read() {
    let t = Fixture::new();
    t.write(".aws/credentials", "secret\n");
    let policy = workspace_policy(&t.0);
    let r = fetch(
        &qj(&t.0.join(".aws/credentails"), serde_json::json!({})),
        &policy,
    );
    assert!(r.error_code.is_some(), "{r:?}");
    assert!(
        r.next.as_ref().is_none_or(|next| next.read.is_none()),
        "{:?}",
        r.next
    );
}
