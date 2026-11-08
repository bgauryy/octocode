//! The units of a row-listing page (ghSearchHistory, ghGetHistoryItem,
//! structureSearch): each listed row, entry or discussion item is one
//! candidate, located by its JSON pointer in the page's data. Selection keeps
//! the shown ones in place; its unfiltered lead keeps the page.
use crate::id::ToolId;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

/// One candidate: where it sits in the page and what the judge sees of it
/// (`None`: nothing to judge, so it stays unknown).
struct Unit {
    pointer: String,
    state: Option<Value>,
}

pub(super) struct Rows {
    units: Vec<Unit>,
    /// Paths whose names the classification egress withholds.
    withheld: Vec<String>,
}

/// The search-row collections of ghSearchHistory; one is present per page.
const SEARCH_ROWS: [&str; 3] = ["pullRequests", "issues", "commits"];
/// The discussion lists of one pull request or issue row.
const DISCUSSION: [(&str, &str); 3] = [
    ("comments", "comment"),
    ("reviews", "review"),
    ("reviewComments", "reviewComment"),
];

fn text(value: &Value) -> Option<&str> {
    value.as_str().filter(|text| !text.trim().is_empty())
}

fn entries<'a>(data: &'a Value, field: &str) -> impl Iterator<Item = (usize, &'a Value)> {
    data.get(field)
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .enumerate()
}

impl Rows {
    /// The units of `data`, in page order. A page with none of the tool's
    /// collections has no units.
    pub(super) fn of(tool: ToolId, data: &Value) -> Self {
        let mut rows = Self {
            units: Vec::new(),
            withheld: Vec::new(),
        };
        match tool {
            ToolId::GhSearchHistory => rows.search_rows(data),
            ToolId::GhGetHistoryItem => rows.discussion(data),
            _ => rows.listing(data),
        }
        rows
    }

    fn push(&mut self, pointer: String, state: Option<Value>) {
        self.units.push(Unit { pointer, state });
    }

    /// A commit, pull request or issue row: its title and body.
    fn search_rows(&mut self, data: &Value) {
        for field in SEARCH_ROWS {
            for (at, row) in entries(data, field) {
                let title = ["title", "messageHeadline"]
                    .iter()
                    .find_map(|key| text(&row[*key]));
                let body = text(&row["body"]);
                let state = (title.is_some() || body.is_some())
                    .then(|| json!({"title": title, "body": body}));
                self.push(format!("/{field}/{at}"), state);
            }
        }
    }

    /// A comment, review or review comment of a pull request or issue.
    fn discussion(&mut self, data: &Value) {
        for field in ["pullRequests", "issues"] {
            for (row_at, row) in entries(data, field) {
                for (list, kind) in DISCUSSION {
                    for (at, item) in entries(row, list) {
                        let state = || {
                            text(&item["body"]).map(|body| {
                                json!({"kind": kind, "author": item["author"], "path": item["path"], "body": body})
                            })
                        };
                        let pointer = format!("/{field}/{row_at}/{list}/{at}");
                        self.push_named(pointer, item["path"].as_str(), state);
                    }
                }
            }
        }
    }

    /// A listed entry: a group's `<name>` under its dir, or a path row.
    fn listing(&mut self, data: &Value) {
        for (at, row) in entries(data, "files") {
            if let Some(dir) = row["dir"].as_str() {
                for (entry_at, entry) in entries(row, "files") {
                    let name = entry.as_str().map(entry_name);
                    let path = name.map(|name| format!("{dir}/{name}"));
                    self.path_unit(format!("/files/{at}/files/{entry_at}"), path);
                }
            } else {
                let path = row["path"].as_str().map(str::to_owned);
                self.path_unit(format!("/files/{at}"), path);
            }
        }
    }

    fn path_unit(&mut self, pointer: String, path: Option<String>) {
        let state = || path.as_ref().map(|path| json!({"path": path}));
        self.push_named(pointer, path.as_deref(), state);
    }

    /// A unit that names a file: one rule withholds a credential-named file
    /// from the judge (the unit stays unknown and its path is disclosed).
    fn push_named(
        &mut self,
        pointer: String,
        path: Option<&str>,
        state: impl FnOnce() -> Option<Value>,
    ) {
        match path.filter(|path| crate::policy::discovery::is_sensitive_repository_file(path)) {
            Some(path) => {
                self.withheld.push(path.to_owned());
                self.push(pointer, None);
            }
            None => self.push(pointer, state()),
        }
    }

    /// The row notes of a selected page: the withheld candidates, then the
    /// discussion sections a unit-less history-item read left unread; and
    /// whether any such section was named.
    pub(super) fn notes(&self, tool: ToolId, query: &Value, data: &Value) -> (Vec<String>, bool) {
        let unread = self.unread_notes(tool, query, data);
        let named = !unread.is_empty();
        let notes = self.withheld_note().into_iter().chain(unread).collect();
        (notes, named)
    }

    /// When the read held no unit: the discussion sections of each pull
    /// request or issue row that may hold the entries classification judges
    /// but were not requested, so a zero-unit receipt does not read as "no
    /// entry matches". A section is named when its row counts entries in it,
    /// or does not say: reviews and review comments are never counted, and
    /// a row without its header (`title`; an issue section read restates
    /// only number and state) does not count comments. A section the query
    /// requested came back empty and is not named.
    fn unread_notes(&self, tool: ToolId, query: &Value, data: &Value) -> Vec<String> {
        if tool != ToolId::GhGetHistoryItem || !self.units.is_empty() {
            return Vec::new();
        }
        let requested = query["sections"]
            .as_array()
            .map_or_else(Vec::new, |sections| {
                sections.iter().filter_map(Value::as_str).collect()
            });
        let mut notes = Vec::new();
        for (field, judged) in [
            (
                "pullRequests",
                &["comments", "reviews", "reviewComments"][..],
            ),
            ("issues", &["comments"][..]),
        ] {
            for (_, row) in entries(data, field) {
                let comments = row["commentsCount"].as_u64().filter(|count| *count > 0);
                let uncounted = comments.is_none() && row.get("title").is_none();
                let unread = judged
                    .iter()
                    .copied()
                    .filter(|section| !requested.contains(section))
                    .filter(|section| *section != "comments" || comments.is_some() || uncounted)
                    .collect::<Vec<_>>();
                if unread.is_empty() {
                    continue;
                }
                let counts = comments
                    .filter(|_| unread.contains(&"comments"))
                    .map(|count| {
                        format!(
                            "; #{} counts {}",
                            row["number"],
                            octocode_engine::text::counted(count, "comment", "comments")
                        )
                    })
                    .unwrap_or_default();
                notes.push(format!(
                    "classification judged nothing: it judges only {}, and this read returned none{counts}; add sections:{} to judge them.",
                    judged.join(", "),
                    json!(unread)
                ));
            }
        }
        notes
    }

    /// What the judge sees of each unit.
    pub(super) fn states(&self) -> Vec<Option<Value>> {
        self.units.iter().map(|unit| unit.state.clone()).collect()
    }

    /// The disclosure of withheld candidates, which no receipt names.
    fn withheld_note(&self) -> Option<String> {
        super::provider_page::withheld_paths_note(&self.withheld)
    }

    /// Cut `data` to the units at the `shown` ordinals (ascending), in place.
    /// A listing drops the groups it emptied.
    pub(super) fn retain(&self, data: &mut Value, shown: &[usize]) {
        let mut dropped: BTreeMap<&str, BTreeSet<usize>> = BTreeMap::new();
        for (ordinal, unit) in self.units.iter().enumerate() {
            if shown.binary_search(&ordinal).is_ok() {
                continue;
            }
            if let Some((parent, at)) = unit.pointer.rsplit_once('/')
                && let Ok(at) = at.parse()
            {
                dropped.entry(parent).or_default().insert(at);
            }
        }
        // Deepest arrays first: removing an element never shifts a pointer
        // still to be applied.
        let mut parents: Vec<_> = dropped.into_iter().collect();
        parents.sort_by_key(|(parent, _)| std::cmp::Reverse(parent.matches('/').count()));
        for (parent, indices) in parents {
            if let Some(list) = data.pointer_mut(parent).and_then(Value::as_array_mut) {
                let mut at = 0;
                list.retain(|_| {
                    at += 1;
                    !indices.contains(&(at - 1))
                });
            }
        }
        if let Some(groups) = data.get_mut("files").and_then(Value::as_array_mut) {
            groups.retain(|group| group.get("dir").is_none() || group["files"] != json!([]));
        }
    }
}

/// The name of a listing entry `<name>[/][ (<fields>)]`.
fn entry_name(entry: &str) -> &str {
    match entry
        .strip_suffix(')')
        .and_then(|fields| fields.rsplit_once(" ("))
    {
        Some((name, _)) => name,
        None => entry,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_listing_entry_names_its_path_under_its_group() {
        let data = json!({"files": [{"dir": "src", "files": ["a.rs (10)", "sub/"]}]});
        let rows = Rows::of(ToolId::StructureSearch, &data);
        assert_eq!(
            rows.states(),
            vec![
                Some(json!({"path": "src/a.rs"})),
                Some(json!({"path": "src/sub/"}))
            ]
        );
    }

    #[test]
    fn retaining_keeps_shown_units_and_drops_emptied_groups() {
        let mut data = json!({"files": [
            {"dir": "a", "files": ["x (1)", "y (2)"]},
            {"dir": "b", "files": ["z (3)"]},
        ]});
        let rows = Rows::of(ToolId::StructureSearch, &data);
        rows.retain(&mut data, &[1]);
        assert_eq!(data, json!({"files": [{"dir": "a", "files": ["y (2)"]}]}));
    }

    #[test]
    fn history_rows_are_judged_by_title_and_body_and_empty_ones_stay_unknown() {
        let data = json!({"commits": [
            {"messageHeadline": "fix: crash"},
            {"commitSha": "abc"},
        ]});
        let rows = Rows::of(ToolId::GhSearchHistory, &data);
        assert_eq!(
            rows.states(),
            vec![Some(json!({"title": "fix: crash", "body": null})), None]
        );
    }

    #[test]
    fn discussion_units_cut_each_list_of_each_row() {
        let mut data = json!({"pullRequests": [{
            "comments": [{"body": "a"}, {"body": "b"}],
            "reviews": [{"body": "c"}],
        }]});
        let rows = Rows::of(ToolId::GhGetHistoryItem, &data);
        assert_eq!(rows.states().len(), 3);
        rows.retain(&mut data, &[1, 2]);
        assert_eq!(
            data,
            json!({"pullRequests": [{"comments": [{"body": "b"}], "reviews": [{"body": "c"}]}]})
        );
    }
}
