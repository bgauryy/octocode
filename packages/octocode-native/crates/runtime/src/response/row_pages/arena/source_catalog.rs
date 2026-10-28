use super::{Catalog, DependencyError, Evidence, Fragment, FragmentArena, Selection};
use serde_json::Value;
use tokio_util::sync::CancellationToken;

const FIELD_CHARS: usize = "\"sourceEvidence\":,".len();

impl FragmentArena {
    pub(in crate::response::row_pages) fn take_catalog(
        &mut self,
        row: &mut Value,
        cancelled: &CancellationToken,
    ) -> Result<Option<usize>, DependencyError> {
        let Some(catalog) = Catalog::take(row, cancelled)? else {
            return Ok(None);
        };
        let index = self.catalogs.len();
        self.catalogs.push(catalog);
        Ok(Some(index))
    }

    pub(in crate::response::row_pages) fn dependencies(
        &self,
        catalog: Option<usize>,
        value: &Value,
        cancelled: &CancellationToken,
    ) -> Result<Evidence, DependencyError> {
        catalog
            .map(|index| {
                Ok(Selection {
                    catalog: index,
                    dependencies: self.catalogs[index].dependencies(value, cancelled)?,
                })
            })
            .transpose()
    }

    pub(in crate::response::row_pages) fn evidence_chars(
        &self,
        selection: &Evidence,
    ) -> Result<usize, DependencyError> {
        let Some(selection) = selection else {
            return Ok(0);
        };
        let chars = self.catalogs[selection.catalog].chars(&selection.dependencies)?;
        Ok(chars
            + if selection.dependencies.is_empty() {
                0
            } else {
                FIELD_CHARS
            })
    }

    pub(in crate::response::row_pages) fn evidence_delta(
        &self,
        current: &Evidence,
        added: &Evidence,
        cancelled: &CancellationToken,
    ) -> Result<usize, DependencyError> {
        match (current, added) {
            (_, None) => Ok(0),
            (None, _) => self.evidence_chars(added),
            (Some(left), Some(right)) if left.catalog == right.catalog => {
                let delta = self.catalogs[left.catalog].delta(
                    &left.dependencies,
                    &right.dependencies,
                    cancelled,
                )?;
                Ok(delta
                    + if left.dependencies.is_empty() && !right.dependencies.is_empty() {
                        FIELD_CHARS
                    } else {
                        0
                    })
            }
            _ => Err(DependencyError::ForeignSet),
        }
    }

    pub(in crate::response::row_pages) fn merge_evidence(
        &self,
        current: &mut Evidence,
        added: &Evidence,
        cancelled: &CancellationToken,
    ) -> Result<(), DependencyError> {
        match (current.as_mut(), added) {
            (_, None) => {}
            (None, Some(_)) => *current = added.clone(),
            (Some(left), Some(right)) if left.catalog == right.catalog => {
                self.catalogs[left.catalog].merge_delta(
                    &mut left.dependencies,
                    &right.dependencies,
                    cancelled,
                )?;
            }
            _ => return Err(DependencyError::ForeignSet),
        }
        Ok(())
    }

    pub(in crate::response::row_pages) fn attach_evidence(
        &self,
        fragment: &Fragment,
        value: &mut Value,
        cancelled: &CancellationToken,
    ) -> Result<(), DependencyError> {
        if let Some(selection) = &fragment.evidence {
            let catalog = &self.catalogs[selection.catalog];
            // Pruning and restored first-page fields can change the final closure.
            let dependencies = catalog.dependencies(value, cancelled)?;
            catalog.attach(value, &dependencies, cancelled)?;
        }
        Ok(())
    }
}
