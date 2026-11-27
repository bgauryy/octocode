//! Email, messaging, design and hosting platform tokens.
//!
//! A contiguous slice of [`super::PATTERNS`]: its order is part of the
//! evaluation order.

use super::SecretPattern;

pub(super) const PATTERNS: &[SecretPattern] = &[
    SecretPattern {
        name: "sendgridApiKey",
        regex: r###"\bSG\.[A-Za-z0-9_-]{20,22}\.[A-Za-z0-9_-]{43}\b"###,
    },
    SecretPattern {
        name: "mailgunApiKey",
        regex: r###"\bkey-[0-9a-z]{32}\b"###,
    },
    SecretPattern {
        name: "mailchimpApiKey",
        regex: r###"\b[0-9a-f]{32}-us[0-9]{1,2}\b"###,
    },
    SecretPattern {
        name: "telegramBotToken",
        regex: r###"\b[0-9]{8,10}:[A-Za-z0-9_-]{35}\b"###,
    },
    SecretPattern {
        name: "twilioApiKey",
        regex: r###"\bSK[a-z0-9]{32}\b"###,
    },
    SecretPattern {
        name: "dockerHubToken",
        regex: r###"dckr_pat_[a-zA-Z0-9_]{36}"###,
    },
    SecretPattern {
        name: "pypiApiToken",
        // A macaroon of any length: its caveats lengthen it past the
        // shortest token.
        regex: r###"pypi-[a-zA-Z0-9_-]{84,}"###,
    },
    SecretPattern {
        name: "figmaToken",
        regex: r###"figd_[a-zA-Z0-9_-]{43}"###,
    },
    SecretPattern {
        name: "renderToken",
        regex: r###"\brnd_[a-zA-Z0-9_-]{43}\b"###,
    },
    SecretPattern {
        name: "airtablePersonalAccessToken",
        regex: r###"\bpat[a-zA-Z0-9]{14}\.[a-zA-Z0-9]{64}\b"###,
    },
    SecretPattern {
        name: "typeformToken",
        regex: r###"tfp_[a-zA-Z0-9_-]{43}"###,
    },
    SecretPattern {
        name: "intercomAccessToken",
        regex: r###"\bdG9rOi[a-zA-Z0-9+/]{46,48}={0,2}\b"###,
    },
    SecretPattern {
        name: "digitalOceanToken",
        regex: r###"dop_v1_[a-f0-9]{64}"###,
    },
    SecretPattern {
        name: "digitalOceanOAuthToken",
        regex: r###"doo_v1_[a-f0-9]{64}"###,
    },
    SecretPattern {
        name: "digitalOceanRefreshToken",
        regex: r###"dor_v1_[a-f0-9]{64}"###,
    },
    SecretPattern {
        name: "cloudflareApiKey",
        regex: r###"(?i)\b['"]?(?:cloudflare)(?:[\s\w.-]{0,20})['"]?\s*(?::|=>|=)\s*['"]?(?P<secret>[a-z0-9_-]{40})['"]?\b"###,
    },
    SecretPattern {
        name: "cloudflareGlobalApiKey",
        regex: r###"(?i)\b['"]?(?:cloudflare)(?:[\s\w.-]{0,20})['"]?\s*(?::|=>|=)\s*['"]?(?P<secret>[a-f0-9]{37})['"]?\b"###,
    },
    SecretPattern {
        name: "cloudflareOriginCaKey",
        regex: r###"v1\.0-[a-f0-9]{24}-[a-f0-9]{146}"###,
    },
    SecretPattern {
        name: "flyioAccessToken",
        regex: r###"fo1_[\w-]{43}"###,
    },
    SecretPattern {
        name: "flyioMachineToken",
        regex: r###"fm[12][ar]?_[a-zA-Z0-9+/]{100,}={0,3}"###,
    },
    SecretPattern {
        name: "dopplerApiToken",
        regex: r###"(?i)dp\.pt\.[a-z0-9]{43}"###,
    },
    SecretPattern {
        name: "dynatraceApiToken",
        regex: r###"(?i)dt0c01\.[a-z0-9]{24}\.[a-z0-9]{64}"###,
    },
    SecretPattern {
        name: "netlifyAccessToken",
        regex: r###"(?i)\b['"]?(?:netlify)(?:[\s\w.-]{0,20})['"]?\s*(?::|=>|=)\s*['"]?(?P<secret>[a-z0-9=_-]{40,46})['"]?\b"###,
    },
    SecretPattern {
        name: "scalingoApiToken",
        regex: r###"tk-us-[\w-]{48}"###,
    },
    SecretPattern {
        name: "infracostApiToken",
        regex: r###"\bico-[a-zA-Z0-9]{32}\b"###,
    },
    SecretPattern {
        name: "harnessApiKey",
        regex: r###"\b(?:pat|sat)\.[a-zA-Z0-9_-]{22}\.[a-zA-Z0-9]{24}\.[a-zA-Z0-9]{20}\b"###,
    },
    SecretPattern {
        name: "azureAdClientSecret",
        // The delimiters around the secret are the text's own and stay.
        regex: r###"(?:^|[\\'"` \s>=:(,)])(?P<secret>[a-zA-Z0-9_~.]{3}\dQ~[a-zA-Z0-9_~.-]{31,34})(?:$|[\\'"` \s<),])"###,
    },
    SecretPattern {
        name: "herokuApiKeyV2",
        regex: r###"HRKU-AA[0-9a-zA-Z_-]{58}"###,
    },
    SecretPattern {
        name: "microsoftTeamsWebhook",
        regex: r###"(?i)https:\/\/[a-z0-9]+\.webhook\.office\.com\/webhookb2\/[a-z0-9]{8}-(?:[a-z0-9]{4}-){3}[a-z0-9]{12}@[a-z0-9]{8}-(?:[a-z0-9]{4}-){3}[a-z0-9]{12}\/IncomingWebhook\/[a-z0-9]{32}\/[a-z0-9]{8}-(?:[a-z0-9]{4}-){3}[a-z0-9]{12}"###,
    },
    SecretPattern {
        name: "oktaAccessToken",
        regex: r###"(?i)\b['"]?(?:okta)(?:[\s\w.-]{0,20})['"]?\s*(?::|=>|=)\s*['"]?(?P<secret>00[\w=-]{40})['"]?\b"###,
    },
    SecretPattern {
        name: "openshiftUserToken",
        regex: r###"sha256~[\w-]{43}"###,
    },
    SecretPattern {
        name: "denoDeployToken",
        regex: r###"ddp_[a-zA-Z0-9]{40}"###,
    },
    SecretPattern {
        name: "resendApiKey",
        regex: r###"\bre_[a-zA-Z0-9]{30,}\b"###,
    },
    SecretPattern {
        name: "azureOpenaiApiKey",
        regex: r###"(?i)\b['"]?(?:AZURE_OPENAI|azure_openai)_?(?:API|api)?_?(?:KEY|key)['"]?\s*(?::|=>|=)\s*['"]?(?P<secret>[a-f0-9]{32})['"]?\b"###,
    },
    SecretPattern {
        name: "railwayApiToken",
        regex: r###"(?i)\b['"]?(?:RAILWAY|railway)_?(?:API|api)?_?(?:TOKEN|token)['"]?\s*(?::|=>|=)\s*['"]?(?P<secret>[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12})['"]?\b"###,
    },
    SecretPattern {
        name: "convexDeployKey",
        regex: r###"\b(?:prod|dev):[a-zA-Z0-9_-]+:[a-zA-Z0-9_-]{40,}\b"###,
    },
    SecretPattern {
        name: "upstashKafkaCredentials",
        regex: r###"(?i)\b['"]?(?:UPSTASH_KAFKA)(?:[\s\w.-]{0,20})['"]?\s*(?::|=>|=)\s*['"]?(?P<secret>[a-zA-Z0-9=_-]{40,})['"]?\b"###,
    },
    SecretPattern {
        name: "cloudflareApiTokenPrefixed",
        regex: r###"cf(?:k|ut|at)_[a-zA-Z0-9]{40}[a-fA-F0-9]{8}"###,
    },
    SecretPattern {
        name: "googleOauthClientSecret",
        regex: r###"\bGOCSPX-[A-Za-z0-9_-]{28}"###,
    },
    SecretPattern {
        name: "dockerPersonalAccessToken",
        regex: r###"\bdckr_pat_[A-Za-z0-9_-]{27}"###,
    },
    SecretPattern {
        name: "githubAppInstallationTokenV1",
        regex: r###"\bv1\.[0-9a-f]{40}\b"###,
    },
    SecretPattern {
        name: "signedUrlSignature",
        // The signature a signed URL carries in its query (an Azure SAS
        // `sig`, an AWS or Google presigned URL's): whoever holds the URL
        // holds the grant.
        regex: r###"[?&](?i-u:sig|signature|x-amz-signature|x-goog-signature)=(?P<secret>[A-Za-z0-9%+/_-]{32,}={0,2})"###,
    },
    SecretPattern {
        name: "tokenInUrlUserinfo",
        // A URL whose userinfo is one opaque token and no password
        // (`https://<key>@host`, a Sentry DSN): `credentialsInUrl` reads the
        // `user:password` form.
        regex: r###"\b[a-zA-Z][a-zA-Z0-9+.-]{2,10}://(?P<secret>[A-Za-z0-9_~%+-]{20,128})@[^\s'"\\/@:]+"###,
    },
    SecretPattern {
        name: "sqlDriverDsnPassword",
        // A driver's data source name: Go's `user:password@tcp(host)/db`,
        // and libpq's keyword list, where `password=` sits beside the
        // keywords that place the server.
        regex: r###"\b[A-Za-z0-9_.-]{1,64}:(?P<secret>[^\s:@/"'`]{1,128})@(?:tcp|udp|unix)[46]?\(|\b(?:host|hostaddr|dbname|user|port|sslmode)=[^\s'"]+(?:[ \t]+[a-z_]+=[^\s'"]+)*?[ \t]+password=(?P<secret_after>[^\s'"]+)|\bpassword=(?P<secret_before>[^\s'"]+)[ \t]+(?:host|hostaddr|dbname|user|port|sslmode)="###,
    },
    SecretPattern {
        name: "passwordHash",
        // A stored password in the forms `crypt`, Apache and LDAP write:
        // enough to verify a guess offline.
        regex: r###"(?:\$apr1\$|\$[156y]\$)(?:rounds=[0-9]+\$)?[A-Za-z0-9./]{1,16}\$[A-Za-z0-9./]{22,86}|\$2[abxy]\$[0-9]{2}\$[A-Za-z0-9./]{53}|\{S?SHA(?:256|512)?\}[A-Za-z0-9+/]{27,}={0,2}"###,
    },
];
