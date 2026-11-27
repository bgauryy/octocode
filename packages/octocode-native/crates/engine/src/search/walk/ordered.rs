//! The sorted, lazy walk of a tree: siblings by name, depth first, a
//! directory before its contents, the order `rg --sort path` and the `ignore`
//! walker sorted by file name give, and the order every candidate index of a
//! stream is stated in.
//!
//! One consumer takes the entries in order; only it decides them, so what a
//! walk reports does not depend on timing. What it may overlap is reading:
//! once it has read a few directories itself, the subdirectories its rules
//! already leave open are read ahead by a few threads
//! ([`read_ahead`]), at most two listings per thread held, and the walk takes
//! each by its path in its own order. A directory the rules leave out is never
//! listed by anyone. A directory's rule files are found from its own listing
//! ([`listed_rules`]), and each entry's type comes with the listing, so an
//! entry costs no `stat`.
//!
//! The rules are the caller's ([`Judge`]); this walk owns the order, the
//! listings and the depth limit, and states a directory it cannot read as an
//! error the judge records (the text `ignore` gives: the path, then the OS
//! error).

use std::fs;
use std::path::{Path, PathBuf};

use super::ignore_rules::{DirRules, ListedRules};
use super::read_ahead::{self, Detail, Key, ReadAhead, Taken};
use crate::types::fs_resume::Child;

/// One entry as the judge reads it.
#[derive(Clone, Copy)]
pub(crate) struct Probe<'a> {
    pub(crate) path: &'a Path,
    /// The walk root is depth 0, its entries depth 1.
    pub(crate) depth: usize,
    /// The type its directory listing gave.
    pub(crate) file_type: Option<fs::FileType>,
}

impl Probe<'_> {
    pub(crate) fn is_dir(&self) -> bool {
        self.file_type.is_some_and(|kind| kind.is_dir())
    }
}

/// What the walk does after the judge decided an entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Step {
    /// Nothing: the entry is left out (a left out directory is not entered).
    Skip,
    /// A directory to enter.
    Enter,
    /// A file to hand to the caller.
    Emit,
    /// The walk is over: nothing more is reachable (the root is left out).
    End,
}

/// The rules of one walk.
pub(crate) trait Judge {
    /// What the rules say of an entry with no side effect, so the walk may
    /// ask before it reaches the entry, and a directory the rules leave open
    /// is the only kind read ahead.
    type Verdict: Copy;

    /// The rules of the directory `dir`, entered; `listed` shows its rule
    /// files when its listing was complete.
    fn rules(&self, dir: &Path, listed: Option<ListedRules>) -> Option<DirRules>;

    fn verdict(&self, rules: Option<&DirRules>, entry: Probe<'_>) -> Self::Verdict;

    /// Whether a directory with this verdict may be entered.
    fn opens(&self, verdict: Self::Verdict) -> bool;

    /// Decide the entry, in walk order; the one place a decision has an effect.
    fn judge(&mut self, entry: Probe<'_>, verdict: Self::Verdict) -> Step;

    /// Record an entry or directory that could not be read.
    fn failed(&mut self, error: String);

    /// Whether the walk must end before its next entry.
    fn halted(&mut self) -> bool;
}

/// A directory the walk is inside.
struct Frame<V> {
    dir: PathBuf,
    /// The depth of its entries.
    depth: usize,
    children: Vec<Child>,
    /// The verdicts asked ahead of the walk (directories only).
    verdicts: Vec<Option<V>>,
    next: usize,
    rules: Option<DirRules>,
    /// The subdirectories the rules leave open, by their index in
    /// `children`.
    open: Vec<usize>,
    /// The first of `open` the walk has not passed.
    passed: usize,
}

/// The walk of one tree, lazily.
pub(crate) struct OrderedWalk<V> {
    root: Option<PathBuf>,
    max_depth: Option<usize>,
    stack: Vec<Frame<V>>,
    ahead: Option<ReadAhead>,
    /// Threads the read-ahead takes once it starts; none: never.
    ahead_workers: usize,
    directories_read: u32,
    /// The walk is finding where to start ([`Self::seek_after`]): it reads
    /// nothing ahead, since what it passes it never takes.
    seeking: bool,
    scratch: PathBuf,
    /// The walk reached its end or its root left everything out.
    ended: bool,
    over: bool,
}

impl<V: Copy> OrderedWalk<V> {
    /// A walk of `root`; `max_depth` in `ignore` terms (the root is depth 0).
    /// `ahead_workers` threads read directories ahead once the walk has read
    /// [`read_ahead::START_AFTER_DIRECTORIES`] itself (none when zero).
    pub(crate) fn new(root: &Path, max_depth: Option<usize>, ahead_workers: usize) -> Self {
        Self {
            root: Some(root.to_path_buf()),
            max_depth,
            stack: Vec::new(),
            ahead: None,
            ahead_workers,
            directories_read: 0,
            seeking: false,
            scratch: PathBuf::new(),
            ended: false,
            over: false,
        }
    }

    /// The walk reached its end (rather than being stopped).
    pub(crate) fn ended(&self) -> bool {
        self.ended
    }

    /// The next file, or `None` when the walk is over.
    pub(crate) fn next<J: Judge<Verdict = V>>(&mut self, judge: &mut J) -> Option<PathBuf> {
        loop {
            if self.over {
                return None;
            }
            if judge.halted() {
                return self.finish(false);
            }
            if let Some(root) = self.root.take() {
                match self.begin(root, judge) {
                    Begun::Emit(path) => return Some(path),
                    Begun::Over(ended) => return self.finish(ended),
                    Begun::Walking => {}
                }
                continue;
            }
            let Some(frame) = self.stack.last_mut() else {
                return self.finish(true);
            };
            if frame.next >= frame.children.len() {
                self.stack.pop();
                self.offer_next();
                continue;
            }
            match self.step(judge) {
                Step::Emit => return Some(self.scratch.clone()),
                Step::End => return self.finish(true),
                Step::Skip | Step::Enter => {}
            }
        }
    }

    /// Start the walk just past the entry `after` names (one name per
    /// component below the root, as the listings spell them): the next file
    /// is the first one past it in walk order, as the tree is now. Only the
    /// directories on the way to it are listed, and each is decided as the
    /// walk decides it on reaching it; the entries before the way are passed
    /// with no decision. Called before the first [`Self::next`]. The
    /// directories read on the way count for no read-ahead: it starts once
    /// the walk has read its own few from there.
    pub(crate) fn seek_after<J: Judge<Verdict = V>>(&mut self, after: &[Vec<u8>], judge: &mut J) {
        self.seeking = true;
        self.seek(after, judge);
        self.seeking = false;
        self.directories_read = 0;
    }

    fn seek<J: Judge<Verdict = V>>(&mut self, after: &[Vec<u8>], judge: &mut J) {
        let Some(root) = self.root.take() else {
            return;
        };
        match self.begin(root, judge) {
            Begun::Walking => {}
            // A root file is the walk's one candidate: nothing lies past it.
            Begun::Emit(_) => {
                self.finish(true);
                return;
            }
            Begun::Over(ended) => {
                self.finish(ended);
                return;
            }
        }
        for (at, name) in after.iter().enumerate() {
            let Some(frame) = self.stack.last_mut() else {
                return;
            };
            let index = frame
                .children
                .partition_point(|child| child.name.as_encoded_bytes() < name.as_slice());
            frame.next = index;
            let Some(child) = frame
                .children
                .get(index)
                .filter(|child| child.name.as_encoded_bytes() == name.as_slice())
            else {
                return;
            };
            if !child.file_type.is_some_and(|kind| kind.is_dir()) {
                // A file, or a link the walk does not follow, sorts at or
                // before `after`: the walk goes on past it.
                frame.next = index + 1;
                return;
            }
            if at + 1 == after.len() {
                // `after` is a directory now: what it holds lies past it.
                return;
            }
            if judge.halted() {
                self.finish(false);
                return;
            }
            let depth = self.stack.len();
            match self.step(judge) {
                Step::Enter if self.stack.len() > depth => {}
                Step::End => {
                    self.finish(true);
                    return;
                }
                // Left out, or not listed: the walk goes on past it.
                _ => return,
            }
        }
    }

    /// Decide the entry the innermost directory stands at and move past it,
    /// into it when the judge enters it. An emitted file's path is in
    /// `scratch`.
    fn step<J: Judge<Verdict = V>>(&mut self, judge: &mut J) -> Step {
        let Some(frame) = self.stack.last_mut() else {
            return Step::End;
        };
        let at = frame.next;
        frame.next += 1;
        let depth = frame.depth;
        let file_type = frame.children[at].file_type;
        self.scratch.clear();
        self.scratch.push(&frame.dir);
        self.scratch.push(&frame.children[at].name);
        let probe = Probe {
            path: &self.scratch,
            depth,
            file_type,
        };
        let verdict =
            frame.verdicts[at].unwrap_or_else(|| judge.verdict(frame.rules.as_ref(), probe));
        let step = judge.judge(probe, verdict);
        match step {
            Step::Skip => {
                if probe.is_dir()
                    && let Some(ahead) = self.ahead.as_ref()
                {
                    ahead.drop_unused(&self.scratch);
                }
            }
            Step::Enter => {
                let dir = self.scratch.clone();
                self.descend(dir, depth, judge);
                self.offer_next();
            }
            Step::Emit | Step::End => {}
        }
        step
    }

    fn finish(&mut self, ended: bool) -> Option<PathBuf> {
        self.over = true;
        self.ended = ended;
        self.ahead = None;
        None
    }

    /// The root entry, as `ignore`'s walker reports it: a link is a link
    /// (and a directory behind one is still walked), except a link to a file,
    /// which is the file.
    fn begin<J: Judge<Verdict = V>>(&mut self, root: PathBuf, judge: &mut J) -> Begun {
        let metadata = match fs::symlink_metadata(&root) {
            Ok(metadata) => metadata,
            Err(error) => {
                judge.failed(path_error(&root, &error));
                return Begun::Over(true);
            }
        };
        let mut file_type = metadata.file_type();
        let mut walkable = file_type.is_dir();
        if file_type.is_symlink() {
            match fs::metadata(&root) {
                Ok(target) if target.is_file() => file_type = target.file_type(),
                Ok(target) => walkable = target.is_dir(),
                Err(error) => {
                    judge.failed(path_error(&root, &error));
                    return Begun::Over(true);
                }
            }
        }
        let probe = Probe {
            path: &root,
            depth: 0,
            file_type: Some(file_type),
        };
        let verdict = judge.verdict(None, probe);
        match judge.judge(probe, verdict) {
            Step::Emit => Begun::Emit(root),
            Step::End => Begun::Over(true),
            Step::Skip | Step::Enter if walkable => {
                self.descend(root, 0, judge);
                Begun::Walking
            }
            Step::Skip | Step::Enter => Begun::Over(true),
        }
    }

    /// Push the directory `dir`, an entry at `depth`, with the listing the
    /// read-ahead holds or one read now.
    fn descend<J: Judge<Verdict = V>>(&mut self, dir: PathBuf, depth: usize, judge: &mut J) {
        if self.max_depth.is_some_and(|max| depth >= max) {
            return;
        }
        let listing = match self.ahead.as_ref().map(|ahead| ahead.take(&dir, None)) {
            Some(Taken::Read(listing)) => listing,
            _ => read_ahead::read_listing_with(&dir, Detail::Names),
        };
        self.directories_read += 1;
        if self.ahead.is_none()
            && !self.seeking
            && self.ahead_workers > 0
            && self.directories_read == read_ahead::START_AFTER_DIRECTORIES
        {
            self.ahead = ReadAhead::start(self.ahead_workers, Detail::Names);
        }
        let (children, errors) = match listing.entries {
            Ok(read) => read,
            Err(error) => {
                judge.failed(path_error(&dir, &error));
                return;
            }
        };
        // `walkdir` reports what it could not read of a listing before the
        // entries it could.
        for error in &errors {
            judge.failed(error.to_string());
        }
        let rules = judge.rules(&dir, errors.is_empty().then(|| listed_rules(&children)));
        let entries = depth + 1;
        let mut verdicts = vec![None; children.len()];
        let mut open = Vec::new();
        if self.ahead.is_some() && self.max_depth.is_none_or(|max| entries < max) {
            for (at, child) in children.iter().enumerate() {
                if !child.file_type.is_some_and(|kind| kind.is_dir()) {
                    continue;
                }
                let verdict = judge.verdict(
                    rules.as_ref(),
                    Probe {
                        path: &dir.join(&child.name),
                        depth: entries,
                        file_type: child.file_type,
                    },
                );
                verdicts[at] = Some(verdict);
                if judge.opens(verdict) {
                    open.push(at);
                }
            }
        }
        self.stack.push(Frame {
            dir,
            depth: entries,
            children,
            verdicts,
            next: 0,
            rules,
            open,
            passed: 0,
        });
        self.offer_next();
    }

    /// Offer the read-ahead the subdirectories the innermost directory has
    /// still to enter, as many as it holds: the nearest first.
    fn offer_next(&mut self) {
        let (Some(ahead), Some(frame)) = (self.ahead.as_ref(), self.stack.last_mut()) else {
            return;
        };
        while frame
            .open
            .get(frame.passed)
            .is_some_and(|at| *at < frame.next)
        {
            frame.passed += 1;
        }
        ahead.queue(
            frame
                .open
                .iter()
                .skip(frame.passed)
                .take(ahead.window())
                .map(|&at| Key::from(frame.dir.join(&frame.children[at].name))),
        );
    }
}

/// How the root began.
enum Begun {
    Emit(PathBuf),
    /// The walk is over: reached its end (`true`), or stopped.
    Over(bool),
    Walking,
}

/// An error on `path` as the `ignore` walker words it.
pub(crate) fn path_error(path: &Path, error: &std::io::Error) -> String {
    let shown = path.display();
    format!("{shown}: IO error for operation on {shown}: {error}")
}

#[cfg(test)]
mod tests;

/// Whether a listing sorted by name holds `name`.
pub(crate) fn listed<'c>(children: &'c [Child], name: &str) -> Option<&'c Child> {
    children
        .binary_search_by(|child| child.name.as_os_str().cmp(std::ffi::OsStr::new(name)))
        .ok()
        .map(|at| &children[at])
}

/// What a directory's name-sorted listing shows of its ignore rules.
pub(crate) fn listed_rules(children: &[Child]) -> ListedRules {
    ListedRules {
        gitignore: listed(children, ".gitignore").is_some(),
        ignore: listed(children, ".ignore").is_some(),
        git: listed(children, ".git").is_some(),
        jj: listed(children, ".jj").is_some(),
    }
}
