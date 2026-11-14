//! A search kept to the lines a scope names in each file.
use super::*;
use crate::types::{LineScope, SpanValues};

/// A scope that keeps lines 2-3 of `*.json` files and holds no other file;
/// every file whose spans it read is recorded.
fn json_lines_two_to_three(read: &std::sync::Arc<std::sync::Mutex<Vec<String>>>) -> LineScope {
    let read = std::sync::Arc::clone(read);
    LineScope {
        holds: std::sync::Arc::new(|path: &Path| {
            path.extension()
                .is_some_and(|extension| extension == "json")
        }),
        spans: std::sync::Arc::new(move |path: &Path| {
            read.lock().expect("lock").push(
                path.file_name()
                    .expect("name")
                    .to_string_lossy()
                    .into_owned(),
            );
            std::sync::Arc::new(vec![[2, 3]])
        }),
    }
}

/// Every view keeps exactly the hits on scoped lines: hits elsewhere in a
/// held file and every hit of a file the scope cannot hold are left out, and
/// only a held file with a hit has its spans read.
#[test]
fn every_view_keeps_only_scoped_lines() {
    let t = TmpDir::new();
    t.write("a.json", "hit\nhit hit\nhit\nhit\n");
    t.write("b.txt", "hit\nhit\nhit\n");
    t.write("c.json", "none\n");
    for output in [
        SearchOutput::Lines,
        SearchOutput::Spans(SpanValues::Every),
        SearchOutput::Spans(SpanValues::Counted),
        SearchOutput::CountMatches,
        SearchOutput::CountLines,
        SearchOutput::FilesWithMatch,
    ] {
        let read = std::sync::Arc::default();
        let mut options = opts(t.path(), "hit");
        options.output = output;
        options.line_scope = Some(json_lines_two_to_three(&read));
        let result = search(options).expect("search");
        let names = result
            .files
            .iter()
            .map(|file| {
                Path::new(&file.path)
                    .file_name()
                    .expect("name")
                    .to_string_lossy()
                    .into_owned()
            })
            .collect::<Vec<_>>();
        assert_eq!(names, ["a.json"], "{output:?}");
        let file = &result.files[0];
        assert!(
            file.matches.iter().all(|hit| (2..=3).contains(&hit.line)),
            "{output:?}: {:?}",
            file.matches
        );
        // Lines 2-3 hold three hits on two lines.
        let shown = file
            .matches
            .iter()
            .map(|hit| hit.count.unwrap_or(1))
            .sum::<u32>();
        match output {
            SearchOutput::CountMatches => assert_eq!(file.match_count, 3, "{output:?}"),
            SearchOutput::CountLines => assert_eq!(file.match_count, 2, "{output:?}"),
            SearchOutput::Spans(_) => assert_eq!(shown, 3, "{output:?}"),
            SearchOutput::Lines => assert_eq!(shown, 2, "{output:?}"),
            _ => {}
        }
        assert_eq!(*read.lock().expect("lock"), ["a.json"], "{output:?}");
    }
}

/// A files-without view lists a held file whose scoped lines lack the term,
/// never a file the scope cannot hold.
#[test]
fn files_without_lists_held_files_whose_scope_lacks_a_hit() {
    let t = TmpDir::new();
    t.write("a.json", "hit\nnone\nnone\nhit\n");
    t.write("b.json", "none\nhit\n");
    t.write("c.txt", "none\n");
    let read = std::sync::Arc::default();
    let mut options = opts(t.path(), "hit");
    options.output = SearchOutput::FilesWithoutMatch;
    options.line_scope = Some(json_lines_two_to_three(&read));
    let result = search(options).expect("search");
    let names = result
        .files
        .iter()
        .map(|file| Path::new(&file.path).file_name().expect("name").to_owned())
        .collect::<Vec<_>>();
    assert_eq!(names, ["a.json"], "{result:?}");
}
