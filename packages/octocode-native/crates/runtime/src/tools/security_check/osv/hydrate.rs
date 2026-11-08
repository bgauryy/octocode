//! The record behind each id `/v1/querybatch` lists: `GET /v1/vulns/{id}`, once
//! for each distinct id of a call, and once ever for an id whose `modified`
//! stamp is unchanged. A record's `modified` changes whenever it does, so an
//! answer kept under `(id, modified)` is exact and needs no lifetime or
//! validator; a record read newer than the stamp listed is kept under its own.
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
    keep(net, &record);
    Ok(record)
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

/// The record kept for `(id, modified)`: only one that states that same stamp.
fn kept(net: &Net<'_>, id: &str, modified: Option<&str>) -> Option<Value> {
    let saved = net.answers?.load(&scope(net)?, &identity(id, modified?))?;
    if saved.reuse(true, None, now_millis()) != Reuse::Pinned {
        return None;
    }
    let record = serde_json::from_slice::<Value>(saved.answer.body.as_bytes()).ok()?;
    (record.get("id")?.as_str()? == id && record.get("modified")?.as_str()? == modified?)
        .then_some(record)
}

/// Keeps a record that states its own id and stamp, under exactly those.
fn keep(net: &Net<'_>, record: &Value) {
    let (Some(store), Some(scope)) = (net.answers, scope(net)) else {
        return;
    };
    let (Some(id), Some(modified)) = (
        record.get("id").and_then(Value::as_str),
        record.get("modified").and_then(Value::as_str),
    ) else {
        return;
    };
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
