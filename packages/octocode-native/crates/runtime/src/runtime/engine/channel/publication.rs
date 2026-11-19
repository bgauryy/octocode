//! Cursor publication binds saved work to the access identity used by the call.
use super::ChannelRun;
use crate::runtime::engine::cursors::{Pending, SavedWork};
use crate::runtime::row_indices::RowOrigin;
use octocode_runtime_base::contracts;
use octocode_runtime_base::execution::{ExecutionContext, ExecutionError};
use octocode_runtime_base::response::pager::continuing_block_chars as page_block_chars;
use octocode_runtime_base::response::{
    continuations, cursor::Cursor, outcome::ToolOutcome, read_share,
};
use octocode_runtime_base::security::Sanitized;
use octocode_tools_shared::selection::{RecoveryLead, recovery_lead};
use serde_json::{Value, json};

impl ChannelRun {
    pub(super) fn pending(&self) -> Pending {
        self.cursors.pending(self.access, None)
    }

    pub(super) fn retain_cursors(
        &self,
        pending: Pending,
        context: &ExecutionContext,
    ) -> Result<(), ExecutionError> {
        if !self.can_resume && !pending.is_empty() {
            return Err(ExecutionError::ContinuationLifetime);
        }
        self.cursors.retain(pending, context)
    }

    pub(super) fn retain_pending(
        &self,
        mut pending: Pending,
        context: &ExecutionContext,
    ) -> Result<(), ExecutionError> {
        if pending.is_empty() {
            return Ok(());
        }
        let identity = self
            .dispatcher
            .github_access
            .identity()
            .map_err(|()| ExecutionError::WorkerFailed)?;
        if let Some(expected) = &identity {
            let current = self.dispatcher.github_response_identity(context);
            context.check()?;
            if !current.as_ref().is_ok_and(|current| current == expected) {
                return Err(ExecutionError::WorkerFailed);
            }
        }
        pending.bind_github_identity(identity);
        self.retain_cursors(pending, context)
    }

    /// A runtime that keeps no saved work (a command that exits after one
    /// call) delivers a classified result without the leads that need it,
    /// instead of failing the call after the provider billed it. The
    /// whole-read lead (`next.unfiltered`) moves to the row's `related` as a
    /// fresh request when it holds none of the private progress a cursor
    /// saves; every other lead of saved plan pages, and a whole read that
    /// is no fresh request, is left out with a warning on its row that counts
    /// the units those leads would have reached. Other leads are not
    /// touched: an ordinary page of a read still needs its cursor
    /// ([`Self::retain_cursors`]).
    pub(super) fn settle_unsaved_leads(
        &self,
        structured: &mut Value,
    ) -> Result<(), contracts::ContractValidationError> {
        if self.can_resume {
            return Ok(());
        }
        let mut leads = Vec::new();
        continuations::visit_calls(self.id, structured, &mut |call, path| {
            if let [row @ .., next, channel] = path
                && next == "next"
                && call["kind"] == "continue"
                && let Some(lead) = recovery_lead(channel)
            {
                let fresh = (lead == RecoveryLead::Whole)
                    .then(|| fresh_whole_read(call))
                    .flatten();
                leads.push((
                    octocode_runtime_base::json_pointer::of(row),
                    channel.clone(),
                    fresh,
                ));
            }
        })?;
        for (row_pointer, channel, fresh) in leads.into_iter().rev() {
            let Some(row) = structured.pointer_mut(&row_pointer) else {
                continue;
            };
            if let Some(next) = row.get_mut("next").and_then(Value::as_object_mut) {
                next.shift_remove(&channel);
                if next.is_empty()
                    && let Some(fields) = row.as_object_mut()
                {
                    fields.shift_remove("next");
                }
            }
            let warning = match fresh {
                Some(call) => {
                    octocode_runtime_base::response::result::attach_related(row, call);
                    // Naming `related[]` keeps the call on a successful row
                    // ([`octocode_runtime_base::response::workflow_lead`]).
                    format!(
                        "The {channel} lead is not offered: this command exits after one call and keeps no saved work, so the read without classification is the related[] request."
                    )
                }
                None => unsaved_lead_warning(row, &channel),
            };
            octocode_runtime_base::response::paging::push_warnings(row, [warning]);
        }
        Ok(())
    }

    pub(super) fn publish(
        &self,
        structured: &mut Value,
        scope: &continuations::Scope<'_>,
        pending: &mut Pending,
    ) -> Result<(), contracts::ContractValidationError> {
        self.settle_unsaved_leads(structured)?;
        continuations::finalize_with(
            structured,
            self.id,
            scope,
            &crate::runtime::engine::admission::checked::checked_input,
            &mut |target, mut input, origin| {
                if input.get("responseChars").is_none()
                    && let Some(chars) = self.response_query.get("responseChars")
                {
                    input["responseChars"] = chars.clone();
                }
                // The call's own tool continuations own what its rows
                // observed, taken once when the first is issued.
                if target == self.id && self.id.is_github() && !pending.owns_evidence() {
                    let access = &self.dispatcher.github_access;
                    match access.retained_digest() {
                        Some(digest) => pending.keep_evidence(self.id, digest),
                        None => pending.own_evidence(self.id, access.observed()),
                    }
                }
                Some(pending.insert(
                    target,
                    input,
                    SavedWork::ToolResume {
                        index: self.resumes(origin),
                    },
                ))
            },
        )
    }

    /// The query a continuation issued at row position `origin` resumes.
    fn resumes(&self, origin: Option<usize>) -> Option<usize> {
        match &self.origin {
            RowOrigin::Query(index) => Some(*index),
            origin_of_row => origin.map(|position| origin_of_row.of(position)),
        }
    }

    /// What a saved page keeps of its rows' queries: a page re-executes only
    /// the admitted rows, so positions that are not their indices are kept.
    fn page_origin(&self) -> (Option<usize>, Option<Vec<usize>>) {
        match &self.origin {
            RowOrigin::Query(index) => (Some(*index), None),
            RowOrigin::Rows(indices) => (None, Some(indices.clone())),
            RowOrigin::Request => {
                let identity = self
                    .row_indices
                    .iter()
                    .copied()
                    .eq(0..self.row_indices.len());
                (None, (!identity).then(|| self.row_indices.clone()))
            }
        }
    }

    /// Move the walks of the rows the response page splits to
    /// `responsePagination.next`: a page break sits inside such a row, so
    /// its walk can only follow its last part, and one loop through the
    /// pages then reaches it. A row whose own page is shown whole keeps its
    /// `next.nextPage` where it is, so finishing the pages never walks rows
    /// the reader is done with. A walk that continues the page its row
    /// answered (`continues_page`: the rest of that page is the rest of this
    /// response) moves too, with every other walk. A section's own walk
    /// (`next.<section>`) continues that section alone: it stays on its row,
    /// and a split row places it on the row's last part (`row_pages`,
    /// `take_enclosing_walks`). Rows that fill the response page together
    /// ([`read_share::fill_together`]) continue together: when two or more
    /// of them read on, one call continues them all and fills its page
    /// again, so the rest of a batch never costs a call per row.
    ///
    /// `true` when it measured the envelope and left it as it is because it
    /// fits one page, so the stage need not measure it again
    /// ([`octocode_runtime_base::response::page_model::ResponsePageOptions::fits_page`]).
    pub(super) fn hoist_walks(
        &self,
        structured: &mut Sanitized,
        pending: &mut Pending,
        continues_page: bool,
    ) -> Result<bool, contracts::ContractValidationError> {
        let mut walks = continuations::linear_walks(self.id, structured)?;
        walks.retain(|walk| {
            walk.last()
                .is_some_and(|name| name == continuations::LINEAR_WALK)
        });
        let together = read_share::fill_together(self.id) && walks.len() > 1;
        if !continues_page && !together {
            let budget = self
                .options
                .response_chars
                .unwrap_or(self.output.auto_page_chars);
            if budget == 0 {
                return Ok(false);
            }
            if !self.options.pages(structured, self.output.auto_page_chars) {
                return Ok(true);
            }
            let split = split_rows(structured, budget.saturating_sub(page_block_chars(self.id)));
            walks.retain(|walk| {
                walk.get(1)
                    .and_then(|row| row.parse::<usize>().ok())
                    .is_some_and(|row| split.contains(&row))
            });
        }
        let handles = continuations::walk_handles(structured, &walks);
        if handles.is_empty() {
            return Ok(false);
        }
        let Some(token) = pending.walk_handle(self.id, &handles) else {
            return Ok(false);
        };
        structured.edit(|output| {
            continuations::hoist(output, &walks, self.id, &token);
        });
        Ok(false)
    }

    /// Every read of a section plan restarts its idle lifetime: a page read
    /// from a saved clasify response refreshes each plan it continues, named
    /// in its rows or in the walk handle a split batch's next page is hoisted
    /// to (`responsePagination.next`), so a reader walking the pages still
    /// reaches the next batch. Opening the handle restarts its own lifetime,
    /// as a read of it does.
    pub(super) fn refresh_plans(&self, saved: &Value) {
        let walk = saved
            .pointer("/responsePagination/next/input/cursor")
            .and_then(Value::as_str)
            .and_then(|token| self.cursors.open(self.id, token, self.access).ok());
        let inputs = std::iter::once(saved).chain(walk.as_ref().map(|walk| &walk.member().input));
        octocode_tool_clasify::clasify::sections::refresh_continued(
            &self.section_sessions,
            &self.access,
            inputs,
        );
    }

    pub(super) fn register_page(
        &self,
        outcome: &mut ToolOutcome,
        cursor: Option<Cursor>,
        pending: &mut Pending,
    ) -> Result<(), ExecutionError> {
        let Some(cursor) = cursor else {
            return Ok(());
        };
        outcome.structured_content.edit(|output| {
            let next = &mut output["responsePagination"]["next"];
            let (index, rows) = self.page_origin();
            let token = pending.insert(
                self.id,
                self.response_query.clone(),
                SavedWork::ResponsePage {
                    cursor,
                    shape: self.page_shape.clone(),
                    index,
                    rows,
                },
            );
            // The page was measured with the placeholder handle, which has
            // the issued handle's exact length (`cursor::issued`), so the
            // page size and its oversized flag stand.
            next["input"] = json!({"cursor":token});
            Ok(())
        })
    }
}

/// The positions of the rows that alone fill more than a page of `budget`
/// chars: the rows a response page splits into parts.
fn split_rows(structured: &Value, budget: usize) -> Vec<usize> {
    let size = |value: &Value| octocode_engine::json_size::utf16_units(value).unwrap_or(0);
    let Some(rows) = structured.get("results").and_then(Value::as_array) else {
        return Vec::new();
    };
    let sizes: Vec<usize> = rows.iter().map(size).collect();
    let rest = size(structured).saturating_sub(sizes.iter().sum::<usize>());
    sizes
        .iter()
        .enumerate()
        .filter(|(_, row)| rest + **row > budget)
        .map(|(index, _)| index)
        .collect()
}

/// What a row loses when its `channel` lead cannot be saved: the units its
/// receipt left undecided, which only saved plan pages reach, or the whole
/// read that cannot restart without private progress.
fn unsaved_lead_warning(row: &Value, channel: &str) -> String {
    let counts = |field: &str| row["selection"][field].as_u64();
    match (
        recovery_lead(channel),
        counts("unknownItems"),
        counts("totalItems"),
    ) {
        (Some(RecoveryLead::Saved), Some(unknown), Some(total)) => format!(
            "The {channel} lead is not offered: this command exits after one call and keeps no saved pages, so {unknown} of {total} units stay undecided; ask a narrower query to judge them, or use a live session."
        ),
        _ => format!(
            "The {channel} lead is not offered: this command exits after one call and keeps no saved work; repeat the read without classification to see everything."
        ),
    }
}

/// A continued whole-read lead as a fresh related call, or `None` when
/// it is not one once its private progress is dropped.
fn fresh_whole_read(call: &Value) -> Option<Value> {
    let target = octocode_runtime_base::id::ToolId::from_name(call["tool"].as_str()?)?;
    let mut input = call["input"].clone();
    for row in input.get_mut("queries")?.as_array_mut()? {
        *row = octocode_runtime_base::response::result::fresh_query_row(target, row);
    }
    let mut checked =
        crate::runtime::engine::admission::checked::checked_input(target, true, &input).ok()?;
    // Validation fills in the contract defaults; a call states none.
    for row in checked.get_mut("queries")?.as_array_mut()? {
        octocode_runtime_base::response::result::canonical_row(target, row);
    }
    Some(json!({"kind": "related", "tool": target.as_str(), "input": checked}))
}
