//! The input gate: the one place a caller's query is checked, and the only
//! place a [`CheckedQuery`] is made. Every tool runs on a `CheckedQuery`, so a
//! query that skipped this gate cannot reach a tool.
//!
//! The walk sanitizes the owned query in place: a string is replaced only by
//! its redacted form, and an entry the policy refuses (a blocked key, an
//! over-long string, a too-large array) is removed where it stands. Nothing is
//! rebuilt or cloned, so a clean query costs one scan of its strings.

use std::ops::Deref;

use serde_json::{Map, Value};

use super::ContentSecurity;
use crate::contracts::ValidationIssue;

/// Security ceiling, not a data cap: an input string past it is refused at
/// the gate (paths, patterns and keywords are far shorter), so no caller
/// value can force unbounded scanning or redaction work.
const MAX_STRING_LENGTH: usize = 10_000;
/// Contract `limits.maxInputArrayItems`.
const MAX_ARRAY_LENGTH: usize = crate::id::MAX_INPUT_ARRAY_ITEMS;
/// Security ceiling, not a data cap: JSON nesting deeper than this is
/// refused, so a hostile query cannot exhaust the stack of the recursive
/// walk. No tool input nests anywhere near it.
const MAX_DEPTH: usize = 20;
/// The field of a clasify row that holds the evidence under judgment.
const EVIDENCE: &str = "content";
/// The field of an astSearch row that holds inline source in place of a file.
const SNIPPET: &str = "content";

/// The one field of a row whose text is source or evidence, not the
/// caller's own words: bounded by its own limit and redacted in place.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Evidence {
    field: &'static str,
    limit: usize,
    /// Whether `limit` bounds the field's serialized JSON (any JSON value:
    /// clasify `content`) or the characters of its string (a snippet).
    serialized: bool,
}

/// A query row that passed [`ContentSecurity::check_input`]: its keys are
/// safe, its strings are within bounds, and it holds no credential outside
/// the evidence a policy may redact in place and the private progress a
/// saved row restores ([`ContentSecurity::check_saved_input`]).
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
#[serde(transparent)]
pub struct CheckedQuery(
    Value,
    /// The row's field the gate redacted in place as evidence or source
    /// ([`InputPolicy::ClasifyEvidence`], [`InputPolicy::LocalSnippet`]), not
    /// the caller's own words.
    #[serde(skip)]
    Option<&'static str>,
    /// The row's private progress fields a saved row restored, which the
    /// gate left as the runtime wrote them: no caller's words either.
    #[serde(skip)]
    Vec<String>,
);

impl CheckedQuery {
    pub fn into_value(self) -> Value {
        self.0
    }
}

#[cfg(any(test, feature = "test-support"))]
impl CheckedQuery {
    /// `query` through the gate with [`InputPolicy::Query`]; panics when the
    /// gate refuses it, so a test row is always one a caller could send.
    #[expect(
        clippy::panic,
        reason = "a test row the gate refuses fails its test at once"
    )]
    pub fn checked(query: Value) -> Self {
        ContentSecurity::new()
            .check_input(query, InputPolicy::Query)
            .unwrap_or_else(|issues| panic!("the input gate refused a test query: {issues:?}"))
    }
}

impl std::borrow::Borrow<Value> for CheckedQuery {
    fn borrow(&self) -> &Value {
        &self.0
    }
}

impl Deref for CheckedQuery {
    type Target = Value;

    fn deref(&self) -> &Value {
        &self.0
    }
}

/// The caller's own text: every string the input gate admitted in the rows
/// of one call, a search term, a question, an address. Only admitted rows
/// make one ([`Admitted::of`]), so each string in it is one the gate would
/// not refuse, and a response that repeats it (a lead's input, an error that
/// quotes a pattern) says nothing the caller did not send. The output gate
/// therefore lets a redaction stand only where it replaces more than bytes
/// of such a string ([`Admitted::restore`]): text of a file around the
/// string, or a value the string is only a part of, stays hidden.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Admitted(Vec<String>);

impl Admitted {
    /// The strings of `rows`. A clasify row's `content` and an astSearch
    /// row's `code` are evidence the gate redacts, not text it admits, and a
    /// saved row's private progress is state the gate leaves unjudged: none
    /// is any part of it.
    pub fn of<'a>(rows: impl IntoIterator<Item = &'a CheckedQuery>) -> Self {
        fn leaves(value: &Value, out: &mut Vec<String>) {
            match value {
                Value::String(text) if !text.is_empty() => out.push(text.clone()),
                Value::Array(items) => items.iter().for_each(|item| leaves(item, out)),
                Value::Object(fields) => fields.values().for_each(|field| leaves(field, out)),
                _ => {}
            }
        }
        let mut own = Vec::new();
        for CheckedQuery(row, evidence, private) in rows {
            match row {
                Value::Object(fields) => fields
                    .iter()
                    .filter(|(key, _)| *evidence != Some(key.as_str()))
                    .filter(|(key, _)| !private.contains(key))
                    .for_each(|(_, field)| leaves(field, &mut own)),
                other => leaves(other, &mut own),
            }
        }
        own.sort_unstable();
        own.dedup();
        Self(own)
    }

    pub(super) fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Where `text` repeats an admitted string, by byte position.
    pub(super) fn spans(&self, text: &str) -> Vec<std::ops::Range<usize>> {
        self.0
            .iter()
            .filter(|own| own.len() <= text.len())
            .flat_map(|own| {
                memchr::memmem::find_iter(text.as_bytes(), own.as_bytes())
                    .map(|at| at..at + own.len())
            })
            .collect()
    }

    /// Whether `text` repeats an admitted string anywhere.
    pub(super) fn occurs_in(&self, text: &str) -> bool {
        self.0
            .iter()
            .any(|own| memchr::memmem::find(text.as_bytes(), own.as_bytes()).is_some())
    }

    /// `clean`, the redaction of `text` that replaced `edits`, with each
    /// replacement that took only bytes of one admitted string undone;
    /// `None` when that leaves `text` as it was.
    pub(super) fn restore(
        &self,
        text: &str,
        clean: &str,
        edits: &octocode_engine::security::Edits,
    ) -> Option<String> {
        restore_within(&self.spans(text), text, clean, edits)
    }

    /// `text` with `secret` replaced by `placeholder` wherever it stands
    /// outside every admitted string `text` repeats; `None` when no place is.
    pub(super) fn replace_outside(
        &self,
        text: &str,
        secret: &str,
        placeholder: &str,
    ) -> Option<String> {
        if secret.is_empty() || !text.contains(secret) {
            return None;
        }
        if self.is_empty() {
            return Some(text.replace(secret, placeholder));
        }
        let spans = self.spans(text);
        let mut replaced = String::with_capacity(text.len());
        let mut from = 0;
        let mut any = false;
        for (at, _) in text.match_indices(secret) {
            let end = at + secret.len();
            if at < from || spans.iter().any(|span| span.start <= at && end <= span.end) {
                continue;
            }
            replaced.push_str(text.get(from..at)?);
            replaced.push_str(placeholder);
            from = end;
            any = true;
        }
        replaced.push_str(text.get(from..)?);
        any.then_some(replaced)
    }
}

/// `clean`, the redaction of `text` that replaced `edits`, with each
/// replacement that took only bytes of one of `spans` undone; `None` when
/// that leaves `text` as it was.
pub(super) fn restore_within(
    spans: &[std::ops::Range<usize>],
    text: &str,
    clean: &str,
    edits: &octocode_engine::security::Edits,
) -> Option<String> {
    let mut restored = String::with_capacity(clean.len());
    let mut from = 0;
    for (taken, replacement) in edits.runs(clean) {
        let own = spans
            .iter()
            .any(|span| span.start <= taken.start && taken.end <= span.end);
        let (Some(before), Some(original)) = (text.get(from..taken.start), text.get(taken.clone()))
        else {
            return Some(clean.to_owned());
        };
        restored.push_str(before);
        restored.push_str(if own { original } else { replacement });
        from = taken.end;
    }
    restored.push_str(text.get(from..).unwrap_or_default());
    (restored != text).then_some(restored)
}

/// The fields of a local tool's row that name a path on this machine
/// ([`InputPolicy::LocalQuery`]).
const PATH_FIELDS: [&str; 4] = ["path", "paths", "target", "workspaceRoot"];

/// What a row may contain beyond the standard bounds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputPolicy {
    /// Every string is bounded and a credential-shaped one is refused:
    /// redacting it would silently change what the tool reads, and leak the
    /// placeholder into continuations.
    Query,
    /// A local tool's row: a path field names a file or directory on this
    /// machine, where a name may hold a credential (`deploy-ghp_….txt`). It
    /// never leaves the machine, and the display of the path stays redacted,
    /// so a credential-shaped path is no refusal. Every other string is
    /// bounded and refused as in [`InputPolicy::Query`].
    LocalQuery,
    /// A clasify row: its `content` may be as long as `content_limit` serialized
    /// characters, and a credential inside `content` is redacted in place: it
    /// is the evidence under judgment, which holds what its source holds.
    /// Every other string is the caller's own words (`questions`, `background`,
    /// a source's address), where a credential is refused as in any query.
    ClasifyEvidence { content_limit: usize },
    /// An astSearch row: a local row ([`InputPolicy::LocalQuery`]) whose
    /// `content` is inline source standing in for a file. It may be as long as
    /// `content_limit` characters (the contract's own bound of the field), and
    /// a credential inside it is redacted in place, as a read of a file with
    /// that text shows it: the tool parses the redacted text, so positions
    /// count what is shown, and no continuation, saved page or lead holds
    /// what a read would hide.
    LocalSnippet { content_limit: usize },
}

impl InputPolicy {
    /// The policy of `tool`'s rows.
    pub fn of(tool: crate::id::ToolId) -> Self {
        if tool.is_clasify() {
            return Self::ClasifyEvidence {
                content_limit: crate::id::clasify_policy::REQUEST_BYTES,
            };
        }
        let snippet_limit = (tool == crate::id::ToolId::AstSearch)
            .then(|| crate::contracts::query_schema_number(tool, None, SNIPPET, "maxLength"))
            .flatten();
        match snippet_limit {
            Some(limit) => Self::LocalSnippet {
                content_limit: octocode_engine::num::saturating_usize(limit),
            },
            None if tool.is_local() => Self::LocalQuery,
            None => Self::Query,
        }
    }

    fn evidence(self) -> Option<Evidence> {
        match self {
            Self::Query | Self::LocalQuery => None,
            Self::ClasifyEvidence { content_limit } => Some(Evidence {
                field: EVIDENCE,
                limit: content_limit,
                serialized: true,
            }),
            Self::LocalSnippet { content_limit } => Some(Evidence {
                field: SNIPPET,
                limit: content_limit,
                serialized: false,
            }),
        }
    }

    /// Whether a path field may hold a credential-shaped name.
    fn local_paths(self) -> bool {
        matches!(self, Self::LocalQuery | Self::LocalSnippet { .. })
    }
}

/// The facts one walk found.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Verdict {
    pub(crate) is_valid: bool,
    pub(crate) has_secrets: bool,
    pub warnings: Vec<String>,
    /// Dotted paths of the string leaves whose value sanitization rewrote
    /// (`matchString`, `keywords[]`, `outer.key`).
    pub(crate) secret_fields: Vec<String>,
}

impl Verdict {
    fn clean() -> Self {
        Self {
            is_valid: true,
            has_secrets: false,
            warnings: Vec::new(),
            secret_fields: Vec::new(),
        }
    }

    fn refused(&mut self, warning: impl Into<String>) {
        self.is_valid = false;
        self.warnings.push(warning.into());
    }

    fn invalid(warning: impl Into<String>) -> Self {
        let mut verdict = Self::clean();
        verdict.refused(warning);
        verdict
    }

    /// Fold a nested object's verdict into this one, naming its secret
    /// fields with `field` and its warnings with `warning`.
    fn absorb(
        &mut self,
        nested: Self,
        field: impl Fn(&str) -> String,
        warning: impl Fn(&str) -> String,
    ) {
        self.has_secrets |= nested.has_secrets;
        self.is_valid &= nested.is_valid;
        self.secret_fields
            .extend(nested.secret_fields.iter().map(|name| field(name)));
        self.warnings
            .extend(nested.warnings.iter().map(|text| warning(text)));
    }
}

/// A walk's verdict and the query as the walk left it: refused entries
/// removed, credential-shaped strings redacted.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ValidationResult {
    pub(crate) sanitized_params: Map<String, Value>,
    verdict: Verdict,
}

impl Deref for ValidationResult {
    type Target = Verdict;

    fn deref(&self) -> &Verdict {
        &self.verdict
    }
}

impl ContentSecurity {
    /// The one input gate. `query` is sanitized in place; the result is the
    /// proof every tool runs on, or the field issues (relative to the row)
    /// that refused it.
    pub fn check_input(
        &self,
        query: Value,
        policy: InputPolicy,
    ) -> Result<CheckedQuery, Vec<ValidationIssue>> {
        let evidence = policy.evidence();
        let checked = self.walk(query, evidence, policy.local_paths());
        let field = evidence.map(|evidence| evidence.field);
        let issues = input_issues(&checked, field);
        if issues.is_empty() {
            Ok(CheckedQuery(
                Value::Object(checked.sanitized_params),
                field,
                Vec::new(),
            ))
        } else {
            Err(issues)
        }
    }

    /// The gate for a row of `tool` restored from the issued-cursor store,
    /// as a cursor opens it. Its private progress (the fields `tool`'s
    /// contract marks `x-cursor`, [`crate::contracts::state_fields`]: a
    /// selection plan, the files a walk listed, a provider's page cursor) is
    /// the runtime's own state, proven by where it comes from: a fresh call
    /// cannot send it (`input.private-state`), the runtime wrote it when it
    /// issued the cursor, only the handle it issued restores it, and the
    /// contract checked it against its own schema on restore. It is no
    /// caller's text, so it is not judged as caller text (a credential's
    /// shape, a caller string's length) and stays as written. Every other
    /// field passes the gate as on a fresh call.
    pub fn check_saved_input(
        &self,
        mut query: Value,
        policy: InputPolicy,
        tool: crate::id::ToolId,
    ) -> Result<CheckedQuery, Vec<ValidationIssue>> {
        let state = crate::contracts::state_fields(tool);
        // Held out in place, so the row keeps its field order.
        let mut held = Vec::new();
        if let Value::Object(row) = &mut query {
            for (field, value) in row.iter_mut().filter(|(field, _)| state.contains(*field)) {
                held.push((field.clone(), value.take()));
            }
        }
        let mut checked = self.check_input(query, policy)?;
        if let Value::Object(row) = &mut checked.0 {
            for (field, value) in held {
                if let Some(slot) = row.get_mut(&field) {
                    *slot = value;
                    checked.2.push(field);
                }
            }
        }
        Ok(checked)
    }

    /// The walk over a borrowed query, for inspecting a verdict.
    #[cfg(test)]
    pub(crate) fn validate_input_parameters(&self, params: &Value) -> ValidationResult {
        self.walk(params.clone(), None, false)
    }

    /// The walk with a clasify `content` allowance, for inspecting a verdict.
    #[cfg(test)]
    pub(crate) fn validate_content_parameters(
        &self,
        params: &Value,
        content_limit: usize,
    ) -> ValidationResult {
        self.walk(
            params.clone(),
            InputPolicy::ClasifyEvidence { content_limit }.evidence(),
            false,
        )
    }

    fn walk(&self, params: Value, evidence: Option<Evidence>, paths: bool) -> ValidationResult {
        let Value::Object(mut object) = params else {
            return ValidationResult {
                sanitized_params: Map::new(),
                verdict: Verdict::invalid("Invalid parameters: must be an object"),
            };
        };
        if let Some(Evidence {
            field,
            limit,
            serialized,
        }) = evidence
            && let Some(state) = object.get(field)
        {
            let refusal = match state {
                _ if serialized => (state.to_string().chars().count() > limit).then(|| {
                    format!(
                        "Parameter {field} exceeds maximum serialized length ({limit} characters)"
                    )
                }),
                Value::String(text) => (text.chars().count() > limit).then(|| snippet_limit(limit)),
                _ => None,
            };
            if let Some(refusal) = refusal {
                return ValidationResult {
                    sanitized_params: Map::new(),
                    verdict: Verdict::invalid(refusal),
                };
            }
        }
        let mut verdict = self.walk_object(&mut object, 0, MAX_STRING_LENGTH, evidence, paths);
        // The evidence is judged as text of no known file, in place. Text
        // only some other file's rules would hide (a manifest quoted in a
        // question) is no credential the caller is known to have written:
        // it is hidden in place and refuses nothing.
        if let Some(evidence) = evidence {
            verdict.has_secrets |= self.hide_provider_text(&mut object, evidence.field);
        }
        ValidationResult {
            sanitized_params: object,
            verdict,
        }
    }

    /// A state allowance applies at the declared root only; its already bounded
    /// subtree inherits it without granting it to unrelated nested `content` keys.
    fn walk_object(
        &self,
        object: &mut Map<String, Value>,
        depth: usize,
        limit: usize,
        evidence: Option<Evidence>,
        paths: bool,
    ) -> Verdict {
        let evidence_limit = |key: &str| {
            evidence
                .filter(|evidence| evidence.field == key)
                .map(|evidence| evidence.limit)
        };
        if depth > MAX_DEPTH {
            object.clear();
            return Verdict::invalid("Maximum nesting depth exceeded");
        }
        let mut out = Verdict::clean();
        object.retain(|key, value| {
            // Keys also become provider input and diagnostic field paths.
            // Reject credentials here; renaming keys would change the data.
            if holds_credential(key) {
                out.refused("A parameter key contains a credential; remove it before retrying");
                out.has_secrets = true;
                return false;
            }
            // Ordinary parameter limits count UTF-16 units. The complete state
            // was bounded in Unicode scalar values; allow both UTF-16 units of
            // an astral character without rejecting valid evidence.
            let limit = evidence_limit(key).map_or(limit, |bound| bound.saturating_mul(2));
            if key.trim().is_empty() {
                out.refused(format!("Invalid parameter key: {key}"));
                return false;
            }
            if matches!(key.as_str(), "__proto__" | "constructor" | "prototype") {
                out.refused(format!("Dangerous parameter key blocked: {key}"));
                return false;
            }
            match value {
                Value::String(text) => {
                    if text.encode_utf16().count() > limit {
                        out.refused(format!(
                            "Parameter {key} exceeds maximum length ({limit} characters)"
                        ));
                        return false;
                    }
                    // The evidence under judgment is no request text: it is
                    // judged as text of no known file is, in place
                    // ([`ContentSecurity::hide_provider_text`]).
                    let evidence = evidence_limit(key).is_some();
                    if !evidence
                        && !(paths && PATH_FIELDS.contains(&key.as_str()))
                        && holds_credential(text)
                    {
                        let result = self.sanitize_text(text, None);
                        out.has_secrets = true;
                        out.secret_fields.push(key.clone());
                        for secret in &result.secrets_detected {
                            out.warnings
                                .push(format!("Secrets detected in {key}: {secret}"));
                        }
                        *text = hidden(result.content);
                    }
                    true
                }
                Value::Array(values) => {
                    // The evidence under judgment is bounded whole, by its
                    // serialized length (checked before this walk): its
                    // items are data, however many, never a parameter list.
                    let evidence = evidence_limit(key).is_some();
                    self.walk_array(key, values, depth, limit, !evidence, paths, &mut out)
                }
                Value::Object(nested) => {
                    let result = self.walk_object(nested, depth + 1, limit, None, paths);
                    out.absorb(
                        result,
                        |field| format!("{key}.{field}"),
                        |warning| format!("Invalid nested object in parameter {key}: {warning}"),
                    );
                    true
                }
                _ => true,
            }
        });
        out.warnings.sort();
        out.warnings.dedup();
        out.secret_fields.sort();
        out.secret_fields.dedup();
        out
    }

    /// Sanitizes parameter `key`'s items in place; `false` (with a warning in
    /// `out`) when the array must go whole: too deep or too many items.
    fn walk_array(
        &self,
        key: &str,
        values: &mut Vec<Value>,
        depth: usize,
        limit: usize,
        counted: bool,
        paths: bool,
        out: &mut Verdict,
    ) -> bool {
        if depth > MAX_DEPTH {
            out.refused("Maximum nesting depth exceeded");
            return false;
        }
        if counted && values.len() > MAX_ARRAY_LENGTH {
            out.refused(format!(
                "Parameter {key} array exceeds maximum length (100 items)"
            ));
            return false;
        }
        let mut item_secrets = false;
        let judged = counted && !(paths && PATH_FIELDS.contains(&key));
        values.retain_mut(|item| match item {
            Value::String(text) if text.encode_utf16().count() > limit => {
                out.refused(format!(
                    "Parameter {key}[] exceeds maximum length ({limit} characters)"
                ));
                false
            }
            // An item of the evidence (`counted` false) is judged with it.
            Value::String(text) => {
                if judged && holds_credential(text) {
                    item_secrets = true;
                    *text = hidden(self.sanitize_text(text, None).content);
                }
                true
            }
            Value::Object(nested) => {
                let result = self.walk_object(nested, depth + 1, limit, None, paths);
                out.absorb(
                    result,
                    |field| format!("{key}[].{field}"),
                    |warning| format!("{key}[]: {warning}"),
                );
                true
            }
            // Nested arrays recurse, so string leaves at any array depth are
            // sanitized.
            Value::Array(inner) => {
                self.walk_array(key, inner, depth + 1, limit, counted, paths, out)
            }
            _ => true,
        });
        if item_secrets {
            out.has_secrets = true;
            out.secret_fields.push(format!("{key}[]"));
        }
        true
    }
}

/// Whether a request string holds a credential: whatever a read of text of
/// no known file would hide (the file judge itself, so nothing a read hides
/// leaves the machine in a request, and a declaration a read shows,
/// `password: str`, runs), and the body of a private key under its armor
/// line ([`super::content::holds_private_key_body`]), as written or as a
/// regex spells it. The armor line alone
/// (`-----BEGIN RSA PRIVATE KEY-----`) is the same text in every key and no
/// part of any: a search for it runs. A scan that fails counts as a
/// credential.
fn holds_credential(text: &str) -> bool {
    use octocode_engine::security::sanitizer::{request_holds_credential, request_literal_form};
    request_holds_credential(text).unwrap_or(true)
        || super::content::holds_private_key_body(text)
        || request_literal_form(text)
            .is_some_and(|plain| super::content::holds_private_key_body(&plain))
}

/// The text a string that holds a credential leaves in its place: `redacted`
/// (what the file judge made of it, the judge the request was refused by),
/// and the scan-failure placeholder when it rewrote nothing (a credential
/// only a regex spelling of the string holds), so such a value never stays
/// as written.
fn hidden(redacted: std::borrow::Cow<'_, str>) -> String {
    match redacted {
        std::borrow::Cow::Owned(content) => content,
        std::borrow::Cow::Borrowed(_) => {
            octocode_engine::security::SANITIZER_FAILURE_PLACEHOLDER.to_owned()
        }
    }
}

/// The refusal of an inline snippet longer than `limit` characters: it names
/// the limit and what reads longer source.
fn snippet_limit(limit: usize) -> String {
    format!(
        "Parameter {SNIPPET} is over its limit of {limit} characters: send a shorter snippet, or save it as a file and search that with path"
    )
}

/// A walk's refusals as field issues, so they isolate and render like any
/// other invalid field. Only a row's evidence (clasify `content`, astSearch
/// `code`) may be redacted in place. Any other rewritten value would silently change what a
/// tool reads or what a question asks, and leak the placeholder into
/// continuations.
fn input_issues(checked: &ValidationResult, evidence: Option<&str>) -> Vec<ValidationIssue> {
    let issue = |rule_id: &str, field: Option<&str>, message: String| ValidationIssue {
        rule_id: rule_id.into(),
        path: field.map(str::to_owned).into_iter().collect(),
        message,
        schema: None,
        received: None,
    };
    let mut issues = Vec::new();
    if !checked.is_valid {
        issues.extend(
            checked
                .warnings
                .iter()
                .filter(|warning| !warning.starts_with("Secrets detected"))
                .map(|warning| {
                    let hint = if warning.contains("exceeds maximum length") {
                        "; send a shorter, distinctive fragment"
                    } else {
                        ""
                    };
                    issue("security.input", None, format!("{warning}{hint}"))
                }),
        );
    }
    let redactable = |field: &str| {
        evidence.is_some_and(|evidence| {
            field == evidence
                || field.starts_with(&format!("{evidence}."))
                || field.starts_with(&format!("{evidence}["))
        })
    };
    issues.extend(
        checked
            .secret_fields
            .iter()
            .filter(|field| !redactable(field))
            .map(|field| {
                issue(
                    "security.credential",
                    Some(field),
                    format!(
                        "Value looks like a credential and was not run; {}",
                        credential_repair(field)
                    ),
                )
            }),
    );
    issues
}

/// The fields whose text is the caller's prose (a goal, a reasoning, a
/// task's instructions, background or criteria, a question): nothing searches
/// it, so its repair is the text without the credential.
const PROSE_FIELDS: [&str; 6] = [
    "mainGoal",
    "reasoning",
    "instructions",
    "background",
    "criteria",
    "questions",
];

/// The repair a credential refusal names, fitted to the field that held it:
/// a URL runs once its credentials are dropped, prose once the credential is
/// removed from it; a search term or any other text names what to send in
/// place of the secret.
fn credential_repair(field: &str) -> &'static str {
    let segments = || {
        field
            .split('.')
            .map(|segment| segment.split('[').next().unwrap_or(segment))
    };
    let leaf = segments().next_back().unwrap_or(field);
    if leaf.to_ascii_lowercase().ends_with("url") {
        "drop the credentials from the URL (a user:password@ part or a token in its query) and send it without them"
    } else if segments().any(|segment| PROSE_FIELDS.contains(&segment)) {
        "remove the credential from the text and send the rest"
    } else {
        "its redacted form would match unrelated text: search a non-secret fragment instead, such as its prefix or the identifier that holds it"
    }
}

#[cfg(test)]
mod credential_repair_tests;
#[cfg(test)]
mod saved_state_tests;
#[cfg(test)]
mod snippet_tests;
