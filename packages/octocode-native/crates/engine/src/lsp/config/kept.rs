//! Answers of discovery kept with the stamps of what they read ([`Proof`]),
//! the mechanism the rustup answer uses ([`super::invocation::rustup`]): an
//! answer is served while every stamp holds and found again as soon as one
//! differs, with no time bound but the process memory's idle release.
use crate::stamp::{Proof, settle_clock};
use octocode_io::memo::Memo;
use std::ffi::OsStr;
use std::fmt::Debug;
use std::hash::Hash;
use std::path::Path;

/// One kept answer and the stamps that prove it.
pub(super) struct Kept<V> {
    inputs: Proof,
    value: V,
}

/// `memo`'s answer for `key` while its inputs hold; otherwise `answer()`,
/// kept when `inputs()`, stamped before it runs, and `searched(&answer)`, what
/// its answer shows it searched, name everything it read (`None` from either:
/// nothing can prove it later, as when a question it asked got no answer) and
/// nothing stamped was written within the settle window before it started (a
/// write could race its reads; a path changed while it ran is newer than that,
/// so it may be stamped after).
pub(super) fn kept<K, V>(
    memo: &Memo<K, Kept<V>>,
    key: K,
    inputs: impl FnOnce() -> Option<Proof>,
    answer: impl FnOnce() -> V,
    searched: impl FnOnce(&V) -> Option<Proof>,
) -> V
where
    K: Hash + Eq + Ord + Clone + Send + Debug + 'static,
    V: Clone + Send + Sync + Debug + 'static,
{
    if let Some(hit) = memo.get(&key).filter(|hit| hit.value.inputs.holds()) {
        memo.touch(&key, crate::memory::IDLE);
        return hit.value.value.clone();
    }
    let started = settle_clock();
    let inputs = inputs();
    let value = answer();
    let inputs = inputs.and_then(|inputs| Some(inputs.with(&searched(&value)?)));
    if let Some(inputs) = inputs.filter(|inputs| inputs.settled_before(started)) {
        let weight = std::mem::size_of::<Kept<V>>()
            + std::mem::size_of::<K>()
            + format!("{key:?}{value:?}").len()
            + inputs.weight();
        memo.insert(
            key,
            Kept {
                inputs,
                value: value.clone(),
            },
            weight,
            Some(crate::memory::IDLE),
        );
    }
    value
}

/// Stamp the directories of `path_var` a `which` that `found` a program (or
/// `None`: found none) searched: each up to and including the one holding it,
/// since a later directory cannot take over, or all of them. A program
/// appears in or goes from a directory with a change of the directory's mtime;
/// one already there becomes runnable (`chmod +x`) or is replaced with a
/// change of its own file's, so each `programs` entry of each searched
/// directory is stamped too (absent counts).
pub(super) fn follow_path_search(
    inputs: &mut Proof,
    path_var: Option<&OsStr>,
    programs: &[&str],
    found: Option<&Path>,
) {
    let holder = found.and_then(Path::parent);
    for dir in path_var.into_iter().flat_map(std::env::split_paths) {
        if dir.as_os_str().is_empty() {
            continue;
        }
        let last = holder == Some(dir.as_path());
        for program in programs {
            inputs.follow(dir.join(program));
        }
        inputs.follow(dir.clone());
        if last {
            break;
        }
    }
}
