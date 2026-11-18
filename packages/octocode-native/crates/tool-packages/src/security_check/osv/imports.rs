//! Narrowing a Go module's records to one package inside it.
//!
//! OSV keys Go by module, so a query for `golang.org/x/net/http2` is answered
//! with the records of `golang.org/x/net`. A record states which packages it
//! affects in `affected[].ecosystem_specific.imports[].path`; that statement,
//! never a title, decides. A record that states no imports covers its whole
//! module and stays; one that states imports without the package is set apart.
use super::Selection;
use crate::artifact::alias_groups;
use crate::security_check::record::comparable;
use serde_json::Value;

/// What a record says about one package.
#[derive(Clone, Copy, Eq, Ord, PartialEq, PartialOrd)]
enum Verdict {
    /// States imports of the module, none of them the package.
    Elsewhere,
    /// States no imports for the module: every package of it.
    ModuleWide,
    /// Names the package, by import path or by an entry of its own.
    Names,
}

/// `records` split into the ones that concern `import` and the module-only
/// ones. Records of one advisory (a GHSA record and its GO twin) share a
/// verdict: the strongest statement among them. Without an import, or for a
/// selection naming no module, every record stays.
pub(super) fn split(
    records: Vec<Value>,
    import: Option<&str>,
    selection: &Selection<'_>,
) -> (Vec<Value>, Vec<Value>) {
    let (Some(import), Selection::Named(ecosystem, modules, _)) = (import, selection) else {
        return (records, Vec::new());
    };
    let verdicts = records
        .iter()
        .map(|record| {
            verdict(record, import, |name| {
                modules
                    .iter()
                    .any(|module| comparable(*ecosystem, module) == comparable(*ecosystem, name))
            })
        })
        .collect::<Vec<_>>();
    let mut decided = vec![Verdict::ModuleWide; records.len()];
    for group in alias_groups(&records, identifiers) {
        // `Names` beats `Elsewhere` beats `ModuleWide` only in that a naming
        // twin keeps the advisory, and an exclusion outranks silence.
        let named = group.iter().any(|&at| verdicts[at] == Verdict::Names);
        let elsewhere = group.iter().any(|&at| verdicts[at] == Verdict::Elsewhere);
        let all = match (named, elsewhere) {
            (true, _) => Verdict::Names,
            (false, true) => Verdict::Elsewhere,
            (false, false) => Verdict::ModuleWide,
        };
        for at in group {
            decided[at] = all;
        }
    }
    let (mut kept, mut apart) = (Vec::new(), Vec::new());
    for (record, decided) in records.into_iter().zip(decided) {
        if decided == Verdict::Elsewhere {
            apart.push(record);
        } else {
            kept.push(record);
        }
    }
    (kept, apart)
}

fn identifiers(record: &Value) -> Vec<&str> {
    std::iter::once(record.get("id"))
        .chain(
            record
                .get("aliases")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .map(Some),
        )
        .filter_map(|id| id?.as_str())
        .collect()
}

fn verdict(record: &Value, import: &str, is_module: impl Fn(&str) -> bool) -> Verdict {
    let entries = record
        .get("affected")
        .and_then(Value::as_array)
        .map_or(&[][..], Vec::as_slice);
    let mut stated = false;
    let mut unstated = false;
    for entry in entries {
        let Some(name) = entry.pointer("/package/name").and_then(Value::as_str) else {
            continue;
        };
        if name == import {
            return Verdict::Names;
        }
        if !is_module(name) {
            continue;
        }
        let paths = entry
            .pointer("/ecosystem_specific/imports")
            .and_then(Value::as_array)
            .filter(|imports| !imports.is_empty());
        match paths {
            None => unstated = true,
            Some(imports) => {
                if imports
                    .iter()
                    .any(|row| row.get("path").and_then(Value::as_str) == Some(import))
                {
                    return Verdict::Names;
                }
                stated = true;
            }
        }
    }
    if stated && !unstated {
        Verdict::Elsewhere
    } else {
        Verdict::ModuleWide
    }
}
