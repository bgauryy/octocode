use super::*;

#[test]
fn every_view_delivers_each_matching_file_or_line_once_at_multiple_budgets() {
    let root = tempfile::tempdir().expect("fixture");
    for file in 0..400 {
        fs::write(
            root.path().join(format!("f{file:03}.txt")),
            "alpha needle\nbeta needle needle\nother\n",
        )
        .expect("file");
    }
    for view in [
        "content",
        "files",
        "filesWithout",
        "countLines",
        "countMatches",
        "matchOnly",
    ] {
        for budget in [2500, 9000] {
            let term = if view == "filesWithout" {
                "missing"
            } else {
                "needle"
            };
            let (walk, pages) = walk_search(
                root.path(),
                serde_json::json!({
                    "path":root.path(),"matchString":[term],"view":view,"sort":{"by":"path","order":"asc"}
                }),
                Some(budget),
            );
            let count = if view == "matchOnly" {
                1200
            } else if view == "content" {
                800
            } else {
                400
            };
            assert_eq!(walk.rows.len(), count, "{view} budget {budget}");
            assert!(pages.len() > 1, "fixture must exercise continuation");
            for page in &pages {
                for file in page["files"].as_array().expect("files") {
                    if view == "countLines" {
                        assert_eq!(file["matchedLineCount"], 2, "{file}");
                    }
                    if view == "countMatches" {
                        assert_eq!(file["matchCount"], 3, "{file}");
                    }
                }
            }
        }
    }
}

#[test]
fn overlapping_context_windows_reconstruct_every_requested_source_line_once() {
    let root = tempfile::tempdir().expect("fixture");
    let source: Vec<String> = (1..=240)
        .map(|line| {
            if matches!(line % 11, 0 | 3 | 5 | 7) {
                format!("let needle_{line} = {line};")
            } else {
                format!("let other_{line} = {line};")
            }
        })
        .collect();
    fs::write(root.path().join("a.rs"), source.join("\n") + "\n").expect("source");
    for (before, after) in [(3, 3), (4, 0), (0, 4), (1, 5)] {
        let (_, pages) = walk_search(
            root.path(),
            serde_json::json!({"path":root.path(),"matchString":["needle"],"contextLines":{"before":before,"after":after}}),
            Some(2500),
        );
        assert!(pages.len() > 1);
        let mut seen = std::collections::BTreeMap::new();
        for page in &pages {
            for file in page["files"].as_array().expect("files") {
                for row in file["matches"].as_array().expect("matches") {
                    for line in row["value"].as_str().expect("value").lines() {
                        let (number, text) = line.split_once('\t').expect("source coordinate");
                        let number = number.parse::<usize>().expect("line number");
                        assert_eq!(text, source[number - 1]);
                        assert!(
                            seen.insert(number, text.to_owned()).is_none(),
                            "line {number} duplicated"
                        );
                    }
                }
            }
        }
        let expected: std::collections::BTreeSet<_> = source
            .iter()
            .enumerate()
            .filter(|(_, text)| text.contains("needle"))
            .flat_map(|(index, _)| {
                (index + 1).saturating_sub(before).max(1)..=(index + 1 + after).min(source.len())
            })
            .collect();
        assert_eq!(
            seen.keys()
                .copied()
                .collect::<std::collections::BTreeSet<_>>(),
            expected
        );
    }
}

#[test]
fn truncated_search_values_have_reads_that_recover_the_full_source() {
    let root = tempfile::tempdir().expect("fixture");
    let source = format!("needle {} END_OF_EVIDENCE\n", "λ🦀text ".repeat(3000));
    fs::write(root.path().join("a.txt"), &source).expect("source");
    let (policy, security) = policy_for(root.path());
    let q = ls_query(
        serde_json::json!({"path":root.path(),"matchString":["needle"]}),
        None,
    );
    let found = search_in(&q, &policy, &security, Some(2500)).expect("search");
    let serialized = serde_json::to_value(&found).expect("result");
    crate::contracts::validate_output(
        "localSearch",
        &serde_json::json!({"results": [{"index": 0, "data": {"files": serialized["files"]}}]}),
    )
    .expect("clipped match evidence satisfies the core contract");
    let shown = &serialized["files"][0]["matches"][0];
    if shown.get("originalChars").is_some() {
        let (reads, content) = follow_expansions(&found, &policy);
        assert!(reads > 0, "{serialized}");
        assert!(
            content.contains("END_OF_EVIDENCE"),
            "full source must remain reachable"
        );
    } else {
        assert_eq!(shown["value"], source.trim_end());
    }
}

#[test]
fn every_page_keeps_enclosing_declaration_coordinates() {
    let root = tempfile::tempdir().expect("fixture");
    fs::write(root.path().join("a.rs"), let_heavy_rust("handler", 60)).expect("source");
    let (walk, pages) = walk_search(
        root.path(),
        serde_json::json!({"path":root.path(),"matchString":["let "]}),
        Some(3000),
    );
    assert_eq!(walk.rows.len(), 300);
    for page in &pages {
        for file in page["files"].as_array().expect("files") {
            for index in 0..file["matches"].as_array().expect("matches").len() {
                let owner = owner_of(file, index).expect("declaration");
                let line = file["matches"][index]["line"].as_u64().expect("hit line");
                assert!(owner["line"].as_u64().expect("declaration start") <= line);
                assert!(owner["endLine"].as_u64().expect("declaration end") >= line);
                assert!(
                    owner["symbolName"]
                        .as_str()
                        .expect("name")
                        .starts_with("handler_")
                );
            }
        }
    }
}

#[test]
fn match_only_uniqueness_counts_complete_values_per_file() {
    let files = [
        ("a.txt", "alpha alpha\nbeta\nalpha\n"),
        ("b.txt", "alpha\n"),
    ];
    let result = search_fixture(
        &files,
        ls_query(
            serde_json::json!({"matchString":["alpha","beta"],"view":"matchOnly","unique":"count"}),
            None,
        ),
    );
    let first = result["files"]
        .as_array()
        .expect("files")
        .iter()
        .find(|file| file["path"] == "a.txt")
        .expect("a");
    let rows = first["matches"].as_array().expect("rows");
    assert_eq!(rows.len(), 2, "{result}");
    let counts: std::collections::BTreeMap<_, _> = rows
        .iter()
        .map(|row| {
            (
                row["value"].as_str().expect("value"),
                row["count"].as_u64().unwrap_or(1),
            )
        })
        .collect();
    assert_eq!(counts["alpha"], 3);
    assert_eq!(counts["beta"], 1);
}
