//! `retract` directives of a `go.mod`: the versions its module withdraws.
use super::super::types::Retraction;
use semver::Version;
use std::cmp::Ordering;

/// A Go version (`v1.2.3`, a pseudo-version, `+incompatible`) as semver;
/// `None` for a spelling semver does not read.
fn parse(version: &str) -> Option<Version> {
    Version::parse(version.trim().strip_prefix('v')?).ok()
}

/// One `retract` operand: a version or a `[low, high]` range, both ends
/// included.
fn covers(operand: &str, version: &Version) -> bool {
    let operand = operand.trim();
    let Some(range) = operand
        .strip_prefix('[')
        .and_then(|rest| rest.strip_suffix(']'))
    else {
        return parse(operand).is_some_and(|retracted| retracted.cmp_precedence(version).is_eq());
    };
    let Some((low, high)) = range.split_once(',') else {
        return false;
    };
    match (parse(low), parse(high)) {
        (Some(low), Some(high)) => {
            low.cmp_precedence(version) != Ordering::Greater
                && version.cmp_precedence(&high) != Ordering::Greater
        }
        _ => false,
    }
}

/// The retraction of `version` in `go_mod`, the `go.mod` of the module's
/// latest version (the one the go command reads retractions from): the
/// directive whose version or range holds it, with its rationale. A
/// rationale is the comment on the directive's line, else the comment lines
/// right above it.
pub(super) fn retraction(go_mod: &str, version: &str) -> Option<Retraction> {
    let version = parse(version)?;
    let mut in_block = false;
    let mut above: Vec<&str> = Vec::new();
    for line in go_mod.lines() {
        let (code, comment) = line
            .split_once("//")
            .map_or((line, ""), |(code, comment)| (code, comment));
        let code = code.trim();
        let comment = comment.trim();
        if code.is_empty() {
            if comment.is_empty() {
                above.clear();
            } else {
                above.push(comment);
            }
            continue;
        }
        let operand = if in_block {
            if code == ")" {
                in_block = false;
                above.clear();
                continue;
            }
            code
        } else {
            let Some(rest) = code
                .strip_prefix("retract")
                .filter(|rest| rest.starts_with([' ', '\t', '(']))
            else {
                above.clear();
                continue;
            };
            let rest = rest.trim();
            if rest == "(" {
                in_block = true;
                above.clear();
                continue;
            }
            rest
        };
        if covers(operand, &version) {
            let reason = if comment.is_empty() {
                (!above.is_empty()).then(|| above.join(" "))
            } else {
                Some(comment.to_owned())
            };
            return Some(Retraction { reason });
        }
        above.clear();
    }
    None
}
