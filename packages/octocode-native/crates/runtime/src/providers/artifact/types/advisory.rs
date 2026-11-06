//! The advisories a registry lists for a version, and the merging of
//! records that alias one another.
use serde::Serialize;

/// One advisory a registry lists for a version, in the registry's own words.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Advisory {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub aliases: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub severity: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub fixed_in: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub withdrawn: Option<String>,
}

/// The groups of `items` that name one advisory: two share a group when any
/// identifier (its own id or an alias) of one is an identifier of the other,
/// as a GHSA record and its PYSEC or GO twin each alias the other and their
/// CVE. Each group lists item positions in input order; groups follow their
/// first item.
pub(crate) fn alias_groups<T>(
    items: &[T],
    identifiers: impl Fn(&T) -> Vec<&str>,
) -> Vec<Vec<usize>> {
    let names = items.iter().map(&identifiers).collect::<Vec<_>>();
    let mut group_of: Vec<usize> = (0..items.len()).collect();
    fn root(group_of: &mut [usize], mut at: usize) -> usize {
        while group_of[at] != at {
            group_of[at] = group_of[group_of[at]];
            at = group_of[at];
        }
        at
    }
    for later in 0..items.len() {
        for earlier in 0..later {
            if names[later]
                .iter()
                .any(|name| names[earlier].contains(name))
            {
                let (a, b) = (root(&mut group_of, earlier), root(&mut group_of, later));
                group_of[a.max(b)] = a.min(b);
            }
        }
    }
    let mut groups: Vec<(usize, Vec<usize>)> = Vec::new();
    for at in 0..items.len() {
        let first = root(&mut group_of, at);
        match groups.iter_mut().find(|(root, _)| *root == first) {
            Some((_, members)) => members.push(at),
            None => groups.push((first, vec![at])),
        }
    }
    groups.into_iter().map(|(_, members)| members).collect()
}

/// `values` with each later one not yet listed appended, in order.
fn union_into(values: &mut Vec<String>, more: impl IntoIterator<Item = String>) {
    for value in more {
        if !values.contains(&value) {
            values.push(value);
        }
    }
}

impl Advisory {
    /// One row per advisory: records that alias each other ([`alias_groups`])
    /// merge into their first, which keeps every id and alias of the group
    /// in `aliases` and every fixed version; a text only a later twin states
    /// fills the first's gap.
    pub(crate) fn merged(advisories: Vec<Self>) -> Vec<Self> {
        let groups = alias_groups(&advisories, |advisory| {
            advisory
                .id
                .iter()
                .chain(&advisory.aliases)
                .map(String::as_str)
                .collect()
        });
        let mut slots = advisories.into_iter().map(Some).collect::<Vec<_>>();
        groups
            .into_iter()
            .filter_map(|members| {
                let mut members = members.into_iter().filter_map(|at| slots[at].take());
                let mut first = members.next()?;
                for twin in members {
                    union_into(&mut first.aliases, twin.id.into_iter().chain(twin.aliases));
                    union_into(&mut first.fixed_in, twin.fixed_in);
                    first.url = first.url.or(twin.url);
                    first.severity = first.severity.or(twin.severity);
                    first.summary = first.summary.or(twin.summary);
                    first.details = first.details.or(twin.details);
                    first.withdrawn = first.withdrawn.or(twin.withdrawn);
                }
                if let Some(id) = &first.id {
                    first.aliases.retain(|alias| alias != id);
                }
                Some(first)
            })
            .collect()
    }
}
