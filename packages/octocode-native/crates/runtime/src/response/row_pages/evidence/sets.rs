use super::*;
use std::collections::BTreeSet;

/// Private ordered indices belong to exactly one catalog; accepted items insert only new indices.
#[derive(Clone, Debug)]
pub(crate) struct DependencySet {
    pub(super) indices: [BTreeSet<usize>; 3],
    pub(super) owner: Arc<()>,
    member_chars: usize,
}

impl DependencySet {
    pub(crate) fn is_empty(&self) -> bool {
        self.indices[UNITS].is_empty()
    }
}

impl Catalog {
    pub(crate) fn empty(&self) -> DependencySet {
        DependencySet {
            indices: Default::default(),
            owner: Arc::clone(&self.owner),
            member_chars: 0,
        }
    }

    pub(super) fn check_owner(&self, set: &DependencySet) -> Result<(), DependencyError> {
        if Arc::ptr_eq(&self.owner, &set.owner) {
            Ok(())
        } else {
            Err(DependencyError::ForeignSet)
        }
    }

    /// Exact serialized catalog size; excludes the enclosing object's property key/comma.
    pub(crate) fn chars(&self, set: &DependencySet) -> Result<usize, DependencyError> {
        self.check_owner(set)?;
        if set.is_empty() {
            return Ok(0);
        }
        let commas = set.indices.iter().try_fold(0usize, |sum, indices| {
            sum.checked_add(indices.len().saturating_sub(1))
                .ok_or(DependencyError::SizeOverflow)
        })?;
        self.empty_chars
            .checked_add(set.member_chars)
            .and_then(|chars| chars.checked_add(commas))
            .ok_or(DependencyError::SizeOverflow)
    }

    /// Read-only candidate price: visits added indices only, without copying the growing set.
    pub(crate) fn delta(
        &self,
        target: &DependencySet,
        added: &DependencySet,
        cancel: &CancellationToken,
    ) -> Result<usize, DependencyError> {
        self.additions(target, added, cancel)
            .map(|(delta, _)| delta)
    }

    fn additions(
        &self,
        target: &DependencySet,
        added: &DependencySet,
        cancel: &CancellationToken,
    ) -> Result<(usize, usize), DependencyError> {
        check(cancel)?;
        self.check_owner(target)?;
        self.check_owner(added)?;
        let (mut delta, mut member_chars) = (0usize, 0usize);
        for kind in 0..3 {
            let mut new_count = 0usize;
            for &index in &added.indices[kind] {
                check(cancel)?;
                if !target.indices[kind].contains(&index) {
                    new_count = new_count
                        .checked_add(1)
                        .ok_or(DependencyError::SizeOverflow)?;
                    member_chars = member_chars
                        .checked_add(self.rows[kind].chars[index])
                        .ok_or(DependencyError::SizeOverflow)?;
                }
            }
            let old_count = target.indices[kind].len();
            let merged_count = old_count
                .checked_add(new_count)
                .ok_or(DependencyError::SizeOverflow)?;
            delta = delta
                .checked_add(merged_count.saturating_sub(1) - old_count.saturating_sub(1))
                .ok_or(DependencyError::SizeOverflow)?;
        }
        if target.is_empty() && !added.is_empty() {
            delta = delta
                .checked_add(self.empty_chars)
                .ok_or(DependencyError::SizeOverflow)?;
        }
        delta = delta
            .checked_add(member_chars)
            .ok_or(DependencyError::SizeOverflow)?;
        self.chars(target)?
            .checked_add(delta)
            .ok_or(DependencyError::SizeOverflow)?;
        Ok((delta, member_chars))
    }

    /// Validates owner and size arithmetic first. On cancellation discard the incomplete plan/set.
    pub(crate) fn merge_delta(
        &self,
        target: &mut DependencySet,
        added: &DependencySet,
        cancel: &CancellationToken,
    ) -> Result<usize, DependencyError> {
        let (delta, member_chars) = self.additions(target, added, cancel)?;
        target
            .member_chars
            .checked_add(member_chars)
            .ok_or(DependencyError::SizeOverflow)?;
        for kind in 0..3 {
            for &index in &added.indices[kind] {
                check(cancel)?;
                if target.indices[kind].insert(index) {
                    // All additions and their total were checked before the first insertion.
                    target.member_chars += self.rows[kind].chars[index];
                }
            }
        }
        Ok(delta)
    }

    pub(super) fn collect_set(
        &self,
        units: BTreeSet<usize>,
        cancel: &CancellationToken,
    ) -> Result<DependencySet, DependencyError> {
        check(cancel)?;
        let mut representations = BTreeSet::new();
        let mut snapshots = BTreeSet::new();
        for &unit in &units {
            check(cancel)?;
            let representation = self.unit_representation[unit];
            if representations.insert(representation) {
                for &source in &self.representation_sources[representation] {
                    check(cancel)?;
                    snapshots.insert(source);
                }
            }
        }
        let mut result = self.empty();
        result.indices = [snapshots, representations, units];
        for (kind, indices) in result.indices.iter().enumerate() {
            for &index in indices {
                check(cancel)?;
                result.member_chars = result
                    .member_chars
                    .checked_add(self.rows[kind].chars[index])
                    .ok_or(DependencyError::SizeOverflow)?;
            }
        }
        Ok(result)
    }
}
