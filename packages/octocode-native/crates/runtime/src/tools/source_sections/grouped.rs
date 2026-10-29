//! Section pages for assessment: adjacent small sections share one page, so a
//! file of many short declarations is not one provider call per item.
use super::{Section, visit};
use std::ops::Range;
use std::path::Path;

/// Kind of a page that joins more than one section; its `name` lists the
/// members' names in source order.
pub(crate) const GROUP_KIND: &str = "declarations";

/// [`visit`], with consecutive byte-contiguous sections merged while their
/// combined target stays within `target_chars`. A merged page keeps the first
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
    let complete = visit(source, path, selected, target_chars, |section| {
        let chars = source[section.target.clone()].chars().count();
        if let Some(group) = &mut pending
            && group.section.target.end == section.target.start
            && group.chars.saturating_add(chars) <= target_chars
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

    fn finish(mut self) -> Section {
        if self.members > 1 {
            self.section.kind = GROUP_KIND.to_owned();
            self.section.name = (!self.names.is_empty()).then(|| self.names.join(", "));
        }
        self.section
    }
}
