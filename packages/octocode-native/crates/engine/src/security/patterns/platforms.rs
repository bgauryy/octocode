//! Email, messaging, design and hosting platform tokens.
//!
//! A contiguous slice of [`super::PATTERNS`]: its order is part of the
//! evaluation order.

use super::SecretPattern;

pub(super) const PATTERNS: &[SecretPattern] = &[
    SecretPattern {
        name: "sendgridApiKey",
        file_context: None,
        regex: r###"\bSG\.[A-Za-z0-9_-]{20,22}\.[A-Za-z0-9_-]{43}\b"###,
    },
    SecretPattern {
        name: "mailgunApiKey",
        file_context: None,
        regex: r###"\bkey-[0-9a-z]{32}\b"###,
    },
    SecretPattern {
        name: "mailchimpApiKey",
        file_context: None,
        regex: r###"\b[0-9a-f]{32}-us[0-9]{1,2}\b"###,
    },
    SecretPattern {
        name: "telegramBotToken",
        file_context: None,
        regex: r###"\b[0-9]{8,10}:[A-Za-z0-9_-]{35}\b"###,
    },
    SecretPattern {
        name: "twilioApiKey",
        file_context: None,
        regex: r###"\bSK[a-z0-9]{32}\b"###,
    },
    SecretPattern {
        name: "twilioAccountSid",
        file_context: None,
        regex: r###"\bAC[0-9a-fA-F]{32}\b"###,
    },
    SecretPattern {
        name: "dockerHubToken",
        file_context: None,
        regex: r###"dckr_pat_[a-zA-Z0-9_]{36}"###,
    },
    SecretPattern {
        name: "pypiApiToken",
        file_context: None,
        regex: r###"pypi-[a-zA-Z0-9_-]{84}"###,
    },
    SecretPattern {
        name: "figmaToken",
        file_context: None,
        regex: r###"figd_[a-zA-Z0-9_-]{43}"###,
    },
    SecretPattern {
        name: "renderToken",
        file_context: None,
        regex: r###"\brnd_[a-zA-Z0-9_-]{43}\b"###,
    },
    SecretPattern {
        name: "airtablePersonalAccessToken",
        file_context: None,
        regex: r###"\bpat[a-zA-Z0-9]{14}\.[a-zA-Z0-9]{64}\b"###,
    },
    SecretPattern {
        name: "typeformToken",
        file_context: None,
        regex: r###"tfp_[a-zA-Z0-9_-]{43}"###,
    },
    SecretPattern {
        name: "intercomAccessToken",
        file_context: None,
        regex: r###"\bdG9rOi[a-zA-Z0-9+/]{46,48}={0,2}\b"###,
    },
    SecretPattern {
        name: "digitalOceanToken",
        file_context: None,
        regex: r###"dop_v1_[a-f0-9]{64}"###,
    },
    SecretPattern {
        name: "digitalOceanOAuthToken",
        file_context: None,
        regex: r###"doo_v1_[a-f0-9]{64}"###,
    },
    SecretPattern {
        name: "digitalOceanRefreshToken",
        file_context: None,
        regex: r###"dor_v1_[a-f0-9]{64}"###,
    },
    SecretPattern {
        name: "cloudflareApiKey",
        file_context: None,
        regex: r###"(?i)\b['"]?(?:cloudflare)(?:[\s\w.-]{0,20})['"]?\s*(?::|=>|=)\s*['"]?(?P<secret>[a-z0-9_-]{40})['"]?\b"###,
    },
    SecretPattern {
        name: "cloudflareGlobalApiKey",
        file_context: None,
        regex: r###"(?i)\b['"]?(?:cloudflare)(?:[\s\w.-]{0,20})['"]?\s*(?::|=>|=)\s*['"]?(?P<secret>[a-f0-9]{37})['"]?\b"###,
    },
    SecretPattern {
        name: "cloudflareOriginCaKey",
        file_context: None,
        regex: r###"v1\.0-[a-f0-9]{24}-[a-f0-9]{146}"###,
    },
    SecretPattern {
        name: "flyioAccessToken",
        file_context: None,
        regex: r###"fo1_[\w-]{43}"###,
    },
    SecretPattern {
        name: "flyioMachineToken",
        file_context: None,
        regex: r###"fm[12][ar]?_[a-zA-Z0-9+/]{100,}={0,3}"###,
    },
    SecretPattern {
        name: "dopplerApiToken",
        file_context: None,
        regex: r###"(?i)dp\.pt\.[a-z0-9]{43}"###,
    },
    SecretPattern {
        name: "dynatraceApiToken",
        file_context: None,
        regex: r###"(?i)dt0c01\.[a-z0-9]{24}\.[a-z0-9]{64}"###,
    },
    SecretPattern {
        name: "netlifyAccessToken",
        file_context: None,
        regex: r###"(?i)\b['"]?(?:netlify)(?:[\s\w.-]{0,20})['"]?\s*(?::|=>|=)\s*['"]?(?P<secret>[a-z0-9=_-]{40,46})['"]?\b"###,
    },
    SecretPattern {
        name: "scalingoApiToken",
        file_context: None,
        regex: r###"tk-us-[\w-]{48}"###,
    },
    SecretPattern {
        name: "infracostApiToken",
        file_context: None,
        regex: r###"\bico-[a-zA-Z0-9]{32}\b"###,
    },
    SecretPattern {
        name: "harnessApiKey",
        file_context: None,
        regex: r###"\b(?:pat|sat)\.[a-zA-Z0-9_-]{22}\.[a-zA-Z0-9]{24}\.[a-zA-Z0-9]{20}\b"###,
    },
    SecretPattern {
        name: "azureAdClientSecret",
        file_context: None,
        regex: r###"(?:^|[\\'"` \s>=:(,)])([a-zA-Z0-9_~.]{3}\dQ~[a-zA-Z0-9_~.-]{31,34})(?:$|[\\'"` \s<),])"###,
    },
    SecretPattern {
        name: "herokuApiKeyV2",
        file_context: None,
        regex: r###"HRKU-AA[0-9a-zA-Z_-]{58}"###,
    },
    SecretPattern {
        name: "microsoftTeamsWebhook",
        file_context: None,
        regex: r###"(?i)https:\/\/[a-z0-9]+\.webhook\.office\.com\/webhookb2\/[a-z0-9]{8}-(?:[a-z0-9]{4}-){3}[a-z0-9]{12}@[a-z0-9]{8}-(?:[a-z0-9]{4}-){3}[a-z0-9]{12}\/IncomingWebhook\/[a-z0-9]{32}\/[a-z0-9]{8}-(?:[a-z0-9]{4}-){3}[a-z0-9]{12}"###,
    },
    SecretPattern {
        name: "oktaAccessToken",
        file_context: None,
        regex: r###"(?i)\b['"]?(?:okta)(?:[\s\w.-]{0,20})['"]?\s*(?::|=>|=)\s*['"]?(?P<secret>00[\w=-]{40})['"]?\b"###,
    },
    SecretPattern {
        name: "openshiftUserToken",
        file_context: None,
        regex: r###"sha256~[\w-]{43}"###,
    },
    SecretPattern {
        name: "denoDeployToken",
        file_context: None,
        regex: r###"ddp_[a-zA-Z0-9]{40}"###,
    },
    SecretPattern {
        name: "resendApiKey",
        file_context: None,
        regex: r###"\bre_[a-zA-Z0-9]{30,}\b"###,
    },
    SecretPattern {
        name: "azureOpenaiApiKey",
        file_context: None,
        regex: r###"(?i)\b['"]?(?:AZURE_OPENAI|azure_openai)_?(?:API|api)?_?(?:KEY|key)['"]?\s*(?::|=>|=)\s*['"]?(?P<secret>[a-f0-9]{32})['"]?\b"###,
    },
    SecretPattern {
        name: "railwayApiToken",
        file_context: None,
        regex: r###"(?i)\b['"]?(?:RAILWAY|railway)_?(?:API|api)?_?(?:TOKEN|token)['"]?\s*(?::|=>|=)\s*['"]?(?P<secret>[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12})['"]?\b"###,
    },
    SecretPattern {
        name: "convexDeployKey",
        file_context: None,
        regex: r###"\b(?:prod|dev):[a-zA-Z0-9_-]+:[a-zA-Z0-9_-]{40,}\b"###,
    },
    SecretPattern {
        name: "upstashKafkaCredentials",
        file_context: None,
        regex: r###"(?i)\b['"]?(?:UPSTASH_KAFKA)(?:[\s\w.-]{0,20})['"]?\s*(?::|=>|=)\s*['"]?(?P<secret>[a-zA-Z0-9=_-]{40,})['"]?\b"###,
    },
    SecretPattern {
        name: "cloudflareApiTokenPrefixed",
        file_context: None,
        regex: r###"cf(?:k|ut|at)_[a-zA-Z0-9]{40}[a-fA-F0-9]{8}"###,
    },
];
