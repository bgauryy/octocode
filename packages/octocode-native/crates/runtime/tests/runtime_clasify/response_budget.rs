//! `responseChars` bounds a clasify page like every tool's: a row of many
//! units either splits into pages within the budget, or the page that holds
//! an indivisible value says `oversized`.

use super::noul;
use crate::support::{Run as _, Workspace, clasify_runtime};
use serde_json::json;

/// A 12-unit `perItem` row asked for pages of 500 chars: every page fits the
/// budget or discloses `oversized`, and the pages together carry all 12 units.
#[tokio::test]
async fn a_twelve_unit_row_pages_within_response_chars_or_says_oversized() {
    for shape in ["units", "summary-units", "questions"] {
        check(shape).await;
    }
}

async fn check(shape: &str) {
    let server = super::provider(if shape == "questions" { 1 } else { 12 }).await;
    let workspace = Workspace::new();
    let runtime = clasify_runtime(&workspace, &server, &[]);
    let budget = 500;
    let items: Vec<_> = (0..12)
        .map(|at| json!(format!("comment {at}")))
        .collect();
    let row = match shape {
        "units" => json!({"operation":"evaluate","view":"details","perItem":true,
            "content":items,"questions":{"keep":noul("The comment is marked to keep.")}}),
        "summary-units" => json!({"operation":"evaluate","view":"summary","perItem":true,
            "content":items,"questions":{"keep":noul("The comment is marked to keep.")}}),
        _ => {
            let questions: serde_json::Map<_, _> = (0..12)
                .map(|at| (format!("question{at:02}"), noul("Is it a comment?")))
                .collect();
            json!({"operation":"evaluate","view":"details","content":"comment","questions":questions})
        }
    };
    let mut page = runtime
        .run(
            "budget".into(),
            "clasify".into(),
            json!({"queries":[row],"responseChars":budget}),
        )
        .await
        .expect("first page")
        .structured_content
        .into_value();
    let mut pages = 0;
    let mut units = std::collections::BTreeSet::new();
    loop {
        assert!(pages < 40, "page chain must progress");
        pages += 1;
        let size = octocode_engine::json_size::utf16_units(&page).expect("size");
        assert!(
            size <= budget || page["oversized"] == true,
            "{shape} page {pages} is {size} chars over responseChars {budget} and does not say oversized: {page}"
        );
        for row in page["results"].as_array().into_iter().flatten() {
            for key in row["data"]["units"]
                .as_object()
                .into_iter()
                .flat_map(|units| units.keys())
            {
                units.insert(key.clone());
            }
        }
        let next = page["responsePagination"]["next"]["input"].clone();
        if next.is_null() {
            break;
        }
        page = runtime
            .run(format!("budget{pages}"), "clasify".into(), next)
            .await
            .expect("next page")
            .structured_content
            .into_value();
    }
    // A summary keeps only the units that said yes: none here, so it fits.
    assert!(shape == "summary-units" || shape == "questions" || pages > 1, "{shape}: a 12-unit row does not fit one {budget}-char page");
    if shape == "units" {
        assert_eq!(units.len(), 12, "every unit reaches the reader once");
    }
    runtime.close().await;
}

/// A file whose many hit answers are one indivisible member wider than
/// `responseChars` says `oversized` instead of passing as an ordinary page.
#[tokio::test]
async fn a_file_of_many_hit_answers_wider_than_response_chars_says_oversized() {
    let server = super::provider(1).await;
    let workspace = Workspace::new();
    let text: String = (0..40).map(|at| format!("needle {at}\n")).collect();
    workspace.write("dense.txt", text);
    let runtime = clasify_runtime(&workspace, &server, &[]);
    let budget = 500;
    let row = json!({"operation":"evaluate","view":"details",
        "source":{"tool":"localSearch","query":{"path":"dense.txt","matchString":["needle"]}},
        "questions":{"keep":noul("The line is marked to keep.")}});
    let mut page = runtime
        .run(
            "hits".into(),
            "clasify".into(),
            json!({"queries":[row],"responseChars":budget}),
        )
        .await
        .expect("first page")
        .structured_content
        .into_value();
    let mut lines = std::collections::BTreeSet::new();
    for pages in 1..40 {
        let size = octocode_engine::json_size::utf16_units(&page).expect("size");
        let within = size <= budget || page["oversized"] == true;
        assert!(within, "page {pages} is {size} chars and does not say oversized: {page}");
        for line in page["results"][0]["data"]["files"]["dense.txt"]
            .as_object()
            .into_iter()
            .flat_map(|lines| lines.keys())
        {
            assert!(lines.insert(line.clone()), "line {line} repeated");
        }
        let next = page["responsePagination"]["next"]["input"].clone();
        if next.is_null() {
            break;
        }
        page = runtime
            .run(format!("hits{pages}"), "clasify".into(), next)
            .await
            .expect("next page")
            .structured_content
            .into_value();
    }
    assert_eq!(lines.len(), 40, "every line answer is reached exactly once");
    runtime.close().await;
}
