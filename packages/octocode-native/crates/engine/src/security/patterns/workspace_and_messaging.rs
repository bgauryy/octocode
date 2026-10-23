//! PEM and PGP key blocks, shipping, project-management, chat and social
//! tokens.
//!
//! A contiguous slice of [`super::PATTERNS`]: its order is part of the
//! evaluation order.

use super::SecretPattern;

pub(super) const PATTERNS: &[SecretPattern] = &[
    SecretPattern {
        name: "privateKeyPem",
        file_context: None,
        regex: r###"-----BEGIN\s+(?:(?:RSA|DSA|EC|OPENSSH|ENCRYPTED)\s+)?PRIVATE\s+KEY(?:\s+BLOCK)?-----[\s\S]*?-----END\s+(?:(?:RSA|DSA|EC|OPENSSH|ENCRYPTED)\s+)?PRIVATE\s+KEY(?:\s+BLOCK)?-----"###,
    },
    SecretPattern {
        name: "pgpPrivateKeyBlock",
        file_context: None,
        regex: r###"-----BEGIN\s+PGP\s+PRIVATE\s+KEY\s+BLOCK-----[\s\S]*?-----END\s+PGP\s+PRIVATE\s+KEY\s+BLOCK-----"###,
    },
    SecretPattern {
        name: "shippoApiToken",
        file_context: None,
        regex: r###"\bshippo_(?:live|test)_[a-fA-F0-9]{40}\b"###,
    },
    SecretPattern {
        name: "easypostApiToken",
        file_context: None,
        regex: r###"(?i)\bEZAK[a-z0-9]{54}\b"###,
    },
    SecretPattern {
        name: "easypostTestApiToken",
        file_context: None,
        regex: r###"(?i)\bEZTK[a-z0-9]{54}\b"###,
    },
    SecretPattern {
        name: "duffelApiToken",
        file_context: None,
        regex: r###"(?i)\bduffel_(?:test|live)_[a-z0-9_\-=]{43}\b"###,
    },
    SecretPattern {
        name: "frameioApiToken",
        file_context: None,
        regex: r###"(?i)\bfio-u-[a-z0-9\-_=]{64}\b"###,
    },
    SecretPattern {
        name: "maxmindLicenseKey",
        file_context: None,
        regex: r###"\b[A-Za-z0-9]{6}_[A-Za-z0-9]{29}_mmk\b"###,
    },
    SecretPattern {
        name: "asanaPersonalAccessToken",
        file_context: None,
        regex: r###"(?i)\b['"]?(?:asana)(?:[\s\w.-]{0,20})['"]?\s*(?::|=>|=)\s*['"]?(?P<secret>[0-9]{16})['"]?\b"###,
    },
    SecretPattern {
        name: "mondayApiToken",
        file_context: None,
        regex: r###"(?i)\b['"]?(?:monday)(?:[\s\w.-]{0,20})['"]?\s*(?::|=>|=)\s*['"]?(?P<secret>eyJ[a-zA-Z0-9_-]{100,})['"]?\b"###,
    },
    SecretPattern {
        name: "trelloApiKey",
        file_context: None,
        regex: r###"(?i)\b['"]?(?:trello)(?:[\s\w.-]{0,20})['"]?\s*(?::|=>|=)\s*['"]?(?P<secret>[a-f0-9]{32})['"]?\b"###,
    },
    SecretPattern {
        name: "jiraApiToken",
        file_context: None,
        regex: r###"(?i)\b['"]?(?:jira)(?:[\s\w.-]{0,20})['"]?\s*(?::|=>|=)\s*['"]?(?P<secret>[a-zA-Z0-9]{24})['"]?\b"###,
    },
    SecretPattern {
        name: "settlemintApplicationAccessToken",
        file_context: None,
        regex: r###"\bsm_aat_[a-zA-Z0-9]{16}\b"###,
    },
    SecretPattern {
        name: "settlemintPersonalAccessToken",
        file_context: None,
        regex: r###"\bsm_pat_[a-zA-Z0-9]{16}\b"###,
    },
    SecretPattern {
        name: "settlemintServiceAccessToken",
        file_context: None,
        regex: r###"\bsm_sat_[a-zA-Z0-9]{16}\b"###,
    },
    SecretPattern {
        name: "slackBotToken",
        file_context: None,
        regex: r###"\bxoxb-[0-9]{10,13}-[0-9]{10,13}[a-zA-Z0-9-]*\b"###,
    },
    SecretPattern {
        name: "slackUserToken",
        file_context: None,
        regex: r###"\bxoxp-[0-9]{10,13}-[0-9]{10,13}[a-zA-Z0-9-]*\b"###,
    },
    SecretPattern {
        name: "slackWorkspaceToken",
        file_context: None,
        regex: r###"\bxoxa-[0-9]{10,13}-[0-9]{10,13}[a-zA-Z0-9-]*\b"###,
    },
    SecretPattern {
        name: "slackRefreshToken",
        file_context: None,
        regex: r###"\bxoxr-[0-9]{10,13}-[0-9]{10,13}[a-zA-Z0-9-]*\b"###,
    },
    SecretPattern {
        name: "slackWebhookUrl",
        file_context: None,
        regex: r###"(?i)(?:https?:\/\/)?hooks\.slack\.com\/(?:services|workflows|triggers)\/[A-Za-z0-9+/]{43,56}"###,
    },
    SecretPattern {
        name: "slackWebhookUrlClassic",
        file_context: Some("(?:\\.env|config|settings|secrets)"),
        regex: r###"\bhttps:\/\/hooks\.slack\.com\/services\/[A-Z0-9]{8,12}\/[A-Z0-9]{8,12}\/[A-Za-z0-9]{20,32}\b"###,
    },
    SecretPattern {
        name: "slackAppToken",
        file_context: None,
        regex: r###"(?i)\bxapp-\d-[A-Z0-9]+-\d+-[a-z0-9]+\b"###,
    },
    SecretPattern {
        name: "slackConfigAccessToken",
        file_context: None,
        regex: r###"(?i)\bxoxe\.xox[bp]-\d-[A-Z0-9]{163,166}\b"###,
    },
    SecretPattern {
        name: "sendbirdAccessToken",
        file_context: None,
        regex: r###"(?i)\b['"]?(?:sendbird)(?:[\s\w.-]{0,20})['"]?\s*(?::|=>|=)\s*['"]?(?P<secret>[a-f0-9]{40})['"]?\b"###,
    },
    SecretPattern {
        name: "messagebirdApiToken",
        file_context: None,
        regex: r###"(?i)\b['"]?(?:messagebird|message_bird|message-bird)(?:[\s\w.-]{0,20})['"]?\s*(?::|=>|=)\s*['"]?(?P<secret>[a-z0-9]{25})['"]?\b"###,
    },
    SecretPattern {
        name: "mattermostAccessToken",
        file_context: None,
        regex: r###"(?i)\b['"]?(?:mattermost)(?:[\s\w.-]{0,20})['"]?\s*(?::|=>|=)\s*['"]?(?P<secret>[a-z0-9]{26})['"]?\b"###,
    },
    SecretPattern {
        name: "zendeskSecretKey",
        file_context: None,
        regex: r###"(?i)\b['"]?(?:zendesk)(?:[\s\w.-]{0,20})['"]?\s*(?::|=>|=)\s*['"]?(?P<secret>[a-z0-9]{40})['"]?\b"###,
    },
    SecretPattern {
        name: "freshdeskApiKey",
        file_context: None,
        regex: r###"(?i)\b['"]?(?:freshdesk)(?:[\s\w.-]{0,20})['"]?\s*(?::|=>|=)\s*['"]?(?P<secret>[a-zA-Z0-9]{20})['"]?\b"###,
    },
    SecretPattern {
        name: "sendinblueApiToken",
        file_context: None,
        regex: r###"\bxkeysib-[a-f0-9]{64}-[a-z0-9]{16}\b"###,
    },
    SecretPattern {
        name: "pusherAppSecret",
        file_context: None,
        regex: r###"(?i)\b['"]?(?:PUSHER|pusher)_?(?:APP|app)?_?(?:SECRET|secret)['"]?\s*(?::|=>|=)\s*['"]?(?P<secret>[a-f0-9]{20})['"]?\b"###,
    },
    SecretPattern {
        name: "streamApiSecret",
        file_context: None,
        regex: r###"(?i)\b['"]?(?:STREAM|stream|GETSTREAM)_?(?:API|api)?_?(?:SECRET|secret|KEY|key)['"]?\s*(?::|=>|=)\s*['"]?(?P<secret>[a-z0-9]{40,})['"]?\b"###,
    },
    SecretPattern {
        name: "postmarkServerToken",
        file_context: Some("postmark"),
        regex: r###"\b[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}\b"###,
    },
    SecretPattern {
        name: "vonageApiSecret",
        file_context: None,
        regex: r###"(?i)\b['"]?(?:VONAGE|NEXMO|vonage|nexmo)_?(?:API|api)?_?(?:SECRET|secret)['"]?\s*(?::|=>|=)\s*['"]?(?P<secret>[a-zA-Z0-9]{16})['"]?\b"###,
    },
    SecretPattern {
        name: "customerIoApiKey",
        file_context: None,
        regex: r###"(?i)\b['"]?(?:CUSTOMERIO|customer_io|CUSTOMER_IO)_?(?:API|api)?_?(?:KEY|key)['"]?\s*(?::|=>|=)\s*['"]?(?P<secret>[a-f0-9]{32,})['"]?\b"###,
    },
    SecretPattern {
        name: "twitterBearerToken",
        file_context: None,
        regex: r###"\bAAAAAAAAAAAAAAAAAAAAA[a-zA-Z0-9%]{50,}\b"###,
    },
    SecretPattern {
        name: "facebookAccessToken",
        file_context: None,
        regex: r###"\bEAA[a-zA-Z0-9]{80,120}\b"###,
    },
    SecretPattern {
        name: "facebookPageAccessToken",
        file_context: None,
        regex: r###"\bEAAB[a-zA-Z0-9+/]{100,}\b"###,
    },
    SecretPattern {
        name: "instagramAccessToken",
        file_context: None,
        regex: r###"\bIGQV[a-zA-Z0-9_-]{100,}\b"###,
    },
    SecretPattern {
        name: "discordSocialBotToken",
        file_context: None,
        regex: r###"\b[MN][A-Za-z\d]{23}\.[A-Za-z\d\-_]{6}\.[A-Za-z\d\-_]{27}\b"###,
    },
    SecretPattern {
        name: "discordSocialWebhookUrl",
        file_context: None,
        regex: r###"\bhttps:\/\/discord(?:app)?\.com\/api\/webhooks\/[0-9]{17,19}\/[A-Za-z0-9_-]{68}\b"###,
    },
    SecretPattern {
        name: "pinterestAccessToken",
        file_context: None,
        regex: r###"\bpina_[a-zA-Z0-9]{32}\b"###,
    },
    SecretPattern {
        name: "linkedinApiToken",
        file_context: None,
        regex: r###"(?i)\b['"]?(?:linkedin|linked_in|linked-in)(?:[\s\w.-]{0,20})['"]?\s*(?::|=>|=)\s*['"]?(?P<secret>[a-z0-9]{14,16})['"]?\b"###,
    },
    SecretPattern {
        name: "youtubeApiKey",
        file_context: None,
        regex: r###"(?i)\b['"]?(?:youtube)(?:[\s\w.-]{0,20})['"]?\s*(?::|=>|=)\s*['"]?(?P<secret>AIza[a-zA-Z0-9_-]{35})['"]?\b"###,
    },
    SecretPattern {
        name: "tiktokApiToken",
        file_context: None,
        regex: r###"(?i)\b['"]?(?:tiktok)(?:[\s\w.-]{0,20})['"]?\s*(?::|=>|=)\s*['"]?(?P<secret>[a-zA-Z0-9_-]{40,})['"]?\b"###,
    },
];
