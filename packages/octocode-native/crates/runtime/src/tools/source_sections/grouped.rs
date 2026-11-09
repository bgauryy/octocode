//! Section pages for assessment: adjacent small sections share one page, so a
//! file of many short declarations is not one provider call per item. A page
//! is only a share of the section grain, so a file that fits one section
//! still splits into pages an answer can point into.
use super::{Section, visit};
use std::ops::Range;
use std::path::Path;

/// Kind of a page that joins more than one section; its `name` is its first
/// and last member's and its count ([`Group::finish`]).
pub(crate) const GROUP_KIND: &str = "declarations";

/// A merged page holds at most this share of the section grain
/// (`target_chars`), the same share a section's supporting context gets. Whole
/// pages at the full grain made every file under one section's size a single
/// page, which no answer could narrow.
const PAGE_SHARE: usize = 4;

/// [`visit`], with consecutive byte-contiguous sections merged while their
/// combined target stays within a share of `target_chars` (a section already
/// larger than the share stays alone). A merged page keeps the first
/// member's supporting context and covers exactly its members' bytes, so
/// every selected byte still belongs to exactly one target.
pub(crate) fn visit_grouped<E>(
    source: &str,
    path: &Path,
    selected: &[Range<usize>],
    target_chars: usize,
    mut emit: impl FnMut(Section) -> Result<bool, E>,
) -> Result<bool, E> {
    let mut pending: Option<Group> = None;
    let page_chars = target_chars / PAGE_SHARE;
    let complete = visit(source, path, selected, target_chars, |section| {
        let chars = source[section.target.clone()].chars().count();
        if let Some(group) = &mut pending
            && group.section.target.end == section.target.start
            && group.chars.saturating_add(chars) <= page_chars
        {
            group.join(section, chars);
            return Ok(true);
        }
        if let Some(group) = pending.replace(Group::new(section, chars))
            && !emit(group.finish())?
        {
            pending = None;
            return Ok(false);
        }
        Ok(true)
    })?;
    match pending {
        Some(group) if complete => emit(group.finish()),
        _ => Ok(complete),
    }
}

struct Group {
    section: Section,
    names: Vec<String>,
    members: usize,
    chars: usize,
}

impl Group {
    fn new(section: Section, chars: usize) -> Self {
        Self {
            names: section.name.iter().cloned().collect(),
            section,
            members: 1,
            chars,
        }
    }

    fn join(&mut self, section: Section, chars: usize) {
        self.section.target.end = section.target.end;
        self.names.extend(section.name);
        self.members += 1;
        self.chars += chars;
    }

    /// A merged page is named by its first and last named member, then how
    /// many sections it joins, named or not. Its target lines hold every
    /// member and sit beside the name, as does the page's own id, so the
    /// name repeats neither: names may repeat where declaration names do.
    fn finish(mut self) -> Section {
        if self.members > 1 {
            let span = format!("{} {GROUP_KIND}", self.members);
            self.section.kind = GROUP_KIND.to_owned();
            self.section.name = Some(match self.names.as_slice() {
                [] => span,
                [only] => format!("{only} ({span})"),
                [first, last] if self.members == 2 => format!("{first}, {last} ({span})"),
                [first, .., last] => format!("{first} … {last} ({span})"),
            });
        }
        self.section
    }
}
