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
        name: "dockerRegistryAuth",
        file_context: None,
        // A text naming a Docker client config's `auths` map, as `config.json`
        // or a `dockerconfigjson` (held escaped in a string) writes it, under
        // any file name and in any case (Go's `encoding/json` reads `"Auths"`
        // too). `detector::document` hides the credentials of each registry's
        // entry and keeps the rest. The match starts at the quote, so its
        // prefix `"auths` gates it: backslashes before the quote change no text
        // it matches, and an optional lead would leave it no literal gate.
        regex: r###""(?i:auths)\\*"\s*:\s*\{"###,
    },
    SecretPattern {
        name: "pypiApiToken",
        file_context: None,
        // A macaroon of any length: its caveats lengthen it past the
        // shortest token.
        regex: r###"pypi-[a-zA-Z0-9_-]{84,}"###,
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
        // The delimiters around the secret are the text's own and stay.
        regex: r###"(?:^|[\\'"` \s>=:(,)])(?P<secret>[a-zA-Z0-9_~.]{3}\dQ~[a-zA-Z0-9_~.-]{31,34})(?:$|[\\'"` \s<),])"###,
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
    SecretPattern {
        name: "googleOauthClientSecret",
        file_context: None,
        regex: r###"\bGOCSPX-[A-Za-z0-9_-]{28}"###,
    },
    SecretPattern {
        name: "googleOauthRefreshToken",
        file_context: None,
        regex: r###"\b1//0[A-Za-z0-9_-]{40,}"###,
    },
    SecretPattern {
        name: "dockerPersonalAccessToken",
        file_context: None,
        regex: r###"\bdckr_pat_[A-Za-z0-9_-]{27}"###,
    },
    SecretPattern {
        name: "githubAppInstallationTokenV1",
        file_context: None,
        regex: r###"\bv1\.[0-9a-f]{40}\b"###,
    },
    SecretPattern {
        name: "signedUrlSignature",
        file_context: None,
        // The signature a signed URL carries in its query (an Azure SAS
        // `sig`, an AWS or Google presigned URL's): whoever holds the URL
        // holds the grant.
        regex: r###"[?&](?i-u:sig|signature|x-amz-signature|x-goog-signature)=(?P<secret>[A-Za-z0-9%+/_-]{32,}={0,2})"###,
    },
    SecretPattern {
        name: "tokenInUrlUserinfo",
        file_context: None,
        // A URL whose userinfo is one opaque token and no password
        // (`https://<key>@host`, a Sentry DSN): `credentialsInUrl` reads the
        // `user:password` form.
        regex: r###"\b[a-zA-Z][a-zA-Z0-9+.-]{2,10}://(?P<secret>[A-Za-z0-9_~%+-]{20,128})@[^\s'"\\/@:]+"###,
    },
    SecretPattern {
        name: "sqlDriverDsnPassword",
        file_context: None,
        // A driver's data source name: Go's `user:password@tcp(host)/db`,
        // and libpq's keyword list, where `password=` sits beside the
        // keywords that place the server.
        regex: r###"\b[A-Za-z0-9_.-]{1,64}:(?P<secret>[^\s:@/"'`]{1,128})@(?:tcp|udp|unix)[46]?\(|\b(?:host|hostaddr|dbname|user|port|sslmode)=[^\s'"]+(?:[ \t]+[a-z_]+=[^\s'"]+)*?[ \t]+password=(?P<secret_after>[^\s'"]+)|\bpassword=(?P<secret_before>[^\s'"]+)[ \t]+(?:host|hostaddr|dbname|user|port|sslmode)="###,
    },
    SecretPattern {
        name: "passFileLine",
        file_context: None,
        // A line of a password file wherever it is kept (libpq's `.pgpass`
        // under another name, pasted into notes or a script):
        // `host:port:database:user:password`, five fields between colons, the
        // port a number or `*`. Dates, addresses and log lines have the shape
        // too, so the last field is one opaque run (`opaque_chars`) of a
        // generated password's length, and the detector's opacity test
        // judges it (`ValueTest::Opaque`).
        regex: concat!(
            r###"(?m)^[ \t]*[A-Za-z0-9_.*/\-]+:(?:[0-9]{1,5}|\*):[^\s:]+:[^\s:]+:(?P<secret>["###,
            opaque_chars!(),
            r###"]{16,})[ \t]*\r?$"###
        ),
    },
    SecretPattern {
        name: "urlQueryCredential",
        file_context: None,
        // A credential a URL's query or a form body names (`?password=…`,
        // `&client_secret=…`). Which values count is the value test's
        // (`ValueTest::Chosen`).
        regex: r###"[?&](?i-u:password|passwd|pwd|pass|secret|client_secret|token|access_token|refresh_token|api_?key|auth)=(?P<secret>[^&\s"'`<>#\\]{6,})"###,
    },
    SecretPattern {
        name: "commandLinePassword",
        file_context: None,
        // The password a command takes as an argument: `curl -u user:…`,
        // `mysql -p…`, `sshpass -p …`, `docker login -p …`.
        regex: r###"\bcurl\b[^\r\n|;&]*?[ \t](?:-u|--user)[ \t=]+["']?[^\s:"']+:(?P<secret>[^\s"']+)|\bmysql(?:dump|admin|import|show)?\b[^\r\n|;&]*?[ \t]-p(?P<secret_glued>[^\s"'-][^\s"']*)|\bsshpass[ \t]+-p[ \t]*["']?(?P<secret_sshpass>[^\s"']+)|\bdocker[ \t]+login\b[^\r\n|;&]*?[ \t](?:-p|--password)[ \t=]+["']?(?P<secret_login>[^\s"']+)"###,
    },
    SecretPattern {
        name: "passwordHash",
        file_context: None,
        // A stored password in the forms `crypt`, Apache and LDAP write:
        // enough to verify a guess offline.
        regex: r###"(?:\$apr1\$|\$[156y]\$)(?:rounds=[0-9]+\$)?[A-Za-z0-9./]{1,16}\$[A-Za-z0-9./]{22,86}|\$2[abxy]\$[0-9]{2}\$[A-Za-z0-9./]{53}|\{S?SHA(?:256|512)?\}[A-Za-z0-9+/]{27,}={0,2}"###,
    },
];
