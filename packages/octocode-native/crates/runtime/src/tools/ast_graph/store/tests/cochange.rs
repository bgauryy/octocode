//! `cochange`: the Git change-coupling overlay, read at query time.
use super::*;
use crate::tools::ast_graph::test_support::next_offset;

/// Runs git in `dir` with a fixed identity; `false` when git is missing.
fn git(dir: &Path, args: &[&str]) -> bool {
    std::process::Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["-c", "user.email=t@t", "-c", "user.name=t"])
        .args(args)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn append(dir: &Fixture, files: &[&str]) {
    for file in files {
        let path = dir.0.as_path().join(file);
        let mut text = std::fs::read_to_string(&path).unwrap_or_default();
        text.push_str("// edit\n");
        std::fs::write(&path, text).expect("edit");
    }
}

fn commit(dir: &Fixture, files: &[&str], message: &str) {
    append(dir, files);
    assert!(git(dir.0.as_path(), &["add", "-A"]));
    assert!(git(dir.0.as_path(), &["commit", "-qm", message]));
}

/// A repository whose scope `app` holds `a.ts` (imports `b.ts`), `b.ts`,
/// `c.ts`, `d.ts` (imports `a.ts`) and `e.ts`, with a file outside the
/// scope. Commits touching `a.ts`: the base (every file), 1 {a,b,c,outside},
/// 2 {a,b}, 3 {a,d}, and on a merged branch 4 {a,e}; one more commit
/// touches only `b.ts`, another only `c.ts`.
/// `None` when git is unavailable.
fn history() -> Option<Fixture> {
    let dir = Fixture::new();
    let root = dir.0.as_path();
    if !git(root, &["init", "-q"]) {
        return None;
    }
    write_under(
        &dir,
        "app/src",
        &[
            (
                "a.ts",
                "import { b } from './b';\nexport const a = () => b();\n",
            ),
            ("b.ts", "export const b = () => 1;\n"),
            ("c.ts", "export const c = 1;\n"),
            (
                "d.ts",
                "import { a } from './a';\nexport const d = () => a();\n",
            ),
            ("e.ts", "export const e = 1;\n"),
        ],
    );
    std::fs::write(root.join("outside.txt"), "x\n").expect("outside");
    assert!(git(root, &["add", "-A"]));
    assert!(git(root, &["commit", "-qm", "base"]));
    commit(
        &dir,
        &[
            "app/src/a.ts",
            "app/src/b.ts",
            "app/src/c.ts",
            "outside.txt",
        ],
        "1",
    );
    commit(&dir, &["app/src/a.ts", "app/src/b.ts"], "2");
    commit(&dir, &["app/src/a.ts", "app/src/d.ts"], "3");
    commit(&dir, &["app/src/b.ts"], "b only");
    assert!(git(root, &["checkout", "-qb", "side"]));
    commit(&dir, &["app/src/a.ts", "app/src/e.ts"], "4");
    assert!(git(root, &["checkout", "-q", "-"]));
    commit(&dir, &["app/src/c.ts"], "main moves on");
    assert!(git(
        root,
        &["merge", "-q", "--no-ff", "-m", "merge", "side"]
    ));
    assert_eq!(ingest_fixture(root, None).class, ExitClass::Success);
    Some(dir)
}

fn cochange(dir: &Fixture, target: &str, tweak: impl FnOnce(&mut QueryOptions)) -> GraphOutput {
    ask(dir.0.as_path(), "cochange", Some(target), tweak)
}

/// Rows count the non-merge commits shared with the target, divide by the
/// target's own, mark files the snapshot joins to it, and leave out the
/// target and files outside the scope; most commits first, then path.
#[test]
fn cochange_counts_shared_commits_and_marks_linked_files() {
    let Some(dir) = history() else {
        return; // git unavailable in this environment
    };
    let out = cochange(&dir, "src/a.ts", |_| {});
    assert_eq!(out.class, ExitClass::Success, "{}", out.value);
    let target_commits = out.value["summary"]["targetCommits"]
        .as_u64()
        .expect("targetCommits");
    // The base commit and commits 1 to 4.
    assert_eq!(target_commits, 5, "{}", out.value);
    let rows = out.value["rows"].as_array().expect("rows");
    let shape = rows
        .iter()
        .map(|row| {
            (
                row["path"].as_str().unwrap_or_default().to_owned(),
                row["commits"].as_u64().unwrap_or_default(),
                row["linked"].as_bool().unwrap_or_default(),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        shape,
        vec![
            ("app/src/b.ts".to_owned(), 3, true),
            ("app/src/c.ts".to_owned(), 2, false),
            ("app/src/d.ts".to_owned(), 2, true),
            ("app/src/e.ts".to_owned(), 2, false),
        ],
        "{}",
        out.value
    );
    assert!(out.value.get("warnings").is_none(), "{}", out.value);
    for row in rows {
        let share = row["share"].as_f64().expect("share");
        let expected = row["commits"].as_f64().unwrap_or_default() / target_commits as f64;
        assert!((share - expected).abs() < 1e-3, "{row}");
    }
}

/// Walking every page with a small limit reaches each row exactly once,
/// in the first page's order, and only the first page states the summary.
#[test]
fn cochange_pages_reach_every_row_once() {
    let Some(dir) = history() else {
        return;
    };
    let whole = cochange(&dir, "src/a.ts", |_| {});
    let expected = whole.value["rows"].as_array().expect("rows").clone();
    let mut seen = Vec::new();
    let mut offset = 0;
    loop {
        let page = cochange(&dir, "src/a.ts", |o| {
            o.limit = Some(1);
            o.offset = Some(offset);
        });
        assert_eq!(page.class, ExitClass::Success, "{}", page.value);
        assert_eq!(
            page.value.get("summary").is_some(),
            offset == 0,
            "{}",
            page.value
        );
        seen.extend(page.value["rows"].as_array().expect("rows").iter().cloned());
        match next_offset(&page) {
            Some(next) => offset = next,
            None => break,
        }
    }
    assert_eq!(seen, expected);
}

/// A target Git has never committed is an empty answer that says so, not
/// an error.
#[test]
fn cochange_of_a_file_without_history_is_empty_with_a_reason() {
    let Some(dir) = history() else {
        return;
    };
    dir.write("app/src/fresh.ts", "export const fresh = 1;\n");
    assert_eq!(
        ingest_fixture(dir.0.as_path(), None).class,
        ExitClass::Success
    );
    let out = cochange(&dir, "src/fresh.ts", |_| {});
    assert!(out.value.get("errorCode").is_none(), "{}", out.value);
    assert_eq!(out.value["rows"], json!([]), "{}", out.value);
    assert_eq!(out.value["summary"]["targetCommits"], 0, "{}", out.value);
    assert!(out.value["summary"]["reason"].is_string(), "{}", out.value);
}

/// Outside a Git work tree the overlay has no history to read: the error
/// names Git, not a missing node.
#[test]
fn cochange_outside_git_is_an_actionable_git_error() {
    let dir = fixture();
    assert_eq!(
        ingest_fixture(dir.0.as_path(), None).class,
        ExitClass::Success
    );
    let out = cochange(&dir, "src/lib.ts", |_| {});
    assert_eq!(
        out.value["errorCode"],
        json!(ErrorCode::GitFailed),
        "{}",
        out.value
    );
    let message = out.value["error"].as_str().unwrap_or_default();
    assert!(message.contains("Git"), "{}", out.value);
}

/// A shallow clone of `history` holding its last `depth` commits on each
/// path, built and ingested; `None` when git is unavailable.
fn shallow_clone(source: &Fixture, depth: u32) -> Option<Fixture> {
    let clone = Fixture::new();
    let url = format!("file://{}", source.0.as_path().display());
    let depth = depth.to_string();
    if !git(
        clone.0.as_path(),
        &["clone", "-q", "--depth", &depth, &url, "."],
    ) {
        return None;
    }
    assert_eq!(
        ingest_fixture(clone.0.as_path(), None).class,
        ExitClass::Success
    );
    Some(clone)
}

/// A shallow clone's boundary commits have no parents here, so Git diffs
/// them against nothing and lists every file of their tree: no change set.
/// They count toward no row and no `targetCommits`, and the answer says the
/// history is shallow. Depth 3 of `history` reaches the merge, "main moves
/// on" and "4"; "b only" is the boundary. Only "4" {a,e} counts for `a.ts`.
#[test]
fn cochange_on_a_shallow_clone_skips_boundary_commits_and_says_so() {
    let Some(dir) = history() else {
        return;
    };
    let Some(clone) = shallow_clone(&dir, 3) else {
        return;
    };
    let out = cochange(&clone, "src/a.ts", |_| {});
    assert_eq!(out.class, ExitClass::Success, "{}", out.value);
    assert_eq!(out.value["summary"]["targetCommits"], 1, "{}", out.value);
    let rows = out.value["rows"].as_array().expect("rows");
    let shape = rows
        .iter()
        .map(|row| {
            (
                row["path"].as_str().unwrap_or_default(),
                row["commits"].as_u64().unwrap_or_default(),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(shape, vec![("app/src/e.ts", 1)], "{}", out.value);
    let warnings = out.value["warnings"].as_array().expect("warnings");
    assert!(
        warnings
            .iter()
            .any(|w| w.as_str().unwrap_or_default().contains("shallow")),
        "{}",
        out.value
    );
}

/// When the target's only commit is the boundary of a depth-1 clone, no
/// file is co-changed, and the empty answer's reason names the shallow
/// history rather than an untracked file.
#[test]
fn cochange_on_a_depth_one_clone_lists_no_boundary_rows() {
    let Some(dir) = history() else {
        return;
    };
    let Some(clone) = shallow_clone(&dir, 1) else {
        return;
    };
    let out = cochange(&clone, "src/a.ts", |_| {});
    assert_eq!(out.class, ExitClass::Empty, "{}", out.value);
    assert_eq!(out.value["rows"], json!([]), "{}", out.value);
    assert_eq!(out.value["summary"]["targetCommits"], 0, "{}", out.value);
    let reason = out.value["summary"]["reason"].as_str().unwrap_or_default();
    assert!(reason.contains("shallow"), "{}", out.value);
}
