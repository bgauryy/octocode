//! Databases that disagree on a version. OSV lists, at a version, each record
//! whose ranges hold it; one advisory is often kept by several databases
//! (GitHub's, RUSTSEC, PYSEC), each its own record naming the others as
//! aliases. When a listed record names an alias that OSV keeps for the same
//! package and does not list at the version, the two databases disagree on
//! whether the version is affected. The row states that per record instead of
//! picking one: OSV's server decides what a range holds, never this module.
use super::Net;
use super::osv;
use super::record::{Selection, own_entries};
use super::source::{Confirmed, SourceResult};
use futures_util::{StreamExt as _, stream};
use std::collections::{BTreeMap, BTreeSet};

/// Each listed record's aliases that OSV keeps for this package and does not
/// list at the version, set on the record (`disputed_by`), and the row's
/// warnings: each disagreement, and each alias that could not be read.
pub(super) async fn mark(
    confirmed: &Confirmed,
    results: &mut [SourceResult],
    net: &Net<'_>,
) -> Vec<String> {
    let Some(version) = confirmed.version.as_deref() else {
        return Vec::new();
    };
    // Only OSV's own answer at the version: a walk that is not whole, or a
    // version compared here, leaves nothing to set against.
    if results
        .iter()
        .any(|result| result.resume.is_some() || result.compared_here)
    {
        return Vec::new();
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
                "The record {alias} was not read ({}), so whether its database agrees that {version} is affected is unknown.",
                error.message
            )),
        }
    }
    let mut disputed = BTreeMap::<String, Vec<String>>::new();
    for row in results.iter_mut().flat_map(|result| &mut result.advisories) {
        row.disputed_by = row
            .aliases
            .iter()
            .filter(|alias| disputing.contains(*alias))
            .cloned()
            .collect();
        if !row.disputed_by.is_empty() {
            disputed.insert(row.id.clone(), row.disputed_by.clone());
        }
    }
    for (id, by) in disputed {
        warnings.push(format!(
            "The databases disagree on {version}: OSV lists {id} at it, but {} names {} too and is not listed at it. Read each record's ranges; the row picks neither.",
            by.join(", "),
            confirmed.name
        ));
    }
    warnings
}
