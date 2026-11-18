use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};

/// A scanner refusal reaches the provider error as the scanner's own
/// message (the reason the text was withheld), not its code.
#[test]
fn provider_text_refusals_carry_the_scanner_message() {
    struct Refusing;
    impl ContentScan for Refusing {
        fn sanitize(&self, _: &str, _: &Path) -> ScanOutcome {
            Err((
                ErrorCode::ContentSecurityLimit,
                "The selected content view exceeds the secret scanner size limit.".into(),
            ))
        }
    }
    let error = sanitize_provider_text(&Refusing, "text", Path::new("github-history"))
        .expect_err("refused");
    assert_eq!(error.kind, octocode_github::ProviderErrorKind::Validation);
    assert_eq!(
        &*error.message,
        "The selected content view exceeds the secret scanner size limit."
    );
}

/// Redacts `SECRET` (a multi-line hit when it spans lines) and counts
/// its scans.
struct Counting(AtomicUsize);

impl ContentScan for Counting {
    fn sanitize(&self, text: &str, _: &Path) -> ScanOutcome {
        self.0.fetch_add(1, Ordering::Relaxed);
        Ok((text.replace("SECRET", "[R]"), Vec::new()))
    }
}

fn source() -> String {
    let mut text = "plain line\n".repeat(2_000);
    text.push_str("key = SECRET\n");
    text
}

/// Repeated reads and changed sources independently pass the same scanner.
#[test]
fn whole_source_line_redaction_scans_each_read() {
    let scanner = Counting(AtomicUsize::new(0));
    let scan = &scanner;
    let source = source();
    let path = Path::new("/w/a.env.example");
    let first = scan.redact_source_lines(&source, path).expect("redacted");
    assert!(first.1 && first.0.ends_with("key = [R]\n"));
    assert_eq!(scanner.0.load(Ordering::Relaxed), 1);
    assert_eq!(scan.redact_source_lines(&source, path), Ok(first));
    assert_eq!(
        scanner.0.load(Ordering::Relaxed),
        2,
        "each read independently applies the scanner"
    );
    scan.redact_source_lines(&source, path)
        .expect("other bytes");
    scan.redact_source_lines(&source, Path::new("/w/b"))
        .expect("other path");
    assert_eq!(scanner.0.load(Ordering::Relaxed), 4);
}

/// Concurrent reads own their scan work and retain identical redaction.
#[test]
fn concurrent_line_redactions_are_independent() {
    struct Slow(AtomicUsize);
    impl ContentScan for Slow {
        fn sanitize(&self, text: &str, _: &Path) -> ScanOutcome {
            self.0.fetch_add(1, Ordering::SeqCst);
            std::thread::sleep(std::time::Duration::from_millis(50));
            Ok((text.replace("SECRET", "[R]"), Vec::new()))
        }
    }
    let scanner = Slow(AtomicUsize::new(0));
    let scan = &scanner;
    let source = source();
    let gate = std::sync::Barrier::new(8);
    let outcomes = std::thread::scope(|scope| {
        let rows = (0..8)
            .map(|_| {
                scope.spawn(|| {
                    gate.wait();
                    scan.redact_source_lines(&source, Path::new("/w/a.env"))
                })
            })
            .collect::<Vec<_>>();
        rows.into_iter()
            .map(|row| row.join().expect("row"))
            .collect::<Vec<_>>()
    });
    assert_eq!(scanner.0.load(Ordering::SeqCst), outcomes.len());
    assert!(outcomes.iter().all(|outcome| outcome == &outcomes[0]));
    assert!(
        outcomes[0]
            .as_ref()
            .is_ok_and(|(text, changed)| { *changed && text.ends_with("key = [R]\n") })
    );
}

#[test]
fn repeated_reads_do_not_retain_scanner_results() {
    let scanner = Counting(AtomicUsize::new(0));
    let scan = &scanner;
    let text = source();
    let path = Path::new("/w/repeated.env");
    let first = scan.redact_source_lines(&text, path).unwrap();
    let second = scan.redact_source_lines(&text, path).unwrap();
    assert_eq!(first, second);
    assert_eq!(scanner.0.load(Ordering::Relaxed), 2);
}

/// Text past the scanner's limit, as a read of a source that grew when
/// decoded hands it over: `line`, then pad words past the limit.
fn over_the_limit(line: &str) -> String {
    let pad = "ordinary words\n";
    let lines = octocode_engine::security::MAX_CONTENT_SIZE / pad.len() + 1;
    format!("{line}\n{}", pad.repeat(lines))
}

/// Past what a read holds at once, a source is judged as any other: a known
/// secret is hidden, and the text around it stays.
#[test]
fn past_the_limit_a_source_is_judged_whole() {
    let security = crate::security::ContentSecurity::new();
    let path = Path::new("/w/dump.json");
    let token = format!("ghp_{}", "c2VjcmV0".repeat(5));
    let text = over_the_limit(&format!("{{\"token\":\"{token}\"}}"));
    let (clean, changed) = security.redact_source_lines(&text, path).expect("read");
    assert!(changed);
    assert!(!clean.contains(&token));
    assert!(clean.contains("ordinary words"));
    for plain in [
        "{\"SecretName\":\"a\"}",
        "\"kind\": \"Deployment\"",
        "plain",
    ] {
        let text = over_the_limit(plain);
        let (clean, changed) = security.redact_source_lines(&text, path).expect("read");
        assert!(!changed, "{plain}");
        assert_eq!(clean.len(), text.len(), "{plain}");
    }
}

/// A whole scan that changes the line count (a rewrite that joined the
/// lines of a continued value) is never answered by judging lines one by
/// one: that serves the value's tail unredacted. Every line is withheld,
/// lines and their breaks keep their numbers, and the read says it
/// redacted.
#[test]
fn a_line_count_change_withholds_every_line() {
    struct Joining;
    impl ContentScan for Joining {
        fn sanitize(&self, text: &str, _: &Path) -> ScanOutcome {
            Ok((text.replace("a\\\nb", "[R]"), Vec::new()))
        }
    }
    let source = "a\\\nb\nsecondline\r\nlast";
    let (text, changed) = redact_lines(&Joining, source, Path::new("x")).expect("ok");
    let marker = octocode_engine::security::SANITIZER_FAILURE_PLACEHOLDER;
    assert!(changed);
    assert_eq!(text, format!("{marker}\n{marker}\n{marker}\r\n{marker}"));
    assert!(!text.contains("secondline"));
    assert_eq!(text.matches('\n').count(), source.matches('\n').count());
}

/// A scanner that refuses the whole source refuses the read; no line is
/// scanned on its own.
#[test]
fn a_whole_source_refusal_is_the_reads_refusal() {
    struct Refusing;
    impl ContentScan for Refusing {
        fn sanitize(&self, _: &str, _: &Path) -> ScanOutcome {
            Err((ErrorCode::ContentSecurityLimit, "too large".into()))
        }
    }
    let refused = redact_lines(&Refusing, "a\nb\n", Path::new("x")).expect_err("refused");
    assert_eq!(refused.0, ErrorCode::ContentSecurityLimit);
}
