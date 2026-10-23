//! AI and ML providers, product analytics and monitoring keys.
//!
//! A contiguous slice of [`super::PATTERNS`]: its order is part of the
//! evaluation order.

use super::SecretPattern;

pub(super) const PATTERNS: &[SecretPattern] = &[
    SecretPattern {
        name: "openaiApiKeyLegacy",
        file_context: None,
        regex: r###"\b(sk-[a-zA-Z0-9_-]+T3BlbkFJ[a-zA-Z0-9_-]+)\b"###,
    },
    SecretPattern {
        name: "openaiApiKeyClassic",
        file_context: None,
        regex: r###"\bsk-[a-zA-Z0-9]{40,}\b"###,
    },
    SecretPattern {
        name: "openaiProjectApiKey",
        file_context: None,
        regex: r###"\bsk-proj-[a-zA-Z0-9_-]{20,}\b"###,
    },
    SecretPattern {
        name: "openaiServiceAccountKey",
        file_context: None,
        regex: r###"\bsk-svcacct-[a-zA-Z0-9_-]{20,}\b"###,
    },
    SecretPattern {
        name: "openaiAdminKey",
        file_context: None,
        regex: r###"\bsk-admin-[a-zA-Z0-9_-]{20,}\b"###,
    },
    SecretPattern {
        name: "openaiOrgId",
        file_context: None,
        regex: r###"\borg-[a-zA-Z0-9]{20,}\b"###,
    },
    SecretPattern {
        name: "groqApiKey",
        file_context: None,
        regex: r###"\bgsk_[a-zA-Z0-9-_]{51,52}\b"###,
    },
    SecretPattern {
        name: "cohereApiKey",
        file_context: None,
        regex: r###"\bco-[a-zA-Z0-9-_]{38,64}\b"###,
    },
    SecretPattern {
        name: "huggingFaceToken",
        file_context: None,
        regex: r###"\bhf_[a-zA-Z0-9]{34}\b"###,
    },
    SecretPattern {
        name: "perplexityApiKey",
        file_context: None,
        regex: r###"\bpplx-[a-zA-Z0-9]{30,64}\b"###,
    },
    SecretPattern {
        name: "replicateApiToken",
        file_context: None,
        regex: r###"\br8_[a-zA-Z0-9]{30,}\b"###,
    },
    SecretPattern {
        name: "anthropicApiKey",
        file_context: None,
        regex: r###"\bsk-ant-(?:admin01|api03|sid01)-[\w-]{80,120}\b"###,
    },
    SecretPattern {
        name: "mistralApiKey",
        file_context: None,
        regex: r###"\b(?:mistral-|mist_)[a-zA-Z0-9]{32,}\b"###,
    },
    SecretPattern {
        name: "tavilyApiKey",
        file_context: None,
        regex: r###"\btvly-[a-zA-Z0-9]{30,}\b"###,
    },
    SecretPattern {
        name: "deepseekApiKey",
        file_context: None,
        regex: r###"\b['"]?(?:DEEPSEEK|deepseek|DeepSeek)_?(?:API|api)?_?(?:KEY|key)['"]?\s*(?::|=>|=)\s*['"]?(?P<secret>sk-[a-zA-Z0-9]{32,64})['"]?\b"###,
    },
    SecretPattern {
        name: "togetherApiKey",
        file_context: None,
        regex: r###"\b['"]?(?:TOGETHER|together)_?(?:API|api)?_?(?:KEY|key)['"]?\s*(?::|=>|=)\s*['"]?(?P<secret>[a-zA-Z0-9]{40,64})['"]?\b"###,
    },
    SecretPattern {
        name: "fireworksApiKey",
        file_context: None,
        regex: r###"\b['"]?(?:FIREWORKS|fireworks)_?(?:API|api)?_?(?:KEY|key)['"]?\s*(?::|=>|=)\s*['"]?(?P<secret>[a-zA-Z0-9]{40,64})['"]?\b"###,
    },
    SecretPattern {
        name: "xaiApiKey",
        file_context: None,
        regex: r###"\bxai-[a-zA-Z0-9]{48,}\b"###,
    },
    SecretPattern {
        name: "openRouterApiKey",
        file_context: None,
        regex: r###"\bsk-or-v1-[a-zA-Z0-9]{64}\b"###,
    },
    SecretPattern {
        name: "amazonBedrockApiKey",
        file_context: None,
        regex: r###"\bABSK[A-Za-z0-9+/]{109,269}={0,2}\b"###,
    },
    SecretPattern {
        name: "ai21ApiKey",
        file_context: None,
        regex: r###"\b['"]?(?:AI21|ai21)_?(?:API|api)?_?(?:KEY|key)['"]?\s*(?::|=>|=)\s*['"]?(?P<secret>[a-zA-Z0-9]{40,64})['"]?\b"###,
    },
    SecretPattern {
        name: "stabilityApiKey",
        file_context: None,
        regex: r###"\b['"]?(?:STABILITY|stability|Stability)_?(?:AI|ai)?_?(?:API|api)?_?(?:KEY|key)['"]?\s*(?::|=>|=)\s*['"]?(?P<secret>sk-[a-zA-Z0-9]{48,})['"]?\b"###,
    },
    SecretPattern {
        name: "voyageApiKey",
        file_context: None,
        regex: r###"\bpa-[a-zA-Z0-9]{40,}\b"###,
    },
    SecretPattern {
        name: "elevenLabsApiKey",
        file_context: None,
        regex: r###"(?i)\b['"]?(?:ELEVENLABS|elevenlabs)_?(?:API|api)?_?(?:KEY|key)['"]?\s*(?::|=>|=)\s*['"]?(?P<secret>[a-zA-Z0-9]{32,})['"]?\b"###,
    },
    SecretPattern {
        name: "assemblyaiApiKey",
        file_context: None,
        regex: r###"\b['"]?(?:ASSEMBLYAI|assemblyai|AssemblyAI)_?(?:API|api)?_?(?:KEY|key)['"]?\s*(?::|=>|=)\s*['"]?(?P<secret>[a-f0-9]{32})['"]?\b"###,
    },
    SecretPattern {
        name: "pineconeApiKeyPrefixed",
        file_context: None,
        regex: r###"\bpcsk_[a-zA-Z0-9_]{50,}\b"###,
    },
    SecretPattern {
        name: "wandbApiKey",
        file_context: Some("wandb"),
        regex: r###"\b[a-f0-9]{40}\b"###,
    },
    SecretPattern {
        name: "cometApiKey",
        file_context: None,
        regex: r###"\b['"]?(?:COMET|comet)_?(?:API|api)?_?(?:KEY|key)['"]?\s*(?::|=>|=)\s*['"]?(?P<secret>[a-zA-Z0-9]{32,64})['"]?\b"###,
    },
    SecretPattern {
        name: "langchainApiKey",
        file_context: None,
        regex: r###"\blsv2_[a-zA-Z0-9_]{20,}\b"###,
    },
    SecretPattern {
        name: "unstructuredApiKey",
        file_context: None,
        regex: r###"\b['"]?(?:UNSTRUCTURED|unstructured)_?(?:API|api)?_?(?:KEY|key)['"]?\s*(?::|=>|=)\s*['"]?(?P<secret>[a-zA-Z0-9]{32,})['"]?\b"###,
    },
    SecretPattern {
        name: "vercelToken",
        file_context: None,
        regex: r###"\b(?:vcp|vci|vca|vcr|vck)_[a-zA-Z0-9]{24,}\b"###,
    },
    SecretPattern {
        name: "posthogApiKey",
        file_context: None,
        regex: r###"\bphc_[a-zA-Z0-9_-]{39}\b"###,
    },
    SecretPattern {
        name: "posthogPersonalApiKey",
        file_context: None,
        regex: r###"\bphx_[a-zA-Z0-9_-]{39}\b"###,
    },
    SecretPattern {
        name: "posthogFeatureFlagsSecureApiKey",
        file_context: None,
        regex: r###"\bphs_[a-zA-Z0-9_-]{39}\b"###,
    },
    SecretPattern {
        name: "posthogOauthAccessToken",
        file_context: None,
        regex: r###"\bpha_[a-zA-Z0-9_-]{39}\b"###,
    },
    SecretPattern {
        name: "posthogOauthRefreshToken",
        file_context: None,
        regex: r###"\bphr_[a-zA-Z0-9_-]{39}\b"###,
    },
    SecretPattern {
        name: "datadogApiKey",
        file_context: None,
        regex: r###"(?i)\bdatadog[\s\w]*(?:api|app)[\s\w]*key[\s:=]*["']?[a-fA-F0-9]{32,40}["']?"###,
    },
    SecretPattern {
        name: "honeycombApiKey",
        file_context: None,
        regex: r###"\bhcaik_[a-zA-Z0-9_-]{32,64}\b"###,
    },
];
