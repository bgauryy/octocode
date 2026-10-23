//! Package registries, CI and developer SaaS tokens, and credential-named
//! assignments.
//!
//! A contiguous slice of [`super::PATTERNS`]: its order is part of the
//! evaluation order.

use super::SecretPattern;

pub(super) const PATTERNS: &[SecretPattern] = &[
    SecretPattern {
        name: "npmAccessToken",
        file_context: None,
        regex: r###"\bnpm_[a-zA-Z0-9]{36}\b"###,
    },
    SecretPattern {
        name: "nugetApiKey",
        file_context: None,
        regex: r###"\boy2[a-z0-9]{43}\b"###,
    },
    SecretPattern {
        name: "artifactoryApiKey",
        file_context: None,
        regex: r###"\bAKCp[A-Za-z0-9]{69}\b"###,
    },
    SecretPattern {
        name: "herokuApiKey",
        file_context: None,
        regex: r###"(?i)\bheroku.*[0-9A-F]{8}-[0-9A-F]{4}-[0-9A-F]{4}-[0-9A-F]{4}-[0-9A-F]{12}\b"###,
    },
    SecretPattern {
        name: "terraformCloudToken",
        file_context: None,
        regex: r###"\b[a-zA-Z0-9]{14}\.[a-zA-Z0-9]{6}\.[a-zA-Z0-9]{16}\b"###,
    },
    SecretPattern {
        name: "pulumiAccessToken",
        file_context: None,
        regex: r###"\bpul-[a-f0-9]{40}\b"###,
    },
    SecretPattern {
        name: "atlassianApiToken",
        file_context: None,
        regex: r###"\bATATT3[A-Za-z0-9_\-=]{186}\b"###,
    },
    SecretPattern {
        name: "sourcegraphApiKey",
        file_context: None,
        regex: r###"\bsgp_[a-zA-Z0-9]{32}\b"###,
    },
    SecretPattern {
        name: "linearApiKey",
        file_context: None,
        regex: r###"\blin_api_[0-9A-Za-z]{40}\b"###,
    },
    SecretPattern {
        name: "notionIntegrationToken",
        file_context: None,
        regex: r###"\bntn_[a-zA-Z0-9_-]{43}\b"###,
    },
    SecretPattern {
        name: "notionIntegrationTokenLegacy",
        file_context: None,
        regex: r###"\bsecret_[a-zA-Z0-9]{43}\b"###,
    },
    SecretPattern {
        name: "stackhawkApiKey",
        file_context: None,
        regex: r###"\bhawk\.[0-9A-Za-z\-_]{20}\.[0-9A-Za-z\-_]{20}\b"###,
    },
    SecretPattern {
        name: "sentryAuthToken",
        file_context: None,
        regex: r###"(?i)\bsentry[\s\w]*(?:auth|token)[\s:=]*["']?[a-f0-9]{64}["']?\b"###,
    },
    SecretPattern {
        name: "bugsnagApiKey",
        file_context: None,
        regex: r###"(?i)\bbugsnag[\s\w]*(?:api|key)[\s:=]*["']?[a-f0-9]{32}["']?\b"###,
    },
    SecretPattern {
        name: "rollbarAccessToken",
        file_context: None,
        regex: r###"(?i)\brollbar[\s\w]*(?:access|token)[\s:=]*["']?[a-f0-9]{32}["']?\b"###,
    },
    SecretPattern {
        name: "postmanApiToken",
        file_context: None,
        regex: r###"(?i)\bPMAK-[a-f0-9]{24}-[a-f0-9]{34}\b"###,
    },
    SecretPattern {
        name: "prefectApiToken",
        file_context: None,
        regex: r###"\bpnu_[a-zA-Z0-9]{36}\b"###,
    },
    SecretPattern {
        name: "readmeApiToken",
        file_context: None,
        regex: r###"\brdme_[a-z0-9]{70}\b"###,
    },
    SecretPattern {
        name: "rubygemsApiToken",
        file_context: None,
        regex: r###"\brubygems_[a-f0-9]{48}\b"###,
    },
    SecretPattern {
        name: "clojarsApiToken",
        file_context: None,
        regex: r###"(?i)\bCLOJARS_[a-z0-9]{60}\b"###,
    },
    SecretPattern {
        name: "snykApiToken",
        file_context: None,
        regex: r###"(?i)\b['"]?(?:snyk[_.-]?(?:(?:api|oauth)[_.-]?)?(?:key|token))['"]?\s*(?::|=>|=)\s*['"]?(?P<secret>[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12})['"]?\b"###,
    },
    SecretPattern {
        name: "sonarqubeToken",
        file_context: None,
        regex: r###"(?i)\b(?:squ_|sqp_|sqa_)[a-z0-9=_-]{40}\b"###,
    },
    SecretPattern {
        name: "travisciAccessToken",
        file_context: None,
        regex: r###"(?i)\b['"]?(?:travis)(?:[\s\w.-]{0,20})['"]?\s*(?::|=>|=)\s*['"]?(?P<secret>[a-z0-9]{22})['"]?\b"###,
    },
    SecretPattern {
        name: "codecovAccessToken",
        file_context: None,
        regex: r###"(?i)\b['"]?(?:codecov)(?:[\s\w.-]{0,20})['"]?\s*(?::|=>|=)\s*['"]?(?P<secret>[a-z0-9]{32})['"]?\b"###,
    },
    SecretPattern {
        name: "droneCiAccessToken",
        file_context: None,
        regex: r###"(?i)\b['"]?(?:droneci|drone)(?:[\s\w.-]{0,20})['"]?\s*(?::|=>|=)\s*['"]?(?P<secret>[a-z0-9]{32})['"]?\b"###,
    },
    SecretPattern {
        name: "octopusDeployApiKey",
        file_context: None,
        regex: r###"\bAPI-[A-Z0-9]{26}\b"###,
    },
    SecretPattern {
        name: "circleciToken",
        file_context: None,
        regex: r###"(?i)\b['"]?(?:circleci|circle)(?:[\s\w.-]{0,20})['"]?\s*(?::|=>|=)\s*['"]?(?P<secret>[a-f0-9]{40})['"]?\b"###,
    },
    SecretPattern {
        name: "buildkiteAgentToken",
        file_context: None,
        regex: r###"\bbkagent_[a-f0-9]{40}\b"###,
    },
    SecretPattern {
        name: "launchdarklyAccessToken",
        file_context: None,
        regex: r###"(?i)\b['"]?(?:launchdarkly)(?:[\s\w.-]{0,20})['"]?\s*(?::|=>|=)\s*['"]?(?P<secret>[a-z0-9=_-]{40})['"]?\b"###,
    },
    SecretPattern {
        name: "algoliaApiKey",
        file_context: None,
        regex: r###"(?i)\b['"]?(?:algolia)(?:[\s\w.-]{0,20})['"]?\s*(?::|=>|=)\s*['"]?(?P<secret>[a-z0-9]{32})['"]?\b"###,
    },
    SecretPattern {
        name: "clerkPublishableKey",
        file_context: None,
        regex: r###"\bpk_(?:live|test)_[a-zA-Z0-9]{24,}\b"###,
    },
    SecretPattern {
        name: "launchdarklySdkKey",
        file_context: None,
        regex: r###"\bsdk-[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}\b"###,
    },
    SecretPattern {
        name: "vercelOidcToken",
        file_context: None,
        regex: r###"\b['"]?(?:VERCEL_OIDC_TOKEN)['"]?\s*(?::|=>|=)\s*['"]?(?P<secret>eyJ[a-zA-Z0-9_-]{100,})['"]?\b"###,
    },
    SecretPattern {
        name: "novuApiKey",
        file_context: None,
        regex: r###"(?i)\b['"]?(?:NOVU|novu)_?(?:API|api)?_?(?:KEY|key)['"]?\s*(?::|=>|=)\s*['"]?(?P<secret>[a-zA-Z0-9]{32,})['"]?\b"###,
    },
    SecretPattern {
        name: "triggerDevApiKey",
        file_context: None,
        regex: r###"\btr_(?:dev|prod)_[a-zA-Z0-9]{20,}\b"###,
    },
    SecretPattern {
        name: "nxCloudAccessToken",
        file_context: None,
        regex: r###"\b['"]?(?:NX_CLOUD_ACCESS_TOKEN|nxCloudAccessToken)['"]?\s*(?::|=>|=)\s*['"]?(?P<secret>[a-zA-Z0-9]{36,})['"]?\b"###,
    },
    SecretPattern {
        name: "depotToken",
        file_context: None,
        regex: r###"\bdpt_[a-zA-Z0-9]{40,}\b"###,
    },
    SecretPattern {
        name: "grafbaseApiKey",
        file_context: None,
        regex: r###"(?i)\b['"]?(?:GRAFBASE|grafbase)_?(?:API|api)?_?(?:KEY|key)['"]?\s*(?::|=>|=)\s*['"]?(?P<secret>eyJ[a-zA-Z0-9_-]{50,})['"]?\b"###,
    },
    SecretPattern {
        name: "shopifyStorefrontAccessToken",
        file_context: None,
        regex: r###"\bshpatf_[0-9a-f]{32}\b"###,
    },
    SecretPattern {
        name: "woocommerceConsumerKey",
        file_context: None,
        regex: r###"\bck_[a-f0-9]{40}\b"###,
    },
    SecretPattern {
        name: "woocommerceConsumerSecret",
        file_context: None,
        regex: r###"\bcs_[a-f0-9]{40}\b"###,
    },
    SecretPattern {
        name: "contentfulAccessToken",
        file_context: None,
        regex: r###"\bCFPAT-[0-9a-zA-Z]{20}\b"###,
    },
    SecretPattern {
        name: "mailchimpEcommerceApiKey",
        file_context: None,
        regex: r###"\b[0-9a-f]{32}-[a-z]{2,3}[0-9]{1,2}\b"###,
    },
    SecretPattern {
        name: "credentialsInUrl",
        file_context: None,
        regex: r###"\b[a-zA-Z]{3,10}:\/\/[^\\/\s:@]{3,20}:[^\\/\s:@]{3,20}@[^\s'"]+\b"###,
    },
    SecretPattern {
        name: "envVarSecrets",
        file_context: None,
        // A quoted value assigned to a credential-named identifier: snake,
        // kebab or dotted (`API_KEY`, `client-secret`, `db.password`) or
        // camelCase (`awsSecret`, `clientSecretKey`), optionally quoted as a
        // JSON/YAML key, with `=`, `:=`, `:` or `=>`.
        regex: r###"(?:\b(?:[A-Za-z0-9]+[_.-])*(?i:secret|password|passwd|passphrase|key|token|credential)s?(?:[_.-][A-Za-z0-9]+)*|\b[a-z][a-z0-9]*(?:[A-Z][a-z0-9]*)*(?:Secret|Password|Passwd|Passphrase|Key|Token|Credential)s?(?:[A-Z][a-z0-9]*)*)["']?\s*(?:=>|:=|=|:)\s*["'](?P<secret>[^"'\r\n]{16,})["']"###,
    },
    SecretPattern {
        name: "dotenvSecretAssignment",
        file_context: None,
        // An unquoted dotenv/INI/shell line `NAME=value` whose name holds a
        // credential word. The value must run to the end of the line (or a
        // comment), so code such as `SECRET = os.environ["X"]` is left alone.
        regex: r###"(?m)^[ \t]*(?:export[ \t]+)?(?:[A-Za-z0-9]+[_.])*(?i:secret|password|passwd|passphrase|key|token|credential)s?(?:[_.][A-Za-z0-9]+)*[ \t]*=[ \t]*(?P<secret>[^\s"'$#(){}\[\]:][^\s"'(){}\[\]:]{11,})[ \t]*(?:#.*)?$"###,
    },
    SecretPattern {
        name: "yamlSecretValue",
        file_context: None,
        // An unquoted YAML scalar under a credential-named key, which may sit
        // under a slash-separated path (`secret/data/app/api_token:`). A bare
        // `key:` is too common in YAML (label selectors, tolerations) to count.
        regex: r###"(?m)^[ \t]*(?:-[ \t]+)?(?:[A-Za-z0-9_.-]+/)*(?:(?:[A-Za-z0-9]+[_.-])*(?i:secret|password|passwd|passphrase|token|api[_-]?key|access[_-]?key|private[_-]?key|auth[_-]?key|credential)s?(?:[_.-][A-Za-z0-9]+)*|[a-z][a-z0-9]*(?:[A-Z][a-z0-9]*)*(?:Secret|Password|Passwd|Passphrase|Token|ApiKey|AccessKey|PrivateKey|AuthKey|Credential)s?(?:[A-Z][a-z0-9]*)*)[ \t]*:[ \t]+(?P<secret>[^\s"'#$&*!|>{}\[\]:%@`][^\s"'#:]{11,})[ \t]*(?:#.*)?$"###,
    },
];
