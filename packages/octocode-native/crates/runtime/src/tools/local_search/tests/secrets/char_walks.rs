//! A walk of short matchOnly spans over a line with a secret: the walk runs
//! over the line a read serves, so no span holds a byte of the secret, even
//! when its text occurs elsewhere on the line, and the rest of the line comes
//! back once, exactly. Every value below is fake.
use super::*;

const TOKEN: &str = "ghp_Z9Q8W7E6R5T4Y3U2I1O0P9L8K7J6H5G4F3D2";

/// `x <token> y`, Unicode, then `repeated`: text that holds again what the
/// walk's spans inside the token show.
fn line(repeated: &str) -> String {
    format!("x {TOKEN} y ü·é {repeated}")
}

/// Every shown row of a walk of `pattern` over `line`, as (column, value), in
/// column order.
fn walked(line: &str, pattern: &str) -> Vec<(u64, String)> {
    let root = tempfile::tempdir().expect("fixture directory");
    fs::write(root.path().join("one.txt"), format!("{line}\n")).expect("fixture");
    let (walk, pages) = walk_search(
        root.path(),
        serde_json::json!({
            "path": root.path(), "matchString": [pattern], "regexEngine": "rust",
            "view": "matchOnly"
        }),
        Some(900),
    );
    assert!(pages.len() > 1, "the spans cross page edges");
    let mut rows: Vec<(u64, String)> = pages
        .iter()
        .flat_map(|page| page["files"].as_array().cloned().unwrap_or_default())
        .flat_map(|file| file["matches"].as_array().cloned().unwrap_or_default())
        .map(|row| {
            (
                row["column"].as_u64().expect("column"),
                row["value"].as_str().expect("value").to_owned(),
            )
        })
        .collect();
    assert_eq!(rows.len(), walk.rows.len(), "each row once");
    rows.sort_by_key(|(column, _)| *column);
    rows
}

/// `value` without its redaction placeholders (a pattern's, or a span's bare
/// one): the source text it shows.
fn unmasked(value: &str) -> String {
    octocode_engine::security::placeholder_regex()
        .replace_all(value, "")
        .replace("[REDACTED]", "")
}

fn assert_no_token_fragment(shown: &str) {
    let chars: Vec<char> = TOKEN.chars().collect();
    for pair in chars.windows(2) {
        let fragment: String = pair.iter().collect();
        assert!(
            !shown.contains(&fragment),
            "token fragment {fragment:?} shown: {shown}"
        );
    }
}

/// The walk of `.{width}` over `line`: each span's byte range in it.
fn spans(line: &str, width: usize) -> Vec<std::ops::Range<usize>> {
    let starts: Vec<usize> = line
        .char_indices()
        .map(|(at, _)| at)
        .chain([line.len()])
        .collect();
    (0..(starts.len() - 1) / width)
        .map(|k| starts[k * width]..starts[(k + 1) * width])
        .collect()
}

/// `line` as a read serves it: the token is its placeholder.
fn served(line: &str) -> String {
    let security = crate::security::ContentSecurity::new();
    let served = security
        .sanitize_text(line, Some(std::path::Path::new("one.txt")))
        .content
        .into_owned();
    assert!(!served.contains(TOKEN) && served != line, "{served}");
    served
}

/// Walk `.{width}` over a line whose other text repeats `repeated`: the walk
/// runs over the line a read serves, so its rows are that line's spans, one
/// per span; a span that is not the placeholder's shows its text exactly,
/// and no row shows any part of the token.
fn check_walk(width: usize, repeated: &str) {
    let line = line(repeated);
    let served = served(&line);
    let pattern = if width == 1 {
        ".".to_owned()
    } else {
        format!(".{{{width}}}")
    };
    let rows = walked(&line, &pattern);
    let spans = spans(&served, width);
    assert_eq!(
        rows.len(),
        spans.len(),
        "one row per span of the read's line"
    );
    let at = served.find("[REDACTED").expect("placeholder");
    let placeholder = at..at + served[at..].find(']').expect("closed") + 1;
    for ((column, value), span) in rows.iter().zip(&spans) {
        if span.start >= placeholder.end || span.end <= placeholder.start {
            assert_eq!(value, &served[span.clone()], "column {column}");
        } else {
            assert_no_token_fragment(value);
        }
    }
}

/// One character per span: none of the secret's characters comes back, though
/// each also occurs raw elsewhere on the line; joined, the walk shows no two
/// of the secret's characters in a row.
#[test]
fn a_one_character_walk_masks_every_character_of_a_secret() {
    let repeated = TOKEN.chars().map(|c| format!("{c} ")).collect::<String>();
    check_walk(1, &repeated);
    let rows = walked(&line(&repeated), ".");
    let shown: String = rows.iter().map(|(_, value)| unmasked(value)).collect();
    assert_no_token_fragment(&shown);
}

/// Five characters per span, where the line repeats each five-character span
/// the walk takes inside the secret: none of them comes back.
#[test]
fn a_five_character_walk_shows_no_part_of_a_secret() {
    let bare = line("");
    let token = bare.find(TOKEN).expect("token");
    let repeated: String = spans(&bare, 5)
        .into_iter()
        .filter(|span| span.start >= token && span.end <= token + TOKEN.len())
        .map(|span| format!("{} ", &bare[span]))
        .collect();
    assert!(!repeated.is_empty());
    check_walk(5, &repeated);
}
