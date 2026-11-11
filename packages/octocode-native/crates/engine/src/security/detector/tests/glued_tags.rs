//! A token behind a fixed vendor tag (`ghp_`, `glpat-`) is a secret wherever it
//! starts: glued to the word characters before it (minified text, a joined
//! string, `Bearerghp_…`) it is hidden as it is alone. A shape with no tag of
//! its own keeps its left boundary, or every longer word would match.

use super::every_pattern::path_for;
use super::sampling::{Lcg, sample};
use super::*;
use regex_syntax::hir::literal::{ExtractKind, Extractor};
use regex_syntax::hir::{Class, Hir, HirKind};

/// Word characters a token can be glued to: letters, digits and `_`.
const GLUES: &[&str] = &["a", "aaa", "Bearer", "7", "_"];

/// Bytes of the shortest literal a tag may be: a shorter one is a common
/// word ending (`sk-`, `re_`) and needs the boundary.
const SHORTEST_TAG: usize = 4;

/// The node a match of `hir` begins with: through captures and the first part
/// of a concatenation.
fn first(hir: &Hir) -> &Hir {
    match hir.kind() {
        HirKind::Capture(capture) => first(&capture.sub),
        HirKind::Concat(parts) => parts.first().map_or(hir, first),
        _ => hir,
    }
}

/// The node a match of `hir` ends with: through captures and the last part
/// of a concatenation.
fn last(hir: &Hir) -> &Hir {
    match hir.kind() {
        HirKind::Capture(capture) => last(&capture.sub),
        HirKind::Concat(parts) => parts.last().map_or(hir, last),
        _ => hir,
    }
}

fn is_word(ch: char) -> bool {
    ch.is_alphanumeric() || ch == '_'
}

/// Whether `hir` begins with an explicit left context: a boundary or line
/// start assertion, a class of non-word characters, or a choice of those.
fn begins_with_left_context(hir: &Hir) -> bool {
    match first(hir).kind() {
        HirKind::Look(_) => true,
        HirKind::Class(Class::Unicode(class)) => class
            .ranges()
            .iter()
            .all(|range| (range.start()..=range.end()).all(|ch| !is_word(ch))),
        HirKind::Class(Class::Bytes(class)) => class
            .ranges()
            .iter()
            .all(|range| (range.start()..=range.end()).all(|byte| !is_word(char::from(byte)))),
        HirKind::Alternation(branches) => branches.iter().all(begins_with_left_context),
        _ => false,
    }
}

/// The literals one of which begins every match of `hir`, when that set is
/// finite. The total is high enough to keep a `(?i)` tag's case variants at
/// full length: below it the extractor shortens them.
fn prefix_literals(hir: &Hir) -> Option<Vec<String>> {
    let mut extractor = Extractor::new();
    extractor.kind(ExtractKind::Prefix).limit_total(1 << 14);
    let literals = extractor.extract(hir).literals()?.to_vec();
    let literals: Vec<String> = literals
        .iter()
        .map(|literal| String::from_utf8_lossy(literal.as_bytes()).into_owned())
        .collect();
    (!literals.is_empty()).then_some(literals)
}

/// Whether every match of `hir` holds a fixed literal of [`SHORTEST_TAG`]
/// bytes or more: a part of its concatenation, through captures.
fn holds_a_tag(hir: &Hir) -> bool {
    match hir.kind() {
        HirKind::Capture(capture) => holds_a_tag(&capture.sub),
        HirKind::Concat(parts) => parts.iter().any(holds_a_tag),
        HirKind::Literal(literal) => literal.0.len() >= SHORTEST_TAG,
        _ => false,
    }
}

/// How a match of one top-level branch of a pattern begins.
#[derive(Debug, PartialEq)]
enum Start {
    /// Never inside a word: a left context, or a non-word first character.
    Bounded,
    /// A tag: one of a finite set of literals, each [`SHORTEST_TAG`] bytes or
    /// more, or an opening literal the match follows with a fixed literal that
    /// long (`sk-…T3BlbkFJ`), so it matches glued to a word.
    Tagged,
    /// Neither: a shape that would match inside longer words.
    Untagged,
}

fn start_of(branch: &Hir) -> Start {
    if begins_with_left_context(branch) {
        return Start::Bounded;
    }
    let Some(literals) = prefix_literals(branch) else {
        return Start::Untagged;
    };
    if literals
        .iter()
        .all(|literal| literal.chars().next().is_some_and(|ch| !is_word(ch)))
    {
        Start::Bounded
    } else if literals.iter().all(|literal| literal.len() >= SHORTEST_TAG)
        || (matches!(first(branch).kind(), HirKind::Literal(_)) && holds_a_tag(branch))
    {
        Start::Tagged
    } else {
        Start::Untagged
    }
}

/// The top-level branches of pattern `idx`: each is a match of it.
fn branches(idx: usize) -> Vec<Hir> {
    let hir = regex_syntax::parse(PATTERNS[idx].regex).expect(PATTERNS[idx].name);
    let mut top = &hir;
    while let HirKind::Capture(capture) = top.kind() {
        top = &capture.sub;
    }
    match top.kind() {
        HirKind::Alternation(branches) => branches.clone(),
        _ => vec![hir],
    }
}

/// The patterns that match glued to a word, sorted: each dropped its left
/// boundary with a measured cost of zero (see the `patterns` module doc), so
/// a change to this set is a deliberate edit here.
const TAGGED: &[&str] = &[
    "ageSecretKey",
    "anthropicApiKey",
    "awsAccessKeyId",
    "awsMwsAuthToken",
    "base64PemPrivateKey",
    "bitbucketAppPassword",
    "bitbucketRepoToken",
    "buildkiteAgentToken",
    "clojarsApiToken",
    "cloudflareApiTokenPrefixed",
    "cloudflareOriginCaKey",
    "contentfulAccessToken",
    "denoDeployToken",
    "depotToken",
    "digitalOceanOAuthToken",
    "digitalOceanRefreshToken",
    "digitalOceanToken",
    "dockerHubToken",
    "dopplerApiToken",
    "duffelApiToken",
    "dynatraceApiToken",
    "figmaToken",
    "flutterwaveKeys",
    "flyioAccessToken",
    "flyioMachineToken",
    "frameioApiToken",
    "githubAppInstallationToken",
    "githubFineGrainedToken",
    "githubTokens",
    "gitlabCiJobToken",
    "gitlabDeployToken",
    "gitlabFeatureFlagToken",
    "gitlabFeedToken",
    "gitlabIncomingMailToken",
    "gitlabK8sAgentToken",
    "gitlabOAuthAppSecret",
    "gitlabPersonalAccessToken",
    "gitlabPipelineTriggerToken",
    "gitlabRunnerToken",
    "gitlabScimToken",
    "gitlabSessionCookie",
    "googleApiKey",
    "googleOauthToken",
    "grafanaCloudApiKey",
    "grafanaServiceAccountToken",
    "groqApiKey",
    "herokuApiKeyV2",
    "honeycombApiKey",
    "langchainApiKey",
    "linearApiKey",
    "mapboxPublicToken",
    "mapboxSecretToken",
    "microsoftTeamsWebhook",
    "minisignSecretKey",
    "newRelicApiKey",
    "newRelicBrowserApiToken",
    "newRelicInsertKey",
    "newRelicInsightKey",
    "nostrSecretKey",
    "notionIntegrationToken",
    "npmAccessToken",
    "onePasswordServiceAccountToken",
    "openRouterApiKey",
    "openaiApiKeyLegacy",
    "openaiServiceAccountKey",
    "openshiftUserToken",
    "opensshPrivateKey",
    "perplexityApiKey",
    "pineconeApiKeyPrefixed",
    "pinterestAccessToken",
    "planetScaleToken",
    "posthogPersonalApiKey",
    "postmanApiToken",
    "prefectApiToken",
    "pypiApiToken",
    "razorpayApiKey",
    "readmeApiToken",
    "rubygemsApiToken",
    "scalingoApiToken",
    "sendinblueApiToken",
    "sentryOrgToken",
    "sentryUserToken",
    "sessionIds",
    "settlemintApplicationAccessToken",
    "settlemintPersonalAccessToken",
    "settlemintServiceAccessToken",
    "shippoApiToken",
    "shopifyAccessToken",
    "shopifyPrivateAppPassword",
    "shopifyStorefrontAccessToken",
    "shopifyWebhookToken",
    "slackAppToken",
    "slackBotToken",
    "slackConfigAccessToken",
    "slackRefreshToken",
    "slackUserToken",
    "slackWebhookUrl",
    "slackWorkspaceToken",
    "sonarqubeToken",
    "sourcegraphApiKey",
    "squareApplicationId",
    "squareOauthSecret",
    "stripeSecretKey",
    "stripeWebhookSecret",
    "supabaseSecretKey",
    "supabaseServiceKey",
    "tavilyApiKey",
    "typeformToken",
    "vaultBatchToken",
    "vaultPeriodicToken",
    "vaultServiceToken",
    "vercelToken",
    "xaiApiKey",
];

/// The tagged patterns that match glued to word characters on the right,
/// sorted: a branch that ends in no assertion. A new entry needs a measured
/// cost of zero without its trailing `\b` (see the `patterns` module doc), so
/// a change to this set is a deliberate edit here.
const RIGHT_OPEN: &[&str] = &[
    "anthropicApiKey",
    "awsMwsAuthToken",
    "base64PemPrivateKey",
    "bitbucketAppPassword",
    "bitbucketRepoToken",
    "buildkiteAgentToken",
    "clojarsApiToken",
    "cloudflareApiTokenPrefixed",
    "cloudflareOriginCaKey",
    "contentfulAccessToken",
    "denoDeployToken",
    "digitalOceanOAuthToken",
    "digitalOceanRefreshToken",
    "digitalOceanToken",
    "dockerHubToken",
    "dopplerApiToken",
    "duffelApiToken",
    "dynatraceApiToken",
    "figmaToken",
    "flyioAccessToken",
    "flyioMachineToken",
    "frameioApiToken",
    "githubAppInstallationToken",
    "githubFineGrainedToken",
    "githubTokens",
    "gitlabCiJobToken",
    "gitlabDeployToken",
    "gitlabFeatureFlagToken",
    "gitlabFeedToken",
    "gitlabIncomingMailToken",
    "gitlabK8sAgentToken",
    "gitlabOAuthAppSecret",
    "gitlabPipelineTriggerToken",
    "gitlabRunnerToken",
    "gitlabScimToken",
    "gitlabSessionCookie",
    "grafanaCloudApiKey",
    "grafanaServiceAccountToken",
    "groqApiKey",
    "herokuApiKeyV2",
    "honeycombApiKey",
    "linearApiKey",
    "mapboxSecretToken",
    "microsoftTeamsWebhook",
    "minisignSecretKey",
    "newRelicApiKey",
    "newRelicBrowserApiToken",
    "newRelicInsertKey",
    "newRelicInsightKey",
    "nostrSecretKey",
    "notionIntegrationToken",
    "npmAccessToken",
    "onePasswordServiceAccountToken",
    "openRouterApiKey",
    "openshiftUserToken",
    "opensshPrivateKey",
    "perplexityApiKey",
    "pinterestAccessToken",
    "planetScaleToken",
    "posthogPersonalApiKey",
    "postmanApiToken",
    "prefectApiToken",
    "pypiApiToken",
    "razorpayApiKey",
    "readmeApiToken",
    "rubygemsApiToken",
    "scalingoApiToken",
    "sendinblueApiToken",
    "sentryOrgToken",
    "sentryUserToken",
    "sessionIds",
    "settlemintApplicationAccessToken",
    "settlemintPersonalAccessToken",
    "settlemintServiceAccessToken",
    "shippoApiToken",
    "shopifyAccessToken",
    "shopifyPrivateAppPassword",
    "shopifyStorefrontAccessToken",
    "shopifyWebhookToken",
    "slackConfigAccessToken",
    "slackWebhookUrl",
    "sonarqubeToken",
    "sourcegraphApiKey",
    "squareApplicationId",
    "squareOauthSecret",
    "stripeSecretKey",
    "supabaseSecretKey",
    "supabaseServiceKey",
    "typeformToken",
];

/// The tagged patterns whose regex lets a match end glued to a word are
/// exactly [`RIGHT_OPEN`]: a dropped or added trailing boundary shows here.
#[test]
fn the_tagged_patterns_that_match_glued_on_the_right_are_the_pinned_set() {
    let mut open: Vec<&str> = (0..PATTERNS.len())
        .filter(|&idx| {
            branches(idx).iter().any(|branch| {
                start_of(branch) == Start::Tagged
                    && !matches!(last(branch).kind(), HirKind::Look(_))
            })
        })
        .map(|idx| PATTERNS[idx].name)
        .collect();
    open.sort_unstable();
    assert_eq!(open, RIGHT_OPEN);
}

/// The patterns whose regex lets a match begin glued to a word are exactly
/// [`TAGGED`]: a dropped or added boundary shows here.
#[test]
fn the_patterns_that_match_glued_to_a_word_are_the_pinned_set() {
    let mut tagged: Vec<&str> = (0..PATTERNS.len())
        .filter(|&idx| {
            branches(idx)
                .iter()
                .any(|branch| start_of(branch) == Start::Tagged)
        })
        .map(|idx| PATTERNS[idx].name)
        .collect();
    tagged.sort_unstable();
    assert_eq!(tagged, TAGGED);
}

/// A fixed literal inside a match makes a tag only behind an opening literal
/// (`sk-…T3BlbkFJ`): a shape that opens with a class (Dropbox's app key
/// without its boundary, even behind a choice of letters) would match inside
/// longer words.
#[test]
fn an_inner_literal_tags_a_branch_only_behind_an_opening_literal() {
    let start = |regex: &str| start_of(&regex_syntax::parse(regex).expect("fixture"));
    assert_eq!(
        start(r"[a-z0-9]{15}\.(?:app|apps)\.dropbox\.com\b"),
        Start::Untagged
    );
    assert_eq!(
        start(r"[xy][a-z0-9]{15}\.apps\.dropbox\.com\b"),
        Start::Untagged
    );
    assert_eq!(
        start(r"(sk-[a-zA-Z0-9_-]+T3BlbkFJ[a-zA-Z0-9_-]+)\b"),
        Start::Tagged
    );
}

/// The GitHub token families, each glued to letters, digits, `_` or a header
/// word, are hidden whole, in a file and in the pathless net.
#[test]
fn github_tokens_glued_to_word_characters_are_redacted() {
    let tail: String = "Ab3dE5gH7jK9mN1pQ3sT5vW7yZ9bC1eF3hJ5kL7".into();
    let fine_grained = format!("github_pat_{}", "Xy7_Kq2Lm9".repeat(8) + "Zz");
    let mut tokens: Vec<String> = ["ghp_", "gho_", "ghu_", "ghs_", "ghr_", "github_pat_"]
        .iter()
        .map(|tag| format!("{tag}{tail}"))
        .collect();
    tokens.push(fine_grained);
    let mut leaks = Vec::new();
    for token in &tokens {
        for glue in GLUES {
            let text = format!("x = \"{glue}{token}\"\n");
            for scope in [
                Scope::Pathless,
                Scope::File("src/app.js"),
                Scope::File(".env"),
            ] {
                if detect_in(&text, scope).sanitized.contains(token.as_str()) {
                    leaks.push(format!("{scope:?}: {text:?}"));
                }
            }
        }
    }
    assert_eq!(leaks, Vec::<String>::new());
}

/// Keys a scanner reads as credentials, glued as they leak: behind a header
/// word (`Bearersk_live_…`), a joined name (`backup_AKIA…`, `keyAIza…`) or
/// any letters. An OpenAI key carries its `T3BlbkFJ` anchor.
#[test]
fn vendor_keys_glued_to_word_characters_are_redacted() {
    let alnum = |n: usize| -> String {
        "Ab3dE5gH7jK9mN1pQ3sT5vW7yZ9bC1"
            .chars()
            .cycle()
            .take(n)
            .collect()
    };
    let upper = "AB3DE5GH7JK9MN1P";
    let cases = [
        ("Bearer", format!("sk_live_{}", alnum(24))),
        ("aaa", format!("rk_live_{}", alnum(24))),
        ("backup_", format!("AKIA{upper}")),
        ("id", format!("AKIA{upper}")),
        ("sts", format!("ASIA{upper}")),
        ("key", format!("AIza{}", alnum(35))),
        ("aaa", format!("ATBB{}", alnum(24))),
        ("aaa", format!("ATCTT3{}", alnum(24))),
        ("aaa", format!("phx_{}", alnum(39))),
        ("aaa", format!("pypi-{}", alnum(84))),
        ("aaa", format!("sk-proj-{}T3BlbkFJ{}", alnum(20), alnum(20))),
        ("Bearer", format!("sk-{}T3BlbkFJ{}", alnum(20), alnum(20))),
    ];
    let mut leaks = Vec::new();
    for (glue, key) in &cases {
        let text = format!("x = \"{glue}{key}\"\n");
        for scope in [Scope::Pathless, Scope::File("src/app.js")] {
            if detect_in(&text, scope).sanitized.contains(key.as_str()) {
                leaks.push(format!("{scope:?}: {text:?}"));
            }
        }
    }
    assert_eq!(leaks, Vec::<String>::new());
}

/// A fixed-length token glued on the right (`npm_<36>xyz`) is hidden: the
/// token part goes, whatever follows.
#[test]
fn tokens_glued_on_the_right_are_redacted() {
    let alnum = |n: usize| -> String {
        "Ab3dE5gH7jK9mN1pQ3sT5vW7yZ9bC1"
            .chars()
            .cycle()
            .take(n)
            .collect()
    };
    let cases = [
        (format!("npm_{}", alnum(36)), "xyz"),
        (format!("ATBB{}", alnum(24)), "_suffix"),
        (format!("shpat_{}", "a1b2c3d4".repeat(4)), "Z9"),
        (format!("dop_v1_{}", "a1b2c3d4".repeat(8)), "ff"),
    ];
    let mut leaks = Vec::new();
    for (token, glue) in &cases {
        for text in [
            format!("x = \"{token}{glue}\"\n"),
            format!("{token}{glue}\n"),
        ] {
            for scope in [Scope::Pathless, Scope::File("src/app.js")] {
                if detect_in(&text, scope).sanitized.contains(token.as_str()) {
                    leaks.push(format!("{scope:?}: {text:?}"));
                }
            }
        }
    }
    assert_eq!(leaks, Vec::<String>::new());
}

/// A tag that is a common word ending keeps its boundary: kebab slugs that
/// hold `sk-proj-` and identifiers that hold `rk_test_` stay readable.
#[test]
fn identifiers_that_hold_a_short_tag_stay_readable() {
    let contents = [
        "route: task-proj-management-settings-panel\n",
        "suite network_test_abcdefghijklmnopqrstuvwxyz\n",
    ];
    let mut redacted = Vec::new();
    for content in contents {
        for scope in [Scope::Pathless, Scope::File("src/app.js")] {
            let result = detect_in(content, scope);
            if result.sanitized != content {
                redacted.push(format!("{scope:?}: {}", result.sanitized));
            }
        }
    }
    assert_eq!(redacted, Vec::<String>::new());
}

/// Every pattern with no left context hides a sample of itself glued to a
/// word exactly as it hides the sample alone: the glue stays, the secret goes.
#[test]
fn every_tagged_pattern_hides_a_glued_sample_as_it_hides_it_alone() {
    let mut rng = Lcg(0x91ed);
    let mut differ = Vec::new();
    let mut tagged = 0;
    for idx in 0..PATTERNS.len() {
        if document::Form::of(idx).is_some() {
            continue;
        }
        for hir in branches(idx) {
            if start_of(&hir) != Start::Tagged {
                continue;
            }
            tagged += 1;
            differ.extend(glued_differences(idx, &hir, &mut rng));
        }
    }
    assert!(tagged > 0, "no tagged pattern");
    assert_eq!(differ, Vec::<String>::new());
}

/// Samples of `branch` of pattern `idx` that pattern hides differently glued
/// to a word than alone.
fn glued_differences(idx: usize, hir: &Hir, rng: &mut Lcg) -> Vec<String> {
    let mut differ = Vec::new();
    for _ in 0..40 {
        let path = path_for(PATTERNS[idx].file_context, rng);
        let mut token = String::new();
        sample(hir, rng, &mut token);
        let scope = Scope::File(&path);
        let Some(hidden) = redact(&format!("\n{token}\n"), scope) else {
            continue;
        };
        for glue in GLUES {
            let glued = format!("\n{glue}{token}\n");
            let expected = format!("\n{glue}{}", &hidden.sanitized[1..]);
            if detect_in(&glued, scope).sanitized != expected {
                differ.push(format!("{}: {glued:?}", PATTERNS[idx].name));
            }
        }
    }
    differ
}

/// A pattern matches glued to a word only behind a tag of its own: every
/// branch with no left context begins with a finite set of literals, each
/// at least [`SHORTEST_TAG`] bytes. An entropy shape (`[a-f0-9]{40}`) or a
/// short tag (`sk-`) that lost its boundary would match inside longer words.
#[test]
fn a_branch_without_a_left_context_begins_with_a_tag() {
    let untagged: Vec<String> = (0..PATTERNS.len())
        .flat_map(|idx| {
            branches(idx)
                .into_iter()
                .filter(|branch| start_of(branch) == Start::Untagged)
                .map(move |branch| {
                    format!("{}: {:?}", PATTERNS[idx].name, prefix_literals(&branch))
                })
        })
        .collect();
    assert_eq!(untagged, Vec::<String>::new());
}
