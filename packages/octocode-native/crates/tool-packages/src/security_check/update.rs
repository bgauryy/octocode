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
    /// The source repository the release names, when it names one.
    pub repository: Option<String>,
}

/// `latestVersion`, `latestPublishedAt`, `currentPublishedAt`,
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
    // The same string is the latest even when it is not a dotted number.
    if current == latest.version {
        data["outdated"] = json!(false);
    } else {
        match forward {
            Move::Unknown => {}
            Move::None => data["outdated"] = json!(false),
            _ => data["outdated"] = json!(true),
        }
    }
    if let Some(kind) = forward.kind() {
        data["upgradeKind"] = json!(kind);
    }
}

#[cfg(test)]
#[path = "update_tests.rs"]
mod tests;
