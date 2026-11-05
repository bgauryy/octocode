//! A streamed window of a source too large to scan whole. A walk of the source hides
//! what its structure says is secret ([`super::masks`]). Beyond that, a value of the
//! shape a JSON Web Key's private member, or at a path a whole-file pattern applies
//! to a Kubernetes Secret's value, holds is hidden whatever the source says: a
//! window alone shows no object.
use super::masks::with_placeholders;
use crate::security::scan::ContentScan;
use octocode_contract_types::ErrorCode;
use regex::Regex;
use std::borrow::Cow;
use std::path::Path;
use std::sync::LazyLock;

pub(super) struct Unjudged<'a, S> {
    pub(super) scan: &'a S,
    /// A whole-file pattern applies at this source's path.
    pub(super) path_gated: bool,
}

/// The scan of a streamed window, which may hold masks of secret bytes
/// ([`super::masks`]). A page's scan turns them into placeholders and judges the
/// page with its margins. Nothing is judged before a page is cut from the window: a
/// byte page seeks by the source's offsets, so the view must keep them.
pub(super) struct Masked<'a, S> {
    pub(super) scan: &'a S,
    pub(super) masked: bool,
}

impl<S: ContentScan> ContentScan for Masked<'_, S> {
    fn sanitize(
        &self,
        text: &str,
        path: &Path,
    ) -> Result<(String, Vec<String>), (ErrorCode, String)> {
        if self.masked {
            return self.scan.sanitize(&with_placeholders(text), path);
        }
        self.scan.sanitize(text, path)
    }

    fn redact_key_blocks<'a>(&self, content: &'a str) -> (Cow<'a, str>, bool) {
        self.scan.redact_key_blocks(content)
    }

    fn redact_source_lines<'a>(
        &self,
        source: &'a str,
        _: &Path,
    ) -> Result<(Cow<'a, str>, bool), (ErrorCode, String)> {
        Ok((source.into(), false))
    }
}

impl<S: ContentScan> ContentScan for Unjudged<'_, S> {
    fn sanitize(
        &self,
        text: &str,
        path: &Path,
    ) -> Result<(String, Vec<String>), (ErrorCode, String)> {
        let (clean, warnings) = self.scan.sanitize(text, path)?;
        let clean = octocode_engine::security::hide_jwk_members(&clean).unwrap_or(clean);
        let clean = if self.path_gated {
            hide_values(&clean)
        } else {
            clean
        };
        Ok((clean, warnings))
    }
}

/// `text` with the value of each `  name: <base64>` line replaced.
fn hide_values(text: &str) -> String {
    #[expect(clippy::expect_used, reason = "static pattern, valid by construction")]
    static VALUE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r#"(?m)^([ \t]+[^\s:#]+:[ \t]*["']?)[A-Za-z0-9+/]{16,}={0,3}(["']?[ \t]*\r?)$"#)
            .expect("static pattern compiles")
    });
    let placeholder = octocode_engine::security::redaction_placeholder("kubernetesSecrets");
    VALUE
        .replace_all(text, format!("${{1}}{placeholder}${{2}}"))
        .into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A window of a large source may hold a private member with its `kty` out
    /// of reach: hidden at any path, the public members and other values kept.
    #[test]
    fn a_window_hides_a_private_member_at_any_path() {
        let scalar = "Fake0Scalar9Not4Real8Value7Long6Enough5Xy";
        let public = "Public0Coordinate9Stays8Readable7Always6Ab";
        let scan = crate::security::ContentSecurity::new();
        let unjudged = Unjudged {
            scan: &scan,
            path_gated: false,
        };
        let text = format!(
            "  \"x\": \"{public}\",\n  \"d\": \"{scalar}\",\n  \"sha\": \"{public}\"\n  d: localScalarName\n"
        );
        let (clean, _) = unjudged
            .sanitize(&text, Path::new("/w/big.json"))
            .expect("scanned");
        assert!(!clean.contains(scalar), "{clean}");
        assert_eq!(clean.matches(public).count(), 2, "{clean}");
        assert!(clean.contains("localScalarName"), "{clean}");
    }

    #[test]
    fn only_a_value_a_secret_could_hold_is_hidden() {
        let text = "data:\n  tls.crt: QUJDREVGR0hJSktMTU5PUFFSU1RVVldYWVo=\n  note: \"QUJDREVGR0hJSktMTU5P\"\r\n  name: short\n  title: a sentence of plain words here\n  image: repo/app:1.2.3\nQUJDREVGR0hJSktMTU5PUFFSU1RVVldYWVo=\n";
        let hidden = hide_values(text);
        assert_eq!(
            hidden,
            "data:\n  tls.crt: [REDACTED-KUBERNETESSECRETS]\n  note: \"[REDACTED-KUBERNETESSECRETS]\"\r\n  name: short\n  title: a sentence of plain words here\n  image: repo/app:1.2.3\nQUJDREVGR0hJSktMTU5PUFFSU1RVVldYWVo=\n"
        );
    }
}
