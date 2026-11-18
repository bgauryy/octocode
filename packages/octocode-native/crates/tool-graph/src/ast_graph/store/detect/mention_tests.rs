//! The byte tokenizer against the scan it replaced, and the mention memo.
use super::*;
use octocode_engine::search::walk::GlobalIgnore;
use std::sync::Arc;

/// The tokenizer the byte scan replaced, word for word: one line at a time,
/// a `char` at a time. The scan must find exactly what this finds.
fn reference_scan<'k>(
    text: &str,
    code: bool,
    stems: &'k BTreeSet<String>,
    names: &'k BTreeSet<String>,
) -> Found<'k> {
    fn tokens(line: &str) -> impl Iterator<Item = (usize, &str)> {
        let mut start = None;
        line.char_indices()
            .chain(std::iter::once((line.len(), ' ')))
            .filter_map(move |(at, c)| {
                if c.is_alphanumeric() || "_$./@-".contains(c) {
                    start.get_or_insert(at);
                    None
                } else {
                    start.take().map(|from| (from, &line[from..at]))
                }
            })
    }
    fn is_quoted(line: &str, offset: usize, token: &str) -> bool {
        let before = line[..offset].chars().next_back();
        let after = line[offset + token.len()..].chars().next();
        matches!(before, Some('\'' | '"' | '`')) && before == after
    }
    // A path names its file's directory unless that is `.`, `..`, none or a
    // build output's.
    fn path_key(token: &str) -> String {
        let segments = token.split('/').collect::<Vec<_>>();
        let last = segments[segments.len() - 1];
        let stem = last.split('.').next().unwrap_or(last);
        match segments.len().checked_sub(2).map(|at| segments[at]) {
            Some(dir)
                if !["", ".", ".."].contains(&dir)
                    && !crate::ast_graph::store::workspace::BUILD_OUTPUT_DIRS.contains(&dir) =>
            {
                format!("{dir}/{stem}")
            }
            _ => stem.to_owned(),
        }
    }
    let key = |set: &'k BTreeSet<String>, text: &str| set.get(text).map(String::as_str);
    let mut local: Found<'k> = Default::default();
    for (index, line) in text.lines().enumerate() {
        for (offset, token) in tokens(line) {
            let named = token.trim_end_matches('.');
            if named.contains(['/', '.']) {
                if let Some(stem) = key(stems, &path_key(named)) {
                    local[STEMS].entry(stem).or_insert(index + 1);
                }
            } else if let Some(stem) = key(stems, named)
                && is_quoted(line, offset, named)
            {
                local[STEMS].entry(stem).or_insert(index + 1);
            }
            let mut at = 0;
            for part in token.split(['.', '/', '-', '@']) {
                let start = at;
                at += part.len() + 1;
                let Some(name) = key(names, part) else {
                    continue;
                };
                let qualified = token[..start].strip_suffix(['.', '/']).is_some_and(|head| {
                    head.ends_with(|c: char| c.is_alphanumeric() || "_$".contains(c))
                });
                let kind = if qualified {
                    QUALIFIED
                } else if code {
                    BARE_CODE
                } else {
                    WORDS
                };
                local[kind].entry(name).or_insert(index + 1);
            }
        }
    }
    local
}

fn set(items: &[&str]) -> BTreeSet<String> {
    items.iter().map(|item| (*item).to_owned()).collect()
}

/// The code references of `name` (qualified anywhere, bare in code), in
/// path order.
fn references(hits: &MentionHits, name: &str) -> Option<Vec<String>> {
    let mut all = [&hits.qualified, &hits.bare]
        .into_iter()
        .filter_map(|kind| kind.get(name))
        .flatten()
        .cloned()
        .collect::<Vec<_>>();
    all.sort_by(|a, b| {
        a.rsplit_once(':')
            .map(|(f, _)| f)
            .cmp(&b.rsplit_once(':').map(|(f, _)| f))
    });
    (!all.is_empty()).then_some(all)
}

/// A path names the directory it is in: `./elsewhere/index` hits only the
/// key `elsewhere/index`, while `./index`, `../index`, `index.ts` and a
/// build-output path (`dist/index.js`) hit the bare stem.
#[test]
fn a_path_token_keys_the_directory_it_names() {
    let stems = set(&["index", "elsewhere/index", "legacy/index"]);
    let names = BTreeSet::new();
    let keys = Keys::new(&stems, &names);
    let hits = |text: &str| {
        scan_text(text, true, &keys)[STEMS]
            .keys()
            .map(|key| (*key).to_owned())
            .collect::<Vec<_>>()
    };
    assert_eq!(hits("x = \"./elsewhere/index\""), ["elsewhere/index"]);
    assert_eq!(hits("x = 'src/legacy/index.ts'"), ["legacy/index"]);
    for parentless in [
        "'./index'",
        "'../index'",
        "'index.ts'",
        "'dist/index.js'",
        "'../dist/index.js'",
    ] {
        assert_eq!(hits(parentless), ["index"], "{parentless}");
    }
}

/// A file is named by its name with an extension or by a path: a stem as a
/// word of prose is no mention, a sentence's closing dot included ("the
/// test oracle.").
#[test]
fn a_word_ending_a_sentence_names_no_file() {
    let stems = set(&["oracle"]);
    let names = BTreeSet::new();
    let keys = Keys::new(&stems, &names);
    for code in [false, true] {
        let hits = |text: &str| scan_text(text, code, &keys)[STEMS].len();
        for word in [
            "/// The whole snapshot decoded at once: the test oracle.",
            "ask the oracle... then",
            "oracle",
        ] {
            assert_eq!(hits(word), 0, "{word}");
        }
        for named in [
            "see oracle.py.",
            "../oracle.py",
            "import ./oracle",
            "'oracle'",
        ] {
            assert_eq!(hits(named), 1, "{named}");
        }
    }
}

/// The edges of the old line-and-char scan: CRLF and lone CR, Unicode letters
/// inside and beside tokens, a quote at each end of a line, an empty stem, a
/// long key, qualification through a non-ASCII head.
#[test]
fn the_byte_scan_finds_what_the_char_scan_found_on_edge_text() {
    let long = "k".repeat(70);
    let stems = set(&["", "worker", "tool", "é", "日本", long.as_str()]);
    let names = set(&["run", "é", "ñame", "x", "a-b", long.as_str()]);
    let long_line = format!("mod.{long} './{long}.ts' '{long}'");
    for text in [
        "",
        "\n\n",
        "run",
        "run\n",
        "\r\nrun\r\nx.run\r\n",
        "a\rrun\rb.run",
        "'worker'\n'worker\nworker'\n\"worker\"`worker`'worker\"",
        "./.hidden .x / ./ . .. ../tool.ts src/tool.d.ts tool. /tool",
        "é.run é/run ñame é-ñame x@run @run -run run- .run /run a-b a.b-x",
        "日本 '日本' 日本.run ٣run run٣ \u{a0}run\u{a0} \u{2028}run",
        "«run» “worker” 'é' ‘run’ run… …run",
        "$.run _.run $run _run run$ 9.run",
        long_line.as_str(),
    ] {
        let keys = Keys::new(&stems, &names);
        for code in [false, true] {
            assert_eq!(
                scan_text(text, code, &keys),
                reference_scan(text, code, &stems, &names),
                "{text:?}"
            );
        }
    }
}

/// The same over this crate's real source, with its file stems and a broad
/// set of names, short ones too.
#[test]
fn the_byte_scan_finds_what_the_char_scan_found_on_real_source() {
    fn sources(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
        for entry in std::fs::read_dir(dir).expect("read dir") {
            let path = entry.expect("entry").path();
            if path.is_dir() {
                sources(&path, out);
            } else {
                out.push(path);
            }
        }
    }
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut files = Vec::new();
    sources(&manifest.join("src/ast_graph"), &mut files);
    sources(&manifest.join("../runtime-base/src/contracts"), &mut files);
    assert!(files.len() > 100, "{} sources found", files.len());
    let stems = files
        .iter()
        .map(|file| file_stem(&file.to_string_lossy()))
        .collect::<BTreeSet<_>>();
    let names = set(&[
        "run",
        "self",
        "fn",
        "use",
        "new",
        "Value",
        "path",
        "x",
        "a",
        "i",
        "main",
        "json",
        "tests",
        "file",
        "root",
        "scan_mentions",
        "é",
    ]);
    let keys = Keys::new(&stems, &names);
    let mut hits = 0;
    for file in files {
        let text = String::from_utf8_lossy(&std::fs::read(&file).expect("read")).into_owned();
        let code = file.extension().is_some_and(|ext| ext == "rs");
        let scanned = scan_text(&text, code, &keys);
        assert_eq!(
            scanned,
            reference_scan(&text, code, &stems, &names),
            "{}",
            file.display()
        );
        hits += scanned.iter().map(BTreeMap::len).sum::<usize>();
    }
    assert!(hits > 1000, "{hits} hits");
}

/// The memo holds while every listed file's stamp holds, and misses on
/// anything that could change an answer: an edit that keeps size and
/// modification time, an added or removed file, a `.gitignore` change, a
/// different key.
#[test]
fn the_mention_memo_covers_every_file_it_read() {
    let dir = tempfile::tempdir().expect("dir");
    let root = dir.path();
    let write = |name: &str, text: &str| std::fs::write(root.join(name), text).expect("write");
    write("a.ts", "run();\n");
    write("b.md", "nothing here\n");
    let names = set(&["run"]);
    let stems = BTreeSet::new();
    let settle = octocode_engine::stamp::settle_written_so_far;
    let run = |scan: &MentionHits| references(scan, "run");

    settle();
    let first =
        memoized_mentions(root, GlobalIgnore::NONE, &stems, &names, &|| false).expect("scan");
    assert_eq!(run(&first), Some(vec!["a.ts:1".to_owned()]));
    let again =
        memoized_mentions(root, GlobalIgnore::NONE, &stems, &names, &|| false).expect("scan");
    assert!(Arc::ptr_eq(&first, &again), "an unchanged tree is one scan");

    let other_keys = memoized_mentions(
        root,
        GlobalIgnore::NONE,
        &stems,
        &set(&["run", "nothing"]),
        &|| false,
    )
    .expect("scan");
    assert!(!Arc::ptr_eq(&first, &other_keys), "the keys are in the key");

    // Same size, modification time restored: only the status time moves.
    let path = root.join("b.md");
    let modified = std::fs::metadata(&path)
        .expect("meta")
        .modified()
        .expect("mtime");
    write("b.md", "nothing run!\n");
    std::fs::File::options()
        .write(true)
        .open(&path)
        .expect("open")
        .set_modified(modified)
        .expect("restore mtime");
    settle();
    let edited =
        memoized_mentions(root, GlobalIgnore::NONE, &stems, &names, &|| false).expect("scan");
    assert!(!Arc::ptr_eq(&first, &edited));
    assert_eq!(edited.words.get("run"), Some(&vec!["b.md:1".to_owned()]));

    write("c.ts", "run();\n");
    settle();
    let added =
        memoized_mentions(root, GlobalIgnore::NONE, &stems, &names, &|| false).expect("scan");
    assert_eq!(
        run(&added),
        Some(vec!["a.ts:1".to_owned(), "c.ts:1".to_owned()])
    );

    write(".gitignore", "c.ts\n");
    std::fs::create_dir_all(root.join(".git")).expect("git marker");
    settle();
    let ignored =
        memoized_mentions(root, GlobalIgnore::NONE, &stems, &names, &|| false).expect("scan");
    assert_eq!(run(&ignored), Some(vec!["a.ts:1".to_owned()]));

    std::fs::remove_file(root.join("a.ts")).expect("remove");
    settle();
    let removed =
        memoized_mentions(root, GlobalIgnore::NONE, &stems, &names, &|| false).expect("scan");
    assert_eq!(run(&removed), None);
}

/// A file written inside the settle window has a stamp a later write in the
/// same tick would not move: its scan is not kept.
#[test]
fn the_mention_memo_keeps_no_unsettled_scan() {
    let dir = tempfile::tempdir().expect("dir");
    std::fs::write(dir.path().join("a.ts"), "run();\n").expect("write");
    let names = set(&["run"]);
    let stems = BTreeSet::new();
    let first =
        memoized_mentions(dir.path(), GlobalIgnore::NONE, &stems, &names, &|| false).expect("scan");
    let second =
        memoized_mentions(dir.path(), GlobalIgnore::NONE, &stems, &names, &|| false).expect("scan");
    assert!(!Arc::ptr_eq(&first, &second));
    assert_eq!(references(&first, "run"), references(&second, "run"));
}

/// The scan reads past any amount of earlier text: a name in a file that sorts
/// after half a gigabyte of data is a mention like any other. The data is sparse,
/// so the test holds no disk.
#[test]
fn a_mention_after_much_text_is_still_found() {
    let dir = tempfile::tempdir().expect("tempdir");
    let each = MENTION_MAX_FILE - 1;
    let files = (512_u64 << 20) / each + 2;
    for index in 0..files {
        let file =
            std::fs::File::create(dir.path().join(format!("a_{index:04}.bin"))).expect("data");
        file.set_len(each).expect("size");
    }
    std::fs::write(dir.path().join("zz_late.ts"), "mod.zzOnlyTextMention();\n").expect("late");
    let hits = scan_mentions(
        dir.path(),
        &BTreeSet::new(),
        &set(&["zzOnlyTextMention"]),
        &|| false,
    )
    .expect("scan");
    assert_eq!(
        hits.qualified.get("zzOnlyTextMention"),
        Some(&vec!["zz_late.ts:1".to_owned()])
    );
}

/// A file of any size is read: a mention past the whole-read size, past
/// several read windows, in a file with no line break, or at the end of a
/// token longer than a window, is a mention like any other, at its own line.
#[test]
fn a_mention_in_a_large_file_is_found_at_its_line() {
    let dir = tempfile::tempdir().expect("tempdir");
    let past = 3 * usize::try_from(MENTION_MAX_FILE).expect("fits");
    let lines = past / "word é word\n".len();
    std::fs::write(
        dir.path().join("lines.txt"),
        format!("{}see mod.run here\n", "word é word\n".repeat(lines)),
    )
    .expect("lines");
    std::fs::write(
        dir.path().join("one_line.js"),
        format!("{}run();", "x ".repeat(past / 2)),
    )
    .expect("one line");
    std::fs::write(
        dir.path().join("token.txt"),
        format!("{}.run\n", "a".repeat(past)),
    )
    .expect("token");
    let scanned =
        scan_mentions(dir.path(), &BTreeSet::new(), &set(&["run"]), &|| false).expect("scan");
    assert_eq!(
        references(&scanned, "run"),
        Some(vec![
            format!("lines.txt:{}", lines + 1),
            "one_line.js:1".to_owned(),
            "token.txt:1".to_owned(),
        ])
    );
}

/// A quote around a token stays beside it across a read window's end: a bare
/// name as a whole string literal is a stem, however the window falls.
#[test]
fn a_quoted_stem_at_a_window_end_is_found() {
    let dir = tempfile::tempdir().expect("tempdir");
    let window = super::mention_stream::WINDOW;
    let pad = usize::try_from(MENTION_MAX_FILE).expect("fits");
    std::fs::write(
        dir.path().join("big.txt"),
        format!(
            "{}{}'worker' next\n",
            "\n".repeat(pad),
            " ".repeat(window - 1)
        ),
    )
    .expect("write");
    let scanned =
        scan_mentions(dir.path(), &set(&["worker"]), &BTreeSet::new(), &|| false).expect("scan");
    assert_eq!(
        scanned.stems.get("worker"),
        Some(&vec![format!("big.txt:{}", pad + 1)])
    );
}

/// A stopped scan is no answer and leaves nothing to reuse.
#[test]
fn a_stopped_mention_scan_keeps_nothing() {
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(dir.path().join("a.ts"), "run();\n").expect("write");
    octocode_engine::stamp::settle_written_so_far();
    let (stems, names) = (BTreeSet::new(), set(&["run"]));
    assert!(memoized_mentions(dir.path(), GlobalIgnore::NONE, &stems, &names, &|| true).is_none());
    let scanned =
        memoized_mentions(dir.path(), GlobalIgnore::NONE, &stems, &names, &|| false).expect("scan");
    assert_eq!(references(&scanned, "run"), Some(vec!["a.ts:1".to_owned()]));
}
