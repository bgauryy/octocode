//! Whether the confirmed version is the registry's latest, and what moving to it
//! changes. Versions compare through `upgrade`, the one dotted-number comparator;
//! any other spelling reads `unknown`, never a guess.
use super::upgrade::{Move, movement};
use serde_json::{Value, json};

/// The registry's latest stable release.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Latest {
    pub version: String,
    /// `YYYY-MM-DD`, when the registry says.
    pub published_at: Option<String>,
}

/// `latestVersion`, `latestPublishedAt`, `currentPublishedAt`, `isLatest`,
/// `outdated` and `upgradeKind` for `current`. `outdated` is absent when the two
/// versions cannot be compared; `upgradeKind` only when there is a newer release.
pub(super) fn apply(
    data: &mut Value,
    current: &str,
    current_published_at: Option<&str>,
    latest: &Latest,
) {
    data["latestVersion"] = json!(latest.version);
    if let Some(published) = &latest.published_at {
        data["latestPublishedAt"] = json!(published);
    }
    if let Some(published) = current_published_at {
        data["currentPublishedAt"] = json!(published);
    }
    let forward = movement(current, &latest.version);
    let same = current == latest.version
        || (forward == Move::None && movement(&latest.version, current) == Move::None);
    data["isLatest"] = json!(same);
    match forward {
        Move::Unknown => {}
        Move::None => data["outdated"] = json!(false),
        _ => data["outdated"] = json!(true),
    }
    if let Some(kind) = forward.kind() {
        data["upgradeKind"] = json!(kind);
    }
}

#[cfg(test)]
#[path = "update_tests.rs"]
mod tests;
