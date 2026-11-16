//! The detector's invariants over the known-format list itself: every pattern
//! finds a sample of its own syntax, hides only its secret part, and leaves
//! every other byte as written.

mod sampling;

use super::prescan::{prescan_candidates, shipped_and_reference};
use super::regex_forms::{ascii_pattern_regex, pattern_regex};
use super::*;
use sampling::{Lcg, sample};

/// Samples tried per pattern.
const TRIES: usize = 200;

/// Text before and after a sample: ordinary code on lines of its own.
const BEFORE: &str = "const first = 1;\n";
const AFTER: &str = "\nconst last = 2;\n";

/// `text` with each of `spans` replaced by its placeholder and the line
/// breaks it spanned: what a redaction of exactly these spans writes.
fn replaced(text: &str, spans: &[SecretSpan]) -> String {
    let mut out = String::new();
    let mut from = 0;
    for span in spans {
        out.push_str(&text[from..span.range.start]);
        out.push_str(span.placeholder());
        out.push_str(&kept_line_breaks(&text[span.range.clone()]));
        from = span.range.end;
    }
    out.push_str(&text[from..]);
    out
}

/// The samples of pattern `idx`'s own syntax tree that its regex matches on
/// a line of their own, each between [`BEFORE`] and [`AFTER`].
fn matching_samples(idx: usize, rng: &mut Lcg) -> Vec<String> {
    let hir = regex_syntax::parse(PATTERNS[idx].regex).expect(PATTERNS[idx].name);
    (0..TRIES)
        .filter_map(|_| {
            let mut text = String::from(BEFORE);
            sample(&hir, rng, &mut text);
            text.push_str(AFTER);
            pattern_regex(idx).is_match(&text).then_some(text)
        })
        .collect()
}

/// Every pattern redacts a sample of itself, and only inside it: the text
/// before and after stays byte for byte, the spans are ascending and
/// disjoint, the output is exactly those spans replaced, and the pattern's
/// own secret lies in a span. A pattern no sample of which matches on its own
/// line never runs, and this names it.
#[test]
fn every_pattern_redacts_only_its_sample() {
    let mut rng = Lcg(0x5eed);
    let mut dead = Vec::new();
    for (idx, pattern) in PATTERNS.iter().enumerate() {
        let samples = matching_samples(idx, &mut rng);
        if samples.is_empty() {
            dead.push(pattern.name);
            continue;
        }
        for text in samples {
            let spans = secret_spans(&text);
            let inner = BEFORE.len()..text.len() - AFTER.len();
            assert!(
                spans
                    .windows(2)
                    .all(|pair| pair[0].range.end <= pair[1].range.start),
                "{}: {spans:?}",
                pattern.name
            );
            assert!(
                spans
                    .iter()
                    .all(|span| inner.start <= span.range.start && span.range.end <= inner.end),
                "{} redacts outside its sample in {text:?}: {spans:?}",
                pattern.name
            );
            let regex = pattern_regex(idx);
            let groups = secret_groups(regex);
            let own = regex
                .captures(&text)
                .and_then(|captures| secret_of(&captures, &groups));
            // A match whose secret group is empty (a key armor with no body)
            // or that went through its template branch hides nothing.
            let Some(own) = own.filter(|own| !own.is_empty()) else {
                continue;
            };
            assert!(
                spans
                    .iter()
                    .any(|span| span.range.start <= own.start() && own.end() <= span.range.end),
                "{}: its secret {:?} is left in {text:?}",
                pattern.name,
                own.as_str()
            );
            let result = redact(&text).expect("a secret");
            assert_eq!(
                result.sanitized,
                replaced(&text, &spans),
                "{}",
                pattern.name
            );
            assert!(result.sanitized.starts_with(BEFORE) && result.sanitized.ends_with(AFTER));
            assert_eq!(
                result.sanitized.lines().count(),
                text.lines().count(),
                "{}: a redaction keeps every line",
                pattern.name
            );
            assert!(
                redact(&result.sanitized).is_none(),
                "{}: a redacted text is redacted again: {:?}",
                pattern.name,
                result.sanitized
            );
        }
    }
    assert_eq!(dead, Vec::<&str>::new());
}

/// The prescan automaton is a DFA for speed; it must report exactly the
/// overlapping hits of the contiguous NFA on pattern samples, on the source
/// of every crate, and on mixed-case and non-ASCII text.
#[test]
fn dfa_prescan_matches_the_nfa() {
    let (dfa, nfa) = shipped_and_reference();
    let hits = |automaton: &aho_corasick::AhoCorasick, text: &str| {
        automaton
            .find_overlapping_iter(text)
            .map(|hit| (hit.pattern().as_usize(), hit.start(), hit.end()))
            .collect::<Vec<_>>()
    };
    let mut rng = Lcg(0xd1ff);
    let mut texts: Vec<String> = (0..PATTERNS.len())
        .flat_map(|idx| matching_samples(idx, &mut rng))
        .collect();
    texts.push("GHP_Ghp_ ſk_live_ AKIA\u{1F600}ghp_ \u{212A}ey".to_string());
    let crates = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut stack = vec![crates];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path
                .extension()
                .is_some_and(|ext| ext == "rs" || ext == "json")
                && let Ok(text) = std::fs::read_to_string(&path)
            {
                texts.push(text);
            }
        }
    }
    assert!(texts.len() > 500);
    for text in &texts {
        assert_eq!(hits(dfa, text), hits(&nfa, text));
    }
}

/// The prescan gate is necessary: every text a pattern matches selects that
/// pattern, in the full form and in the form a text of its bytes gets.
#[test]
fn every_text_a_pattern_matches_passes_its_prescan_gate() {
    let mut rng = Lcg(0x9a7e);
    for (idx, pattern) in PATTERNS.iter().enumerate() {
        for text in matching_samples(idx, &mut rng) {
            assert!(
                prescan_candidates(&text).iter().any(|found| found == idx),
                "{} matches {text:?} but its prescan gate rejects it",
                pattern.name
            );
        }
    }
}

/// On ASCII text the ASCII form of a pattern finds what its full form finds.
#[test]
fn both_forms_find_the_same_matches_on_ascii_text() {
    let mut rng = Lcg(0xa5c1);
    for (idx, pattern) in PATTERNS.iter().enumerate() {
        for text in matching_samples(idx, &mut rng)
            .into_iter()
            .filter(|text| text.is_ascii())
        {
            let full: Vec<_> = pattern_regex(idx)
                .find_iter(&text)
                .map(|m| m.range())
                .collect();
            let ascii: Vec<_> = ascii_pattern_regex(idx)
                .find_iter(&text)
                .map(|m| m.range())
                .collect();
            assert_eq!(full, ascii, "{}: {text:?}", pattern.name);
        }
    }
}

/// Ordinary code and config that names credentials, reads them from the
/// environment or holds a short password stays byte for byte: only a known
/// format is a secret.
#[test]
fn ordinary_code_and_config_stay_as_written() {
    let corpus = [
        r#"const simpleCacheKey = '["GET",[],null]';"#,
        "webhookSecret: process.env.X, // set in the deploy environment",
        "getToken: async () => {",
        "storageKey = 'app.settings.theme-preference'",
        r#"{"username":"admin","password":"letmein"}"#,
        "password = hunter2hunter2hunter2",
        "API_TOKEN=abcdefghijklmnopqrstuvwx",
        "  secretKey: 'my-very-long-development-secret-value'",
        "private static final String KEY = \"0123456789abcdef0123456789abcdef\";",
        "db.password=changeit",
        "<password>p4ssw0rd-from-the-docs</password>",
        "kind: Secret\ndata:\n  token: c2VjcmV0LXZhbHVlLWluLWJhc2U2NA==",
        r#"{"kty":"RSA","n":"0vx7agoebGcQSuu","d":"X4cTteJY_gn4FYPsXB8rdXix5vwsg1FLN45"}"#,
        "commit 4f1c9e2b7d3a8e6f0c5b1a9d2e7f3c8b6a4d0e1f",
        "id: 123e4567-e89b-12d3-a456-426614174000",
        "export function useAuthToken(token: string): string { return token; }",
        "headers['Authorization'] = `Bearer ${token}`;",
        "AUTH_SECRET=${AUTH_SECRET:-}",
        // Public identifiers: they name a resource, an account or a client
        // and authorize nothing on their own.
        r#"role_arn = "arn:aws:iam::123456789012:role/deploy""#,
        r#""Resource": "arn:aws:s3:::my-app-assets""#,
        "FunctionArn: arn:aws:lambda:us-east-1:123456789012:function:resize-images",
        "SecretId: arn:aws:secretsmanager:us-east-1:123456789012:secret:prod/db-AbCdEf",
        "GOOGLE_CLIENT_ID=1234567890-abc123def456ghi789jkl012mno345pq.apps.googleusercontent.com",
        "STRIPE_PUBLISHABLE_KEY=pk_live_AbCdEfGhIjKlMnOpQrStUvWx",
        "NEXT_PUBLIC_CLERK_PUBLISHABLE_KEY=pk_test_AbCdEfGhIjKlMnOpQrStUvWx",
        "OPENAI_ORG_ID=org-AbCdEfGhIjKlMnOpQrStUvWx",
        "mapboxgl.accessToken = 'pk.eyJ1IjoiZXhhbXBsZXVzZXIiLCJhIjoiY2xleGFtcGxlMDAwMDAwMDAwMDAwMDAwMDAwMDAwIn0.ZXhhbXBsZXNpZ25hdHVyZXBhcnQ';",
        "SQUARE_APPLICATION_ID=sq0ids-AbCdEfGhIjKlMnOpQrStUvWxYz0123456789_-AbCdE",
        "TWILIO_ACCOUNT_SID=AC0123456789abcdef0123456789abcdef",
        "PLAID_CLIENT_ID=5f3a9c1b2d4e6f8a0b1c2d3e",
        "posthog.init('phc_AbCdEfGhIjKlMnOpQrStUvWxYz0123456789abc')",
        "NREUM.info = { licenseKey: 'NRJS-0123456789abcdef012' }",
        "key: 'rzp_live_AbCdEfGhIjKlMn',",
        "redirect: https://abcdefghij12345.app.dropbox.com/auth",
        "FLW_PUBLIC_KEY=FLWPUBK_TEST-0123456789abcdef0123456789abcdef-X",
        // A URL password that is wholly an interpolation or a template
        // placeholder names where the secret comes from and holds none.
        "DATABASE_URL=postgres://app:${PGPASSWORD}@db:5432/app",
        "DATABASE_URL=postgresql://app:${DB_PASSWORD:-}@db.internal:5432/app",
        r#"url = "mysql://root:$MYSQL_ROOT_PASSWORD@mysql:3306/app""#,
        "BROKER_URL=amqp://guest:$RABBIT_PASS@rabbit:5672/",
        "redis_url: redis://default:{{ .Values.redis.password }}@redis:6379/0",
        "url: https://deploy:{{token}}@git.internal/org/repo.git",
        "mongodb://app:<password>@cluster0.mongodb.net/app",
        "jdbc:postgresql://admin:<your password>@localhost:5432/app",
        r#"dsn = "postgresql://%s:%s@%s/%s" % (user, password, host, db)"#,
        r#"uri = "mongodb://%(user)s:%(password)s@%(host)s:27017/%(db)s" % cfg"#,
        r#"fmt.Sprintf("redis://:%s@%s:6379", pass, host)"#,
        // A workflow's `id-token` permission and a Docker config that keeps
        // its credentials in a helper: no token value.
        "permissions:\n  contents: read\n  id-token: write",
        r#"{"auths": {"https://index.docker.io/v1/": {}}, "credsStore": "desktop"}"#,
    ];
    for line in corpus {
        assert_eq!(secret_spans(line), Vec::new(), "{line:?}");
        let sanitized = crate::security::sanitizer::sanitize_content(line).expect("scanned");
        assert_eq!(sanitized.content, line);
    }
}

/// A key block's body goes and its armor lines stay; the block keeps every
/// line, so every later line keeps its number.
#[test]
fn a_private_key_body_keeps_its_lines_and_armor() {
    let body = "MIIEowIBAAKCAQEAu1SU1LfVLPHCozMxH2Mo4lgOEePzNm0tRgeLezV6ffAt0gun\n".repeat(5);
    let text = format!(
        "before\n-----BEGIN RSA PRIVATE KEY-----\n{body}-----END RSA PRIVATE KEY-----\nafter\n"
    );
    let result = redact(&text).expect("a key");
    assert_eq!(result.sanitized.lines().count(), text.lines().count());
    assert!(result.sanitized.contains("-----BEGIN RSA PRIVATE KEY-----"));
    assert!(result.sanitized.contains("-----END RSA PRIVATE KEY-----"));
    assert!(!result.sanitized.contains("MIIEow"));
    assert!(result.sanitized.starts_with("before\n") && result.sanitized.ends_with("after\n"));
}

/// A secret deep in a text longer than any read holds at once is found: a
/// pattern runs over the whole text, and no edge cuts a value.
#[test]
fn a_secret_anywhere_in_a_long_text_is_redacted() {
    let token = format!("ghp_{}", "a".repeat(36));
    let filler = "x ".repeat(MAX_CONTENT_SIZE / 2 + 1);
    let text = format!("{filler}{token} {filler}");
    let spans = secret_spans(&text);
    assert_eq!(spans.len(), 1);
    assert_eq!(&text[spans[0].range.clone()], token);
}

/// Two patterns that find overlapping secrets hide their union once.
#[test]
fn overlapping_secrets_join_into_one_span() {
    let token = format!("ghp_{}", "b".repeat(36));
    let text = format!("Authorization: Bearer {token}\n");
    let spans = secret_spans(&text);
    assert_eq!(spans.len(), 1, "{spans:?}");
    assert_eq!(&text[spans[0].range.clone()], token);
}

/// A request that spells a token as a regex does still holds it.
#[test]
fn a_request_spelling_a_token_as_a_regex_holds_it() {
    let spelled = format!(r"ghp_{}\_{}", "a".repeat(18), "a".repeat(18));
    assert!(secret_spans(&spelled).is_empty());
    assert!(holds_credential_as_request(&spelled));
    assert!(!holds_credential_as_request(r"ghp_[A-Za-z0-9]{36}"));
}

/// The secret bytes `text` holds, one string per span.
fn hidden_parts(text: &str) -> Vec<&str> {
    secret_spans(text)
        .into_iter()
        .map(|span| &text[span.range])
        .collect()
}

/// A URL password that only holds an interpolation or a placeholder, beside
/// other text, is a secret still; so is the word `password`.
#[test]
fn a_url_password_that_holds_more_than_a_placeholder_is_hidden() {
    for (text, password) in [
        (
            "postgres://app:${PGPASSWORD}x@db:5432/app",
            "${PGPASSWORD}x",
        ),
        ("https://deploy:pre$TOKEN@git.internal/r.git", "pre$TOKEN"),
        ("postgres://app:$ecretPass@db:5432/app", "$ecretPass"),
        ("https://deploy:$ecretPass@git.internal/r.git", "$ecretPass"),
        ("mysql://root:<pw>s3cr3t@mysql:3306/app", "<pw>s3cr3t"),
        ("redis://u:%s9xK2@redis:6379", "%s9xK2"),
        ("postgres://user:password@localhost:5432/db", "password"),
        (
            "postgres://admin:Tr0ub4dor3xK9zQ@db.internal:5432/app",
            "Tr0ub4dor3xK9zQ",
        ),
    ] {
        assert_eq!(hidden_parts(text), [password], "{text:?}");
    }
}

/// A request for a URL whose password is wholly an interpolation runs: a read
/// of the same text hides nothing.
#[test]
fn a_request_for_an_interpolated_url_password_runs() {
    for term in [
        "postgres://u:${PGPASSWORD}@db",
        "postgres://u:$PGPASSWORD@db",
        "https://ci:$CI_JOB_TOKEN@gitlab.example/group/repo.git",
        "mysql://root:%(password)s@db/app",
    ] {
        assert!(!holds_credential_as_request(term), "{term:?} was refused");
    }
    assert!(holds_credential_as_request("postgres://u:hunter2x@db"));
}

/// The token fields of a Docker config and a kubeconfig's auth provider hide
/// their value and keep their key, wherever the file lives.
#[test]
fn docker_and_kubeconfig_token_fields_hide_only_their_value() {
    let docker = concat!(
        "{\n  \"auths\": {\n    \"registry.example\": {\n",
        "      \"email\": \"ci@example.test\",\n",
        "      \"auth\": \"Y2k6ZmFrZS1wbGFudGVkLXBhc3N3b3Jk\"\n    }\n  },\n",
        "  \"identitytoken\": \"fake-planted-identity-token-0001\",\n",
        "  \"registrytoken\": \"fake-planted-registry-token-0002\"\n}\n",
    );
    assert_eq!(
        hidden_parts(docker),
        [
            "Y2k6ZmFrZS1wbGFudGVkLXBhc3N3b3Jk",
            "fake-planted-identity-token-0001",
            "fake-planted-registry-token-0002"
        ]
    );
    // An entry a credential helper serves holds no `auth`: the first entry
    // that does is found.
    let helper_first =
        r#"{"auths": {"ghcr.io": {}, "registry.example": {"auth": "Y2k6ZmFrZS1wbGFudGVk"}}}"#;
    assert_eq!(hidden_parts(helper_first), ["Y2k6ZmFrZS1wbGFudGVk"]);
    let kubeconfig = concat!(
        "users:\n- name: oidc-user\n  user:\n    auth-provider:\n      config:\n",
        "        client-id: kubernetes\n",
        "        id-token: fake.planted.idtoken0003\n",
        "        idp-issuer-url: https://issuer.example\n",
        "        refresh-token: 'fake-planted-refresh-0004'\n",
        "      name: oidc\n",
    );
    assert_eq!(
        hidden_parts(kubeconfig),
        ["fake.planted.idtoken0003", "fake-planted-refresh-0004"]
    );
}
