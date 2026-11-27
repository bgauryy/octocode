//! The output gate: the one place tool output is sanitized, and the only place
//! a [`Sanitized`] envelope is made. The response stage, `ToolOutcome` and
//! `ToolError` hold nothing else, so a result that skipped this gate cannot
//! leave the runtime.
//!
//! Sanitizing replaces secret text and nothing else: no evidence is dropped,
//! and every key and string leaf of the envelope is scanned, error rows and
//! continuation leads included, except what the runtime authored itself: the
//! state a continuation carries, which is never judged, and the envelope's
//! messages (warnings, hints, notices), never judged as the row's file text:
//! only by the pathless net and for the values the gate hid elsewhere.

use std::ops::Deref;

use serde::{Deserialize, Deserializer};
use serde_json::Value;

use super::{Admitted, ContentSecurity, JudgedLeaves, sanitize_json};

/// A JSON envelope whose keys and string leaves passed the output gate.
///
/// Only this module builds one ([`ContentSecurity::seal_output`],
/// [`Sanitized::scrubbed`], or a reload that seals again). A stage that
/// reshapes the envelope after the gate goes through [`Sanitized::edit`]: it
/// may move, window or annotate what is there, and adds only text the runtime
/// authors or derives from checked input.
///
/// An envelope that holds text of a credential-named source is `held`: that
/// text may carry a secret in no format the gate recognizes, so every copy
/// of it (a saved response, each page and part) stays in this process and
/// never reaches a file, the rule saved provider answers follow.
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
#[serde(transparent)]
pub struct Sanitized(
    Value,
    #[serde(skip)] bool,
    /// Tests: the judged leaves the gate honored, by text and judged path,
    /// and the request text the gate sealed this envelope for, so a later
    /// edit or cut is checked against the same gate.
    #[cfg(test)]
    #[serde(skip)]
    std::sync::Arc<(Vec<(String, String)>, Admitted)>,
);

impl Sanitized {
    /// The envelope, leaving the proof (for the wire).
    #[must_use]
    pub fn into_value(self) -> Value {
        self.0
    }

    /// Whether this envelope holds text of a credential-named source, so
    /// no copy of it may be stored on disk.
    #[must_use]
    pub fn held(&self) -> bool {
        self.1
    }

    /// An error envelope with every string leaf scrubbed, for a failure that
    /// holds no tool rows (a rejected call, a host-boundary error). A secret
    /// echoed by a remote server or embedded in a provider payload is
    /// redacted; a sanitizer failure fails closed to a redaction placeholder.
    #[must_use]
    pub fn scrubbed(mut payload: Value) -> Self {
        let _ = sanitize_json(&mut payload, &mut |text: &str, _: Option<&str>| {
            Ok::<_, std::convert::Infallible>(super::scrub_error_text(text))
        });
        Self::of(payload, false, Vec::new(), &Admitted::default())
    }

    #[cfg(test)]
    fn of(value: Value, held: bool, judged: Vec<(String, String)>, own: &Admitted) -> Self {
        Self(value, held, std::sync::Arc::new((judged, own.clone())))
    }

    #[cfg(not(test))]
    fn of(value: Value, held: bool, _: Vec<(String, String)>, _: &Admitted) -> Self {
        Self(value, held)
    }

    /// Reshape the envelope in place after the gate. In tests every edit is
    /// checked: the result must still be a fixed point of the gate, so a
    /// stage that introduced a secret fails the suite that exercises it.
    pub fn edit<R>(&mut self, reshape: impl FnOnce(&mut Value) -> R) -> R {
        let result = reshape(&mut self.0);
        #[cfg(test)]
        assert_gate_fixed_point(&self.0, &self.2.0, &self.2.1);
        result
    }

    /// A value cut from this envelope (a response page: some of its rows, or
    /// a window of its JSON, with the runtime's page block). It holds only
    /// what the gate already passed, so it stays sealed; tests check it is
    /// still a fixed point of the gate.
    pub fn cut<E>(&self, cut: impl FnOnce(&Value) -> Result<Value, E>) -> Result<Self, E> {
        let page = cut(&self.0)?;
        #[cfg(test)]
        assert_gate_fixed_point(&page, &self.2.0, &self.2.1);
        #[cfg(test)]
        return Ok(Self(page, self.1, self.2.clone()));
        #[cfg(not(test))]
        Ok(Self(page, self.1))
    }
}

impl Deref for Sanitized {
    type Target = Value;

    fn deref(&self) -> &Value {
        &self.0
    }
}

impl std::fmt::Display for Sanitized {
    /// The envelope's compact JSON, as every surface prints it.
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(formatter)
    }
}

impl PartialEq<Value> for Sanitized {
    fn eq(&self, other: &Value) -> bool {
        self.0 == *other
    }
}

/// A saved envelope read back from disk is input again: it passes the gate on
/// the way in, so a tampered store cannot return what the gate would redact.
/// A held envelope is never written, so one read back holds no credential
/// source text unless the gate finds it again.
impl<'de> Deserialize<'de> for Sanitized {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = Value::deserialize(deserializer)?;
        Ok(ContentSecurity::new()
            .seal_output(value, false, || Ok::<_, std::convert::Infallible>(()))
            .unwrap_or_else(|never| match never {}))
    }
}

impl ContentSecurity {
    /// The one output gate. Every string leaf and key of `value` is
    /// sanitized (a leaf of a result that names a source-code `path` is
    /// scanned as that language; every credential pattern applies), then,
    /// when `emails` is set, email addresses are masked (`output.redactEmails`
    /// on a GitHub tool). `check` runs before each pass so a cancelled call
    /// stops between them. A leaf whose source `path` is credential-named
    /// ([`crate::policy::discovery::is_sensitive_repository_file`]) marks the
    /// envelope [`Sanitized::held`]. Continuation state is not judged, and
    /// the envelope's own messages not as file text
    /// ([`super::walk::Messages::Authored`]).
    pub fn seal_output<E>(
        &self,
        value: Value,
        emails: bool,
        check: impl Fn() -> Result<(), E>,
    ) -> Result<Sanitized, E> {
        self.seal_judged(
            value,
            &JudgedLeaves::default(),
            &Admitted::default(),
            super::Pii {
                emails,
                uuids: false,
            },
            check,
        )
    }

    /// [`Self::seal_output`] of a value some of whose leaves a tool judged
    /// against their whole source (`judged`, held beside the value): a
    /// marked leaf whose text still has the marked digest passes only the
    /// patterns that whole pass did not apply
    /// ([`Self::sanitize_judged_leaf`]); every other leaf the full gate.
    /// `own` is the text of the rows the value answers: where a leaf repeats
    /// a string of it, the gate leaves those bytes as the caller sent them
    /// ([`Self::sanitize_response_leaf`]). A marked leaf is text of a file
    /// and is never read against `own`. The personal data `pii` names is
    /// masked last ([`Self::redact_pii`]).
    pub fn seal_judged<E>(
        &self,
        mut value: Value,
        judged: &JudgedLeaves,
        own: &Admitted,
        pii: super::Pii,
        check: impl Fn() -> Result<(), E>,
    ) -> Result<Sanitized, E> {
        check()?;
        #[cfg(test)]
        let honored: Vec<(String, String)> = judged
            .verified(&value)
            .map(|(text, source)| (text.to_owned(), source.to_owned()))
            .collect();
        #[cfg(not(test))]
        let honored = Vec::new();
        let mut held = false;
        let messages = super::walk::Messages::Authored;
        super::walk::sanitize_judged_json(
            &mut value,
            judged,
            own,
            messages,
            &mut |text, path, source| {
                held |= path.is_some_and(crate::policy::discovery::is_sensitive_repository_file);
                Ok(match source {
                    Some(_) => self.sanitize_judged_leaf(text),
                    None => self.sanitize_response_leaf(text, own),
                })
            },
        )?;
        if pii.any() {
            check()?;
            sanitize_json(&mut value, &mut |text, _| Ok(self.redact_pii(text, pii)))?;
        }
        Ok(Sanitized::of(value, held, honored, own))
    }
}

#[cfg(any(test, feature = "test-support"))]
impl Sanitized {
    /// Tests: `value` through the output gate, as a tool result is.
    pub fn sealed(value: Value) -> Self {
        ContentSecurity::new()
            .seal_output(value, false, || Ok::<_, std::convert::Infallible>(()))
            .unwrap_or_else(|never| match never {})
    }
}

/// Tests: `value` is what the gate leaves it. A leaf that is a piece of a
/// leaf the gate honored as judged (a page cut from it) is judged as that
/// leaf was.
#[cfg(test)]
fn assert_gate_fixed_point(value: &Value, judged: &[(String, String)], own: &Admitted) {
    let mut again = value.clone();
    let security = ContentSecurity::new();
    let _ = super::walk::sanitize_judged_json(
        &mut again,
        &JudgedLeaves::default(),
        own,
        super::walk::Messages::Authored,
        &mut |text, _, _| {
            let source = judged
                .iter()
                .find(|(whole, _)| !text.is_empty() && whole.contains(text));
            Ok::<_, std::convert::Infallible>(match source {
                Some(_) => security.sanitize_judged_leaf(text),
                None => security.sanitize_response_leaf(text, own),
            })
        },
    );
    assert!(
        again == *value,
        "a stage added text the output gate would redact:\n{value}\n{again}"
    );
}

#[cfg(test)]
mod tests;
