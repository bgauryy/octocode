//! A streamed window of a source no whole-file pattern can judge. Its lines may
//! be a Kubernetes Secret's values with the `kind` far outside the window, and
//! the source is too large to scan whole, so a value of the shape those hold is
//! hidden whatever the rest of the file turns out to say.
use crate::security::scan::ContentScan;
use octocode_contract_types::ErrorCode;
use regex::Regex;
use std::path::Path;
use std::sync::LazyLock;

pub(super) struct Unjudged<'a, S>(pub(super) &'a S);

impl<S: ContentScan> ContentScan for Unjudged<'_, S> {
    fn sanitize(
        &self,
        text: &str,
        path: &Path,
    ) -> Result<(String, Vec<String>), (ErrorCode, String)> {
        let (clean, warnings) = self.0.sanitize(text, path)?;
        Ok((hide_values(&clean), warnings))
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
