use super::*;
use octocode_engine::signatures::languages::SCRIPT_BEARING_MARKUP;

#[test]
fn feedback_arcs_break_every_cycle() {
    // 0→1→2→0 and 1→0: removing the returned arcs must leave a DAG.
    let adj = |id: u32| match id {
        0 => vec![1],
        1 => vec![2, 0],
        2 => vec![0],
        _ => vec![],
    };
    let cuts = feedback_arcs(&[0, 1, 2], &adj);
    assert!(!cuts.is_empty() && cuts.len() <= 2, "{cuts:?}");
    let pruned = |id: u32| {
        adj(id)
            .into_iter()
            .filter(|to| !cuts.contains(&(id, *to)))
            .collect::<Vec<_>>()
    };
    assert!(strongly_connected(&[0, 1, 2], &pruned, 3).is_empty());
}

#[test]
fn witness_is_the_shortest_cycle_through_the_start() {
    let adj = |id: u32| match id {
        0 => vec![1, 3],
        1 => vec![2],
        2 => vec![0],
        3 => vec![0],
        _ => vec![],
    };
    let members = BTreeSet::from([0, 1, 2, 3]);
    assert_eq!(witness_cycle(0, &members, &adj), vec![0, 3, 0]);
}

/// A name imported by a component's script block (every extension of
/// [`SCRIPT_BEARING_MARKUP`]) is a reference, as in source code; the same
/// bare word in prose stays a word.
#[test]
fn script_bearing_markup_names_are_references() {
    let dir = tempfile::tempdir().expect("dir");
    let import = "<script>\nimport { run } from './cmd'\n</script>\n";
    let markup = SCRIPT_BEARING_MARKUP
        .iter()
        .map(|ext| format!("component.{ext}"))
        .collect::<Vec<_>>();
    for name in &markup {
        std::fs::write(dir.path().join(name), import).expect("write");
    }
    std::fs::write(dir.path().join("README.md"), "Call run to start.\n").expect("write");
    let (hits, gaps) = scan_mentions(
        dir.path(),
        &BTreeSet::new(),
        &BTreeSet::from(["run".to_owned()]),
    );
    assert_eq!(gaps, MentionGaps::default());
    let mut expected = markup
        .iter()
        .map(|name| format!("{name}:2"))
        .collect::<Vec<_>>();
    expected.sort();
    assert_eq!(hits.references.get("run"), Some(&expected));
    assert_eq!(hits.words.get("run"), Some(&vec!["README.md:1".to_owned()]));
}

/// The engine and this crate each name script-bearing markup through
/// [`SCRIPT_BEARING_MARKUP`]: a line that lists all but one of its extensions
/// is a handpicked copy that drifts from it. Only the definition lists them.
#[test]
fn no_source_keeps_its_own_list_of_script_bearing_markup() {
    fn sources(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
        for entry in std::fs::read_dir(dir).expect("read dir") {
            let path = entry.expect("entry").path();
            if path.is_dir() {
                sources(&path, out);
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                out.push(path);
            }
        }
    }
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut files = Vec::new();
    sources(&manifest.join("src"), &mut files);
    sources(&manifest.join("../engine/src"), &mut files);
    assert!(files.len() > 100, "{} sources found", files.len());
    let literals = SCRIPT_BEARING_MARKUP
        .iter()
        .map(|ext| format!("\"{ext}\""))
        .collect::<Vec<_>>();
    let mut copies = Vec::new();
    for file in files {
        let text = std::fs::read_to_string(&file).expect("read source");
        for (index, line) in text.lines().enumerate() {
            let listed = literals.iter().filter(|lit| line.contains(*lit)).count();
            if listed + 1 >= literals.len() && !line.contains("pub const SCRIPT_BEARING_MARKUP") {
                copies.push(format!("{}:{}", file.display(), index + 1));
            }
        }
    }
    assert!(
        copies.is_empty(),
        "derive these from octocode_engine::signatures::languages::SCRIPT_BEARING_MARKUP: {copies:?}"
    );
}

/// The scan skips vendored directories, so a vendored copy that mentions every
/// name corroborates nothing; a directory the graph builds from it reads.
#[test]
fn the_mention_scan_skips_vendored_dirs_and_reads_what_the_graph_builds_from() {
    let dir = tempfile::tempdir().expect("dir");
    for name in crate::tools::ast_graph::classify::VENDORED_DIRS {
        std::fs::create_dir_all(dir.path().join(name)).expect("dir");
        std::fs::write(dir.path().join(name).join("copy.ts"), "run();\n").expect("write");
    }
    for name in ["src", "obj"] {
        std::fs::create_dir(dir.path().join(name)).expect("dir");
        std::fs::write(dir.path().join(name).join("app.ts"), "run();\n").expect("write");
    }
    let (hits, _) = scan_mentions(
        dir.path(),
        &BTreeSet::new(),
        &BTreeSet::from(["run".to_owned()]),
    );
    assert_eq!(
        hits.references.get("run"),
        Some(&vec!["obj/app.ts:1".to_owned(), "src/app.ts:1".to_owned()])
    );
}

#[test]
fn magnitude_is_bounded() {
    assert!((magnitude(0) - 0.6).abs() < 1e-9);
    assert!(magnitude(1_000_000) <= 1.0);
}

/// A file too long for the mention scan is counted: "no mention" must not
/// read as "no file names it".
#[test]
fn the_mention_scan_counts_the_files_it_did_not_read() {
    let dir = tempfile::tempdir().expect("tempdir");
    let long = usize::try_from(MENTION_MAX_FILE).expect("fits") + 1;
    std::fs::write(
        dir.path().join("big.txt"),
        format!("run {}", "x".repeat(long)),
    )
    .expect("write");
    let (hits, gaps) = scan_mentions(
        dir.path(),
        &BTreeSet::new(),
        &BTreeSet::from(["run".to_owned()]),
    );
    assert_eq!(gaps.oversized, 1);
    assert_eq!(hits.references.get("run"), None);
}
