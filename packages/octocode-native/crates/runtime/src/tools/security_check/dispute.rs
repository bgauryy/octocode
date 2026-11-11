//! Databases that disagree on a version. OSV lists, at a version, each record
//! whose ranges hold it; one advisory is often kept by several databases
//! (GitHub's, RUSTSEC, PYSEC), each its own record naming the others as
//! aliases. When a listed record names an alias that OSV keeps for the same
//! package and does not list at the version, the two databases disagree on
//! whether the version is affected. The row counts the record OSV lists, as
//! OSV's server decides what a range holds, never this module, and names the
//! record that disagrees on the row, in the answer and in a warning.
use super::Net;
use super::answer::{stated_id, typed};
use super::osv;
use super::record::{Selection, own_entries};
use super::source::{Confirmed, SourceResult};
use futures_util::{StreamExt as _, stream};
use std::collections::{BTreeMap, BTreeSet};

/// What OSV said of each alias no listed record carries: the ones it keeps
/// for this package, and a warning for each that could not be read.
#[derive(Default)]
pub(super) struct Read {
    version: Option<String>,
    name: String,
    disputing: BTreeSet<String>,
    warnings: Vec<String>,
}

/// Reads each listed record's aliases that OSV does not list at the version.
/// It only reads `results`, so it runs beside the row's other reads of OSV's
/// answer; [`Read::mark`] sets what it found on the records.
pub(super) async fn read(confirmed: &Confirmed, results: &[SourceResult], net: &Net<'_>) -> Read {
    let Some(version) = confirmed.version.as_deref() else {
        return Read::default();
    };
    // Only OSV's own answer at the version: a walk that is not whole, or a
    // version compared here, leaves nothing to set against.
    if results
        .iter()
        .any(|result| result.resume.is_some() || result.compared_here)
    {
        return Read::default();
    }
    let listed = results
        .iter()
        .flat_map(|result| &result.advisories)
        .map(|row| row.id.clone())
        .collect::<BTreeSet<_>>();
    let unlisted = results
        .iter()
        .flat_map(|result| &result.advisories)
        .flat_map(|row| row.aliases.iter().cloned())
        .filter(|alias| !listed.contains(alias))
        .collect::<BTreeSet<_>>();
    let names = [confirmed.name.clone()];
    let selection = Selection::Named(confirmed.ecosystem, &names, confirmed.release.as_deref());
    let read = stream::iter(unlisted)
        .map(|alias| async move {
            let record = osv::by_id(net, &alias).await;
            (alias, record)
        })
        .buffered(osv::CONCURRENT_READS)
        .collect::<Vec<_>>()
        .await;
    let mut disputing = BTreeSet::new();
    let mut warnings = Vec::new();
    for (alias, record) in read {
        match record {
            Ok(Some(record)) if !own_entries(&record, &selection).is_empty() => {
                disputing.insert(alias);
            }
            // OSV keeps no such record, or one about another package.
            Ok(_) => {}
            Err(error) => warnings.push(format!(
                "The record {} was not read ({}), so whether its database agrees that {} is affected is unknown.",
                stated_id(&alias),
                osv::unread_reason(&error),
                typed(version)
            )),
        }
    }
    Read {
        version: Some(version.to_owned()),
        name: confirmed.name.clone(),
        disputing,
        warnings,
    }
}

impl Read {
    /// Each listed record's aliases that OSV keeps for this package and does
    /// not list at the version, set on the record (`disputed_by`), and the
    /// row's warnings: each disagreement, and each alias that could not be read.
    pub(super) fn mark(self, results: &mut [SourceResult]) -> Vec<String> {
        let Some(version) = self.version else {
            return Vec::new();
        };
        let mut warnings = self.warnings;
        let mut disputed = BTreeMap::<String, Vec<String>>::new();
        for row in results.iter_mut().flat_map(|result| &mut result.advisories) {
            row.disputed_by = row
                .aliases
                .iter()
                .filter(|alias| self.disputing.contains(*alias))
                .cloned()
                .collect();
            if !row.disputed_by.is_empty() {
                disputed.insert(row.id.clone(), row.disputed_by.clone());
            }
        }
        // Ids and names as the answer states them: no source writes prose here.
        let version = typed(&version);
        for (id, by) in disputed {
            warnings.push(format!(
                "The databases disagree on {version}: the row counts {} as affecting it, as OSV lists it there; {}, another database's record of it that names {}, does not list {version}. Read each record's ranges.",
                stated_id(&id),
                by.iter().map(|id| stated_id(id)).collect::<Vec<_>>().join(", "),
                typed(&self.name)
            ));
        }
        warnings
    }
}
