//! AI and ML providers, product analytics and monitoring keys.
//!
//! A contiguous slice of [`super::PATTERNS`]: its order is part of the
//! evaluation order.

use super::SecretPattern;

pub(super) const PATTERNS: &[SecretPattern] = &[
    SecretPattern {
        name: "openaiApiKeyLegacy",
        regex: r###"(sk-[a-zA-Z0-9_-]+T3BlbkFJ[a-zA-Z0-9_-]+)\b"###,
    },
    SecretPattern {
        name: "openaiApiKeyClassic",
        regex: r###"\bsk-[a-zA-Z0-9]{40,}\b"###,
    },
    SecretPattern {
        name: "openaiProjectApiKey",
        regex: r###"\bsk-proj-[a-zA-Z0-9_-]{20,}\b"###,
    },
    SecretPattern {
        name: "openaiServiceAccountKey",
        regex: r###"sk-svcacct-[a-zA-Z0-9_-]{20,}\b"###,
    },
    SecretPattern {
        name: "openaiAdminKey",
        regex: r###"\bsk-admin-[a-zA-Z0-9_-]{20,}\b"###,
    },
    SecretPattern {
        name: "groqApiKey",
        regex: r###"gsk_[a-zA-Z0-9-_]{51,52}"###,
    },
    SecretPattern {
        name: "cohereApiKey",
        regex: r###"\bco-[a-zA-Z0-9-_]{38,64}\b"###,
    },
    SecretPattern {
        name: "huggingFaceToken",
        regex: r###"\bhf_[a-zA-Z0-9]{34}\b"###,
    },
    SecretPattern {
        name: "perplexityApiKey",
        regex: r###"pplx-[a-zA-Z0-9]{30,64}"###,
    },
    SecretPattern {
        name: "replicateApiToken",
        regex: r###"\br8_[a-zA-Z0-9]{30,}\b"###,
    },
    SecretPattern {
        name: "anthropicApiKey",
        regex: r###"sk-ant-(?:admin01|api03|sid01)-[\w-]{80,120}"###,
    },
    SecretPattern {
        name: "mistralApiKey",
        regex: r###"\b(?:mistral-|mist_)[a-zA-Z0-9]{32,}\b"###,
    },
    SecretPattern {
        name: "tavilyApiKey",
        regex: r###"tvly-[a-zA-Z0-9]{30,}\b"###,
    },
    SecretPattern {
        name: "deepseekApiKey",
        regex: r###"\b['"]?(?:DEEPSEEK|deepseek|DeepSeek)_?(?:API|api)?_?(?:KEY|key)['"]?\s*(?::|=>|=)\s*['"]?(?P<secret>sk-[a-zA-Z0-9]{32,64})['"]?\b"###,
    },
    SecretPattern {
        name: "togetherApiKey",
        regex: r###"\b['"]?(?:TOGETHER|together)_?(?:API|api)?_?(?:KEY|key)['"]?\s*(?::|=>|=)\s*['"]?(?P<secret>[a-zA-Z0-9]{40,64})['"]?\b"###,
    },
    SecretPattern {
        name: "fireworksApiKey",
        regex: r###"\b['"]?(?:FIREWORKS|fireworks)_?(?:API|api)?_?(?:KEY|key)['"]?\s*(?::|=>|=)\s*['"]?(?P<secret>[a-zA-Z0-9]{40,64})['"]?\b"###,
    },
    SecretPattern {
        name: "xaiApiKey",
        regex: r###"xai-[a-zA-Z0-9]{48,}\b"###,
    },
    SecretPattern {
        name: "openRouterApiKey",
        regex: r###"sk-or-v1-[a-zA-Z0-9]{64}"###,
    },
    SecretPattern {
        name: "amazonBedrockApiKey",
        regex: r###"\bABSK[A-Za-z0-9+/]{109,269}={0,2}\b"###,
    },
    SecretPattern {
        name: "ai21ApiKey",
        regex: r###"\b['"]?(?:AI21|ai21)_?(?:API|api)?_?(?:KEY|key)['"]?\s*(?::|=>|=)\s*['"]?(?P<secret>[a-zA-Z0-9]{40,64})['"]?\b"###,
    },
    SecretPattern {
        name: "stabilityApiKey",
        regex: r###"\b['"]?(?:STABILITY|stability|Stability)_?(?:AI|ai)?_?(?:API|api)?_?(?:KEY|key)['"]?\s*(?::|=>|=)\s*['"]?(?P<secret>sk-[a-zA-Z0-9]{48,})['"]?\b"###,
    },
    SecretPattern {
        name: "voyageApiKey",
        regex: r###"\bpa-[a-zA-Z0-9]{40,}\b"###,
    },
    SecretPattern {
        name: "elevenLabsApiKey",
        regex: r###"(?i)\b['"]?(?:ELEVENLABS|elevenlabs)_?(?:API|api)?_?(?:KEY|key)['"]?\s*(?::|=>|=)\s*['"]?(?P<secret>[a-zA-Z0-9]{32,})['"]?\b"###,
    },
    SecretPattern {
        name: "assemblyaiApiKey",
        regex: r###"\b['"]?(?:ASSEMBLYAI|assemblyai|AssemblyAI)_?(?:API|api)?_?(?:KEY|key)['"]?\s*(?::|=>|=)\s*['"]?(?P<secret>[a-f0-9]{32})['"]?\b"###,
    },
    SecretPattern {
        name: "pineconeApiKeyPrefixed",
        regex: r###"pcsk_[a-zA-Z0-9_]{50,}\b"###,
    },
    SecretPattern {
        name: "cometApiKey",
        regex: r###"\b['"]?(?:COMET|comet)_?(?:API|api)?_?(?:KEY|key)['"]?\s*(?::|=>|=)\s*['"]?(?P<secret>[a-zA-Z0-9]{32,64})['"]?\b"###,
    },
    SecretPattern {
        name: "langchainApiKey",
        regex: r###"lsv2_[a-zA-Z0-9_]{20,}\b"###,
    },
    SecretPattern {
        name: "unstructuredApiKey",
        regex: r###"\b['"]?(?:UNSTRUCTURED|unstructured)_?(?:API|api)?_?(?:KEY|key)['"]?\s*(?::|=>|=)\s*['"]?(?P<secret>[a-zA-Z0-9]{32,})['"]?\b"###,
    },
    SecretPattern {
        name: "vercelToken",
        regex: r###"(?:vcp|vci|vca|vcr|vck)_[a-zA-Z0-9]{24,}\b"###,
    },
    SecretPattern {
        name: "posthogPersonalApiKey",
        regex: r###"phx_[a-zA-Z0-9_-]{39}"###,
    },
    SecretPattern {
        name: "posthogFeatureFlagsSecureApiKey",
        regex: r###"\bphs_[a-zA-Z0-9_-]{39}\b"###,
    },
    SecretPattern {
        name: "posthogOauthAccessToken",
        regex: r###"\bpha_[a-zA-Z0-9_-]{39}\b"###,
    },
    SecretPattern {
        name: "posthogOauthRefreshToken",
        regex: r###"\bphr_[a-zA-Z0-9_-]{39}\b"###,
    },
    SecretPattern {
        name: "datadogApiKey",
        regex: r###"(?i)\bdatadog[\s\w]*(?:api|app)[\s\w]*key[\s:=]*["']?(?P<secret>[a-f0-9]{32,40})\b"###,
    },
    SecretPattern {
        name: "honeycombApiKey",
        regex: r###"hcaik_[a-zA-Z0-9_-]{32,64}"###,
    },
];
