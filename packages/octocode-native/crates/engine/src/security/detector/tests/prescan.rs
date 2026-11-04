//! The literal prescan gate and candidate selection.

use super::sampling::{Lcg, sample};
use super::*;

/// The prescan gate is necessary: every text a pattern matches selects that
/// pattern. Texts are generated from each pattern's own syntax tree (random
/// branches, class members, Unicode case variants and repetition counts), so
/// every gate `build.rs` derives, prefix or structural, is checked against
/// real matches, `(?i)` patterns included.
#[test]
fn every_text_a_pattern_matches_passes_its_prescan_gate() {
    let mut rng = Lcg(0x5eed);
    let mut checked = vec![0usize; PATTERNS.len()];
    for (idx, pattern) in PATTERNS.iter().enumerate() {
        let hir = regex_syntax::parse(pattern.regex).expect(pattern.name);
        for _ in 0..200 {
            let mut text = String::from("\n");
            sample(&hir, &mut rng, &mut text);
            text.push('\n');
            if !pattern_regex(idx).is_match(&text) {
                continue;
            }
            checked[idx] += 1;
            assert!(
                candidate_pattern_indices(&text).contains(&idx),
                "{} matches {text:?} but its prescan gate rejects it",
                pattern.name
            );
        }
    }
    let unchecked: Vec<&str> = PATTERNS
        .iter()
        .zip(&checked)
        .filter(|(_, count)| **count == 0)
        .map(|(pattern, _)| pattern.name)
        .collect();
    assert!(
        unchecked.len() * 10 < PATTERNS.len(),
        "too few patterns produced a matching sample: {unchecked:?}"
    );
}

/// A Unicode case variant of a `(?i)` key (`ſ`, U+017F, folds to `s`) is
/// the same key to the regex, so the gate must not skip it.
#[test]
fn unicode_case_variants_of_case_insensitive_keys_are_redacted() {
    let secret = "abcdefghijklmnopqrstuvwxyz0123456789ABCDEF";
    for key in ["elevenlabſ_api_key", "ELEVENLABS_API_\u{212A}EY"] {
        let text = format!("\"{key}\": \"{secret}\"");
        let result = detect(&text, None);
        assert!(
            !result.sanitized.contains(secret),
            "{key}: {}",
            result.sanitized
        );
    }
}

/// Differential guardrail: the prescan-gated matcher must agree exactly
/// with the complete reference set across gated patterns, fallback-bucket
/// patterns, case-folded keywords, and clean content.
#[test]
fn prescan_agrees_with_reference_regex_set_on_corpus() {
    let reference_set = regex::RegexSetBuilder::new(PATTERNS.iter().map(|p| p.regex))
        .size_limit(256 * 1024 * 1024)
        .dfa_size_limit(256 * 1024 * 1024)
        .build()
        .expect("All patterns must be valid Rust regex");
    let jwt = format!(
        "eyJ{}.eyJ{}.{}",
        "a".repeat(20),
        "b".repeat(20),
        "c".repeat(20)
    );
    let corpus: Vec<String> = vec![
        format!("token={FAKE_GH_TOKEN}"),
        format!("AWS_ACCESS_KEY_ID={FAKE_AWS_KEY}"),
        format!("aws_secret_access_key = {}", "A".repeat(40)),
        jwt,
        "postgres://user:s3cr3t@db.example.com:5432/app".to_string(),
        format!(
            "-----BEGIN RSA PRIVATE KEY-----\nMII{}\n-----END RSA PRIVATE KEY-----",
            "A".repeat(64)
        ),
        format!("uuid-ish {}", "123e4567-e89b-12d3-a456-426614174000"),
        format!(
            "telegram 123456789:{}",
            "AAExampleExampleExampleExampleAAAAA"
        ),
        "perfectly clean prose with no secrets at all".to_string(),
        format!("sk-proj-{}", "x".repeat(48)),
        format!("xoxb-1234567890-1234567890123-{}", "x".repeat(24)),
        format!("SLACK_TOKEN = \"xoxp-{}\"", "1".repeat(30)),
    ];
    for content in &corpus {
        let reference: Vec<usize> = reference_set.matches(content).into_iter().collect();
        let mut prescanned = candidate_pattern_indices(content);
        prescanned.retain(|&idx| pattern_regex(idx).is_match(content));
        assert_eq!(
            reference, prescanned,
            "prescan diverged from reference RegexSet on: {content:?}"
        );
    }
}

#[test]
fn detect_redacts_aws_access_key_id() {
    let input = format!("AWS_ACCESS_KEY_ID={FAKE_AWS_KEY}");
    let result = detect(&input, None);
    assert!(
        result.sanitized.contains("[REDACTED-AWSACCESSKEYID]"),
        "expected redaction, got: {}",
        result.sanitized
    );
    assert!(
        result
            .secrets_detected
            .contains(&"awsAccessKeyId".to_string())
    );
}

/// A detector its file context rules out is never compiled for that text:
/// the Kubernetes Secret automaton (tens of milliseconds to build) and other
/// context-gated detectors cost nothing on a call whose output never reaches
/// them, while the same text in their context still selects them.
#[test]
fn context_gated_detectors_are_selected_only_where_they_apply() {
    let index = |name: &str| {
        PATTERNS
            .iter()
            .position(|pattern| pattern.name == name)
            .expect(name)
    };
    let (kubernetes, wandb) = (index("kubernetesSecrets"), index("wandbApiKey"));
    let message = "No tool is named nope; use one of localSearch, localFetch.";
    let candidates = applicable_candidates(message, Scope::Pathless);
    assert!(!candidates.contains(&kubernetes) && !candidates.contains(&wandb));
    let manifest = "kind: Secret\ndata:\n  password: c2VjcmV0\n";
    assert!(applicable_candidates(manifest, Scope::File("k8s/secret.yaml")).contains(&kubernetes));
    assert!(!applicable_candidates(manifest, Scope::File("k8s/secret.txt")).contains(&kubernetes));
    let detected = detect(manifest, Some("k8s/secret.yaml"));
    assert_eq!(detected.secrets_detected, ["kubernetesSecrets"]);
}

/// A leaf about a file applies what that file's path gates; text about to be
/// kept applies it whatever the path. Only a bare shape still needs its context
/// named, or every commit SHA would read as a key.
#[test]
fn file_context_follows_what_the_text_is_about() {
    let index = |name: &str| {
        PATTERNS
            .iter()
            .position(|pattern| pattern.name == name)
            .expect(name)
    };
    let (kubernetes, wandb) = (index("kubernetesSecrets"), index("wandbApiKey"));
    let manifest = "kind: Secret\ndata:\n  password: c2VjcmV0\n";
    let sha = "9b5e0d8c2f7a41e6b3c58d9f0a1e2b7c4d6f8a03";
    let applies = |text, scope, pattern| applicable_candidates(text, scope).contains(&pattern);
    assert!(applies(manifest, Scope::Leaf("k8s/a.yaml"), kubernetes));
    assert!(!applies(manifest, Scope::Leaf("k8s/a.txt"), kubernetes));
    assert!(!applies(manifest, Scope::Pathless, kubernetes));
    for path in [None, Some("k8s/a.txt"), Some("k8s/a.yaml")] {
        assert!(applies(manifest, Scope::Stored(path), kubernetes));
    }
    assert!(!applies(sha, Scope::Leaf("wandb/a.py"), wandb));
    assert!(applies(sha, Scope::File("wandb/a.py"), wandb));
    assert!(!applies(sha, Scope::Stored(None), wandb));
    assert!(!applies(sha, Scope::Stored(Some("src/a.py")), wandb));
    assert!(applies(sha, Scope::Stored(Some("wandb/a.py")), wandb));
    assert!(applies(&format!("wandb {sha}"), Scope::Stored(None), wandb));
}

/// Only a path a multi-line file-context pattern applies to needs its whole
/// file judged, and only when a line holds what that pattern requires.
#[test]
fn a_whole_source_scan_is_asked_only_where_a_window_cannot_decide() {
    use super::WholeSourcePatterns;
    assert!(WholeSourcePatterns::for_path("src/a.rs").is_none());
    assert!(WholeSourcePatterns::for_path("docker-compose.txt").is_none());
    let patterns = WholeSourcePatterns::for_path("k8s/a.yaml").expect("applies");
    assert!(patterns.named_by("kind: Secret"));
    assert!(patterns.named_by("data:"));
    assert!(!patterns.named_by("  name: x"));
    assert!(!patterns.named_by("ordinary line"));
}
