//! The byte tokenizer against the scan it replaced, and the mention memo.
use super::*;
use std::sync::Arc;

/// The tokenizer the byte scan replaced, word for word: one line at a time,
/// a `char` at a time. The scan must find exactly what this finds.
fn reference_scan<'k>(
    text: &str,
    code: bool,
    stems: &'k BTreeSet<String>,
    names: &'k BTreeSet<String>,
) -> [BTreeMap<&'k str, usize>; 3] {
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
    let key = |set: &'k BTreeSet<String>, text: &str| set.get(text).map(String::as_str);
    let mut local: [BTreeMap<&str, usize>; 3] = Default::default();
    for (index, line) in text.lines().enumerate() {
        for (offset, token) in tokens(line) {
            if token.contains(['/', '.']) {
                let last = token.rsplit('/').next().unwrap_or(token);
                let stem = last.split('.').next().unwrap_or(last);
                if let Some(stem) = key(stems, stem) {
                    local[0].entry(stem).or_insert(index + 1);
                }
            } else if let Some(stem) = key(stems, token)
                && is_quoted(line, offset, token)
            {
                local[0].entry(stem).or_insert(index + 1);
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
                let kind = if code || qualified { 1 } else { 2 };
                local[kind].entry(name).or_insert(index + 1);
            }
        }
    }
    local
}

fn set(items: &[&str]) -> BTreeSet<String> {
    items.iter().map(|item| (*item).to_owned()).collect()
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
    sources(&manifest.join("src/tools/ast_graph"), &mut files);
    sources(&manifest.join("src/contracts"), &mut files);
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
    let references = |scan: &(MentionHits, MentionGaps)| scan.0.references.get("run").cloned();

    settle();
    let first = memoized_mentions(root, &stems, &names);
    assert_eq!(references(&first), Some(vec!["a.ts:1".to_owned()]));
    let again = memoized_mentions(root, &stems, &names);
    assert!(Arc::ptr_eq(&first, &again), "an unchanged tree is one scan");

    let other_keys = memoized_mentions(root, &stems, &set(&["run", "nothing"]));
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
    let edited = memoized_mentions(root, &stems, &names);
    assert!(!Arc::ptr_eq(&first, &edited));
    assert_eq!(edited.0.words.get("run"), Some(&vec!["b.md:1".to_owned()]));

    write("c.ts", "run();\n");
    settle();
    let added = memoized_mentions(root, &stems, &names);
    assert_eq!(
        references(&added),
        Some(vec!["a.ts:1".to_owned(), "c.ts:1".to_owned()])
    );

    write(".gitignore", "c.ts\n");
    std::fs::create_dir_all(root.join(".git")).expect("git marker");
    settle();
    let ignored = memoized_mentions(root, &stems, &names);
    assert_eq!(references(&ignored), Some(vec!["a.ts:1".to_owned()]));

    std::fs::remove_file(root.join("a.ts")).expect("remove");
    settle();
    let removed = memoized_mentions(root, &stems, &names);
    assert_eq!(references(&removed), None);
}

/// A file written inside the settle window has a stamp a later write in the
/// same tick would not move: its scan is not kept.
#[test]
fn the_mention_memo_keeps_no_unsettled_scan() {
    let dir = tempfile::tempdir().expect("dir");
    std::fs::write(dir.path().join("a.ts"), "run();\n").expect("write");
    let names = set(&["run"]);
    let stems = BTreeSet::new();
    let first = memoized_mentions(dir.path(), &stems, &names);
    let second = memoized_mentions(dir.path(), &stems, &names);
    assert!(!Arc::ptr_eq(&first, &second));
    assert_eq!(
        first.0.references.get("run"),
        second.0.references.get("run")
    );
}

/// The gaps travel with the kept scan: a kept answer still says what went
/// unread.
#[test]
fn a_kept_mention_scan_keeps_its_gaps() {
    let dir = tempfile::tempdir().expect("dir");
    let long = usize::try_from(MENTION_MAX_FILE).expect("fits") + 1;
    std::fs::write(dir.path().join("big.txt"), "x".repeat(long)).expect("write");
    octocode_engine::stamp::settle_written_so_far();
    let names = set(&["run"]);
    let stems = BTreeSet::new();
    let first = memoized_mentions(dir.path(), &stems, &names);
    let second = memoized_mentions(dir.path(), &stems, &names);
    assert!(Arc::ptr_eq(&first, &second));
    assert_eq!(second.1.oversized, 1);
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
    let (hits, gaps) = scan_mentions(dir.path(), &BTreeSet::new(), &set(&["zzOnlyTextMention"]));
    assert_eq!(gaps, MentionGaps::default());
    assert_eq!(
        hits.references.get("zzOnlyTextMention"),
        Some(&vec!["zz_late.ts:1".to_owned()])
    );
}
