//! Observability, payment, commerce and crypto exchange keys.
//!
//! A contiguous slice of [`super::PATTERNS`]: its order is part of the
//! evaluation order.

use super::SecretPattern;

pub(super) const PATTERNS: &[SecretPattern] = &[
    SecretPattern {
        name: "mapboxSecretToken",
        regex: r###"sk\.eyJ[a-zA-Z0-9._-]{87}"###,
    },
    SecretPattern {
        name: "grafanaCloudApiKey",
        regex: r###"glc_[a-zA-Z0-9]{32}"###,
    },
    SecretPattern {
        name: "newRelicApiKey",
        regex: r###"NRAK-[A-Z0-9]{27}"###,
    },
    SecretPattern {
        name: "newRelicInsightKey",
        regex: r###"NRIK-[A-Z0-9]{32}"###,
    },
    SecretPattern {
        name: "newRelicInsertKey",
        regex: r###"(?i)NRII-[a-z0-9-]{32}"###,
    },
    SecretPattern {
        name: "grafanaApiKey",
        regex: r###"(?i)\beyJrIjoi[A-Za-z0-9]{70,400}={0,3}\b"###,
    },
    SecretPattern {
        name: "grafanaServiceAccountToken",
        regex: r###"glsa_[A-Za-z0-9]{32}_[A-Fa-f0-9]{8}"###,
    },
    SecretPattern {
        name: "sentryOrgToken",
        regex: r###"sntrys_eyJpYXQiO[a-zA-Z0-9+/]{10,200}(?:LCJyZWdpb25fdXJs|InJlZ2lvbl91cmwi|cmVnaW9uX3VybCI6)[a-zA-Z0-9+/]{10,200}={0,2}_[a-zA-Z0-9+/]{43}"###,
    },
    SecretPattern {
        name: "sentryUserToken",
        regex: r###"sntryu_[a-f0-9]{64}"###,
    },
    SecretPattern {
        name: "sumoLogicAccessId",
        regex: r###"(?i)\b['"]?(?:sumo)(?:[\s\w.-]{0,20})['"]?\s*(?::|=>|=)\s*['"]?(?P<secret>su[a-zA-Z0-9]{12})['"]?\b"###,
    },
    SecretPattern {
        name: "splunkApiToken",
        regex: r###"(?i)\b['"]?(?:splunk)(?:[\s\w.-]{0,20})['"]?\s*(?::|=>|=)\s*['"]?(?P<secret>[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12})['"]?\b"###,
    },
    SecretPattern {
        name: "logdnaApiKey",
        regex: r###"(?i)\b['"]?(?:logdna|mezmo)(?:[\s\w.-]{0,20})['"]?\s*(?::|=>|=)\s*['"]?(?P<secret>[a-f0-9]{32})['"]?\b"###,
    },
    SecretPattern {
        name: "logglyToken",
        regex: r###"(?i)\b['"]?(?:loggly)(?:[\s\w.-]{0,20})['"]?\s*(?::|=>|=)\s*['"]?(?P<secret>[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12})['"]?\b"###,
    },
    SecretPattern {
        name: "stripeSecretKey",
        // A live key opens with its tag; a test key's `rk_test_` also ends
        // identifiers (`network_test_…`), so it keeps the boundary.
        regex: r###"[rs]k_live_[a-zA-Z0-9]{20,247}|\b[rs]k_test_[a-zA-Z0-9]{20,247}"###,
    },
    SecretPattern {
        name: "stripeWebhookSecret",
        regex: r###"whsec_[a-zA-Z0-9]{32,}\b"###,
    },
    SecretPattern {
        name: "paypalAccessToken",
        regex: r###"\bA21AA[a-zA-Z0-9_-]{50,}\b"###,
    },
    SecretPattern {
        name: "paypalBraintreeAccessToken",
        regex: r###"\baccess_token\$(?:production|sandbox)\$[0-9a-z]{16}\$[0-9a-f]{32}\b"###,
    },
    SecretPattern {
        name: "squareAccessToken",
        regex: r###"\b(?:EAAAE[A-Za-z0-9_-]{94,}|sq0[a-z]?atp-[0-9A-Za-z\-_]{22,26})\b"###,
    },
    SecretPattern {
        name: "squareOauthSecret",
        regex: r###"sq0csp-[0-9A-Za-z\-_]{43}"###,
    },
    SecretPattern {
        name: "shopifyPrivateAppPassword",
        regex: r###"shppa_[a-fA-F0-9]{32}"###,
    },
    SecretPattern {
        name: "shopifyAccessToken",
        regex: r###"shpat_[a-fA-F0-9]{32}"###,
    },
    SecretPattern {
        name: "shopifyWebhookToken",
        regex: r###"shpwh_[a-fA-F0-9]{32}"###,
    },
    SecretPattern {
        name: "adyenApiKey",
        regex: r###"\bAQE[a-zA-Z0-9]{70,}\b"###,
    },
    SecretPattern {
        name: "flutterwaveKeys",
        regex: r###"FLWSECK_(?:TEST|LIVE)-[a-h0-9]{32}-X\b"###,
    },
    SecretPattern {
        name: "coinbaseAccessToken",
        regex: r###"(?i)\b['"]?(?:coinbase)(?:[\s\w.-]{0,20})['"]?\s*(?::|=>|=)\s*['"]?(?P<secret>[a-z0-9_-]{64})['"]?\b"###,
    },
    SecretPattern {
        name: "krakenAccessToken",
        regex: r###"(?i)\b['"]?(?:kraken)(?:[\s\w.-]{0,20})['"]?\s*(?::|=>|=)\s*['"]?(?P<secret>[a-z0-9/=_+-]{80,90})['"]?\b"###,
    },
    SecretPattern {
        name: "kucoinAccessToken",
        regex: r###"(?i)\b['"]?(?:kucoin)(?:[\s\w.-]{0,20})['"]?\s*(?::|=>|=)\s*['"]?(?P<secret>[a-f0-9]{24})['"]?\b"###,
    },
    SecretPattern {
        name: "kucoinSecretKey",
        regex: r###"(?i)\b['"]?(?:kucoin)(?:[\s\w.-]{0,20})['"]?\s*(?::|=>|=)\s*['"]?(?P<secret>[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12})['"]?\b"###,
    },
    SecretPattern {
        name: "bittrexAccessKey",
        regex: r###"(?i)\b['"]?(?:bittrex)(?:[\s\w.-]{0,20})['"]?\s*(?::|=>|=)\s*['"]?(?P<secret>[a-z0-9]{32})['"]?\b"###,
    },
    SecretPattern {
        name: "binanceApiKey",
        regex: r###"(?i)\b['"]?(?:binance)(?:[\s\w.-]{0,20})['"]?\s*(?::|=>|=)\s*['"]?(?P<secret>[A-Za-z0-9]{64})['"]?\b"###,
    },
    SecretPattern {
        name: "bybitApiKey",
        regex: r###"(?i)\b['"]?(?:bybit)(?:[\s\w.-]{0,20})['"]?\s*(?::|=>|=)\s*['"]?(?P<secret>[A-Za-z0-9]{18,24})['"]?\b"###,
    },
    SecretPattern {
        name: "gocardlessApiToken",
        regex: r###"(?i)\blive_[a-z0-9\-_=]{40}\b"###,
    },
    SecretPattern {
        name: "plaidApiToken",
        regex: r###"\baccess-(?:sandbox|development|production)-[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}\b"###,
    },
    SecretPattern {
        name: "lemonSqueezyApiKey",
        regex: r###"(?i)\b['"]?(?:LEMONSQUEEZY|LEMON_SQUEEZY|lemonsqueezy)_?(?:API|api)?_?(?:KEY|key)['"]?\s*(?::|=>|=)\s*['"]?(?P<secret>eyJ[a-zA-Z0-9_-]{100,})['"]?\b"###,
    },
    SecretPattern {
        name: "paddleApiKey",
        regex: r###"(?i)\b['"]?(?:PADDLE|paddle)_?(?:API|api)?_?(?:KEY|key)['"]?\s*(?::|=>|=)\s*['"]?(?P<secret>pdl_(?:live|sdbx)_[a-zA-Z0-9]{40,})['"]?\b"###,
    },
];
