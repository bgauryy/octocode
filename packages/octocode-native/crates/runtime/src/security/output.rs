//! The output gate: the one place tool output is sanitized, and the only place
//! a [`Sanitized`] envelope is made. The response stage, `ToolOutcome` and
//! `ToolError` hold nothing else, so a result that skipped this gate cannot
//! leave the runtime.
//!
//! Sanitizing replaces secret text and nothing else: no evidence is dropped,
//! and every key and string leaf of the envelope is scanned, error rows and
//! continuation leads included.

use std::ops::Deref;

use serde::{Deserialize, Deserializer};
use serde_json::Value;

use super::{ContentSecurity, JudgedLeaves, sanitize_json};

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
    /// so a later edit or cut is checked against the same gate.
    #[cfg(test)]
    #[serde(skip)]
    Vec<(String, String)>,
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
    pub(crate) fn held(&self) -> bool {
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
        Self::of(payload, false, Vec::new())
    }

    #[cfg(test)]
    fn of(value: Value, held: bool, judged: Vec<(String, String)>) -> Self {
        Self(value, held, judged)
    }

    #[cfg(not(test))]
    fn of(value: Value, held: bool, _: Vec<(String, String)>) -> Self {
        Self(value, held)
    }

    #[cfg(test)]
    fn judged(&self) -> &[(String, String)] {
        &self.2
    }

    /// Reshape the envelope in place after the gate. In tests every edit is
    /// checked: the result must still be a fixed point of the gate, so a
    /// stage that introduced a secret fails the suite that exercises it.
    pub(crate) fn edit<R>(&mut self, reshape: impl FnOnce(&mut Value) -> R) -> R {
        let result = reshape(&mut self.0);
        #[cfg(test)]
        assert_gate_fixed_point(&self.0, self.judged());
        result
    }

    /// A value cut from this envelope (a response page: some of its rows, or
    /// a window of its JSON, with the runtime's page block). It holds only
    /// what the gate already passed, so it stays sealed; tests check it is
    /// still a fixed point of the gate.
    pub(crate) fn cut<E>(&self, cut: impl FnOnce(&Value) -> Result<Value, E>) -> Result<Self, E> {
        let page = cut(&self.0)?;
        #[cfg(test)]
        assert_gate_fixed_point(&page, self.judged());
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
    /// envelope [`Sanitized::held`].
    pub(crate) fn seal_output<E>(
        &self,
        value: Value,
        emails: bool,
        check: impl Fn() -> Result<(), E>,
    ) -> Result<Sanitized, E> {
        self.seal_judged(value, &JudgedLeaves::default(), emails, check)
    }

    /// [`Self::seal_output`] of a value some of whose leaves a tool judged
    /// against their whole source (`judged`, held beside the value): a
    /// marked leaf whose text still has the marked digest passes only the
    /// patterns that whole pass did not apply
    /// ([`Self::sanitize_judged_leaf`]); every other leaf the full gate.
    pub(crate) fn seal_judged<E>(
        &self,
        mut value: Value,
        judged: &JudgedLeaves,
        emails: bool,
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
        super::walk::sanitize_judged_json(&mut value, judged, &mut |text, path, source| {
            held |= path.is_some_and(crate::policy::discovery::is_sensitive_repository_file);
            Ok(match source {
                Some(source) => self.sanitize_judged_leaf(text, path, source),
                None => self.sanitize_leaf(text, path),
            })
        })?;
        if emails {
            check()?;
            sanitize_json(&mut value, &mut |text, _| Ok(self.redact_emails(text)))?;
        }
        Ok(Sanitized::of(value, held, honored))
    }
}

#[cfg(test)]
impl Sanitized {
    /// Tests: `value` through the output gate, as a tool result is.
    pub(crate) fn sealed(value: Value) -> Self {
        ContentSecurity::new()
            .seal_output(value, false, || Ok::<_, std::convert::Infallible>(()))
            .unwrap_or_else(|never| match never {})
    }
}

/// Tests: `value` is what the gate leaves it. A leaf that is a piece of a
/// leaf the gate honored as judged (a page cut from it) is judged as that
/// leaf was.
#[cfg(test)]
fn assert_gate_fixed_point(value: &Value, judged: &[(String, String)]) {
    let mut again = value.clone();
    let security = ContentSecurity::new();
    let _ = sanitize_json(&mut again, &mut |text, path| {
        let source = judged
            .iter()
            .find(|(whole, _)| !text.is_empty() && whole.contains(text));
        Ok::<_, std::convert::Infallible>(match source {
            Some((_, source)) => security.sanitize_judged_leaf(text, path, source),
            None => security.sanitize_leaf(text, path),
        })
    });
    assert!(
        again == *value,
        "a stage added text the output gate would redact:\n{value}\n{again}"
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// `value` through the output gate, emails masked when `emails`.
    fn seal(value: Value, emails: bool) -> Value {
        if !emails {
            return Sanitized::sealed(value).into_value();
        }
        ContentSecurity::new()
            .seal_output(value, emails, || Ok::<_, std::convert::Infallible>(()))
            .unwrap_or_else(|never| match never {})
            .into_value()
    }

    #[test]
    fn sanitizer_preserves_non_secret_next_query_and_location() {
        // A benign, non-secret-shaped string passes through unchanged — the
        // continuation stays executable and the location is untouched.
        let mut value = json!({
            "content": "token ghp_secretvalue12",
            "next": {
                "nextPage": {
                    "tool": "localFetch",
                    "query": {"queries":[{"path": "/repo/ghp_secretvalue12.rs", "offset": 2}]},
                    "confidence": "exact"
                }
            },
            "location": {"localPath": "/tmp/ghp_secretvalue12/repo"}
        });
        value = seal(value, false);
        assert_eq!(
            value.pointer("/next/nextPage/query/queries/0/path"),
            Some(&json!("/repo/ghp_secretvalue12.rs"))
        );
        assert_eq!(
            value.pointer("/location/localPath"),
            Some(&json!("/tmp/ghp_secretvalue12/repo"))
        );
        // The executable tool identifier survives verbatim.
        assert_eq!(
            value.pointer("/next/nextPage/query/queries/0/offset"),
            Some(&json!(2))
        );
    }

    #[test]
    fn email_redaction_is_opt_in_and_masks_commit_authors() {
        let commit_row = || {
            json!({
                "commits": [{
                    "sha": "abc123",
                    "author": {"name": "Dev One", "email": "dev.one+git@example.co.uk"},
                    "message": "fix: reported by user@example.com"
                }]
            })
        };
        // Default path (no opt-in): emails pass through unchanged.
        let mut untouched = commit_row();
        untouched = seal(untouched, false);
        assert_eq!(untouched, commit_row(), "default output must be unchanged");
        // Opt-in path: every email leaf is masked, structure preserved.
        let mut redacted = commit_row();
        redacted = seal(redacted, true);
        assert_eq!(
            redacted.pointer("/commits/0/author/email"),
            Some(&json!("[REDACTED-EMAIL]"))
        );
        assert_eq!(
            redacted.pointer("/commits/0/message"),
            Some(&json!("fix: reported by [REDACTED-EMAIL]"))
        );
        assert_eq!(
            redacted.pointer("/commits/0/author/name"),
            Some(&json!("Dev One"))
        );
    }

    /// File text a tool returns beside its source-code `path`
    /// is not re-redacted as YAML by the output net, while every credential in
    /// it, and `password:` in config files, stay redacted.
    #[test]
    fn output_net_applies_source_code_context_from_the_result_path() {
        let member = "interface Cfg {\n  password: hunter2hunter2hunter2;\n}\n";
        let token = format!("ghp_{}", "a".repeat(37));
        let pem =
            "-----BEGIN RSA PRIVATE KEY-----\nMIIEowIBAAKCAQEAxyz\n-----END RSA PRIVATE KEY-----";
        let run = |path: &str, content: &str| {
            let mut value = json!({"files":[{"path": path, "matches":[{"value": content}]}]});
            value = seal(value, false);
            value["files"][0]["matches"][0]["value"]
                .as_str()
                .expect("value")
                .to_owned()
        };
        let rust_member = "struct Cfg {\n    password: hunter2hunter2hunter2,\n}\n";
        for (path, member) in [("src/a.ts", member), ("crates/b.rs", rust_member)] {
            assert_eq!(run(path, member), member, "{path}");
            let credential = format!("const t = \"{token}\";\n{pem}\n");
            let out = run(path, &credential);
            assert!(!out.contains(&token) && !out.contains("MIIEow"), "{out}");
        }
        for path in ["config/app.yaml", ".env", "Procfile", ""] {
            let out = run(path, member);
            assert!(!out.contains("hunter2"), "{path}: {out}");
        }
        // No `path` anywhere: the pathless net applies as before.
        let mut bare = json!({"content": member});
        bare = seal(bare, false);
        assert!(
            !bare["content"]
                .as_str()
                .expect("content")
                .contains("hunter2")
        );
    }

    #[test]
    fn sanitizer_redacts_real_secret_in_next_query_and_location() {
        // A real GitHub token embedded in a continuation query or a location
        // leaf is now redacted — better to break the continuation than to leak.
        let token = format!("ghp_{}", "a".repeat(37));
        let mut value = json!({
            "next": {
                "nextPage": {
                    "tool": "localFetch",
                    "query": {"queries":[{"path": format!("/repo/{token}.rs")}]}
                }
            },
            "location": {"localPath": format!("/tmp/{token}/repo")}
        });
        value = seal(value, false);
        // Tool preserved so the call is still routable.
        assert_eq!(
            value.pointer("/next/nextPage/tool"),
            Some(&json!("localFetch"))
        );
        let query_path = value
            .pointer("/next/nextPage/query/queries/0/path")
            .and_then(Value::as_str)
            .expect("query path");
        assert!(!query_path.contains("ghp_"), "token leaked in query path");
        let local_path = value
            .pointer("/location/localPath")
            .and_then(Value::as_str)
            .expect("localPath");
        assert!(!local_path.contains("ghp_"), "token leaked in location");
    }
}
