//! What the shared response stages ask ghSearchHistory about its output.
use crate::id::ToolId;
use crate::response::output::{ToolOutput, retain_lead_reads};
use crate::tools::gh_shared::commit::headline_pull_request;
use serde_json::Value;
use std::collections::HashSet;

/// This tool's selection finish for the shared response stages.
pub(crate) struct Output;

/// What a listed row names that a lead can follow: its pull request or
/// issue number, its commit, or the pull request its headline names.
#[derive(PartialEq, Eq, Hash)]
enum Named {
    Number(u64),
    Commit(String),
}

fn named(row: &Value) -> impl Iterator<Item = Named> + '_ {
    let headline = row["messageHeadline"]
        .as_str()
        .and_then(headline_pull_request);
    row["number"]
        .as_u64()
        .into_iter()
        .chain(headline)
        .map(Named::Number)
        .chain(
            row["commitSha"]
                .as_str()
                .map(|sha| Named::Commit(sha.to_owned())),
        )
}

const ROWS: [&str; 3] = ["pullRequests", "issues", "commits"];

impl ToolOutput for Output {
    /// A body only the judge read leaves the rows; one the caller picked in
    /// `fields` stays.
    fn strip_judge_only(&self, data: &mut Value, query: &Value) {
        let picked = |key: &str| {
            query["fields"]
                .as_array()
                .is_some_and(|fields| fields.iter().any(|field| field == key))
        };
        for rows in ROWS {
            for row in data
                .get_mut(rows)
                .and_then(Value::as_array_mut)
                .into_iter()
                .flatten()
                .filter_map(Value::as_object_mut)
            {
                for key in super::shape::ROW_BODIES
                    .into_iter()
                    .filter(|key| !picked(key))
                {
                    row.shift_remove(key);
                }
            }
        }
    }

    /// A read or search lead that follows a row classification cut goes,
    /// unless a shown row names the same item; leads that follow no row
    /// (the query's own pages, broader or narrower searches) stay.
    fn finish_selection(
        &self,
        data: &mut Value,
        query: &Value,
        _hidden: &[Value],
        removed: &[Value],
    ) {
        let shown = ROWS
            .iter()
            .filter_map(|field| data[*field].as_array())
            .flatten()
            .flat_map(named)
            .collect::<HashSet<_>>();
        let cut = removed
            .iter()
            .flat_map(named)
            .filter(|name| !shown.contains(name))
            .collect::<HashSet<_>>();
        if cut.is_empty() {
            return;
        }
        retain_lead_reads(data, ToolId::GhGetHistoryItem, |read| {
            let number = read["number"].as_u64().map(Named::Number);
            let commit = read["ref"]
                .as_str()
                .map(|sha| Named::Commit(sha.to_owned()));
            !number
                .into_iter()
                .chain(commit)
                .any(|name| cut.contains(&name))
        });
        // The pull-request search a commit row leads to names its SHA as
        // the only keyword; the query's own searches keep its keywords.
        retain_lead_reads(data, ToolId::GhSearchHistory, |search| {
            let keywords = &search["keywords"];
            let follows = match keywords.as_array().map(Vec::as_slice) {
                Some([sha]) => sha
                    .as_str()
                    .is_some_and(|sha| cut.contains(&Named::Commit(sha.to_owned()))),
                _ => false,
            };
            !follows || *keywords == query["keywords"]
        });
    }
}
