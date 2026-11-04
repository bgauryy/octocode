//! An empty first page of a pattern with no return annotation: a pattern
//! matches the parts it spells, so the functions of a typed codebase are not
//! its matches. When its annotated spelling holds some, they are the answer,
//! disclosed once in the row; every continuation is that query's own, so the
//! pages after it run the same pattern and repeat nothing.
use super::exact_miss::exact_miss;
use super::query::MatchQuery;
use super::rows::push_diagnostic;
use super::scope::scope_extension;
use super::types::{Scan, Scope};
use crate::execution::cancel::CancellationCheck;
use crate::policy::path::PathPolicy;
use crate::response::result::ToolError;
use octocode_contract_types::AstSearchRule;
use octocode_engine::structural::pattern_with_return_types;
use serde_json::{Value, json};

/// The page that answers an empty first page in place of the requested
/// pattern's, with `run` executing the retried query: it counts, beside its
/// hits, the declarations neither spelling matched. A cut scan counts nothing,
/// as its scope is unread. `None` leaves the requested answer.
pub(super) fn rewritten_page(
    q: MatchQuery<'_>,
    scope: &Scope,
    scan: &Scan,
    paths: &PathPolicy,
    cancel: &dyn CancellationCheck,
    run: impl FnOnce(MatchQuery<'_>) -> Result<Value, ToolError>,
) -> Result<Option<Value>, ToolError> {
    let Some(retry) = retry(q, scope, scan) else {
        return Ok(None);
    };
    let Some(mut page) = retry.page(q, cancel, run)? else {
        return Ok(None);
    };
    if !scan.truncated
        && let Some(miss) = exact_miss(q, scope, paths, cancel)
    {
        miss.attach_beside_hits(&mut page);
    }
    Ok(Some(page))
}

/// The query to run instead, and what to say about it.
struct Retry {
    requested: String,
    annotated: String,
    rule: AstSearchRule,
    /// The scan was cut, so the requested pattern may still match later: the
    /// retry searches both spellings for the rest of the walk.
    cut: bool,
}

/// The one annotated spelling of `pattern` in every grammar the walk met that
/// has one: a grammar without a return slot has nothing to fill, and grammars
/// that spell it differently share no single query.
fn annotated_spelling(pattern: &str, scope: &Scope, scan: &Scan) -> Option<String> {
    let extensions = if scope.is_file || scope.language.is_some() {
        vec![scope_extension(scope)?]
    } else {
        scan.census.keys().cloned().collect()
    };
    let mut spelled = extensions
        .iter()
        .filter_map(|extension| pattern_with_return_types(pattern, extension));
    let first = spelled.next()?;
    spelled.all(|other| other == first).then_some(first)
}

/// The retry of a first page whose top-level `rule.pattern` matched nothing;
/// `None` when the pattern has no annotated spelling (one that spells a
/// return type, or a rule with no pattern, stops here at no cost).
fn retry(q: MatchQuery<'_>, scope: &Scope, scan: &Scan) -> Option<Retry> {
    if q.page() != 1 || q.match_page() != 1 || q.scan_offset() != 0 {
        return None;
    }
    let requested = q.pattern_text()?;
    let annotated = annotated_spelling(&requested, scope, scan)?;
    let mut rule = q.rule()?;
    let fields = rule.as_object_mut()?;
    if scan.truncated {
        // A caller's own `any` already constrains this node.
        if fields.contains_key("any") {
            return None;
        }
        fields.shift_remove("pattern");
        fields.insert(
            "any".into(),
            json!([{"pattern": requested}, {"pattern": annotated}]),
        );
    } else {
        fields.insert("pattern".into(), json!(annotated));
    }
    Some(Retry {
        rule: serde_json::from_value(rule).ok()?,
        requested,
        annotated,
        cut: scan.truncated,
    })
}

impl Retry {
    /// The retried page, disclosed; `None` leaves the requested pattern's
    /// empty answer, also when the retry fails (a grammar rejects the
    /// spelling). A cut scan's page continues the walk with both spellings
    /// even while it holds no match yet.
    fn page(
        self,
        q: MatchQuery<'_>,
        cancel: &dyn CancellationCheck,
        run: impl FnOnce(MatchQuery<'_>) -> Result<Value, ToolError>,
    ) -> Result<Option<Value>, ToolError> {
        let page = q.under_rule(self.rule.clone(), run);
        cancel.check().map_err(ToolError::cancelled)?;
        let Ok(mut page) = page else {
            return Ok(None);
        };
        if !self.cut && page["matchCount"].as_u64().unwrap_or(0) == 0 {
            return Ok(None);
        }
        push_diagnostic(&mut page, self.diagnostic());
        Ok(Some(page))
    }

    fn diagnostic(&self) -> Value {
        let Self {
            requested,
            annotated,
            cut,
            ..
        } = self;
        let (message, recovery) = if *cut {
            (
                format!(
                    "Searching for pattern \"{annotated}\" as well as the requested pattern \"{requested}\", which returned no matches in the files read so far: a pattern with no return type does not match a function that has one."
                ),
                format!(
                    "Use rule any with patterns \"{requested}\" and \"{annotated}\" to repeat this query explicitly."
                ),
            )
        } else {
            (
                format!(
                    "Executed pattern \"{annotated}\" after the requested pattern \"{requested}\" returned no matches: a pattern with no return type does not match a function that has one."
                ),
                format!("Use pattern: \"{annotated}\" to repeat this query explicitly."),
            )
        };
        json!({
            "code": "structural.query.rewritten",
            "severity": "info",
            "stage": "match",
            "message": message,
            "recovery": recovery,
        })
    }
}
