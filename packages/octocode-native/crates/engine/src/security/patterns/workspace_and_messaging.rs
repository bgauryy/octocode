//! PEM and PGP key blocks, shipping, project-management, chat and social
//! tokens.
//!
//! A contiguous slice of [`super::PATTERNS`]: its order is part of the
//! evaluation order.

use super::SecretPattern;

pub(super) const PATTERNS: &[SecretPattern] = &[
    SecretPattern {
        name: "privateKeyPem",
        // The armor names the block and holds none of it: only the body
        // (`secret`, without the line breaks that end the armor lines) is
        // hidden. The body is key-shaped lines (`key_body!`): code between two
        // armor literals ends a match, and no match runs across another armor.
        regex: concat!(
            r###"-----BEGIN\s+(?:(?:RSA|DSA|EC|OPENSSH|ENCRYPTED)\s+)?PRIVATE\s+KEY(?:\s+BLOCK)?-----[ \t]*(?:\r?\n)?"###,
            armor_headers!(),
            r###"[ \t]*(?P<secret>"###,
            key_body!(),
            r###")(?:\r?\n)?[ \t]*-----END\s+(?:(?:RSA|DSA|EC|OPENSSH|ENCRYPTED)\s+)?PRIVATE\s+KEY(?:\s+BLOCK)?-----"###,
        ),
    },
    SecretPattern {
        name: "pgpPrivateKeyBlock",
        // The armor names the block and holds none of it: only the body
        // (`secret`, without the line breaks that end the armor lines) is
        // hidden. The body is key-shaped lines (`key_body!`): code between two
        // armor literals ends a match, and no match runs across another armor.
        regex: concat!(
            r###"-----BEGIN\s+PGP\s+PRIVATE\s+KEY\s+BLOCK-----[ \t]*(?:\r?\n)?"###,
            armor_headers!(),
            r###"[ \t]*(?P<secret>"###,
            key_body!(),
            r###")(?:\r?\n)?[ \t]*-----END\s+PGP\s+PRIVATE\s+KEY\s+BLOCK-----"###,
        ),
    },
    SecretPattern {
        name: "shippoApiToken",
        regex: r###"shippo_(?:live|test)_[a-fA-F0-9]{40}"###,
    },
    SecretPattern {
        name: "easypostApiToken",
        regex: r###"(?i)\bEZAK[a-z0-9]{54}\b"###,
    },
    SecretPattern {
        name: "easypostTestApiToken",
        regex: r###"(?i)\bEZTK[a-z0-9]{54}\b"###,
    },
    SecretPattern {
        name: "duffelApiToken",
        regex: r###"(?i)duffel_(?:test|live)_[a-z0-9_\-=]{43}"###,
    },
    SecretPattern {
        name: "frameioApiToken",
        regex: r###"(?i)fio-u-[a-z0-9\-_=]{64}"###,
    },
    SecretPattern {
        name: "maxmindLicenseKey",
        regex: r###"\b[A-Za-z0-9]{6}_[A-Za-z0-9]{29}_mmk\b"###,
    },
    SecretPattern {
        name: "asanaPersonalAccessToken",
        regex: r###"(?i)\b['"]?(?:asana)(?:[\s\w.-]{0,20})['"]?\s*(?::|=>|=)\s*['"]?(?P<secret>[0-9]{16})['"]?\b"###,
    },
    SecretPattern {
        name: "mondayApiToken",
        regex: r###"(?i)\b['"]?(?:monday)(?:[\s\w.-]{0,20})['"]?\s*(?::|=>|=)\s*['"]?(?P<secret>eyJ[a-zA-Z0-9_-]{100,})['"]?\b"###,
    },
    SecretPattern {
        name: "trelloApiKey",
        regex: r###"(?i)\b['"]?(?:trello)(?:[\s\w.-]{0,20})['"]?\s*(?::|=>|=)\s*['"]?(?P<secret>[a-f0-9]{32})['"]?\b"###,
    },
    SecretPattern {
        name: "jiraApiToken",
        regex: r###"(?i)\b['"]?(?:jira)(?:[\s\w.-]{0,20})['"]?\s*(?::|=>|=)\s*['"]?(?P<secret>[a-zA-Z0-9]{24})['"]?\b"###,
    },
    SecretPattern {
        name: "settlemintApplicationAccessToken",
        regex: r###"sm_aat_[a-zA-Z0-9]{16}"###,
    },
    SecretPattern {
        name: "settlemintPersonalAccessToken",
        regex: r###"sm_pat_[a-zA-Z0-9]{16}"###,
    },
    SecretPattern {
        name: "settlemintServiceAccessToken",
        regex: r###"sm_sat_[a-zA-Z0-9]{16}"###,
    },
    SecretPattern {
        name: "slackBotToken",
        // A workspace's bot token, and the older form with one id.
        regex: r###"xoxb-(?:[0-9]{10,13}-[0-9]{10,13}[a-zA-Z0-9-]*|[0-9]{8,14}-[a-zA-Z0-9]{18,26})\b"###,
    },
    SecretPattern {
        name: "slackUserToken",
        regex: r###"xoxp-[0-9]{10,13}-[0-9]{10,13}[a-zA-Z0-9-]*\b"###,
    },
    SecretPattern {
        name: "slackWorkspaceToken",
        regex: r###"xoxa-[0-9]{10,13}-[0-9]{10,13}[a-zA-Z0-9-]*\b"###,
    },
    SecretPattern {
        name: "slackRefreshToken",
        regex: r###"xoxr-[0-9]{10,13}-[0-9]{10,13}[a-zA-Z0-9-]*\b"###,
    },
    SecretPattern {
        name: "slackWebhookUrl",
        regex: r###"(?i)(?:https?:\/\/)?hooks\.slack\.com\/(?:services|workflows|triggers)\/[A-Za-z0-9+/]{43,56}"###,
    },
    SecretPattern {
        name: "slackWebhookUrlClassic",
        regex: r###"\bhttps:\/\/hooks\.slack\.com\/services\/[A-Z0-9]{8,12}\/[A-Z0-9]{8,12}\/[A-Za-z0-9]{20,32}\b"###,
    },
    SecretPattern {
        name: "slackAppToken",
        regex: r###"(?i)xapp-\d-[A-Z0-9]+-\d+-[a-z0-9]+\b"###,
    },
    SecretPattern {
        name: "slackConfigAccessToken",
        regex: r###"(?i)xoxe\.xox[bp]-\d-[A-Z0-9]{163,166}"###,
    },
    SecretPattern {
        name: "sendbirdAccessToken",
        regex: r###"(?i)\b['"]?(?:sendbird)(?:[\s\w.-]{0,20})['"]?\s*(?::|=>|=)\s*['"]?(?P<secret>[a-f0-9]{40})['"]?\b"###,
    },
    SecretPattern {
        name: "messagebirdApiToken",
        regex: r###"(?i)\b['"]?(?:messagebird|message_bird|message-bird)(?:[\s\w.-]{0,20})['"]?\s*(?::|=>|=)\s*['"]?(?P<secret>[a-z0-9]{25})['"]?\b"###,
    },
    SecretPattern {
        name: "mattermostAccessToken",
        regex: r###"(?i)\b['"]?(?:mattermost)(?:[\s\w.-]{0,20})['"]?\s*(?::|=>|=)\s*['"]?(?P<secret>[a-z0-9]{26})['"]?\b"###,
    },
    SecretPattern {
        name: "zendeskSecretKey",
        regex: r###"(?i)\b['"]?(?:zendesk)(?:[\s\w.-]{0,20})['"]?\s*(?::|=>|=)\s*['"]?(?P<secret>[a-z0-9]{40})['"]?\b"###,
    },
    SecretPattern {
        name: "freshdeskApiKey",
        regex: r###"(?i)\b['"]?(?:freshdesk)(?:[\s\w.-]{0,20})['"]?\s*(?::|=>|=)\s*['"]?(?P<secret>[a-zA-Z0-9]{20})['"]?\b"###,
    },
    SecretPattern {
        name: "sendinblueApiToken",
        regex: r###"xkeysib-[a-f0-9]{64}-[a-z0-9]{16}"###,
    },
    SecretPattern {
        name: "pusherAppSecret",
        regex: r###"(?i)\b['"]?(?:PUSHER|pusher)_?(?:APP|app)?_?(?:SECRET|secret)['"]?\s*(?::|=>|=)\s*['"]?(?P<secret>[a-f0-9]{20})['"]?\b"###,
    },
    SecretPattern {
        name: "streamApiSecret",
        regex: r###"(?i)\b['"]?(?:STREAM|stream|GETSTREAM)_?(?:API|api)?_?(?:SECRET|secret|KEY|key)['"]?\s*(?::|=>|=)\s*['"]?(?P<secret>[a-z0-9]{40,})['"]?\b"###,
    },
    SecretPattern {
        name: "vonageApiSecret",
        regex: r###"(?i)\b['"]?(?:VONAGE|NEXMO|vonage|nexmo)_?(?:API|api)?_?(?:SECRET|secret)['"]?\s*(?::|=>|=)\s*['"]?(?P<secret>[a-zA-Z0-9]{16})['"]?\b"###,
    },
    SecretPattern {
        name: "customerIoApiKey",
        regex: r###"(?i)\b['"]?(?:CUSTOMERIO|customer_io|CUSTOMER_IO)_?(?:API|api)?_?(?:KEY|key)['"]?\s*(?::|=>|=)\s*['"]?(?P<secret>[a-f0-9]{32,})['"]?\b"###,
    },
    SecretPattern {
        name: "twitterBearerToken",
        regex: r###"\bAAAAAAAAAAAAAAAAAAAAA[a-zA-Z0-9%]{50,}\b"###,
    },
    SecretPattern {
        name: "facebookAccessToken",
        // A long-lived token runs past two hundred characters.
        regex: r###"\bEAA[a-zA-Z0-9]{80,}\b"###,
    },
    SecretPattern {
        name: "facebookPageAccessToken",
        regex: r###"\bEAAB[a-zA-Z0-9+/]{100,}\b"###,
    },
    SecretPattern {
        name: "instagramAccessToken",
        regex: r###"\bIGQV[a-zA-Z0-9_-]{100,}\b"###,
    },
    SecretPattern {
        name: "discordSocialBotToken",
        // An id, a timestamp and a signature, each base64url, of the
        // lengths a bot's or a user's token has. The id is the base64 of a
        // decimal snowflake, so its first two characters encode a digit.
        regex: r###"\b(?:M[TDjz]|N[DTjz]|O[DT])[A-Za-z\d_-]{22,24}\.[A-Za-z\d\-_]{6}\.[A-Za-z\d\-_]{27,38}"###,
    },
    SecretPattern {
        name: "discordSocialWebhookUrl",
        regex: r###"\bhttps:\/\/discord(?:app)?\.com\/api\/webhooks\/[0-9]{17,19}\/[A-Za-z0-9_-]{68}\b"###,
    },
    SecretPattern {
        name: "pinterestAccessToken",
        regex: r###"pina_[a-zA-Z0-9]{32}"###,
    },
    SecretPattern {
        name: "linkedinApiToken",
        regex: r###"(?i)\b['"]?(?:linkedin|linked_in|linked-in)(?:[\s\w.-]{0,20})['"]?\s*(?::|=>|=)\s*['"]?(?P<secret>[a-z0-9]{14,16})['"]?\b"###,
    },
    SecretPattern {
        name: "youtubeApiKey",
        regex: r###"(?i)\b['"]?(?:youtube)(?:[\s\w.-]{0,20})['"]?\s*(?::|=>|=)\s*['"]?(?P<secret>AIza[a-zA-Z0-9_-]{35})['"]?\b"###,
    },
    SecretPattern {
        name: "tiktokApiToken",
        regex: r###"(?i)\b['"]?(?:tiktok)(?:[\s\w.-]{0,20})['"]?\s*(?::|=>|=)\s*['"]?(?P<secret>[a-zA-Z0-9_-]{40,})['"]?\b"###,
    },
];
