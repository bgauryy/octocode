//! An empty rule whose pattern spells a literal token that looks like a
//! metavariable: a metavariable is a whole node, `$` (or `$$$`) and an
//! upper-case name, so `search_$X($$$A)` (one identifier glued to text) and
//! `$x.unwrap()` (a lower-case name) match only source that spells them. The
//! fact comes from the pattern alone; a token some scanned file spells is a
//! real name, not a sigil (JavaScript `$el`), and a pattern under `not`
//! cannot empty a result. The row names each token in each grammar the scope
//! reads and the rule that works, beside the other explainers; the repair call
//! runs it when the rewrite is exact, the same in every grammar.
use super::exact_miss::{asked_view, lead_call, with_rule};
use super::query::{MatchQuery, RulePattern};
use super::reads::row_file;
use super::scope::{push_by_grammar, read_grammars, scope_extension};
use super::types::{Scan, Scope};
use octocode_engine::structural::{
    LiteralSigil, SigilForm, SigilRepair, WrittenPattern, literal_sigils, sigil_repair,
};
use octocode_runtime_base::execution::cancel::CancellationCheck;
use serde_json::{Value, json};
use std::collections::BTreeSet;

/// The message naming a literal `token`.
fn sigil_note(token: &str) -> String {
    format!(
        "`{token}` is literal text, not a metavariable: a metavariable is a whole node, `$` or `$$$` plus upper-case letters, digits or `_`."
    )
}

/// The working rule for `sigil`: `rewritten` is the rule object holding its
/// pattern, rewritten exactly, when there is one; `call` says the repair call
/// runs the whole rule rewritten.
fn sigil_recovery_note(sigil: &LiteralSigil, rewritten: Option<&Value>, call: bool) -> String {
    let rule = match (sigil.form, rewritten) {
        (SigilForm::LowerCase, Some(_)) => format!(
            "Spell the name in upper case: `{}`.",
            sigil.token.to_ascii_uppercase()
        ),
        (SigilForm::Glued, Some(rewritten)) => {
            format!("Match the node and test its text: rule {rewritten}.")
        }
        (_, None) => {
            "Match the whole node with a capture and test its text with `regex`.".to_owned()
        }
    };
    if call {
        format!("{rule} The repair call runs the rule with this rewrite.")
    } else {
        rule
    }
}

/// What one grammar reads of a rule's literal sigils.
struct Reading {
    /// Each distinct token, with the rule object holding its pattern
    /// rewritten exactly, when it is.
    sigils: Vec<(LiteralSigil, Option<Value>)>,
    /// The whole rule rewritten, when it has a sigil and every pattern that
    /// spells one has an exact rewrite.
    repaired: Option<Value>,
}

/// The node kinds of a macro's token trees (a Rust `macro_rules!` pattern and
/// body, and a macro call's arguments): a `$x` inside one is that macro's
/// metavariable, not a name a pattern could match.
const MACRO_TOKEN_TREES: [&str; 3] = ["token_tree", "token_tree_pattern", "token_binding_pattern"];

/// The tokens some file of the scan spells as a whole node of the kind that
/// holds them in the pattern, outside macro token trees (a JavaScript `$el`
/// identifier): each is then a real name of the source, not a sigil the
/// caller meant. A comment, doc comment or string that holds the token is
/// another kind or a longer node, and `$xs` is not `$x`. `wanted` lists, per
/// parser extension, each token with its kind. The scan's own files are read,
/// with no second walk: a byte search first, and only a file that holds a
/// token's bytes is parsed, with the parser the scan read it with. A file
/// that no longer reads or parses proves nothing either way.
fn spelled_in_sources(
    scan: &Scan,
    scope: &Scope,
    wanted: &[(String, Vec<(String, String)>)],
    cancel: &dyn CancellationCheck,
) -> BTreeSet<String> {
    let mut spelled = BTreeSet::new();
    let total = wanted
        .iter()
        .flat_map(|(_, tokens)| tokens.iter().map(|(token, _)| token))
        .collect::<BTreeSet<_>>()
        .len();
    for file in &scan.files {
        if spelled.len() == total || cancel.check().is_err() {
            break;
        }
        let Some(parser) = Some(file.parser.clone())
            .filter(|parser| !parser.is_empty())
            .or_else(|| scope_extension(scope))
        else {
            continue;
        };
        let Some((_, tokens)) = wanted.iter().find(|(extension, _)| *extension == parser) else {
            continue;
        };
        let Some(path) = row_file(scope, &json!({"path": file.path})) else {
            continue;
        };
        let Ok(Some(bytes)) = super::super::source::read_parse_source(&path) else {
            continue;
        };
        let held = tokens
            .iter()
            .filter(|(token, _)| {
                !spelled.contains(token) && memchr::memmem::find(&bytes, token.as_bytes()).is_some()
            })
            .collect::<Vec<_>>();
        if held.is_empty() {
            continue;
        }
        let Ok(source) = super::super::source::matched_text(&path, &bytes) else {
            continue;
        };
        let rule = json!({"any": held
            .iter()
            .map(|(token, kind)| json!({"kind": kind, "regex": format!("^{}$", regex::escape(token))}))
            .collect::<Vec<_>>()});
        let outside_macros = json!({"all": [rule, {"not": {"inside": {
            "any": MACRO_TOKEN_TREES.iter().map(|kind| json!({"kind": kind})).collect::<Vec<_>>(),
            "stopBy": "end",
        }}}]});
        let search = |rule: &Value| {
            octocode_engine::structural::search_detailed(
                &source,
                &path.to_string_lossy(),
                Some(&parser),
                None,
                Some(rule),
            )
        };
        // A grammar with no macro token trees rejects their kinds: its
        // sources have no such nodes to leave out, so the plain rule holds.
        let found = Some(search(&outside_macros))
            .filter(|found| found.status == "ok")
            .unwrap_or_else(|| search(&rule));
        for node in &found.matches {
            if let Some((token, _)) = held.iter().find(|(token, kind)| {
                *token == node.text && node.node_kind.as_deref() == Some(kind.as_str())
            }) {
                spelled.insert(token.clone());
            }
        }
    }
    spelled
}

/// The rule object holding `pattern`, rewritten by `repair`: the pattern
/// respelled, and each constraint of its glued tokens added to its `all`.
/// Returns the rewrite of that object alone, as the recovery shows it.
fn rewrite_holder(
    holder: &mut serde_json::Map<String, Value>,
    pattern: WrittenPattern<'_>,
    repair: SigilRepair,
) -> Value {
    let respelled = match pattern {
        WrittenPattern::Text(_) => json!(repair.pattern),
        WrittenPattern::Context { selector, .. } => {
            json!({"context": repair.pattern, "selector": selector})
        }
    };
    if repair.constraints.is_empty() {
        holder.insert("pattern".into(), respelled.clone());
        return json!({"pattern": respelled});
    }
    let mut parts = vec![json!({"pattern": respelled})];
    parts.extend(repair.constraints);
    holder.shift_remove("pattern");
    let mut all = holder
        .get("all")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    all.extend(parts.iter().cloned());
    holder.insert("all".into(), Value::Array(all));
    json!({"all": parts})
}

/// The literal sigils of each of `patterns` (with the ones the grammar of
/// `extension` reads in it) that no source spells.
fn read_in(
    rule: &Value,
    patterns: &[(&RulePattern, Vec<LiteralSigil>)],
    spelled: &BTreeSet<String>,
    extension: &str,
) -> Reading {
    let mut repaired = Some(rule.clone());
    let mut sigils: Vec<(LiteralSigil, Option<Value>)> = Vec::new();
    for (found, read) in patterns {
        let Some(pattern) = found.written() else {
            continue;
        };
        let kept = read
            .iter()
            .filter(|sigil| !spelled.contains(&sigil.token))
            .cloned()
            .collect::<Vec<_>>();
        if kept.is_empty() {
            continue;
        }
        let holder = repaired
            .as_mut()
            .and_then(|rule| rule.pointer_mut(&found.at))
            .and_then(Value::as_object_mut);
        // A rewrite respells every sigil of the pattern: exact only when no
        // source spells any of them.
        let exact = kept.len() == read.len();
        let rewritten = match (
            exact.then(|| sigil_repair(pattern, extension)).flatten(),
            holder,
        ) {
            (Some(repair), Some(holder)) => Some(rewrite_holder(holder, pattern, repair)),
            _ => {
                repaired = None;
                None
            }
        };
        for sigil in kept {
            if !sigils.iter().any(|(seen, _)| seen.token == sigil.token) {
                sigils.push((sigil, rewritten.clone()));
            }
        }
    }
    if sigils.is_empty() {
        repaired = None;
    }
    Reading { sigils, repaired }
}

/// Name each literal sigil of `q`'s patterns that no file of the scan spells,
/// in each grammar the scope reads, and attach the repair call when every
/// grammar rewrites the rule exactly and alike. A note beside the other
/// explainers, never in place of them: a sigil may not be why the result is
/// empty.
pub(super) fn attach_literal_miss(
    q: MatchQuery<'_>,
    scope: &Scope,
    scan: &Scan,
    cancel: &dyn CancellationCheck,
    out: &mut Value,
) {
    let grammars = read_grammars(scope);
    let Some(rule) = q.rule() else {
        return;
    };
    let patterns = q.patterns();
    let positive = patterns
        .iter()
        .filter(|found| !found.negated())
        .collect::<Vec<_>>();
    let read = grammars
        .iter()
        .map(|(_, extension)| {
            positive
                .iter()
                .map(|found| {
                    let sigils = found
                        .written()
                        .map(|pattern| literal_sigils(pattern, extension))
                        .unwrap_or_default();
                    (*found, sigils)
                })
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    // Each grammar's tokens with the kind that holds them; a token in no
    // named leaf has no source node to be, and stays a sigil.
    let wanted = grammars
        .iter()
        .zip(&read)
        .map(|((_, extension), patterns)| {
            let tokens = patterns
                .iter()
                .flat_map(|(_, sigils)| sigils)
                .filter_map(|sigil| Some((sigil.token.clone(), sigil.kind.clone()?)))
                .collect::<BTreeSet<_>>();
            (extension.clone(), tokens.into_iter().collect::<Vec<_>>())
        })
        .collect::<Vec<_>>();
    if read.iter().flatten().all(|(_, sigils)| sigils.is_empty()) {
        return;
    }
    let spelled = spelled_in_sources(scan, scope, &wanted, cancel);
    let readings = grammars
        .iter()
        .zip(&read)
        .map(|((_, extension), patterns)| read_in(&rule, patterns, &spelled, extension))
        .collect::<Vec<_>>();
    if readings.iter().all(|reading| reading.sigils.is_empty()) {
        return;
    }
    let repaired = readings
        .first()
        .and_then(|first| first.repaired.clone())
        .filter(|rule| {
            readings
                .iter()
                .all(|reading| reading.repaired.as_ref() == Some(rule))
        });
    let call = repaired
        .and_then(|rule| with_rule(q, scope, rule))
        .map(|query| lead_call(query, asked_view(q).as_ref()).build());
    let found = readings
        .iter()
        .map(|reading| {
            reading
                .sigils
                .iter()
                .map(|(sigil, rewritten)| {
                    json!({
                        "code": "structural.pattern.literalSigil",
                        "severity": "warning",
                        "stage": "match",
                        "message": sigil_note(&sigil.token),
                        "recovery": sigil_recovery_note(sigil, rewritten.as_ref(), call.is_some()),
                    })
                })
                .collect()
        })
        .collect();
    push_by_grammar(out, &grammars, found);
    if let Some(call) = call {
        out["calls"]["repair"] = call;
    }
}
