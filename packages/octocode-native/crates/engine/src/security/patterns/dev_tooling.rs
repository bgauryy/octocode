//! Package registries, CI and developer SaaS tokens, Docker and kubeconfig
//! credential fields, and URL userinfo passwords.
//!
//! A contiguous slice of [`super::PATTERNS`]: its order is part of the
//! evaluation order.

use super::SecretPattern;

pub(super) const PATTERNS: &[SecretPattern] = &[
    SecretPattern {
        name: "npmAccessToken",
        regex: r###"npm_[a-zA-Z0-9]{36}"###,
    },
    SecretPattern {
        name: "nugetApiKey",
        regex: r###"\boy2[a-z0-9]{43}\b"###,
    },
    SecretPattern {
        name: "artifactoryApiKey",
        regex: r###"\bAKCp[A-Za-z0-9]{69}\b"###,
    },
    SecretPattern {
        name: "herokuApiKey",
        regex: r###"(?i)\bheroku(?:[\s\w.-]{0,20})['"]?\s*(?::|=>|=)\s*['"]?(?P<secret>[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12})\b"###,
    },
    SecretPattern {
        name: "pulumiAccessToken",
        regex: r###"\bpul-[a-f0-9]{40}\b"###,
    },
    SecretPattern {
        name: "atlassianApiToken",
        regex: r###"\bATATT3[A-Za-z0-9_\-=]{186}\b"###,
    },
    SecretPattern {
        name: "sourcegraphApiKey",
        regex: r###"sgp_[a-zA-Z0-9]{32}"###,
    },
    SecretPattern {
        name: "linearApiKey",
        regex: r###"lin_api_[0-9A-Za-z]{40}"###,
    },
    SecretPattern {
        name: "notionIntegrationToken",
        regex: r###"ntn_[a-zA-Z0-9_-]{43}"###,
    },
    SecretPattern {
        name: "notionIntegrationTokenLegacy",
        regex: r###"\bsecret_[a-zA-Z0-9]{43}\b"###,
    },
    SecretPattern {
        name: "stackhawkApiKey",
        regex: r###"\bhawk\.[0-9A-Za-z\-_]{20}\.[0-9A-Za-z\-_]{20}\b"###,
    },
    SecretPattern {
        name: "sentryAuthToken",
        regex: r###"(?i)\bsentry[\s\w]*(?:auth|token)[\s:=]*["']?(?P<secret>[a-f0-9]{64})\b"###,
    },
    SecretPattern {
        name: "bugsnagApiKey",
        regex: r###"(?i)\bbugsnag[\s\w]*(?:api|key)[\s:=]*["']?(?P<secret>[a-f0-9]{32})\b"###,
    },
    SecretPattern {
        name: "rollbarAccessToken",
        regex: r###"(?i)\brollbar[\s\w]*(?:access|token)[\s:=]*["']?(?P<secret>[a-f0-9]{32})\b"###,
    },
    SecretPattern {
        name: "postmanApiToken",
        regex: r###"(?i)PMAK-[a-f0-9]{24}-[a-f0-9]{34}"###,
    },
    SecretPattern {
        name: "prefectApiToken",
        regex: r###"pnu_[a-zA-Z0-9]{36}"###,
    },
    SecretPattern {
        name: "readmeApiToken",
        regex: r###"rdme_[a-z0-9]{70}"###,
    },
    SecretPattern {
        name: "rubygemsApiToken",
        regex: r###"rubygems_[a-f0-9]{48}"###,
    },
    SecretPattern {
        name: "clojarsApiToken",
        regex: r###"(?i)CLOJARS_[a-z0-9]{60}"###,
    },
    SecretPattern {
        name: "snykApiToken",
        regex: r###"(?i)\b['"]?(?:snyk[_.-]?(?:(?:api|oauth)[_.-]?)?(?:key|token))['"]?\s*(?::|=>|=)\s*['"]?(?P<secret>[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12})['"]?\b"###,
    },
    SecretPattern {
        name: "sonarqubeToken",
        regex: r###"(?i)(?:squ_|sqp_|sqa_)[a-z0-9=_-]{40}"###,
    },
    SecretPattern {
        name: "travisciAccessToken",
        regex: r###"(?i)\b['"]?(?:travis)(?:[\s\w.-]{0,20})['"]?\s*(?::|=>|=)\s*['"]?(?P<secret>[a-z0-9]{22})['"]?\b"###,
    },
    SecretPattern {
        name: "codecovAccessToken",
        regex: r###"(?i)\b['"]?(?:codecov)(?:[\s\w.-]{0,20})['"]?\s*(?::|=>|=)\s*['"]?(?P<secret>[a-z0-9]{32})['"]?\b"###,
    },
    SecretPattern {
        name: "droneCiAccessToken",
        regex: r###"(?i)\b['"]?(?:droneci|drone)(?:[\s\w.-]{0,20})['"]?\s*(?::|=>|=)\s*['"]?(?P<secret>[a-z0-9]{32})['"]?\b"###,
    },
    SecretPattern {
        name: "octopusDeployApiKey",
        regex: r###"\bAPI-[A-Z0-9]{26}\b"###,
    },
    SecretPattern {
        name: "circleciToken",
        regex: r###"(?i)\b['"]?(?:circleci|circle)(?:[\s\w.-]{0,20})['"]?\s*(?::|=>|=)\s*['"]?(?P<secret>[a-f0-9]{40})['"]?\b"###,
    },
    SecretPattern {
        name: "buildkiteAgentToken",
        regex: r###"bkagent_[a-f0-9]{40}"###,
    },
    SecretPattern {
        name: "launchdarklyAccessToken",
        regex: r###"(?i)\b['"]?(?:launchdarkly)(?:[\s\w.-]{0,20})['"]?\s*(?::|=>|=)\s*['"]?(?P<secret>[a-z0-9=_-]{40})['"]?\b"###,
    },
    SecretPattern {
        name: "algoliaApiKey",
        regex: r###"(?i)\b['"]?(?:algolia)(?:[\s\w.-]{0,20})['"]?\s*(?::|=>|=)\s*['"]?(?P<secret>[a-z0-9]{32})['"]?\b"###,
    },
    SecretPattern {
        name: "launchdarklySdkKey",
        regex: r###"\bsdk-[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}\b"###,
    },
    SecretPattern {
        name: "vercelOidcToken",
        regex: r###"\b['"]?(?:VERCEL_OIDC_TOKEN)['"]?\s*(?::|=>|=)\s*['"]?(?P<secret>eyJ[a-zA-Z0-9_-]{100,})['"]?\b"###,
    },
    SecretPattern {
        name: "novuApiKey",
        regex: r###"(?i)\b['"]?(?:NOVU|novu)_?(?:API|api)?_?(?:KEY|key)['"]?\s*(?::|=>|=)\s*['"]?(?P<secret>[a-zA-Z0-9]{32,})['"]?\b"###,
    },
    SecretPattern {
        name: "triggerDevApiKey",
        regex: r###"\btr_(?:dev|prod)_[a-zA-Z0-9]{20,}\b"###,
    },
    SecretPattern {
        name: "nxCloudAccessToken",
        regex: r###"\b['"]?(?:NX_CLOUD_ACCESS_TOKEN|nxCloudAccessToken)['"]?\s*(?::|=>|=)\s*['"]?(?P<secret>[a-zA-Z0-9]{36,})['"]?\b"###,
    },
    SecretPattern {
        name: "depotToken",
        regex: r###"dpt_[a-zA-Z0-9]{40,}\b"###,
    },
    SecretPattern {
        name: "grafbaseApiKey",
        regex: r###"(?i)\b['"]?(?:GRAFBASE|grafbase)_?(?:API|api)?_?(?:KEY|key)['"]?\s*(?::|=>|=)\s*['"]?(?P<secret>eyJ[a-zA-Z0-9_-]{50,})['"]?\b"###,
    },
    SecretPattern {
        name: "shopifyStorefrontAccessToken",
        regex: r###"shpatf_[0-9a-f]{32}"###,
    },
    SecretPattern {
        name: "woocommerceConsumerKey",
        regex: r###"\bck_[a-f0-9]{40}\b"###,
    },
    SecretPattern {
        name: "woocommerceConsumerSecret",
        regex: r###"\bcs_[a-f0-9]{40}\b"###,
    },
    SecretPattern {
        name: "contentfulAccessToken",
        regex: r###"CFPAT-[0-9a-zA-Z]{20}"###,
    },
    SecretPattern {
        name: "mailchimpEcommerceApiKey",
        regex: r###"\b[0-9a-f]{32}-[a-z]{2,3}[0-9]{1,2}\b"###,
    },
    SecretPattern {
        name: "dockerConfigToken",
        // A Docker config's token fields: the identity token `docker login`
        // keeps for a registry's identity provider, and a registry token.
        regex: r###""(?:identitytoken|registrytoken)"\s*:\s*"(?P<secret>[^"\\\s]+)""###,
    },
    SecretPattern {
        name: "dockerConfigAuth",
        // The `auth` field of a Docker config's `auths` entry: base64 of
        // `user:password`. A regex anchors only the first entry that holds
        // one: each match ends at its secret, so a later entry of the same
        // `auths` has no anchor of its own.
        regex: r###""auths"\s*:\s*\{(?:\s*"[^"\\]*"\s*:\s*\{[^{}]*\}\s*,)*?\s*"[^"\\]*"\s*:\s*\{[^{}]*?"auth"\s*:\s*"(?P<secret>[A-Za-z0-9+/]+={0,2})""###,
    },
    SecretPattern {
        name: "kubeconfigIdToken",
        // The `id-token` of a kubeconfig user's `auth-provider` block: the
        // lines indented under `auth-provider:` or blank, up to the field.
        regex: r###"auth-provider:[ \t]*\r?\n(?:(?:[ \t][^\r\n]*)?\r?\n)*?[ \t]+id-token:[ \t]*["']?(?P<secret>[^\s"'|>][^\s"']*)"###,
    },
    SecretPattern {
        name: "kubeconfigRefreshToken",
        // The `refresh-token` of the same block, a pattern of its own: a
        // match ends at its secret, so one pattern finds one field a block.
        regex: r###"auth-provider:[ \t]*\r?\n(?:(?:[ \t][^\r\n]*)?\r?\n)*?[ \t]+refresh-token:[ \t]*["']?(?P<secret>[^\s"'|>][^\s"']*)"###,
    },
    SecretPattern {
        name: "cratesIoToken",
        // The crates.io registry token (cargo's `credentials.toml`): `cio`
        // and 32 letters and digits.
        regex: r###"\bcio[A-Za-z0-9]{32}\b"###,
    },
    SecretPattern {
        name: "terraformApiToken",
        // The Terraform Cloud and Enterprise API token
        // (`credentials.tfrc.json`): 14 letters and digits, `.atlasv1.`, then
        // 60 to 70 characters of the URL-safe alphabet.
        regex: r###"[A-Za-z0-9]{14}\.atlasv1\.[A-Za-z0-9_=-]{60,70}"###,
    },
    SecretPattern {
        name: "credentialsInUrl",
        // Any scheme's userinfo password, whatever the user: empty
        // (`redis://:pw@host`) or short (`amqp://u:pw@host`). A host holds
        // no backslash: a literal's `\n` or `\t` escape ends it, so the next
        // URL is a match of its own.
        regex: concat!(
            r###"\b[a-zA-Z]{3,10}:\/\/[^\\/\s:@]{0,64}:"###,
            url_password!(r###"[^\\/\s:@]{1,128}"###),
            r###"[^\s'"\\]+\b"###
        ),
    },
];
