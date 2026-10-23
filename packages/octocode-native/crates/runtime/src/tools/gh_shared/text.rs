//! GitHub file bytes as text by the local tools' one source admission
//! (`octocode_engine::text::admit`): the provider returns a file's bytes,
//! and this rule, not the provider, decides whether they are text.
use octocode_engine::text::{Decoding, admit};
use octocode_github::{ContentResponse, ProviderError, ProviderErrorKind, ProviderErrorReason};
use std::borrow::Cow;

/// The text of a file's bytes (UTF-8, BOM-marked UTF-16, else Latin-1 or
/// lossy UTF-8, as localFetch shows them), or the typed binary-file failure
/// every GitHub read states for bytes the rule refuses.
pub(crate) fn file_text(bytes: &[u8]) -> Result<Cow<'_, str>, ProviderError> {
    admit(bytes, Decoding::Lenient)
        .map(|decoded| decoded.text)
        .map_err(|_| binary_file(bytes.len(), None))
}

/// A file read the rule admits as text; refused, the typed binary-file
/// failure names the read's blob SHA when the provider learned it.
pub(crate) fn admit_read(read: &ContentResponse) -> Result<(), ProviderError> {
    match admit(&read.bytes, Decoding::Lenient) {
        Ok(_) => Ok(()),
        Err(_) => Err(binary_file(read.bytes.len(), read.blob.as_deref())),
    }
}

/// `Binary file (N bytes[, blob SHA])`: a request a text read cannot serve,
/// not a decode fault.
fn binary_file(len: usize, blob: Option<&str>) -> ProviderError {
    let blob = blob.map(|sha| format!(", blob {sha}")).unwrap_or_default();
    ProviderError::new(
        ProviderErrorKind::Validation,
        format!("Binary file ({len} bytes{blob})"),
    )
    .with_reason(ProviderErrorReason::BinaryFile)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn github_file_bytes_read_as_local_files_do() {
        let text = |bytes: &[u8]| file_text(bytes).ok().map(Cow::into_owned);
        assert_eq!(text(b"fn main() {}\n").as_deref(), Some("fn main() {}\n"));
        // A UTF-8 BOM is no text of line 1.
        assert_eq!(text(b"\xef\xbb\xbfa\n").as_deref(), Some("a\n"));
        // BOM-marked UTF-16 is text, decoded.
        let utf16: Vec<u8> = [0xff, 0xfe]
            .into_iter()
            .chain("hi\n".encode_utf16().flat_map(u16::to_le_bytes))
            .collect();
        assert_eq!(text(&utf16).as_deref(), Some("hi\n"));
        // Bytes that are not UTF-8 decode, they are not dropped.
        assert_eq!(text(b"caf\xe9\n").as_deref(), Some("caf\u{e9}\n"));
    }

    /// A NUL anywhere is binary: the one typed failure, refused once.
    #[test]
    fn binary_bytes_are_the_typed_binary_file_failure() {
        let mut late_nul = vec![b'a'; 20_000];
        late_nul.push(0);
        let error = file_text(&late_nul).expect_err("binary");
        assert_eq!(error.kind, ProviderErrorKind::Validation);
        assert_eq!(error.reason, Some(ProviderErrorReason::BinaryFile));
        assert_eq!(error.message.as_ref(), "Binary file (20001 bytes)");
    }

    /// Through the provider: the bytes come back as they are, and the one
    /// rule decides. A BOM-marked UTF-16 file reads as text (as localFetch
    /// reads it); a binary is refused once, by the rule, not the provider.
    #[tokio::test]
    async fn the_provider_returns_bytes_and_the_rule_admits_them() {
        use crate::tools::gh_shared::test_support::{default_context, one_shot_provider};
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        const SHA: &str = "0123456789abcdef0123456789abcdef01234567";
        let server = MockServer::builder().start().await;
        let utf16: Vec<u8> = [0xff, 0xfe]
            .into_iter()
            .chain("needle\n".encode_utf16().flat_map(u16::to_le_bytes))
            .collect();
        for (name, body) in [("wide.txt", utf16), ("blob.bin", b"\x7fELF\0\0".to_vec())] {
            Mock::given(method("GET"))
                .and(path(format!("/api/v3/repos/o/r/contents/{name}")))
                .respond_with(
                    ResponseTemplate::new(200)
                        .insert_header("content-type", "application/vnd.github.raw")
                        .set_body_bytes(body),
                )
                .expect(1)
                .mount(&server)
                .await;
        }
        let provider = one_shot_provider(&server);
        let read = |name: &str| octocode_github::ContentRequest {
            owner: "o".into(),
            repo: "r".into(),
            path: name.into(),
            reference: Some(SHA.into()),
            force_refresh: false,
        };
        let wide = provider
            .get_file_bytes(&read("wide.txt"), &default_context())
            .await
            .expect("bytes");
        assert_eq!(file_text(&wide.bytes).expect("text"), "needle\n");
        let binary = provider
            .get_file_bytes(&read("blob.bin"), &default_context())
            .await
            .expect("the provider returns the bytes");
        let refused = file_text(&binary.bytes).expect_err("binary");
        assert_eq!(refused.reason, Some(ProviderErrorReason::BinaryFile));
        assert!(admit_read(&wide).is_ok());
        assert_eq!(
            admit_read(&binary).map_err(|error| error.reason),
            Err(Some(ProviderErrorReason::BinaryFile))
        );
        server.verify().await;
    }
}
