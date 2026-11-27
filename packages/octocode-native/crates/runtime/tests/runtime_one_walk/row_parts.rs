//! A later part of a split row says what it continues only when it resumes a
//! value the part before it began: `rowPart.continues` names the array element
//! it resumes, `rowPart.text` the string. A part that begins on a whole element
//! of the row's outermost array carries neither, so a joiner appends its
//! elements; one that resumes a file but begins on a whole hit names the file.
use super::walk;
use crate::support::Workspace;
use octocode_test_support::json_fixture;
use serde_json::{Value, json};

/// The first or last hit of a part's files: `(path, line)`.
fn hit(files: &[Value], last: bool) -> Option<(String, u64)> {
    let file = if last { files.last()? } else { files.first()? };
    let matches = file["matches"].as_array()?;
    let found = if last {
        matches.last()?
    } else {
        matches.first()?
    };
    Some((file["path"].as_str()?.to_owned(), found["line"].as_u64()?))
}

/// Hits of every width: short ones a part holds whole, and long ones whose
/// value a 450-char page cuts.
fn hits(file: usize, lines: usize) -> String {
    (1..=lines)
        .map(|line| {
            format!(
                "needle file {file} line {line} {}\n",
                "padding ".repeat(if line % 5 == 0 { 300 } else { 2 })
            )
        })
        .collect()
}

#[tokio::test]
async fn a_later_part_names_a_continued_value_only_when_one_is_cut() {
    let workspace = Workspace::new();
    for file in 0..2 {
        workspace.write(&format!("docs/f{file}.txt"), hits(file, 40));
    }
    let runtime = workspace.runtime(&[]);
    let pages = walk(
        &runtime,
        "localSearch",
        json_fixture(
            r#"{
                "queries": [
                    {
                        "path": "docs",
                        "matchString": ["needle"],
                        "view": "content",
                        "limit": 60
                    }
                ],
                "responseChars": 450
            }"#,
        ),
    )
    .await;
    // The part before, and how many files the joined row holds so far.
    let mut before: Option<(Vec<Value>, usize)> = None;
    let (mut later, mut bare, mut cut_text) = (0, 0, 0);
    for page in &pages {
        for row in page["results"].as_array().into_iter().flatten() {
            let files = row["data"]["files"].as_array().cloned().unwrap_or_default();
            let marker = &row["rowPart"];
            let part = marker["part"].as_u64().unwrap_or(1);
            if part == 1 {
                before = Some((files.clone(), files.len()));
                continue;
            }
            later += 1;
            let (previous, joined) = before.take().expect("a later part follows its first");
            let keys = marker.as_object().expect("rowPart").keys();
            for key in keys {
                assert!(
                    ["part", "of", "continues", "text"].contains(&key.as_str()),
                    "unknown rowPart key {key}: {marker}"
                );
            }
            let first_path = files.first().and_then(|file| file["path"].as_str());
            let last_path = previous.last().and_then(|file| file["path"].as_str());
            let resumes_file = first_path.is_some() && first_path == last_path;
            let resumes_hit = resumes_file && hit(&files, false) == hit(&previous, true);
            let continues = marker.get("continues");
            assert_eq!(
                continues.is_some(),
                resumes_file,
                "continues is stated exactly when the part resumes the file before it: {marker}"
            );
            assert_eq!(
                marker.get("text").is_some(),
                resumes_hit,
                "text is stated exactly when the part resumes a hit's value: {marker}"
            );
            if let Some(continues) = continues {
                let files_at = joined - 1;
                // A cut hit is the innermost element: its file's hits array
                // (its index counts the joined file's hits, which one part
                // does not show); else the file itself, the joined row's last.
                let (array, index) = if resumes_hit {
                    (format!("/data/files/{files_at}/matches"), None)
                } else {
                    ("/data/files".to_owned(), Some(files_at))
                };
                assert_eq!(continues["array"], json!(array), "{marker}");
                if let Some(index) = index {
                    assert_eq!(continues["index"], json!(index), "{marker}");
                }
            }
            if resumes_hit {
                cut_text += 1;
            }
            if continues.is_none() {
                bare += 1;
            }
            let joined = joined + files.len() - usize::from(resumes_file);
            before = Some((files, joined));
        }
    }
    assert!(
        later > 0 && bare > 0 && cut_text > 0,
        "{later} later parts, {bare} bare, {cut_text} cut values"
    );
    runtime.close().await;
}
