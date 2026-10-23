//! Narrowed scopes skip what the cut scan read, count only candidates, and
//! keep the root walk's ignore rules.

use super::*;

/// Files that sort before the child the walk stopped in were read by the cut
/// scan, those after it were not: a directory's own-files scope skips the
/// read ones, so every candidate is listed by exactly one call, at every
/// level of a nested tree.
fn leads_skip_what_the_cut_scan_read(build: Build) {
    let root = Fixture::new();
    for path in [
        "0first.ts",
        "a/a1.ts",
        "a/a2.ts",
        "b/a.txt",
        "b/b0.ts",
        "b/deep/d1.ts",
        "b/deep/d2.ts",
        "b/notes.txt",
        "b/z.ts",
        "c/c1.ts",
        "c/c2.ts",
        "root1.ts",
    ] {
        root.write(path, "export const value = 1;\n");
    }
    let (first, calls, files) = walk_all(&root, build, json!({}), 5);
    assert_eq!(
        listed(&first),
        ["0first.ts", "a1.ts", "a2.ts", "b0.ts", "d1.ts"]
            .map(str::to_owned)
            .into_iter()
            .collect(),
        "{first}"
    );
    // The frontier's own directory, the one above it, the unread sibling,
    // and the root.
    assert_eq!(
        lead_dirs(&root, &first),
        ["b/deep", "b", "c", ""],
        "{first}"
    );
    // Each own-files scope skips the candidates the cut scan read before
    // it: one in `b/deep` (d1), one in `b` (b0; a.txt is no candidate), one
    // in the root (0first).
    let skips: Vec<_> = leads(&first)
        .iter()
        .map(|(_, lead)| lead["query"]["queries"][0]["scanOffset"].clone())
        .collect();
    assert_eq!(
        skips,
        [json!(1), json!(1), Value::Null, json!(1)],
        "{first}"
    );
    assert_eq!(
        files,
        sorted([
            "0first.ts",
            "a1.ts",
            "a2.ts",
            "b0.ts",
            "d1.ts",
            "d2.ts",
            "z.ts",
            "c1.ts",
            "c2.ts",
            "root1.ts"
        ]),
        "each file once: {calls:?}"
    );
}

#[test]
fn match_leads_list_each_file_once_when_files_sort_before_the_frontier() {
    leads_skip_what_the_cut_scan_read(match_row);
}

#[test]
fn symbols_leads_list_each_file_once_when_files_sort_before_the_frontier() {
    leads_skip_what_the_cut_scan_read(symbols_row);
}

/// A file the scan cannot parse is no candidate: it neither costs a lead
/// nor counts toward a directory's size. With `languageGlobs`, a file the
/// map names is one.
fn unparseable_files_are_not_candidates(build: Build) {
    let root = Fixture::new();
    for path in ["x/x1.ts", "x/x2.ts", "y/y1.ts", "y/y2.ts", "zz.txt"] {
        root.write(path, "export const value = 1;\n");
    }
    let (first, calls, files) = walk_all(&root, build, json!({}), 2);
    // `zz.txt` is the only unread file of the root, and no scan parses it.
    assert_eq!(lead_dirs(&root, &first), ["y"], "{first}");
    assert_eq!(
        files,
        sorted(["x1.ts", "x2.ts", "y1.ts", "y2.ts"]),
        "{calls:?}"
    );

    // A function too: the outline reads a mapped file's functions.
    root.write(
        "zz.foo",
        "export const value = 2;\nexport function mapped() {}\n",
    );
    let mapped = json!({"languageGlobs":{"typescript":["*.foo"]}});
    let (first, calls, files) = walk_all(&root, build, mapped, 2);
    assert_eq!(lead_dirs(&root, &first), ["y", ""], "{first}");
    assert_eq!(
        files,
        sorted(["x1.ts", "x2.ts", "y1.ts", "y2.ts", "zz.foo"]),
        "{calls:?}"
    );
}

#[test]
fn match_ignores_unparseable_files_when_it_names_unread_scopes() {
    unparseable_files_are_not_candidates(match_row);
}

#[test]
fn symbols_ignores_unparseable_files_when_it_names_unread_scopes() {
    unparseable_files_are_not_candidates(symbols_row);
}

/// Only candidates make a directory too big for the walk: many unparseable
/// files beside a few candidates leave a scope that reads them.
fn unparseable_files_do_not_make_a_directory_flat(build: Build) {
    let root = Fixture::new();
    for index in 1..=3 {
        root.write(&format!("a/a{index}.ts"), "export const v = 1;\n");
    }
    root.write("b/b1.ts", "export const v = 1;\n");
    for index in 0..8 {
        root.write(&format!("b/data{index}.txt"), "not code\n");
    }
    let (first, calls, files) = walk_all(&root, build, json!({}), 3);
    assert_eq!(first["partialReasons"], json!(["scanBudget"]), "{first}");
    assert_eq!(lead_dirs(&root, &first), ["b"], "{first}");
    assert_eq!(
        files,
        sorted(["a1.ts", "a2.ts", "a3.ts", "b1.ts"]),
        "{calls:?}"
    );
}

#[test]
fn match_counts_only_candidates_toward_a_flat_directory() {
    unparseable_files_do_not_make_a_directory_flat(match_row);
}

#[test]
fn symbols_counts_only_candidates_toward_a_flat_directory() {
    unparseable_files_do_not_make_a_directory_flat(symbols_row);
}

/// A narrowed call walks under the ignore rules the root walk had, ancestor
/// `.gitignore` files included: neither the listing of an unread scope nor
/// the call that reads it names an ignored entry.
fn narrowed_scopes_keep_the_ancestor_ignore_rules(build: Build) {
    let root = Fixture::new();
    std::fs::create_dir_all(root.0.join(".git")).expect("git dir");
    root.write(".gitignore", "skipme/\n*.gen.ts\n");
    for path in [
        "a/a1.ts",
        "a/a2.ts",
        "b/b1.ts",
        "b/b2.ts",
        "b/b3.gen.ts",
        "b/skipme/s1.ts",
        "b/zz/z1.ts",
        "b/zz/hidden.ts",
    ] {
        root.write(path, "export const value = 1;\n");
    }
    // An intermediate ancestor: the rule of `b` reaches the scope `b/zz`.
    root.write("b/.gitignore", "hidden.ts\n");
    let (first, calls, files) = walk_all(&root, build, json!({}), 3);
    assert_eq!(lead_dirs(&root, &first), ["b/zz", "b"], "{first}");
    assert_eq!(
        files,
        sorted(["a1.ts", "a2.ts", "b1.ts", "b2.ts", "z1.ts"]),
        "{calls:?}"
    );
}

#[test]
fn match_narrowed_scopes_keep_the_ancestor_ignore_rules() {
    narrowed_scopes_keep_the_ancestor_ignore_rules(match_row);
}

#[test]
fn symbols_narrowed_scopes_keep_the_ancestor_ignore_rules() {
    narrowed_scopes_keep_the_ancestor_ignore_rules(symbols_row);
}
