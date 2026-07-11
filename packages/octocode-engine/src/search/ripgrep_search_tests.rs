use super::*;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};

static COUNTER: AtomicU32 = AtomicU32::new(0);

/// Unique temp dir per test (no Date/rand needed): pid + atomic counter.
struct TmpDir(PathBuf);
impl TmpDir {
    fn new() -> Self {
        let id = COUNTER.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!("octo-rg-test-{}-{id}", std::process::id()));
        fs::create_dir_all(&dir).expect("create temp dir");
        TmpDir(dir)
    }
    fn write(&self, rel: &str, content: &str) {
        let p = self.0.join(rel);
        if let Some(parent) = p.parent() {
            fs::create_dir_all(parent).expect("create parent");
        }
        fs::write(p, content).expect("write file");
    }
    fn path(&self) -> String {
        self.0.to_string_lossy().into_owned()
    }
}
impl Drop for TmpDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn opts(path: String, pattern: &str) -> RipgrepSearchOptions {
    RipgrepSearchOptions {
        path,
        pattern: pattern.to_owned(),
        ..Default::default()
    }
}

#[test]
fn finds_matches_with_line_and_column() {
    let t = TmpDir::new();
    t.write("a.txt", "hello world\nno match here\nhello again\n");
    let r = search(opts(t.path(), "hello")).expect("search ok");
    assert_eq!(r.files.len(), 1);
    let f = &r.files[0];
    assert_eq!(f.match_count, 2);
    assert_eq!(f.matches[0].line, 1);
    assert_eq!(f.matches[0].column, 0);
    assert_eq!(f.matches[0].value, "hello world");
    assert_eq!(f.matches[1].line, 3);
    assert_eq!(r.stats.match_count, Some(2));
    assert_eq!(r.stats.files_matched, Some(1));
    assert!(r.stats.bytes_searched.unwrap_or_default() > 0);
    assert!(r
        .stats
        .search_time
        .as_deref()
        .is_some_and(|s| s.ends_with('s')));
}

#[test]
fn column_is_utf16_offset_on_multibyte_line() {
    let t = TmpDir::new();
    // 'b' is at byte 8 but UTF-16 index 7 ('é' is 2 bytes).
    t.write("u.txt", "café = bar\n");
    let r = search(opts(t.path(), "bar")).expect("ok");
    assert_eq!(r.files[0].matches[0].column, 7);
}

#[test]
fn smart_case_default_is_case_insensitive_for_lowercase() {
    let t = TmpDir::new();
    t.write("a.txt", "Hello\nhello\n");
    let r = search(opts(t.path(), "hello")).expect("ok");
    assert_eq!(r.files[0].match_count, 2);
}

#[test]
fn case_sensitive_flag_is_exact() {
    let t = TmpDir::new();
    t.write("a.txt", "Hello\nhello\n");
    let mut o = opts(t.path(), "hello");
    o.case_sensitive = Some(true);
    let r = search(o).expect("ok");
    assert_eq!(r.files[0].match_count, 1);
}

#[test]
fn fixed_string_treats_pattern_literally() {
    let t = TmpDir::new();
    t.write("a.txt", "a.b\naxb\n");
    let mut o = opts(t.path(), "a.b");
    o.fixed_string = Some(true);
    let r = search(o).expect("ok");
    // Literal "a.b" matches only line 1, not "axb".
    assert_eq!(r.files[0].match_count, 1);
    assert_eq!(r.files[0].matches[0].value, "a.b");
}

#[test]
fn perl_regex_supports_lookahead() {
    let t = TmpDir::new();
    t.write("a.txt", "foobar\nfoobaz\n");
    let mut o = opts(t.path(), "foo(?=bar)");
    o.perl_regex = Some(true);
    let r = search(o).expect("ok");
    assert_eq!(r.files[0].match_count, 1);
    assert_eq!(r.files[0].matches[0].line, 1);
}

#[test]
fn perl_regex_catastrophic_backtracking_terminates() {
    // `(a+)+$` against a long line of 'a' followed by a non-matching char is
    // the classic exponential-backtracking blowup. The JIT-stack cap makes
    // PCRE2 fail fast (or complete) rather than spinning; either way the call
    // must return. Run on a worker thread so a regression surfaces as a
    // timeout instead of hanging the whole suite.
    use std::sync::mpsc;
    use std::time::Duration;

    let t = TmpDir::new();
    t.write("a.txt", &format!("{}!\n", "a".repeat(5_000)));
    let path = t.path();
    let (tx, rx) = mpsc::channel();
    let handle = std::thread::spawn(move || {
        let mut o = opts(path, "(a+)+$");
        o.perl_regex = Some(true);
        // Result intentionally ignored: PCRE2 may return Ok (no match) or an
        // Err (JIT stack / match limit hit) — the assertion is termination.
        let _ = search(o);
        let _ = tx.send(());
    });
    assert!(
        rx.recv_timeout(Duration::from_secs(30)).is_ok(),
        "catastrophic PCRE2 pattern must terminate, not hang"
    );
    handle.join().expect("worker thread panicked");
}

#[test]
fn files_only_lists_paths_without_snippets() {
    let t = TmpDir::new();
    t.write("a.txt", "needle\n");
    t.write("b.txt", "haystack\n");
    let mut o = opts(t.path(), "needle");
    o.files_only = Some(true);
    let r = search(o).expect("ok");
    assert_eq!(r.files.len(), 1);
    assert_eq!(r.files[0].match_count, 1);
    assert!(r.files[0].matches.is_empty());
    assert!(r.files[0].path.ends_with("a.txt"));
    assert_eq!(r.stats.files_matched, Some(1));
    assert!(r.stats.bytes_searched.unwrap_or_default() > 0);
}

#[test]
fn files_without_match_inverts_file_set() {
    let t = TmpDir::new();
    t.write("a.txt", "needle\n");
    t.write("b.txt", "haystack\n");
    let mut o = opts(t.path(), "needle");
    o.files_without_match = Some(true);
    let r = search(o).expect("ok");
    assert_eq!(r.files.len(), 1);
    assert!(r.files[0].path.ends_with("b.txt"));
}

#[test]
fn count_matches_counts_submatches_per_file() {
    let t = TmpDir::new();
    t.write("a.txt", "x x x\nx\n");
    let mut o = opts(t.path(), "x");
    o.count_matches_per_file = Some(true);
    let r = search(o).expect("ok");
    assert_eq!(r.files[0].match_count, 4);
    assert_eq!(r.stats.match_count, Some(4));
}

#[test]
fn count_lines_counts_matched_lines_per_file() {
    let t = TmpDir::new();
    t.write("a.txt", "x x x\nx\nnope\n");
    let mut o = opts(t.path(), "x");
    o.count_lines_per_file = Some(true);
    let r = search(o).expect("ok");
    assert_eq!(r.files[0].match_count, 2);
}

#[test]
fn context_lines_are_assembled_into_snippet() {
    let t = TmpDir::new();
    t.write("a.txt", "before\nmatch\nafter\n");
    let mut o = opts(t.path(), "match");
    o.context_lines = Some(1);
    let r = search(o).expect("ok");
    let v = &r.files[0].matches[0].value;
    assert!(
        v.contains("before") && v.contains("match") && v.contains("after"),
        "{v}"
    );
}

#[test]
fn lang_type_filters_by_extension() {
    let t = TmpDir::new();
    t.write("a.ts", "target\n");
    t.write("b.py", "target\n");
    let mut o = opts(t.path(), "target");
    o.lang_type = Some("ts".to_owned());
    let r = search(o).expect("ok");
    assert_eq!(r.files.len(), 1);
    assert!(r.files[0].path.ends_with("a.ts"));
}

#[test]
fn include_glob_restricts_files() {
    let t = TmpDir::new();
    t.write("a.ts", "target\n");
    t.write("b.js", "target\n");
    let mut o = opts(t.path(), "target");
    o.include = Some(vec!["*.ts".to_owned()]);
    let r = search(o).expect("ok");
    assert_eq!(r.files.len(), 1);
    assert!(r.files[0].path.ends_with("a.ts"));
}

#[test]
fn exclude_dir_prunes_directory() {
    let t = TmpDir::new();
    t.write("keep/a.txt", "target\n");
    t.write("skip/b.txt", "target\n");
    let mut o = opts(t.path(), "target");
    o.exclude_dir = Some(vec!["skip".to_owned()]);
    let r = search(o).expect("ok");
    assert_eq!(r.files.len(), 1);
    assert!(r.files[0].path.contains("keep"));
}

#[test]
fn results_are_sorted_by_path() {
    let t = TmpDir::new();
    t.write("z.txt", "m\n");
    t.write("a.txt", "m\n");
    t.write("m.txt", "m\n");
    let r = search(opts(t.path(), "m")).expect("ok");
    let paths: Vec<&str> = r.files.iter().map(|f| f.path.as_str()).collect();
    let mut sorted = paths.clone();
    sorted.sort_unstable();
    assert_eq!(paths, sorted);
}

#[test]
fn sort_reverse_flips_order() {
    let t = TmpDir::new();
    t.write("a.txt", "m\n");
    t.write("z.txt", "m\n");
    let mut o = opts(t.path(), "m");
    o.sort_reverse = Some(true);
    let r = search(o).expect("ok");
    assert!(r.files[0].path.ends_with("z.txt"));
    assert!(r.files[1].path.ends_with("a.txt"));
}

#[test]
fn respects_gitignore_by_default_and_no_ignore_overrides() {
    let t = TmpDir::new();
    // The `ignore` crate (like rg) only applies .gitignore inside a git repo
    // (require_git defaults true); a bare `.git` dir marks the temp tree as one.
    fs::create_dir_all(t.0.join(".git")).expect("create .git");
    t.write(".gitignore", "ignored.txt\n");
    t.write("ignored.txt", "target\n");
    t.write("kept.txt", "target\n");

    let r = search(opts(t.path(), "target")).expect("ok");
    assert_eq!(r.files.len(), 1, "gitignore'd file should be skipped");
    assert!(r.files[0].path.ends_with("kept.txt"));

    let mut o = opts(t.path(), "target");
    o.no_ignore = Some(true);
    let r2 = search(o).expect("ok");
    assert_eq!(r2.files.len(), 2, "--no-ignore searches the ignored file");
}

#[test]
fn whole_word_requires_word_boundary() {
    let t = TmpDir::new();
    t.write("a.txt", "foo\nfoobar\n");
    let mut o = opts(t.path(), "foo");
    o.whole_word = Some(true);
    let r = search(o).expect("ok");
    assert_eq!(r.files[0].match_count, 1);
    assert_eq!(r.files[0].matches[0].line, 1);
}

#[test]
fn empty_result_when_no_match() {
    let t = TmpDir::new();
    t.write("a.txt", "nothing here\n");
    let r = search(opts(t.path(), "absent")).expect("ok");
    assert!(r.files.is_empty());
}

// ── only-matching (rg -o) ───────────────────────────────────────────────

#[test]
fn only_matching_emits_one_match_per_submatch() {
    let t = TmpDir::new();
    t.write("a.txt", "ab ab ab\n");
    let mut o = opts(t.path(), "ab");
    o.only_matching = Some(true);
    let r = search(o).expect("ok");
    let f = &r.files[0];
    assert_eq!(f.match_count, 3);
    assert_eq!(f.matches.len(), 3);
    assert!(f.matches.iter().all(|m| m.value == "ab"));
    assert!(f.matches.iter().all(|m| m.line == 1));
}

#[test]
fn only_matching_value_is_the_span_not_the_line() {
    let t = TmpDir::new();
    t.write("a.txt", "prefix_NEEDLE_suffix\n");
    let mut o = opts(t.path(), "NEEDLE");
    o.only_matching = Some(true);
    let r = search(o).expect("ok");
    assert_eq!(r.files[0].matches.len(), 1);
    assert_eq!(r.files[0].matches[0].value, "NEEDLE");
    // column is the 0-based UTF-16 offset of the span start.
    assert_eq!(r.files[0].matches[0].column, 7);
}

#[test]
fn only_matching_enumerates_every_hit_on_one_minified_line() {
    let t = TmpDir::new();
    // The motivating case: a minified one-liner with many host tokens that
    // line-mode search can only *count*, never enumerate.
    t.write(
        "bundle.js",
        "a=\"x.cursor.sh\";b=\"y.cursor.sh\";c=\"z.cursor.sh\";\n",
    );
    let mut o = opts(t.path(), r"\w+\.cursor\.sh");
    o.only_matching = Some(true);
    let r = search(o).expect("ok");
    let vals: Vec<&str> = r.files[0]
        .matches
        .iter()
        .map(|m| m.value.as_str())
        .collect();
    assert_eq!(vals, vec!["x.cursor.sh", "y.cursor.sh", "z.cursor.sh"]);
}

#[test]
fn only_matching_window_widens_span_with_surrounding_context() {
    let t = TmpDir::new();
    t.write("a.txt", "leftcontext_HIT_rightcontext\n");
    let mut o = opts(t.path(), "HIT");
    o.only_matching = Some(true);
    o.match_window = Some(4);
    let r = search(o).expect("ok");
    let v = &r.files[0].matches[0].value;
    assert!(v.contains("HIT"), "{v}");
    assert!(v.contains("ext_") && v.contains("_rig"), "{v}");
    // window trims both sides, so ellipsis markers are present.
    assert!(v.starts_with('…') && v.ends_with('…'), "{v}");
}

#[test]
fn only_matching_window_is_char_boundary_safe_on_multibyte() {
    let t = TmpDir::new();
    // Multibyte chars on both sides of the hit: window slicing must never
    // panic by cutting a codepoint in half.
    t.write("u.txt", "café→HIT←déjà\n");
    let mut o = opts(t.path(), "HIT");
    o.only_matching = Some(true);
    o.match_window = Some(2);
    let r = search(o).expect("ok");
    assert!(r.files[0].matches[0].value.contains("HIT"));
}

#[test]
fn only_matching_unique_keeps_distinct_values_in_first_occurrence_order() {
    let t = TmpDir::new();
    t.write("a.txt", "ab ab cd ab cd ef\n");
    let mut o = opts(t.path(), r"\w+");
    o.only_matching = Some(true);
    o.unique = Some(true);
    let r = search(o).expect("ok");
    let vals: Vec<&str> = r.files[0]
        .matches
        .iter()
        .map(|m| m.value.as_str())
        .collect();
    assert_eq!(vals, vec!["ab", "cd", "ef"]);
    assert!(r.files[0].matches.iter().all(|m| m.count.is_none()));
}

#[test]
fn only_matching_count_unique_attaches_frequency_sorted_descending() {
    let t = TmpDir::new();
    t.write("a.txt", "ab ab cd ab cd ef\n");
    let mut o = opts(t.path(), r"\w+");
    o.only_matching = Some(true);
    o.count_unique = Some(true);
    let r = search(o).expect("ok");
    let vals: Vec<(&str, Option<u32>)> = r.files[0]
        .matches
        .iter()
        .map(|m| (m.value.as_str(), m.count))
        .collect();
    assert_eq!(
        vals,
        vec![("ab", Some(3)), ("cd", Some(2)), ("ef", Some(1))]
    );
}

#[test]
fn unique_requires_only_matching() {
    let t = TmpDir::new();
    t.write("a.txt", "ab ab\n");
    let mut o = opts(t.path(), "ab");
    o.unique = Some(true);
    let err = search(o).expect_err("unique without onlyMatching is invalid");
    assert!(err.reason.contains("onlyMatching:true"));
}

#[test]
fn only_matching_default_off_keeps_whole_line_value() {
    let t = TmpDir::new();
    t.write("a.txt", "prefix_NEEDLE_suffix\n");
    let r = search(opts(t.path(), "NEEDLE")).expect("ok");
    // Without only_matching the value is the full line, unchanged.
    assert_eq!(r.files[0].matches[0].value, "prefix_NEEDLE_suffix");
}
