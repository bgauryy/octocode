//! What an astSearch scan that stopped at the walk ceiling never read, and the
//! calls that read it.
//!
//! The walk is name-sorted and depth-first, so the candidates it did not read
//! are the first one it left out (the frontier) plus, for each directory above
//! the frontier, the entries that sort after the child leading to it. A
//! depth-1 listing of those directories, under the walk's own ignore, prune,
//! exclude and path-policy rules, names them: each unread directory is a scope
//! to re-run the query over, and a directory's unread direct files are one
//! more (its subdirectories excluded, the candidates the scan read before the
//! leading child skipped by `scanOffset`). A directory that alone holds more
//! candidates than the ceiling has no such scope: no narrower `path` reaches
//! its files, which is a typed terminal limit.
use crate::response::result::Confidence;
use octocode_engine::types::{EntryType, FileSystemQueryOptions, FileSystemQueryResult};
use serde_json::{Value, json};
use std::path::Path;

use crate::id::ToolId;

/// The most astSearch leads one terminal row carries; scopes past it are
/// counted and listed through one structureSearch lead.
pub(super) const MAX_LEADS: usize = 8;

/// The most subdirectories of one directory a listing names.
const MAX_SIBLING_DIRECTORIES: u32 = 10_000;

/// The `partialReasons` entry of a directory holding more candidates than the
/// walk ceiling.
pub(super) const FLAT_DIRECTORY: &str = "flatDirectory";

/// The exclude that leaves a directory's own files and drops every
/// subdirectory below it (gitignore syntax: anchored, directories only).
const SUBDIRECTORIES: &str = "/*/";

/// One scope to re-run the query over.
#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub(super) struct Reach {
    /// The directory (absolute).
    pub(super) dir: String,
    /// Levels of `dir` below the scanned root (the root is 0).
    pub(super) depth: u32,
    /// Only the directory's own files: its subdirectories are other scopes.
    pub(super) files_only: bool,
    /// Candidates of the directory's own files the cut scan already read,
    /// which sort first: the call skips them (`scanOffset`). Always 0 for a
    /// scope of a whole directory.
    pub(super) skip: u32,
}

/// A directory with more candidate files than the walk ceiling.
#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub(super) struct Flat {
    pub(super) dir: String,
    pub(super) files: usize,
}

/// The unread scopes of a scan cut at the walk ceiling.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub(super) struct Unwalked {
    /// In walk order: the scopes the walk would have reached first, first.
    scopes: Vec<Reach>,
    flat: Vec<Flat>,
    /// Unread subdirectories a listing did not name (it holds too many).
    unlisted: usize,
    /// The glob that names a position in the tree, so a narrower `path`
    /// would change what it matches: no scope is offered.
    position_dependent: Option<String>,
}

/// The walk a scan ran, as far as a listing needs to repeat its rules.
pub(super) struct Rules<'a> {
    pub(super) root: &'a Path,
    /// The first candidate the walk left out.
    pub(super) frontier: &'a Path,
    pub(super) ignore: octocode_engine::search::walk::IgnorePolicy,
    pub(super) prune: Vec<String>,
    /// The query's `include` globs: a file they miss is no candidate.
    pub(super) include: Option<Vec<String>>,
    pub(super) exclude: Option<Vec<String>>,
    /// Whether a file is a candidate of the scan: the scan's own rule for the
    /// files it can read (a directory is never asked).
    pub(super) candidate: &'a (dyn Fn(&Path) -> bool + Sync),
    /// Levels below `root` the scan reads (1 = its children).
    pub(super) max_depth: Option<u32>,
    pub(super) admit: &'a (dyn Fn(&Path, Option<std::fs::FileType>) -> bool + Sync),
    pub(super) stop: &'a octocode_engine::search::walk::Stop<'a>,
}

/// Whether the walk can have stopped at the ceiling for a scan whose
/// `maxFiles` is `max_files`: unset, or as high as the ceiling.
pub(super) fn at_ceiling(max_files: Option<u32>) -> bool {
    max_files.is_none_or(|files| files as usize >= octocode_engine::scan_budget::walk_ceiling())
}

/// The first glob of the query `fields` that matches by position (anchored
/// or with a `/` inside), so another `path` would match something else.
fn position_dependent(query: &Value) -> Option<String> {
    let independent = |glob: &str| {
        let glob = glob.strip_prefix('!').unwrap_or(glob);
        glob.starts_with("**/") || !glob.trim_end_matches('/').contains('/')
    };
    let strings = |value: &Value| -> Vec<String> {
        match value {
            Value::Array(items) => items
                .iter()
                .filter_map(|item| item.as_str().map(str::to_owned))
                .collect(),
            _ => Vec::new(),
        }
    };
    let mut globs = strings(&query["include"]);
    globs.extend(strings(&query["exclude"]));
    if let Some(by_language) = query["languageGlobs"].as_object() {
        globs.extend(by_language.values().flat_map(strings));
    }
    globs.into_iter().find(|glob| !independent(glob))
}

impl Unwalked {
    /// The unread scopes of a scan of `query` (its wire form) that walked
    /// `rules`; `None` when listing them failed.
    pub(super) fn of(rules: &Rules<'_>, query: &Value) -> Option<Self> {
        let mut unwalked = Self {
            position_dependent: position_dependent(query),
            ..Self::default()
        };
        let relative = rules.frontier.strip_prefix(rules.root).ok()?;
        let names: Vec<String> = relative
            .components()
            .map(|part| part.as_os_str().to_string_lossy().into_owned())
            .collect();
        let mut dir = rules.root.to_path_buf();
        let mut levels = Vec::new();
        for (depth, name) in names.iter().enumerate() {
            let last = depth + 1 == names.len();
            levels.push(unwalked.level(rules, &dir, u32::try_from(depth).ok()?, name, last)?);
            dir.push(name);
        }
        // The deepest directory holds what the walk would have read first.
        unwalked.scopes = levels.into_iter().rev().flatten().collect();
        Some(unwalked)
    }

    /// The scopes `dir` leaves unread, given that the walk stopped inside its
    /// child `leading` (or at it, when `last`: the frontier file itself).
    fn level(
        &mut self,
        rules: &Rules<'_>,
        dir: &Path,
        depth: u32,
        leading: &str,
        last: bool,
    ) -> Option<Vec<Reach>> {
        let unread = |name: &str| {
            if last {
                name >= leading
            } else {
                name > leading
            }
        };
        let within = |depth: u32| rules.max_depth.is_none_or(|deepest| depth < deepest);
        let mut scopes = Vec::new();
        let directories = list(rules, dir, EntryType::Directory, MAX_SIBLING_DIRECTORIES)?;
        self.unlisted +=
            (directories.total_discovered as usize).saturating_sub(directories.entries.len());
        for entry in directories
            .entries
            .iter()
            .filter(|entry| unread(&entry.name))
        {
            if within(depth + 1) {
                scopes.push(Reach {
                    dir: entry.path.clone(),
                    depth: depth + 1,
                    files_only: false,
                    skip: 0,
                });
            }
        }
        let ceiling = octocode_engine::scan_budget::walk_ceiling();
        let files = list(
            rules,
            dir,
            EntryType::File,
            u32::try_from(ceiling.saturating_add(1)).unwrap_or(u32::MAX),
        )?;
        if within(depth) && files.entries.iter().any(|entry| unread(&entry.name)) {
            let total = files.total_discovered as usize;
            if total > ceiling {
                self.flat.push(Flat {
                    dir: dir.to_string_lossy().into_owned(),
                    files: total,
                });
            } else {
                // The walk read the candidates that sort before the leading
                // child; the scope starts after them.
                let read = files.entries.iter().filter(|entry| !unread(&entry.name));
                scopes.push(Reach {
                    dir: dir.to_string_lossy().into_owned(),
                    depth,
                    files_only: true,
                    skip: u32::try_from(read.count()).ok()?,
                });
            }
        }
        Some(scopes)
    }

    /// Names the unread scopes on a terminal row: one `narrowScope` hint per
    /// scope (`lead` builds its call, `None` when it cannot), the typed
    /// `flatDirectory` limit, and what no hint reaches.
    pub(super) fn attach(&self, out: &mut Value, mut lead: impl FnMut(&Reach) -> Option<Value>) {
        for flat in &self.flat {
            crate::response::paging::terminal_limit(
                out,
                FLAT_DIRECTORY,
                &[format!(
                    "{} holds {} files, over the {} file walk ceiling: no narrower path reaches the files the scan did not read; split them with exclude or include globs.",
                    flat.dir,
                    flat.files,
                    octocode_engine::scan_budget::walk_ceiling()
                )],
            );
        }
        if let Some(glob) = &self.position_dependent {
            crate::response::paging::push_warnings(
                out,
                [format!(
                    "The glob {glob:?} matches by position, so no narrower path keeps its meaning: no narrowScope hint is offered for the unread scopes."
                )],
            );
            return;
        }
        let mut named = 0;
        let mut unled = Vec::new();
        for reach in &self.scopes {
            let call = (named < MAX_LEADS).then(|| lead(reach)).flatten();
            match call {
                Some(call) => {
                    out["next"][crate::id::numbered("narrowScope", named)] = call;
                    named += 1;
                }
                None => unled.push(reach),
            }
        }
        let hidden = unled.len() + self.unlisted;
        if hidden == 0 {
            return;
        }
        let listed_in = unled.first().map(|reach| {
            let dir = Path::new(&reach.dir);
            if reach.files_only {
                dir.to_path_buf()
            } else {
                dir.parent()
                    .map_or_else(|| dir.to_path_buf(), Path::to_path_buf)
            }
        });
        let mut warning = format!(
            "{hidden} more unread scopes have no narrowScope hint (at most {MAX_LEADS} are offered)"
        );
        if let Some(dir) = listed_in {
            let key = crate::id::numbered("narrowScope", named);
            out["next"][&key] = crate::response::result::Continuation::new(
                ToolId::StructureSearch,
                json!({"operation":"tree","path":dir.to_string_lossy(),"maxDepth":1}),
            )
            .build();
            warning.push_str(&format!(
                "; {key} lists the directories under {}",
                dir.display()
            ));
        }
        warning.push('.');
        crate::response::paging::push_warnings(out, [warning]);
    }
}

/// The direct children of `dir` of one kind, under the walk's
/// rules; at most `limit` are stored, all are counted (`total_discovered`).
fn list(
    rules: &Rules<'_>,
    dir: &Path,
    kind: EntryType,
    limit: u32,
) -> Option<FileSystemQueryResult> {
    let admit = |path: &Path, file_type: Option<std::fs::FileType>, _| {
        // The walk follows no link.
        let link = file_type.map_or_else(|| path.is_symlink(), |kind| kind.is_symlink());
        let directory = file_type.map_or_else(|| path.is_dir(), |kind| kind.is_dir());
        !link && (rules.admit)(path, file_type) && (directory || (rules.candidate)(path))
    };
    octocode_engine::search::fs_query::query_file_system(
        FileSystemQueryOptions {
            path: dir.to_string_lossy().into_owned(),
            recursive: Some(false),
            ignore: rules.ignore,
            entry_type: Some(kind),
            names: if kind == EntryType::File {
                rules.include.clone()
            } else {
                None
            },
            default_exclude_dir: Some(rules.prune.clone()),
            exclude: rules.exclude.clone(),
            // Every entry is counted: a directory over the ceiling states its size.
            stop_at_limit: Some(false),
            limit: Some(limit),
            ..Default::default()
        },
        &admit,
        rules.stop,
    )
    .ok()
}

/// The query of `reach`: `query` over its directory, the subdirectories of a
/// files-only scope excluded and the candidates the cut scan read skipped,
/// `maxDepth` counted from the new root, and the page and scan cursors of
/// the cut scan dropped. `None` when `maxDepth`
/// leaves the scope nothing.
pub(super) fn narrowed(query: &Value, reach: &Reach) -> Option<Value> {
    let mut changes = json!({"path": reach.dir, "scanOffset": null, "snapshot": null});
    // Page cursors restart (`matchPage` is a `match` field only).
    for cursor in ["page", "matchPage"] {
        if query.get(cursor).is_some() {
            changes[cursor] = json!(1);
        }
    }
    if let Some(deepest) = query["maxDepth"].as_u64() {
        let remaining = deepest
            .checked_sub(u64::from(reach.depth))
            .filter(|n| *n > 0)?;
        changes["maxDepth"] = json!(remaining);
    }
    if reach.files_only {
        let mut exclude = query["exclude"].as_array().cloned().unwrap_or_default();
        exclude.push(json!(SUBDIRECTORIES));
        changes["exclude"] = Value::Array(exclude);
        if reach.skip > 0 {
            changes["scanOffset"] = json!(reach.skip);
        }
    }
    let narrowed = crate::response::result::with_changes(query.clone(), &changes);
    Some(
        crate::response::result::Continuation::new(ToolId::AstSearch, narrowed)
            .confidence(Confidence::Exact)
            .build(),
    )
}

#[cfg(test)]
mod numbering_tests {
    use super::{Reach, Unwalked};
    use serde_json::json;

    /// With no scope led, the listing lead is the first `narrowScope`, and
    /// the warning names it as published.
    #[test]
    fn a_listing_with_no_scope_led_is_the_first_narrow_scope() {
        let unwalked = Unwalked {
            scopes: vec![Reach {
                dir: "/r/a".into(),
                depth: 1,
                files_only: false,
                skip: 0,
            }],
            ..Unwalked::default()
        };
        let mut out = json!({});
        unwalked.attach(&mut out, |_| None);
        let names = out["next"]
            .as_object()
            .expect("next")
            .keys()
            .collect::<Vec<_>>();
        assert_eq!(names, ["narrowScope"], "{out}");
        assert!(
            out["warnings"]
                .to_string()
                .contains("; narrowScope lists the directories under /r"),
            "{out}"
        );
    }
}
