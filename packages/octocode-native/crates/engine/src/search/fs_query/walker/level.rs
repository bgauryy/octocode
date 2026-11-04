//! One directory of the walk, entry by entry: each decided, listed and counted,
//! the directories under it entered and read ahead.

use std::fs;
use std::path::{Path, PathBuf};

use super::super::compiled::{CompiledQuery, Verdict};
use super::super::filters::{EntryKind, lstat, visit_path};
use super::super::{EntryFilter, Visit};
use super::{MAX_RECURSION_DEPTH, QueryState, walk_children, walk_frame};
use crate::error::Error;
use crate::search::walk::{DirRules, Stop};
use crate::types::fs_resume::{Child, Frame};

/// Entries the walk inspects between two looks at its deadline. A test that
/// advances the walk clock entry by entry looks at every one.
const ENTRIES_PER_CLOCK_LOOK: u32 = if cfg!(any(test, feature = "test-hooks")) {
    1
} else {
    64
};

/// Queue the directories of `children` (entries of `base`) the walk would
/// enter, as far as their names and ignore rules tell, to be read ahead: a dot
/// or ignored one can be far larger than the rest of the tree.
fn queue_ahead(
    children: &[Child],
    base: &Path,
    query: &CompiledQuery,
    state: &mut QueryState,
    rules: Option<&DirRules>,
) {
    let Some(ahead) = state.ahead.as_mut() else {
        return;
    };
    ahead.queue(
        children
            .iter()
            .filter(|child| child.file_type.is_some_and(|kind| kind.is_dir()))
            .filter(|child| query.show_hidden || !child.name.as_encoded_bytes().starts_with(b"."))
            .map(|child| base.join(&child.name))
            .filter(|dir| {
                query
                    .ignore_rules
                    .as_ref()
                    .zip(rules)
                    .is_none_or(|(ignore, rules)| !ignore.is_ignored_in(rules, dir, true))
            }),
    );
}

/// Whether the walk's time budget ran out at this entry, which it then names,
/// leaving the cursor that resumes there. The budget starts at the first new
/// entry, which a positive budget always inspects: every resumed window makes
/// progress.
fn time_ran_out(
    base: &std::path::Path,
    file_name: &std::ffi::OsStr,
    query: &CompiledQuery,
    state: &mut QueryState,
) -> bool {
    let Some(budget) = query.time_budget else {
        return false;
    };
    let first = state.deadline.is_none();
    let deadline = *state
        .deadline
        .get_or_insert_with(|| crate::scan_budget::walk_now() + budget);
    let looks = state.position.is_multiple_of(ENTRIES_PER_CLOCK_LOOK);
    if !(budget.is_zero() || (!first && looks)) || crate::scan_budget::walk_now() < deadline {
        return false;
    }
    let path = base.join(file_name);
    let relative = path.strip_prefix(&query.root).unwrap_or(&path);
    state.time_cut = Some(crate::text::slash_path(relative));
    state.cursor = Some(state.cursor(query.shape, None));
    true
}

pub(super) fn walk_level(
    level: usize,
    inner: &[Frame],
    query: &CompiledQuery,
    state: &mut QueryState,
    admit: &EntryFilter<'_>,
    stop: &Stop<'_>,
    rules: Option<&DirRules>,
) -> Result<(), Error> {
    if let Some((first, rest)) = inner.split_first() {
        walk_frame(first.clone(), rest, query, state, admit, stop)?;
        if state.finished(query) {
            return Ok(());
        }
    }
    let frame = &state.stack[level];
    let (base, depth, start) = (frame.base.clone(), frame.depth, frame.next);
    let children = frame.children.clone().unwrap_or_default();
    // Child directories this one reads at the next depth: the ones read
    // ahead while the walk is inside an earlier sibling.
    let reads_next = query.recursive
        && depth < MAX_RECURSION_DEPTH
        && query.max_depth.is_none_or(|max_depth| depth < max_depth);
    // The directory child last inspected: when the walk did not enter it,
    // its listing read ahead is dropped.
    let mut passed: Option<PathBuf> = None;
    // Each directory's subdirectories are queued once, as soon as the walk
    // reads ahead at all.
    let mut queued = false;

    for (index, child) in children.iter().enumerate().skip(start) {
        if let Some(dir) = passed.take()
            && let Some(ahead) = state.ahead.as_mut()
        {
            ahead.drop_unused(&dir);
        }
        if reads_next && !queued && state.ahead.is_some() {
            queued = true;
            queue_ahead(&children[index..], &base, query, state, rules);
        }
        // The next entry of this directory a resume inspects.
        state.stack[level].next = index;
        // Look ahead to one additional matching entry before declaring overflow.
        // Reaching the stored-entry cap alone does not prove the scan is partial.
        if query.stop_at_limit && state.total_discovered as usize > query.limit {
            return Ok(());
        }
        if stop() {
            return Err(Error::cancelled());
        }
        let file_name = child.name.as_os_str();
        // The leading `skip_entries` entries were inspected by the walk this
        // one resumes: replayed below, they spend none of the time budget.
        let replaying = state.position < query.skip_entries;
        if !replaying && time_ran_out(&base, file_name, query, state) {
            return Ok(());
        }
        state.position += 1;
        state.stack[level].next = index + 1;

        let path = base.join(file_name);
        let file_type = child.file_type;
        let mut metadata = None;
        let kind = match file_type {
            Some(file_type) => EntryKind::of(file_type),
            // A replayed entry was counted by the walk it resumes.
            None if replaying => match fs::symlink_metadata(&path) {
                Ok(meta) => meta.file_type().into(),
                Err(_) => continue,
            },
            None => match lstat(&path, state) {
                Some(meta) => metadata.insert(meta).file_type().into(),
                None => continue,
            },
        };
        let is_directory = kind.is_dir;
        if is_directory {
            passed = Some(path.clone());
        }
        // A replayed entry only counts: the walk it resumes listed it or
        // not, and entered it or not by this same verdict, which only a
        // directory's entry needs again.
        if replaying && !(query.recursive && is_directory) {
            continue;
        }
        let visit = if replaying {
            Visit::Replayed
        } else {
            Visit::Fresh {
                position: state.position,
            }
        };
        match query.verdict(
            &path,
            file_name,
            is_directory,
            (file_type, visit),
            admit,
            rules,
        ) {
            Verdict::Ignored { counted } => {
                if counted && !replaying {
                    let relative = path.strip_prefix(&query.root).unwrap_or(&path);
                    let mut relative = crate::text::slash_path(relative);
                    if is_directory {
                        relative.push('/');
                    }
                    state.tally.ignored(state.position, relative);
                }
                continue;
            }
            Verdict::Pruned => {
                if !replaying {
                    let relative = path.strip_prefix(&query.root).unwrap_or(&path);
                    state
                        .tally
                        .pruned(state.position, crate::text::slash_path(relative));
                }
                continue;
            }
            Verdict::Left => continue,
            Verdict::Enters => {}
        }

        if !replaying {
            let listed = state.entries.len();
            if visit_path(
                &path,
                &file_name.to_string_lossy(),
                depth,
                kind,
                &mut metadata,
                query,
                state,
            )
            .is_err()
            {
                continue;
            }
            if state.entries.len() > listed && state.position > 0 {
                state.listed += 1;
                if query.window_rows == Some(state.listed) {
                    let entering = (query.recursive && is_directory).then(|| Frame {
                        base: path.clone(),
                        depth: depth + 1,
                        children: None,
                        read_at: None,
                        next: 0,
                    });
                    state.window_cursor = Some(state.cursor(query.shape, entering));
                }
            }
        }

        if query.recursive && is_directory {
            walk_children(&path, depth + 1, query, state, admit, stop)?;
            if state.finished(query) {
                return Ok(());
            }
        }
    }
    if let Some(dir) = passed
        && let Some(ahead) = state.ahead.as_mut()
    {
        ahead.drop_unused(&dir);
    }
    Ok(())
}
