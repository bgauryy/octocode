//! A match row cut from a private-key block or a private JSON Web Key. A parser
//! reads source unredacted, and one string of a key pasted across lines
//! (`"MIIE…\n" +`), or a `d` with `kty` above it, holds nothing the output gate
//! could recognize, so the row is judged over the whole file. What the file
//! says is kept for its other pages.
use super::page_rows::PageRows;
use super::types::Group;
use crate::tools::page_memo::PageMemo;
use serde_json::Value;
use std::collections::BTreeMap;
use std::sync::Arc;

/// Stands for text a row shows when its source could not be read to check it.
const UNREADABLE: &str = "[REDACTED: source unreadable for secret check]";

/// The lines of a file a row cut from it cannot be judged alone, and their
/// text. A pure function of the path and the bytes the scan's digest names, so
/// it is kept per snapshot and served only to a page whose stored scan just
/// proved every source unchanged. In memory only, unredacted as a stored scan's
/// matches are: a row it hides still passes the output gate.
#[derive(Default)]
struct Judged {
    blocks: Vec<(u32, u32)>,
    lines: BTreeMap<u32, String>,
}

impl Judged {
    fn of(text: &str, file: &std::path::Path) -> Self {
        let blocks = crate::security::lines_a_row_cannot_judge(text, file);
        let lines = if blocks.is_empty() {
            BTreeMap::new()
        } else {
            text.lines()
                .enumerate()
                .filter_map(|(at, line)| {
                    let number = u32::try_from(at + 1).ok()?;
                    blocks
                        .iter()
                        .any(|&(start, end)| (start..=end).contains(&number))
                        .then(|| (number, line.to_owned()))
                })
                .collect()
        };
        Self { blocks, lines }
    }

    /// About how many bytes it holds.
    fn weight(&self) -> usize {
        self.blocks.len() * std::mem::size_of::<(u32, u32)>()
            + self
                .lines
                .values()
                .map(|line| line.len() + std::mem::size_of::<(u32, String)>())
                .sum::<usize>()
    }
}

static JUDGED: PageMemo<Arc<Judged>> = PageMemo::new();

/// What `file` says of its rows, `None` when it cannot be read. `shown` is its
/// row path and `digest` the digest of the bytes the scan matched: bytes that
/// differ answer this page and are not kept.
fn judged(
    file: &std::path::Path,
    paths: &crate::policy::path::PathPolicy,
    (page, shown, digest): (PageRows<'_>, &str, Option<&str>),
) -> Option<Arc<Judged>> {
    if page.proved
        && let Some(kept) = JUDGED.get(&page.id(shown), page.key, Arc::clone)
    {
        return Some(kept);
    }
    let bytes = paths
        .validate_read(file)
        .ok()
        .and_then(|_| octocode_engine::text::open_regular(file).ok())
        .and_then(|(mut opened, _)| {
            let mut bytes = Vec::new();
            std::io::Read::read_to_end(&mut opened, &mut bytes)
                .ok()
                .map(|_| bytes)
        })?;
    #[cfg(test)]
    JUDGMENTS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .push(file.to_owned());
    let judged = Arc::new(Judged::of(
        &octocode_engine::text::decode(&bytes).text,
        file,
    ));
    if digest.is_some_and(|digest| octocode_engine::digest::sha256(&bytes) == digest) {
        JUDGED.put(
            page.id(shown),
            page.key.to_owned(),
            judged.weight(),
            Arc::clone(&judged),
        );
    }
    Some(judged)
}

/// Every file a page judged over its whole text, for the tests that count them.
#[cfg(test)]
static JUDGMENTS: std::sync::Mutex<Vec<std::path::PathBuf>> = std::sync::Mutex::new(Vec::new());

/// How many times a page judged `file` over its whole text.
#[cfg(test)]
pub(super) fn judgments(file: &std::path::Path) -> usize {
    JUDGMENTS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .iter()
        .filter(|judged| judged.as_path() == file)
        .count()
}

/// Hide what the rows of a page's `groups` show of their files' private-key
/// blocks.
pub(super) fn hide_page_key_blocks(
    groups: &mut [Group],
    (scan, scope): (&super::types::Scan, &super::types::Scope),
    row_paths: super::super::RowPaths<'_>,
    paths: &crate::policy::path::PathPolicy,
    page: PageRows<'_>,
) {
    let digests = scan
        .files
        .iter()
        .filter_map(|file| Some((file.path.as_str(), file.source.as_ref()?.1.as_str())))
        .collect::<std::collections::HashMap<_, _>>();
    for group in groups {
        if let Some(shown) = group.row["path"].as_str().map(str::to_owned) {
            // A single file's scan digests its one source.
            let digest = digests
                .get(shown.as_str())
                .copied()
                .or_else(|| scope.is_file.then_some(scan.source_digest.as_str()));
            hide_key_block_rows(
                group,
                &row_paths.file(&shown),
                paths,
                (page, &shown, digest),
            );
        }
    }
}

/// Hide what `group`'s rows show of `file`'s private-key blocks.
fn hide_key_block_rows(
    group: &mut Group,
    file: &std::path::Path,
    paths: &crate::policy::path::PathPolicy,
    (page, shown, digest): (PageRows<'_>, &str, Option<&str>),
) {
    let Some(rows) = group
        .row
        .get_mut("matches")
        .and_then(Value::as_array_mut)
        .filter(|rows| !rows.is_empty())
    else {
        return;
    };
    let Some(judged) = judged(file, paths, (page, shown, digest)) else {
        for row in rows {
            hide_leaves(row, &|_| Some(UNREADABLE.to_owned()));
        }
        return;
    };
    if judged.blocks.is_empty() {
        return;
    }
    for row in rows {
        let Some(first) = row["line"].as_u64() else {
            continue;
        };
        let last = row["endLine"].as_u64().unwrap_or(first);
        let hidden = judged
            .blocks
            .iter()
            .flat_map(|&(start, end)| {
                let from = u64::from(start).max(first);
                let to = u64::from(end).min(last);
                from..=to
            })
            .filter_map(|line| judged.lines.get(&u32::try_from(line).ok()?))
            .map(String::as_str)
            .collect::<Vec<_>>();
        if !hidden.is_empty() {
            hide_leaves(row, &|line| {
                hidden
                    .iter()
                    .any(|source| source.contains(line))
                    .then(|| crate::security::hide_key_line(line))
            });
        }
    }
}

/// Replace each line of every string under `value` that `hides` names a
/// replacement for: the row's own text and its capture texts.
fn hide_leaves(value: &mut Value, hides: &impl Fn(&str) -> Option<String>) {
    match value {
        Value::String(text) => {
            let mut changed = false;
            let lines = text
                .split('\n')
                .map(|line| {
                    let core = crate::security::cut::strip_clip_markers(line).trim();
                    match hides(core).filter(|_| !core.is_empty()) {
                        Some(replacement) => {
                            changed = true;
                            replacement
                        }
                        None => line.to_owned(),
                    }
                })
                .collect::<Vec<_>>()
                .join("\n");
            if changed {
                *text = lines;
            }
        }
        Value::Array(items) => items.iter_mut().for_each(|item| hide_leaves(item, hides)),
        Value::Object(fields) => fields
            .values_mut()
            .for_each(|field| hide_leaves(field, hides)),
        _ => {}
    }
}
