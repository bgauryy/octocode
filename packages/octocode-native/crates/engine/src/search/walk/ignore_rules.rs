//! Ignore-rule resolution for one path: the files that hold the rules, the
//! precedence the walker applies, and the rule that decided an entry.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, PoisonError, RwLock};

use ignore::Match;
use ignore::gitignore::Gitignore;

mod level;

#[cfg(test)]
pub(crate) use level::LEVELS_STATED;
pub use level::ListedRules;
use level::{Level, git_exclude_file};

/// The ignore rule that keeps a path out of a walk.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IgnoreReason {
    /// The topmost entry on the way to the path that the rule leaves out (a
    /// walk never enters an ignored directory).
    pub entry: PathBuf,
    /// The ignore file the rule is in, spelled under the walk root's own
    /// path; `None` for a rule with no file.
    pub source: Option<PathBuf>,
    /// The rule's 1-based line in `source`, when it can be found.
    pub line: Option<usize>,
    /// The rule as written.
    pub pattern: String,
}

/// The ignore files a directory may hold: `.gitignore`, which applies inside
/// a repository only, and `.ignore`, which applies with or without one.
pub const IGNORE_FILES: [&str; 2] = [".gitignore", ".ignore"];

/// The rules that decide the entries of one walked directory
/// ([`IgnoreRules::enter`]): a walk keeps it for the directory, so an entry
/// costs no cache lookup.
pub struct DirRules(Arc<Chain>);

/// The files outside a walk of `root` whose contents decide its ignore rules
/// ([`walk_builder`](super::walk_builder), [`IgnoreRules`]): the [`IGNORE_FILES`] of each
/// directory above `root`, the `info/exclude` of a repository whose `.git`
/// sits at or above `root` (through a worktree's `gitdir:` pointer and
/// `commondir`), the global excludes file, and the git config files that may
/// name it. A listed path may not exist: creating it changes the rules too.
/// Ignore files at or below `root` are walked entries, and a repository
/// nested below it keeps its exclude file in its own `.git`, which a walk
/// meets; a caller stamps those from its walk.
pub fn external_ignore_inputs(root: &Path) -> Vec<PathBuf> {
    let real = std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
    let mut inputs = Vec::new();
    for dir in real.ancestors().skip(1) {
        inputs.extend(IGNORE_FILES.map(|name| dir.join(name)));
    }
    inputs.extend(real.ancestors().filter_map(git_exclude_file));
    inputs.extend(ignore::gitignore::gitconfig_excludes_path());
    let config_home = crate::process_env::var_os("XDG_CONFIG_HOME")
        .filter(|dir| !dir.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::home_dir().map(|home| home.join(".config")));
    let system = crate::process_env::var_os("GIT_CONFIG_SYSTEM")
        .filter(|path| !path.is_empty())
        .map_or_else(|| PathBuf::from("/etc/gitconfig"), PathBuf::from);
    inputs.extend(
        crate::process_env::var_os("GIT_CONFIG_GLOBAL")
            .filter(|path| !path.is_empty())
            .map(PathBuf::from)
            .into_iter()
            .chain(std::env::home_dir().map(|home| home.join(".gitconfig")))
            .chain(config_home.map(|dir| dir.join("git/config")))
            .chain(Some(system)),
    );
    let mut seen = HashSet::new();
    inputs.retain(|path| seen.insert(path.clone()));
    inputs
}

/// [`walk_builder`](super::walk_builder)'s ignore rules for one path below a walk root: whether
/// the walk leaves the entry out, and which rule decides it. Directory rules
/// load lazily and are cached for the life of the value.
pub struct IgnoreRules {
    root: PathBuf,
    /// `root` with symlinks resolved: the directories above it are read along
    /// its real path, as the walker reads them.
    real_root: Option<PathBuf>,
    /// The directories above the root, nearest first.
    outer: Vec<Level>,
    /// Some directory above the root holds `.git`.
    outer_git: bool,
    global: Gitignore,
    /// The rules in force in each directory at or below the root, by path.
    inner: RwLock<HashMap<PathBuf, Arc<Chain>>>,
}

/// The rules of one directory at or below the root and of each directory
/// between it and the root, nearest first: what decides its entries.
struct Chain {
    levels: Vec<Arc<Level>>,
    /// A directory on the chain or above the root holds `.git`: git rules
    /// apply.
    any_git: bool,
}

impl IgnoreRules {
    pub fn new(root: &Path) -> Self {
        let real_root = std::fs::canonicalize(root).ok();
        let outer: Vec<Level> = real_root
            .iter()
            .flat_map(|real| real.ancestors().skip(1))
            .map(Level::load)
            .collect();
        let (global, _) = Gitignore::global();
        Self {
            root: root.to_path_buf(),
            real_root,
            outer_git: outer.iter().any(|level| level.has_git),
            outer,
            global,
            inner: RwLock::default(),
        }
    }

    /// The rule chain of `dir` (the root or a directory below it), built
    /// once per directory: a parallel walk reads it for every entry, so the
    /// cache is read-locked on the hot path.
    fn chain(&self, dir: &Path) -> Arc<Chain> {
        if let Some(chain) = self
            .inner
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .get(dir)
        {
            return Arc::clone(chain);
        }
        self.chained(dir, Level::load(dir))
    }

    /// The chain of `dir` with `level` its own rules, kept for the walk.
    fn chained(&self, dir: &Path, level: Level) -> Arc<Chain> {
        let mut levels = vec![Arc::new(level)];
        if let Some(parent) = dir
            .parent()
            .filter(|parent| dir != self.root && parent.starts_with(&self.root))
        {
            levels.extend(self.chain(parent).levels.iter().cloned());
        }
        let any_git = self.outer_git || levels.iter().any(|level| level.has_git);
        Arc::clone(
            self.inner
                .write()
                .unwrap_or_else(PoisonError::into_inner)
                .entry(dir.to_path_buf())
                .or_insert_with(|| Arc::new(Chain { levels, any_git })),
        )
    }

    /// The rules of `dir` (the root or a directory below it) that a walk is
    /// entering. `listed`: what its listing shows of the rule files, which
    /// spares looking for them; `None` when the listing is not complete.
    pub fn enter(&self, dir: &Path, listed: Option<ListedRules>) -> DirRules {
        let known = self
            .inner
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .get(dir)
            .cloned();
        DirRules(known.unwrap_or_else(|| match listed {
            Some(listed) => self.chained(dir, Level::listed(dir, listed)),
            None => self.chain(dir),
        }))
    }

    /// Whether the root lies in a git work tree (it or a directory above it
    /// holds `.git`): only there do `.gitignore` rules apply.
    pub(crate) fn in_repository(&self) -> bool {
        self.chain(&self.root).any_git
    }

    /// Whether a walk under the rules leaves out the entry `path` itself
    /// (its directories above are taken as walked). The root is never left
    /// out, and neither is a path outside it.
    pub fn is_ignored(&self, path: &Path, is_dir: bool) -> bool {
        self.decide_entry(path, is_dir).is_ignore()
    }

    /// [`Self::is_ignored`] of an entry of the directory `rules` were
    /// entered for.
    pub(crate) fn is_ignored_in(&self, rules: &DirRules, path: &Path, is_dir: bool) -> bool {
        self.decide_under(&rules.0, path, is_dir, |_| ())
            .is_ignore()
    }

    /// The walker's ignore verdict on the entry `path` itself (its
    /// directories above taken as walked): left out, re-included by a `!`
    /// rule (which also keeps a dot entry), or no rule.
    pub(crate) fn decide_entry(&self, path: &Path, is_dir: bool) -> Match<()> {
        self.decide(path, is_dir, |_| ())
    }

    /// The rule that keeps `path` out of a walk: the rule for the topmost
    /// entry on the way to it that a walk leaves out (a walk never enters an
    /// ignored directory). `None` when a walk reaches it.
    pub fn why(&self, path: &Path, is_dir: bool) -> Option<IgnoreReason> {
        let relative = path.strip_prefix(&self.root).ok()?;
        let parts = relative.components().count();
        let mut prefix = self.root.clone();
        for (index, part) in relative.components().enumerate() {
            prefix.push(part);
            let prefix_is_dir = index + 1 < parts || is_dir;
            let decided = self.decide(&prefix, prefix_is_dir, |glob| {
                (
                    glob.from().map(Path::to_path_buf),
                    glob.original().to_owned(),
                )
            });
            if let Match::Ignore((source, pattern)) = decided {
                return Some(IgnoreReason {
                    entry: prefix,
                    line: source.as_deref().and_then(|file| rule_line(file, &pattern)),
                    source: source.map(|file| self.lexical(&file)),
                    pattern,
                });
            }
        }
        None
    }

    /// `file` spelled under the root's own path: an ignore file above the
    /// root is read along the root's real path (`/private/var/…` for a root
    /// under `/var/…`), and named along the root's.
    fn lexical(&self, file: &Path) -> PathBuf {
        if file.starts_with(&self.root) {
            return file.to_path_buf();
        }
        let Some(real) = &self.real_root else {
            return file.to_path_buf();
        };
        real.ancestors()
            .zip(self.root.ancestors())
            .find_map(|(real, lexical)| Some(lexical.join(file.strip_prefix(real).ok()?)))
            .unwrap_or_else(|| file.to_path_buf())
    }

    /// The deciding rule for one entry, with the walker's precedence: the
    /// nearest `.ignore` rule, else the nearest `.gitignore` rule up to the
    /// repository root, else the repository's exclude file, else the global
    /// gitignore. Git rules apply only inside a repository.
    fn decide<T>(
        &self,
        path: &Path,
        is_dir: bool,
        rule: impl Fn(&ignore::gitignore::Glob) -> T,
    ) -> Match<T> {
        let Ok(relative) = path.strip_prefix(&self.root) else {
            return Match::None;
        };
        if relative.as_os_str().is_empty() {
            return Match::None;
        }
        let Some(chain) = path
            .parent()
            .filter(|dir| dir.starts_with(&self.root))
            .map(|dir| self.chain(dir))
        else {
            return Match::None;
        };
        self.decide_under(&chain, path, is_dir, rule)
    }

    /// [`Self::decide`] of an entry below the root, its directory's rules in
    /// hand.
    fn decide_under<T>(
        &self,
        chain: &Chain,
        path: &Path,
        is_dir: bool,
        rule: impl Fn(&ignore::gitignore::Glob) -> T,
    ) -> Match<T> {
        let any_git = chain.any_git;
        let mut verdicts = Verdicts::NONE;
        let mut saw_git = false;
        for level in &chain.levels {
            verdicts.consult(level, path, is_dir, any_git && !saw_git);
            saw_git |= level.has_git;
        }
        // The rules above the root match the path along the root's real
        // path; it is spelled only when a rule there is still consulted.
        let mut outer_path = None;
        for level in &self.outer {
            let git = any_git && !saw_git;
            let consulted = !level.dot_ignore.is_empty()
                || (git && !(level.gitignore.is_empty() && level.exclude.is_empty()));
            if consulted
                && let Some(matched) = outer_path
                    .get_or_insert_with(|| {
                        let relative = path.strip_prefix(&self.root).ok()?;
                        self.real_root.as_ref().map(|real| real.join(relative))
                    })
                    .as_deref()
            {
                verdicts.consult(level, matched, is_dir, git);
            }
            saw_git |= level.has_git;
        }
        let Verdicts {
            dot_ignore,
            gitignore,
            exclude,
        } = verdicts;
        let global = if any_git {
            self.global.matched(path, is_dir)
        } else {
            Match::None
        };
        dot_ignore.or(gitignore).or(exclude).or(global).map(rule)
    }
}

/// The nearest rule of each kind met so far on the way up from an entry.
struct Verdicts<'a> {
    dot_ignore: Match<&'a ignore::gitignore::Glob>,
    gitignore: Match<&'a ignore::gitignore::Glob>,
    exclude: Match<&'a ignore::gitignore::Glob>,
}

impl<'a> Verdicts<'a> {
    const NONE: Self = Self {
        dot_ignore: Match::None,
        gitignore: Match::None,
        exclude: Match::None,
    };

    /// Consult `level`'s rules for `path` where no nearer rule decided;
    /// its git rules only when `git` (inside a repository, up to its root).
    fn consult(&mut self, level: &'a Level, path: &Path, is_dir: bool, git: bool) {
        if self.dot_ignore.is_none() {
            self.dot_ignore = level.dot_ignore.matched(path, is_dir);
        }
        if git {
            if self.gitignore.is_none() {
                self.gitignore = level.gitignore.matched(path, is_dir);
            }
            if self.exclude.is_none() {
                self.exclude = level.exclude.matched(path, is_dir);
            }
        }
    }
}

/// The last line of `file` that spells `pattern` (later rules win).
fn rule_line(file: &Path, pattern: &str) -> Option<usize> {
    let text = std::fs::read_to_string(file).ok()?;
    text.lines()
        .enumerate()
        .filter(|(_, line)| line.trim_end() == pattern)
        .last()
        .map(|(index, _)| index + 1)
}
