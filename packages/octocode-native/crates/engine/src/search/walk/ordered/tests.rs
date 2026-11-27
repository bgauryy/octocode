//! A walk started past an entry it names lists what lies after it, as the
//! tree is now, and decides only the directories on the way there.

use super::*;
use std::path::Path;

/// Opens every directory and hands out every file, except the names in
/// `left`; records each name it decides.
#[derive(Default)]
struct Plain {
    left: Vec<&'static str>,
    judged: Vec<String>,
}

impl Judge for Plain {
    type Verdict = ();

    fn rules(&self, _: &Path, _: Option<ListedRules>) -> Option<DirRules> {
        None
    }

    fn verdict(&self, _: Option<&DirRules>, _: Probe<'_>) {}

    fn opens(&self, (): ()) -> bool {
        true
    }

    fn judge(&mut self, entry: Probe<'_>, (): ()) -> Step {
        let name = entry
            .path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        self.judged.push(name.clone());
        if entry.depth > 0 && self.left.contains(&name.as_str()) {
            return Step::Skip;
        }
        if entry.is_dir() {
            Step::Enter
        } else if entry.file_type.is_some_and(|kind| kind.is_file()) {
            Step::Emit
        } else {
            Step::Skip
        }
    }

    fn failed(&mut self, _: String) {}

    fn halted(&mut self) -> bool {
        false
    }
}

fn names(path: &str) -> Vec<Vec<u8>> {
    path.split('/')
        .map(|name| name.as_bytes().to_vec())
        .collect()
}

/// The files a walk of `root` lists, past `after` when it names one.
fn rest(root: &Path, after: Option<&str>, judge: &mut Plain) -> Vec<String> {
    let mut walk = OrderedWalk::<()>::new(root, None, 0);
    if let Some(after) = after {
        walk.seek_after(&names(after), judge);
    }
    std::iter::from_fn(|| walk.next(judge))
        .map(|path| crate::text::slash_path(path.strip_prefix(root).expect("under root")))
        .collect()
}

fn tree(files: &[&str]) -> tempfile::TempDir {
    let root = tempfile::tempdir().expect("root");
    for file in files {
        let path = root.path().join(file);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("dirs");
        std::fs::write(path, "x\n").expect("write");
    }
    root
}

#[test]
fn a_walk_started_past_a_file_lists_exactly_the_files_after_it() {
    let root = tree(&["a/x", "a/y", "a-b", "b/c/z", "b/d", "b/e/f", "c"]);
    let whole = rest(root.path(), None, &mut Plain::default());
    assert_eq!(whole, ["a/x", "a/y", "a-b", "b/c/z", "b/d", "b/e/f", "c"]);
    for (at, file) in whole.iter().enumerate() {
        let mut judge = Plain::default();
        assert_eq!(rest(root.path(), Some(file), &mut judge), whole[at + 1..]);
    }
    // Past `b/d`, only `b` is decided on the way: `a`, its files, `a-b`
    // and `b/c` are passed without a decision.
    let mut judge = Plain::default();
    rest(root.path(), Some("b/d"), &mut judge);
    let root_name = root
        .path()
        .file_name()
        .expect("name")
        .to_string_lossy()
        .into_owned();
    assert_eq!(judge.judged, [root_name.as_str(), "b", "e", "f", "c"]);
}

#[test]
fn a_name_gone_or_changed_resumes_where_it_would_sort() {
    let root = tree(&["a/x", "a/y", "b/c/z", "b/d/g", "c"]);
    // Gone: the walk goes on at the next name.
    assert_eq!(
        rest(root.path(), Some("a/xx"), &mut Plain::default()),
        ["a/y", "b/c/z", "b/d/g", "c"]
    );
    assert_eq!(
        rest(root.path(), Some("aa/q"), &mut Plain::default()),
        ["b/c/z", "b/d/g", "c"]
    );
    // A file that is a directory now: what it holds lies past it.
    assert_eq!(
        rest(root.path(), Some("b/d"), &mut Plain::default()),
        ["b/d/g", "c"]
    );
    // A directory on the way that is a file now, or that the rules leave out
    // now: the walk goes on past it.
    assert_eq!(
        rest(root.path(), Some("c/q"), &mut Plain::default()),
        Vec::<String>::new()
    );
    let mut judge = Plain {
        left: vec!["b"],
        ..Plain::default()
    };
    assert_eq!(rest(root.path(), Some("b/c/z"), &mut judge), ["c"]);
}

#[test]
fn past_a_root_file_nothing_is_left() {
    let root = tree(&["only"]);
    let file = root.path().join("only");
    let mut walk = OrderedWalk::<()>::new(&file, None, 0);
    let mut judge = Plain::default();
    walk.seek_after(&[], &mut judge);
    assert_eq!(walk.next(&mut judge), None);
    assert!(walk.ended());
}
