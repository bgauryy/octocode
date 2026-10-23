//! Observability, payment, commerce and crypto exchange keys.
//!
//! A contiguous slice of [`super::PATTERNS`]: its order is part of the
//! evaluation order.

use super::SecretPattern;

pub(super) const PATTERNS: &[SecretPattern] = &[
    SecretPattern {
        name: "mapboxSecretToken",
        file_context: None,
        regex: r###"\bsk\.eyJ[a-zA-Z0-9._-]{87}\b"###,
    },
    SecretPattern {
        name: "mapboxPublicToken",
        file_context: None,
        regex: r###"\bpk\.eyJ[a-zA-Z0-9._-]{80,}\b"###,
    },
    SecretPattern {
        name: "grafanaCloudApiKey",
        file_context: None,
        regex: r###"\bglc_[a-zA-Z0-9]{32}\b"###,
    },
    SecretPattern {
        name: "newRelicApiKey",
        file_context: None,
        regex: r###"\bNRAK-[A-Z0-9]{27}\b"###,
    },
    SecretPattern {
        name: "newRelicInsightKey",
        file_context: None,
        regex: r###"\bNRIK-[A-Z0-9]{32}\b"###,
    },
    SecretPattern {
        name: "newRelicBrowserApiToken",
        file_context: None,
        regex: r###"\bNRJS-[a-f0-9]{19}\b"###,
    },
    SecretPattern {
        name: "newRelicInsertKey",
        file_context: None,
        regex: r###"(?i)\bNRII-[a-z0-9-]{32}\b"###,
    },
    SecretPattern {
        name: "grafanaApiKey",
        file_context: None,
        regex: r###"(?i)\beyJrIjoi[A-Za-z0-9]{70,400}={0,3}\b"###,
    },
    SecretPattern {
        name: "grafanaServiceAccountToken",
        file_context: None,
        regex: r###"\bglsa_[A-Za-z0-9]{32}_[A-Fa-f0-9]{8}\b"###,
    },
    SecretPattern {
        name: "sentryOrgToken",
        file_context: None,
        regex: r###"\bsntrys_eyJpYXQiO[a-zA-Z0-9+/]{10,200}(?:LCJyZWdpb25fdXJs|InJlZ2lvbl91cmwi|cmVnaW9uX3VybCI6)[a-zA-Z0-9+/]{10,200}={0,2}_[a-zA-Z0-9+/]{43}\b"###,
    },
    SecretPattern {
        name: "sentryUserToken",
        file_context: None,
        regex: r###"\bsntryu_[a-f0-9]{64}\b"###,
    },
    SecretPattern {
        name: "sumoLogicAccessId",
        file_context: None,
        regex: r###"(?i)\b['"]?(?:sumo)(?:[\s\w.-]{0,20})['"]?\s*(?::|=>|=)\s*['"]?(?P<secret>su[a-zA-Z0-9]{12})['"]?\b"###,
    },
    SecretPattern {
        name: "splunkApiToken",
        file_context: None,
        regex: r###"(?i)\b['"]?(?:splunk)(?:[\s\w.-]{0,20})['"]?\s*(?::|=>|=)\s*['"]?(?P<secret>[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12})['"]?\b"###,
    },
    SecretPattern {
        name: "logdnaApiKey",
        file_context: None,
        regex: r###"(?i)\b['"]?(?:logdna|mezmo)(?:[\s\w.-]{0,20})['"]?\s*(?::|=>|=)\s*['"]?(?P<secret>[a-f0-9]{32})['"]?\b"###,
    },
    SecretPattern {
        name: "logglyToken",
        file_context: None,
        regex: r###"(?i)\b['"]?(?:loggly)(?:[\s\w.-]{0,20})['"]?\s*(?::|=>|=)\s*['"]?(?P<secret>[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12})['"]?\b"###,
    },
    SecretPattern {
        name: "stripeSecretKey",
        file_context: None,
        regex: r###"\b[rs]k_(?:live|test)_[a-zA-Z0-9]{20,247}\b"###,
    },
    SecretPattern {
        name: "stripeWebhookSecret",
        file_context: None,
        regex: r###"\bwhsec_[a-zA-Z0-9]{32,}\b"###,
    },
    SecretPattern {
        name: "stripePublishableKey",
        file_context: None,
        regex: r###"\bpk_(?:live|test)_[a-zA-Z0-9]{20,247}\b"###,
    },
    SecretPattern {
        name: "paypalAccessToken",
        file_context: None,
        regex: r###"\bA21AA[a-zA-Z0-9_-]{50,}\b"###,
    },
    SecretPattern {
        name: "paypalBraintreeAccessToken",
        file_context: None,
        regex: r###"\baccess_token\$(?:production|sandbox)\$[0-9a-z]{16}\$[0-9a-f]{32}\b"###,
    },
    SecretPattern {
        name: "squareAccessToken",
        file_context: None,
        regex: r###"\b(?:EAAAE[A-Za-z0-9_-]{94,}|sq0[a-z]?atp-[0-9A-Za-z\-_]{22,26})\b"###,
    },
    SecretPattern {
        name: "squareOauthSecret",
        file_context: None,
        regex: r###"\bsq0csp-[0-9A-Za-z\-_]{43}\b"###,
    },
    SecretPattern {
        name: "squareApplicationId",
        file_context: None,
        regex: r###"\bsq0ids-[a-zA-Z0-9_-]{43}\b"###,
    },
    SecretPattern {
        name: "shopifyPrivateAppPassword",
        file_context: None,
        regex: r###"\bshppa_[a-fA-F0-9]{32}\b"###,
    },
    SecretPattern {
        name: "shopifyAccessToken",
        file_context: None,
        regex: r###"\bshpat_[a-fA-F0-9]{32}\b"###,
    },
    SecretPattern {
        name: "shopifyWebhookToken",
        file_context: None,
        regex: r###"\bshpwh_[a-fA-F0-9]{32}\b"###,
    },
    SecretPattern {
        name: "adyenApiKey",
        file_context: None,
        regex: r###"\bAQE[a-zA-Z0-9]{70,}\b"###,
    },
    SecretPattern {
        name: "razorpayApiKey",
        file_context: None,
        regex: r###"\brzp_(?:test|live)_[a-zA-Z0-9]{14}\b"###,
    },
    SecretPattern {
        name: "flutterwaveKeys",
        file_context: None,
        regex: r###"\bFLW(?:PUBK|SECK)_(?:TEST|LIVE)-[a-h0-9]{32}-X\b"###,
    },
    SecretPattern {
        name: "coinbaseAccessToken",
        file_context: None,
        regex: r###"(?i)\b['"]?(?:coinbase)(?:[\s\w.-]{0,20})['"]?\s*(?::|=>|=)\s*['"]?(?P<secret>[a-z0-9_-]{64})['"]?\b"###,
    },
    SecretPattern {
        name: "krakenAccessToken",
        file_context: None,
        regex: r###"(?i)\b['"]?(?:kraken)(?:[\s\w.-]{0,20})['"]?\s*(?::|=>|=)\s*['"]?(?P<secret>[a-z0-9/=_+-]{80,90})['"]?\b"###,
    },
    SecretPattern {
        name: "kucoinAccessToken",
        file_context: None,
        regex: r###"(?i)\b['"]?(?:kucoin)(?:[\s\w.-]{0,20})['"]?\s*(?::|=>|=)\s*['"]?(?P<secret>[a-f0-9]{24})['"]?\b"###,
    },
    SecretPattern {
        name: "kucoinSecretKey",
        file_context: None,
        regex: r###"(?i)\b['"]?(?:kucoin)(?:[\s\w.-]{0,20})['"]?\s*(?::|=>|=)\s*['"]?(?P<secret>[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12})['"]?\b"###,
    },
    SecretPattern {
        name: "bittrexAccessKey",
        file_context: None,
        regex: r###"(?i)\b['"]?(?:bittrex)(?:[\s\w.-]{0,20})['"]?\s*(?::|=>|=)\s*['"]?(?P<secret>[a-z0-9]{32})['"]?\b"###,
    },
    SecretPattern {
        name: "binanceApiKey",
        file_context: None,
        regex: r###"(?i)\b['"]?(?:binance)(?:[\s\w.-]{0,20})['"]?\s*(?::|=>|=)\s*['"]?(?P<secret>[A-Za-z0-9]{64})['"]?\b"###,
    },
    SecretPattern {
        name: "bybitApiKey",
        file_context: None,
        regex: r###"(?i)\b['"]?(?:bybit)(?:[\s\w.-]{0,20})['"]?\s*(?::|=>|=)\s*['"]?(?P<secret>[A-Za-z0-9]{18,24})['"]?\b"###,
    },
    SecretPattern {
        name: "gocardlessApiToken",
        file_context: None,
        regex: r###"(?i)\blive_[a-z0-9\-_=]{40}\b"###,
    },
    SecretPattern {
        name: "plaidApiToken",
        file_context: None,
        regex: r###"\baccess-(?:sandbox|development|production)-[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}\b"###,
    },
    SecretPattern {
        name: "plaidClientId",
        file_context: None,
        regex: r###"(?i)\b['"]?(?:PLAID|plaid)_?(?:CLIENT|client)_?(?:ID|id)['"]?\s*(?::|=>|=)\s*['"]?(?P<secret>[a-f0-9]{24})['"]?\b"###,
    },
    SecretPattern {
        name: "lemonSqueezyApiKey",
        file_context: None,
        regex: r###"(?i)\b['"]?(?:LEMONSQUEEZY|LEMON_SQUEEZY|lemonsqueezy)_?(?:API|api)?_?(?:KEY|key)['"]?\s*(?::|=>|=)\s*['"]?(?P<secret>eyJ[a-zA-Z0-9_-]{100,})['"]?\b"###,
    },
    SecretPattern {
        name: "paddleApiKey",
        file_context: None,
        regex: r###"(?i)\b['"]?(?:PADDLE|paddle)_?(?:API|api)?_?(?:KEY|key)['"]?\s*(?::|=>|=)\s*['"]?(?P<secret>pdl_(?:live|sdbx)_[a-zA-Z0-9]{40,})['"]?\b"###,
    },
    SecretPattern {
        name: "mollieApiKey",
        file_context: Some("mollie"),
        regex: r###"\b(?:live|test)_[a-zA-Z0-9]{30,}\b"###,
    },
];
