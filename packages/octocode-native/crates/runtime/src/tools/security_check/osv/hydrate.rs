//! The record behind each id `/v1/querybatch` lists: `GET /v1/vulns/{id}`, once
//! for each distinct id of a call, and once ever for an id whose `modified`
//! stamp is unchanged. A record's `modified` changes whenever it does, so an
//! answer kept under `(id, modified)` is exact and needs no lifetime or
//! validator; a record read newer than the stamp listed is kept under its own.
//!
//! OSV lists a stamp to the microsecond (`…56.951750Z`) and the record states
//! the same instant to the nanosecond (`…56.951750105Z`, measured 2026-11-11),
//! so the two are one stamp when the record's only adds digits to the listed
//! one ([`same_stamp`]). A record is kept under the stamp it was listed with:
//! the key the next call asks by.
use super::super::Net;
use super::batch::Ref;
use super::{CONCURRENT_READS, by_id, malformed};
use crate::providers::artifact::ArtifactError;
use futures_util::{StreamExt as _, stream};
use octocode_github::{Partition, ProviderAnswer, Reuse, SavedAnswer};
use secrecy::SecretString;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use url::Url;

/// A record's identity: its id and the `modified` stamp it was listed with.
pub(super) type Key = (String, Option<String>);

/// Records read in this call, so none is read twice.
pub(super) type Hydrated = BTreeMap<Key, Value>;

pub(super) fn key(listed: &Ref) -> Key {
    (listed.id.clone(), listed.modified.clone())
}

/// Reads every distinct record of `refs` not in `known` into it, and returns
/// each record that could not be read with why. One failed record never drops
/// another.
pub(super) async fn fetch(
    net: &Net<'_>,
    refs: Vec<&Ref>,
    known: &mut Hydrated,
) -> BTreeMap<Key, ArtifactError> {
    let wanted = refs
        .into_iter()
        .map(key)
        .filter(|key| !known.contains_key(key))
        .collect::<BTreeSet<_>>();
    let read = stream::iter(wanted)
        .map(|key| async move {
            let record = record(net, &key).await;
            (key, record)
        })
        .buffer_unordered(CONCURRENT_READS)
        .collect::<Vec<_>>()
        .await;
    let mut failed = BTreeMap::new();
    for (key, record) in read {
        match record {
            Ok(record) => {
                known.insert(key, record);
            }
            Err(error) => {
                failed.insert(key, error);
            }
        }
    }
    failed
}

async fn record(net: &Net<'_>, (id, modified): &Key) -> Result<Value, ArtifactError> {
    if let Some(kept) = kept(net, id, modified.as_deref()) {
        return Ok(kept);
    }
    let Some(record) = by_id(net, id).await? else {
        return Err(malformed(
            &format!("a listed record, {id}, that it has no record of"),
            id,
        ));
    };
    keep(net, &record, modified.as_deref());
    Ok(record)
}

/// Whether a record's `own` stamp is the `listed` one: the same text, or the
/// same instant with more fractional digits (the listed stamp cut short, as
/// OSV's list states it). Any other difference is another version of the record.
fn same_stamp(listed: &str, own: &str) -> bool {
    if listed == own {
        return true;
    }
    let (Some(listed), Some(own)) = (listed.strip_suffix('Z'), own.strip_suffix('Z')) else {
        return false;
    };
    own.strip_prefix(listed).is_some_and(|finer| {
        listed.contains('.') && !finer.is_empty() && finer.bytes().all(|b| b.is_ascii_digit())
    })
}

/// Answers are kept under OSV's origin and no credential: nothing here sends one.
fn scope(net: &Net<'_>) -> Option<String> {
    let origin = Url::parse(net.osv_api).ok()?.origin().ascii_serialization();
    Some(
        Partition::new(origin, None::<&SecretString>)
            .identity()
            .to_owned(),
    )
}

fn identity(id: &str, modified: &str) -> String {
    format!("OSV record {id}@{modified}")
}

fn now_millis() -> u64 {
    u64::try_from(crate::civil_date::unix_millis()).unwrap_or(0)
}

/// The record kept for `(id, modified)`: only one that states that same stamp
/// ([`same_stamp`]).
fn kept(net: &Net<'_>, id: &str, modified: Option<&str>) -> Option<Value> {
    let saved = net.answers?.load(&scope(net)?, &identity(id, modified?))?;
    if saved.reuse(true, None, now_millis()) != Reuse::Pinned {
        return None;
    }
    let record = serde_json::from_slice::<Value>(saved.answer.body.as_bytes()).ok()?;
    (record.get("id")?.as_str()? == id && same_stamp(modified?, record.get("modified")?.as_str()?))
        .then_some(record)
}

/// Keeps a record that states its own id and stamp: under the stamp it was
/// `listed` with when the record's is that one ([`same_stamp`]), so the next
/// call that lists it finds it; under its own when OSV changed it since.
fn keep(net: &Net<'_>, record: &Value, listed: Option<&str>) {
    let (Some(store), Some(scope)) = (net.answers, scope(net)) else {
        return;
    };
    let (Some(id), Some(own)) = (
        record.get("id").and_then(Value::as_str),
        record.get("modified").and_then(Value::as_str),
    ) else {
        return;
    };
    let modified = listed
        .filter(|listed| same_stamp(listed, own))
        .unwrap_or(own);
    let identity = identity(id, modified);
    store.save(
        &scope,
        &identity,
        SavedAnswer {
            answer: ProviderAnswer {
                request: identity.clone(),
                status: 200,
                headers: Vec::new(),
                body: record.to_string().into(),
                next: None,
                not_found: None,
            },
            observed_ms: now_millis(),
            redirected: false,
        },
    );
}

#[cfg(test)]
mod tests {
    use super::same_stamp;

    #[test]
    fn a_stamp_is_the_listed_one_only_when_it_adds_fractional_digits() {
        let listed = "2026-09-10T03:50:56.951750Z";
        assert!(same_stamp(listed, listed));
        assert!(same_stamp(listed, "2026-09-10T03:50:56.951750105Z"));
        for other in [
            "2026-09-10T03:50:56.951751105Z",
            "2026-09-10T03:50:57.951750105Z",
            "2026-09-10T03:50:56.951750105+01:00",
            "2026-09-10T03:50:56.951750x05Z",
            "2026-09-10T03:50:56.95175Z",
        ] {
            assert!(!same_stamp(listed, other), "{other}");
        }
        // Whole seconds: more digits would name another second.
        assert!(!same_stamp("2026-09-10T03:50:5Z", "2026-09-10T03:50:56Z"));
        assert!(!same_stamp("m1", "m2"));
    }
}
