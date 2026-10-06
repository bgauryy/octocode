//! Canonical ordered secret-detection patterns.

use regex::Regex;
use std::sync::{LazyLock, OnceLock};

#[derive(Debug, Clone, Copy)]
pub struct SecretPattern {
    pub name: &'static str,
    pub file_context: Option<&'static str>,
    pub regex: &'static str,
}

/// Secret patterns in evaluation order; a pattern's index is its identity.
pub static PATTERNS: &[SecretPattern] = &[
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
        regex: r###"\b['"]?(?:DEEPSEEK|deepseek|DeepSeek)_?(?:API|api)?_?(?:KEY|key)['"]?\s*(?::|=>|=)\s*['"]?sk-[a-zA-Z0-9]{32,64}['"]?\b"###,
    },
    SecretPattern {
        name: "togetherApiKey",
        file_context: None,
        regex: r###"\b['"]?(?:TOGETHER|together)_?(?:API|api)?_?(?:KEY|key)['"]?\s*(?::|=>|=)\s*['"]?[a-zA-Z0-9]{40,64}['"]?\b"###,
    },
    SecretPattern {
        name: "fireworksApiKey",
        file_context: None,
        regex: r###"\b['"]?(?:FIREWORKS|fireworks)_?(?:API|api)?_?(?:KEY|key)['"]?\s*(?::|=>|=)\s*['"]?[a-zA-Z0-9]{40,64}['"]?\b"###,
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
        regex: r###"\b['"]?(?:AI21|ai21)_?(?:API|api)?_?(?:KEY|key)['"]?\s*(?::|=>|=)\s*['"]?[a-zA-Z0-9]{40,64}['"]?\b"###,
    },
    SecretPattern {
        name: "stabilityApiKey",
        file_context: None,
        regex: r###"\b['"]?(?:STABILITY|stability|Stability)_?(?:AI|ai)?_?(?:API|api)?_?(?:KEY|key)['"]?\s*(?::|=>|=)\s*['"]?sk-[a-zA-Z0-9]{48,}['"]?\b"###,
    },
    SecretPattern {
        name: "voyageApiKey",
        file_context: None,
        regex: r###"\bpa-[a-zA-Z0-9]{40,}\b"###,
    },
    SecretPattern {
        name: "elevenLabsApiKey",
        file_context: None,
        regex: r###"(?i)\b['"]?(?:ELEVENLABS|elevenlabs)_?(?:API|api)?_?(?:KEY|key)['"]?\s*(?::|=>|=)\s*['"]?[a-zA-Z0-9]{32,}['"]?\b"###,
    },
    SecretPattern {
        name: "assemblyaiApiKey",
        file_context: None,
        regex: r###"\b['"]?(?:ASSEMBLYAI|assemblyai|AssemblyAI)_?(?:API|api)?_?(?:KEY|key)['"]?\s*(?::|=>|=)\s*['"]?[a-f0-9]{32}['"]?\b"###,
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
        regex: r###"\b['"]?(?:COMET|comet)_?(?:API|api)?_?(?:KEY|key)['"]?\s*(?::|=>|=)\s*['"]?[a-zA-Z0-9]{32,64}['"]?\b"###,
    },
    SecretPattern {
        name: "langchainApiKey",
        file_context: None,
        regex: r###"\blsv2_[a-zA-Z0-9_]{20,}\b"###,
    },
    SecretPattern {
        name: "unstructuredApiKey",
        file_context: None,
        regex: r###"\b['"]?(?:UNSTRUCTURED|unstructured)_?(?:API|api)?_?(?:KEY|key)['"]?\s*(?::|=>|=)\s*['"]?[a-zA-Z0-9]{32,}['"]?\b"###,
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
    SecretPattern {
        name: "jwtToken",
        file_context: None,
        regex: r###"\b(ey[a-zA-Z0-9]{17,}\.ey[a-zA-Z0-9/_-]{17,}\.(?:[a-zA-Z0-9/_-]{10,}={0,2})?)\b"###,
    },
    SecretPattern {
        name: "sessionIds",
        file_context: None,
        regex: r###"(?i)(?:JSESSIONID|PHPSESSID|ASP\.NET_SessionId|connect\.sid|session_id)=([a-zA-Z0-9%:._-]+)"###,
    },
    SecretPattern {
        name: "googleOauthToken",
        file_context: None,
        regex: r###"\bya29\.[a-zA-Z0-9_-]+\b"###,
    },
    SecretPattern {
        name: "googleOauthRefreshToken",
        file_context: Some("(?:\\.env|config|settings|secrets)"),
        regex: r###"\b['"]?(?:GOOGLE|google)?_?(?:OAUTH|oauth)?_?(?:REFRESH|refresh)?_?(?:TOKEN|token)['"]?\s*(?::|=>|=)\s*['"]?(1\/\/0[a-zA-Z0-9._-]{40,})['"]?\b"###,
    },
    SecretPattern {
        name: "onePasswordSecretKey",
        file_context: None,
        regex: r###"\bA3-[A-Z0-9]{6}-[A-Z0-9]{5}-[A-Z0-9]{5}-[A-Z0-9]{5}-[A-Z0-9]{5}\b"###,
    },
    SecretPattern {
        name: "onePasswordServiceAccountToken",
        file_context: None,
        regex: r###"\bops_eyJ[a-zA-Z0-9+/]+={0,2}\b"###,
    },
    SecretPattern {
        name: "jsonWebTokenEnhanced",
        file_context: None,
        regex: r###"\bey[a-zA-Z0-9]+\.ey[a-zA-Z0-9/_-]+\.(?:[a-zA-Z0-9/_-]+={0,2})?\b"###,
    },
    SecretPattern {
        name: "authressServiceClientAccessKey",
        file_context: None,
        regex: r###"(?i)\b(?:sc|ext|scauth|authress)_[a-z0-9]+\.[a-z0-9]+\.acc[_-][a-z0-9-]+\.[a-z0-9+/_=-]+\b"###,
    },
    SecretPattern {
        name: "auth0ClientSecret",
        file_context: None,
        regex: r###"(?i)\b['"]?(?:AUTH0|auth0)_?(?:CLIENT|client)?_?(?:SECRET|secret)['"]?\s*(?::|=>|=)\s*['"]?[a-zA-Z0-9_-]{32,64}['"]?\b"###,
    },
    SecretPattern {
        name: "auth0ManagementToken",
        file_context: None,
        regex: r###"(?i)\b['"]?(?:AUTH0|auth0)_?(?:MANAGEMENT|management|MGMT)?_?(?:API)?_?(?:TOKEN|token)['"]?\s*(?::|=>|=)\s*['"]?eyJ[a-zA-Z0-9_-]{50,}['"]?\b"###,
    },
    SecretPattern {
        name: "supertokensApiKey",
        file_context: None,
        regex: r###"(?i)\b['"]?(?:SUPERTOKENS|supertokens)_?(?:API|api)?_?(?:KEY|key)['"]?\s*(?::|=>|=)\s*['"]?[a-zA-Z0-9_-]{30,}['"]?\b"###,
    },
    SecretPattern {
        name: "basicAuthHeader",
        file_context: None,
        regex: r###"(?i)\bBasic\s+[A-Za-z0-9+/]{20,}={0,2}\b"###,
    },
    SecretPattern {
        name: "bearerAuthHeader",
        file_context: None,
        regex: r###"(?i)\bAuthorization\s*:\s*Bearer\s+[A-Za-z0-9._~+/=-]{20,}\b"###,
    },
    SecretPattern {
        name: "awsAccessKeyId",
        file_context: None,
        regex: r###"\b((?:AKIA|ABIA|ACCA|ASIA)[A-Z0-9]{16})\b"###,
    },
    SecretPattern {
        name: "awsAccountId",
        file_context: None,
        regex: r###"\b['"]?(?:AWS|aws|Aws)?_?(?:ACCOUNT|account|Account)_?(?:ID|id|Id)?['"]?\s*(?::|=>|=)\s*['"]?[0-9]{12}['"]?\b"###,
    },
    SecretPattern {
        name: "awsAppSyncApiKey",
        file_context: None,
        regex: r###"\bda2-[a-z0-9]{26}\b"###,
    },
    SecretPattern {
        name: "awsIamRoleArn",
        file_context: None,
        regex: r###"\barn:aws:iam::[0-9]{12}:role\/[a-zA-Z0-9_+=,.@-]+\b"###,
    },
    SecretPattern {
        name: "awsLambdaFunctionArn",
        file_context: None,
        regex: r###"\barn:aws:lambda:[a-z0-9-]+:[0-9]{12}:function:[a-zA-Z0-9_-]+\b"###,
    },
    SecretPattern {
        name: "awsMwsAuthToken",
        file_context: None,
        regex: r###"\bamzn\.mws\.[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}\b"###,
    },
    SecretPattern {
        name: "awsS3BucketArn",
        file_context: None,
        regex: r###"\barn:aws:s3:::[a-zA-Z0-9._-]+\b"###,
    },
    SecretPattern {
        name: "alibabaAccessKeyId",
        file_context: None,
        regex: r###"\bLTAI[a-zA-Z0-9]{20}\b"###,
    },
    SecretPattern {
        name: "awsSecretAccessKey",
        file_context: None,
        regex: r###"\b['"]?(?:AWS|aws|Aws)?_?(?:SECRET|secret|Secret)_?(?:ACCESS|access|Access)_?(?:KEY|key|Key)['"]?\s*(?::|=>|=)\s*['"]?[A-Za-z0-9/+=]{40}['"]?\b"###,
    },
    SecretPattern {
        name: "awsSessionToken",
        file_context: None,
        regex: r###"\b['"]?(?:AWS|aws|Aws)?_?(?:SESSION|session|Session)_?(?:TOKEN|token|Token)['"]?\s*(?::|=>|=)\s*['"]?[A-Za-z0-9/+=]{200,}['"]?\b"###,
    },
    SecretPattern {
        name: "awsSecretsManagerArn",
        file_context: None,
        regex: r###"\barn:aws:secretsmanager:[a-z0-9-]+:[0-9]{12}:secret:[a-zA-Z0-9/_+=.@-]+\b"###,
    },
    SecretPattern {
        name: "googleApiKey",
        file_context: None,
        regex: r###"\bAIza[a-zA-Z0-9_-]{30,}\b"###,
    },
    SecretPattern {
        name: "googleOAuth2ClientId",
        file_context: None,
        regex: r###"\b[0-9]+-[a-z0-9]+\.apps\.googleusercontent\.com\b"###,
    },
    SecretPattern {
        name: "googleOAuthClientSecret",
        file_context: None,
        regex: r###"\b"client_secret":\s*"[a-zA-Z0-9-_]{24}"\b"###,
    },
    SecretPattern {
        name: "gcpServiceAccountEmail",
        file_context: None,
        regex: r###"\b[a-z0-9-]+@[a-z0-9-]+\.iam\.gserviceaccount\.com\b"###,
    },
    SecretPattern {
        name: "azureStorageConnectionString",
        file_context: None,
        regex: r###"\bDefaultEndpointsProtocol=https?;AccountName=[a-z0-9]+;AccountKey=[a-zA-Z0-9+/]+={0,2};EndpointSuffix=core\.windows\.net\b"###,
    },
    SecretPattern {
        name: "azureSubscriptionId",
        file_context: Some("(?:\\.env|config|settings|secrets)"),
        regex: r###"(?i)\b['"]?(?:AZURE|azure)?_?(?:SUBSCRIPTION|subscription)_?(?:ID|id)?['"]?\s*(?::|=>|=)\s*['"]?[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}['"]?\b"###,
    },
    SecretPattern {
        name: "azureTenantDomain",
        file_context: Some("(?:\\.env|config|settings|secrets)"),
        regex: r###"(?i)\b[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}\.onmicrosoft\.com\b"###,
    },
    SecretPattern {
        name: "azureCosmosDbConnectionString",
        file_context: None,
        regex: r###"\bAccountEndpoint=https:\/\/[a-z0-9-]+\.documents\.azure\.com:443\/;AccountKey=[a-zA-Z0-9+/]+={0,2}\b"###,
    },
    SecretPattern {
        name: "azureServiceBusConnectionString",
        file_context: None,
        regex: r###"\bEndpoint=sb:\/\/[a-z0-9-]+\.servicebus\.windows\.net\/;SharedAccessKeyName=[a-zA-Z0-9]+;SharedAccessKey=[a-zA-Z0-9+/]+={0,2}\b"###,
    },
    SecretPattern {
        name: "dropboxAccessToken",
        file_context: None,
        regex: r###"\bsl\.[a-zA-Z0-9_-]{64}\b"###,
    },
    SecretPattern {
        name: "dropboxAppKey",
        file_context: None,
        regex: r###"\b[a-z0-9]{15}\.(?:app|apps)\.dropbox\.com\b"###,
    },
    SecretPattern {
        name: "supabaseServiceKey",
        file_context: None,
        regex: r###"\bsbp_[a-f0-9]{40}\b"###,
    },
    SecretPattern {
        name: "supabaseSecretKey",
        file_context: None,
        regex: r###"\bsb_secret_[a-zA-Z0-9_-]{22}_[a-fA-F0-9]{8}\b"###,
    },
    SecretPattern {
        name: "planetScaleConnectionString",
        file_context: None,
        regex: r###"\bmysql:\/\/[a-zA-Z0-9_-]+:[a-zA-Z0-9_=-]+@[a-z0-9.-]+\.psdb\.cloud\/[a-zA-Z0-9_-]+\?sslaccept=strict\b"###,
    },
    SecretPattern {
        name: "planetScaleToken",
        file_context: None,
        regex: r###"\bpscale_tkn_[a-zA-Z0-9_-]{38,43}\b"###,
    },
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
        regex: r###"\bdckr_pat_[a-zA-Z0-9_]{36}\b"###,
    },
    SecretPattern {
        name: "pypiApiToken",
        file_context: None,
        regex: r###"\bpypi-[a-zA-Z0-9_-]{84}\b"###,
    },
    SecretPattern {
        name: "figmaToken",
        file_context: None,
        regex: r###"\bfigd_[a-zA-Z0-9_-]{43}\b"###,
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
        regex: r###"\btfp_[a-zA-Z0-9_-]{43}\b"###,
    },
    SecretPattern {
        name: "intercomAccessToken",
        file_context: None,
        regex: r###"\bdG9rOi[a-zA-Z0-9+/]{46,48}={0,2}\b"###,
    },
    SecretPattern {
        name: "digitalOceanToken",
        file_context: None,
        regex: r###"\bdop_v1_[a-f0-9]{64}\b"###,
    },
    SecretPattern {
        name: "digitalOceanOAuthToken",
        file_context: None,
        regex: r###"\bdoo_v1_[a-f0-9]{64}\b"###,
    },
    SecretPattern {
        name: "digitalOceanRefreshToken",
        file_context: None,
        regex: r###"\bdor_v1_[a-f0-9]{64}\b"###,
    },
    SecretPattern {
        name: "cloudflareApiKey",
        file_context: None,
        regex: r###"(?i)\b['"]?(?:cloudflare)(?:[\s\w.-]{0,20})['"]?\s*(?::|=>|=)\s*['"]?[a-z0-9_-]{40}['"]?\b"###,
    },
    SecretPattern {
        name: "cloudflareGlobalApiKey",
        file_context: None,
        regex: r###"(?i)\b['"]?(?:cloudflare)(?:[\s\w.-]{0,20})['"]?\s*(?::|=>|=)\s*['"]?[a-f0-9]{37}['"]?\b"###,
    },
    SecretPattern {
        name: "cloudflareOriginCaKey",
        file_context: None,
        regex: r###"\bv1\.0-[a-f0-9]{24}-[a-f0-9]{146}\b"###,
    },
    SecretPattern {
        name: "flyioAccessToken",
        file_context: None,
        regex: r###"\bfo1_[\w-]{43}\b"###,
    },
    SecretPattern {
        name: "flyioMachineToken",
        file_context: None,
        regex: r###"\bfm[12][ar]?_[a-zA-Z0-9+/]{100,}={0,3}\b"###,
    },
    SecretPattern {
        name: "dopplerApiToken",
        file_context: None,
        regex: r###"(?i)\bdp\.pt\.[a-z0-9]{43}\b"###,
    },
    SecretPattern {
        name: "dynatraceApiToken",
        file_context: None,
        regex: r###"(?i)\bdt0c01\.[a-z0-9]{24}\.[a-z0-9]{64}\b"###,
    },
    SecretPattern {
        name: "netlifyAccessToken",
        file_context: None,
        regex: r###"(?i)\b['"]?(?:netlify)(?:[\s\w.-]{0,20})['"]?\s*(?::|=>|=)\s*['"]?[a-z0-9=_-]{40,46}['"]?\b"###,
    },
    SecretPattern {
        name: "scalingoApiToken",
        file_context: None,
        regex: r###"\btk-us-[\w-]{48}\b"###,
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
        regex: r###"\bHRKU-AA[0-9a-zA-Z_-]{58}\b"###,
    },
    SecretPattern {
        name: "microsoftTeamsWebhook",
        file_context: None,
        regex: r###"(?i)https:\/\/[a-z0-9]+\.webhook\.office\.com\/webhookb2\/[a-z0-9]{8}-(?:[a-z0-9]{4}-){3}[a-z0-9]{12}@[a-z0-9]{8}-(?:[a-z0-9]{4}-){3}[a-z0-9]{12}\/IncomingWebhook\/[a-z0-9]{32}\/[a-z0-9]{8}-(?:[a-z0-9]{4}-){3}[a-z0-9]{12}"###,
    },
    SecretPattern {
        name: "oktaAccessToken",
        file_context: None,
        regex: r###"(?i)\b['"]?(?:okta)(?:[\s\w.-]{0,20})['"]?\s*(?::|=>|=)\s*['"]?00[\w=-]{40}['"]?\b"###,
    },
    SecretPattern {
        name: "openshiftUserToken",
        file_context: None,
        regex: r###"\bsha256~[\w-]{43}\b"###,
    },
    SecretPattern {
        name: "denoDeployToken",
        file_context: None,
        regex: r###"\bddp_[a-zA-Z0-9]{40}\b"###,
    },
    SecretPattern {
        name: "resendApiKey",
        file_context: None,
        regex: r###"\bre_[a-zA-Z0-9]{30,}\b"###,
    },
    SecretPattern {
        name: "azureOpenaiApiKey",
        file_context: None,
        regex: r###"(?i)\b['"]?(?:AZURE_OPENAI|azure_openai)_?(?:API|api)?_?(?:KEY|key)['"]?\s*(?::|=>|=)\s*['"]?[a-f0-9]{32}['"]?\b"###,
    },
    SecretPattern {
        name: "railwayApiToken",
        file_context: None,
        regex: r###"(?i)\b['"]?(?:RAILWAY|railway)_?(?:API|api)?_?(?:TOKEN|token)['"]?\s*(?::|=>|=)\s*['"]?[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}['"]?\b"###,
    },
    SecretPattern {
        name: "convexDeployKey",
        file_context: None,
        regex: r###"\b(?:prod|dev):[a-zA-Z0-9_-]+:[a-zA-Z0-9_-]{40,}\b"###,
    },
    SecretPattern {
        name: "upstashKafkaCredentials",
        file_context: None,
        regex: r###"(?i)\b['"]?(?:UPSTASH_KAFKA)(?:[\s\w.-]{0,20})['"]?\s*(?::|=>|=)\s*['"]?[a-zA-Z0-9=_-]{40,}['"]?\b"###,
    },
    SecretPattern {
        name: "cloudflareApiTokenPrefixed",
        file_context: None,
        regex: r###"\bcf(?:k|ut|at)_[a-zA-Z0-9]{40}[a-fA-F0-9]{8}\b"###,
    },
    SecretPattern {
        name: "jwtSecrets",
        file_context: None,
        regex: r###"(?i)\bjwt[_-]?secret\s*[:=]\s*['"][^'"]{16,}['"]\b"###,
    },
    SecretPattern {
        name: "kubernetesSecrets",
        file_context: Some("\\.ya?ml$"),
        regex: r###"(?i)\bkind:\s*["']?Secret["']?[\s\S]{0,2000}?\bdata:\s*[\s\S]{0,2000}?[a-zA-Z0-9_-]+:\s*[a-zA-Z0-9+/]{16,}={0,3}\b"###,
    },
    SecretPattern {
        name: "dockerComposeSecrets",
        file_context: Some("docker-compose\\.ya?ml$"),
        regex: r###"(?i)\b(?:MYSQL_ROOT_PASSWORD|POSTGRES_PASSWORD|REDIS_PASSWORD|MONGODB_PASSWORD)\s*[:=]\s*['"][^'"]{4,}['"]\b"###,
    },
    SecretPattern {
        name: "springBootSecrets",
        file_context: Some("(?:application|bootstrap)(?:-\\w+)?\\.(?:properties|ya?ml)$"),
        regex: r###"(?i)\b(?:spring\.datasource\.password|spring\.security\.oauth2\.client\.registration\..*\.client-secret)\s*[:=]\s*['"][^'"]{4,}['"]\b"###,
    },
    SecretPattern {
        name: "dotnetConnectionStrings",
        file_context: Some("(?:appsettings|web\\.config).*\\.(?:json|config)$"),
        regex: r###"(?i)\b(?:ConnectionStrings?|connectionString)\s*[:=]\s*['"][^'"]*(?:password|pwd)\s*=\s*[^;'"]{4,}[^'"]*['"]\b"###,
    },
    SecretPattern {
        name: "base64EncodedSecrets",
        file_context: None,
        regex: r###"(?i)\b(?:secret|password|key|token)[_-]?(?:base64|encoded)?\s*[:=]\s*['"][A-Za-z0-9+/]{32,}={0,3}['"]\b"###,
    },
    SecretPattern {
        name: "rsaPrivateKey",
        file_context: None,
        regex: r###"-----BEGIN\s+(?:RSA\s+)?PRIVATE\s+KEY-----[\s\S]*?-----END\s+(?:RSA\s+)?PRIVATE\s+KEY-----"###,
    },
    SecretPattern {
        name: "pkcs8PrivateKey",
        file_context: None,
        regex: r###"\b-----BEGIN (?:ENCRYPTED )?PRIVATE KEY-----\s*[\s\S]*?-----END (?:ENCRYPTED )?PRIVATE KEY-----\b"###,
    },
    SecretPattern {
        name: "ecPrivateKey",
        file_context: None,
        regex: r###"\b-----BEGIN EC PRIVATE KEY-----\s*[\s\S]*?-----END EC PRIVATE KEY-----\b"###,
    },
    SecretPattern {
        name: "dsaPrivateKey",
        file_context: None,
        regex: r###"\b-----BEGIN DSA PRIVATE KEY-----\s*[\s\S]*?-----END DSA PRIVATE KEY-----\b"###,
    },
    SecretPattern {
        name: "opensshPrivateKey",
        file_context: None,
        regex: r###"-----BEGIN\s+OPENSSH\s+PRIVATE\s+KEY-----[\s\S]*?-----END\s+OPENSSH\s+PRIVATE\s+KEY-----"###,
    },
    SecretPattern {
        name: "sshPrivateKeyEncrypted",
        file_context: None,
        regex: r###"\b-----BEGIN SSH2 ENCRYPTED PRIVATE KEY-----\s*[\s\S]*?-----END SSH2 ENCRYPTED PRIVATE KEY-----\b"###,
    },
    SecretPattern {
        name: "puttyPrivateKey",
        file_context: None,
        regex: r###"\bPuTTY-User-Key-File-[23]:\s*[\s\S]*?Private-MAC:\b"###,
    },
    SecretPattern {
        name: "pgpPrivateKey",
        file_context: None,
        regex: r###"\b-----BEGIN PGP PRIVATE KEY BLOCK-----\s*[\s\S]*?-----END PGP PRIVATE KEY BLOCK-----\b"###,
    },
    SecretPattern {
        name: "firebaseServiceAccountPrivateKey",
        file_context: None,
        regex: r###"\b"private_key":\s*"-----BEGIN PRIVATE KEY-----\\n[a-zA-Z0-9+/=\\n]+\\n-----END PRIVATE KEY-----"\b"###,
    },
    SecretPattern {
        name: "openvpnClientPrivateKey",
        file_context: None,
        regex: r###"\b<key>\s*-----BEGIN[^<]*-----END[^<]*<\/key>\b"###,
    },
    SecretPattern {
        name: "dhParameters",
        file_context: None,
        regex: r###"\b-----BEGIN DH PARAMETERS-----\s*[\s\S]*?-----END DH PARAMETERS-----\b"###,
    },
    SecretPattern {
        name: "ageSecretKey",
        file_context: None,
        regex: r###"\bAGE-SECRET-KEY-1[QPZRY9X8GF2TVDW0S3JN54KHCE6MUA7L]{58}\b"###,
    },
    SecretPattern {
        name: "vaultBatchToken",
        file_context: None,
        regex: r###"\bhvb\.[a-zA-Z0-9_-]{20,}\b"###,
    },
    SecretPattern {
        name: "vaultServiceToken",
        file_context: None,
        regex: r###"\bhvs\.[a-zA-Z0-9_-]{20,}\b"###,
    },
    SecretPattern {
        name: "vaultPeriodicToken",
        file_context: None,
        regex: r###"\bhvp\.[a-zA-Z0-9_-]{20,}\b"###,
    },
    SecretPattern {
        name: "base64PrivateKeyContent",
        file_context: None,
        regex: r###"(?i)\b(?:private[_-]?key|secret[_-]?key)\s*[:=]\s*["'][A-Za-z0-9+/]{64,}={0,2}["']\b"###,
    },
    SecretPattern {
        name: "hexEncodedKey",
        file_context: None,
        regex: r###"(?i)\b(?:key|secret)\s*[:=]\s*["'][a-fA-F0-9]{32,}["']\b"###,
    },
    SecretPattern {
        name: "postgresqlConnectionString",
        file_context: None,
        regex: r###"(?i)\bpostgresql:\/\/[^:]+:[^@]+@[^/\s]+\/[^?\s]+\b"###,
    },
    SecretPattern {
        name: "mysqlConnectionString",
        file_context: None,
        regex: r###"(?i)\bmysql:\/\/[^:]+:[^@]+@[^/\s]+\/[^?\s]+\b"###,
    },
    SecretPattern {
        name: "jdbcConnectionStringWithCredentials",
        file_context: Some("(?:\\.env|config|settings|secrets)"),
        regex: r###"(?i)\bjdbc:(?:postgresql|mysql):\/\/[^:]+:[^@]+@[^/\s]+\b"###,
    },
    SecretPattern {
        name: "mongodbConnectionString",
        file_context: None,
        regex: r###"\bmongodb(?:\+srv)?:\/\/[a-zA-Z0-9._%-]+:[a-zA-Z0-9._%-]+@[a-zA-Z0-9._-]+(?::[0-9]+)?(?:\/[a-zA-Z0-9._-]*)?\b"###,
    },
    SecretPattern {
        name: "redisConnectionString",
        file_context: None,
        regex: r###"\brediss?:\/\/[a-zA-Z0-9._%-]+:[a-zA-Z0-9._%-]+@[a-zA-Z0-9._-]+:[0-9]+\b"###,
    },
    SecretPattern {
        name: "redisAuthPassword",
        file_context: None,
        regex: r###"(?m)^[ \t]*(?:(?:redis-cli(?:[ \t]+|>)|[a-zA-Z0-9_.-]+:\d+>[ \t]*))?AUTH[ \t]+(?:[a-zA-Z0-9_.@-]+[ \t]+)?[a-zA-Z0-9_-]{8,}\b"###,
    },
    SecretPattern {
        name: "elasticsearchCredentials",
        file_context: None,
        regex: r###"(?i)\bhttps?:\/\/[^:]+:[^@]+@[^/\s]+:9200\b"###,
    },
    SecretPattern {
        name: "couchdbCredentials",
        file_context: None,
        regex: r###"(?i)\bhttp[s]?:\/\/[^:]+:[^@]+@[^/\s]+:5984\b"###,
    },
    SecretPattern {
        name: "neo4jCredentials",
        file_context: None,
        regex: r###"(?i)\bbolt[s]?:\/\/[^:]+:[^@]+@[^/\s]+:7687\b"###,
    },
    SecretPattern {
        name: "timescaledbConnectionString",
        file_context: None,
        regex: r###"(?i)\btimescaledb:\/\/[^:]+:[^@]+@[^/\s]+\/[^?\s]+\b"###,
    },
    SecretPattern {
        name: "clickhouseCredentials",
        file_context: None,
        regex: r###"(?i)\bclickhouse:\/\/[^:]+:[^@]+@[^/\s]+:8123\b"###,
    },
    SecretPattern {
        name: "cassandraConnectionString",
        file_context: None,
        regex: r###"(?i)\bcassandra:\/\/[^:]+:[^@]+@[^/\s]+:9042\b"###,
    },
    SecretPattern {
        name: "faunadbKey",
        file_context: None,
        regex: r###"\bfn[a-zA-Z0-9]{40}\b"###,
    },
    SecretPattern {
        name: "databricksApiToken",
        file_context: None,
        regex: r###"\bdapi[a-f0-9]{32}(?:-\d)?\b"###,
    },
    SecretPattern {
        name: "pineconeApiKey",
        file_context: None,
        regex: r###"(?i)\bpinecone[\s\w]*(?:api|key|env)[\s:=]*["']?[a-zA-Z0-9_-]{32}["']?\b"###,
    },
    SecretPattern {
        name: "databaseUrlWithCredentials",
        file_context: None,
        regex: r###"(?i)\b(?:postgres|mysql|mongodb|redis):\/\/[^:]+:[^@]+@[^/\s]+\b"###,
    },
    SecretPattern {
        name: "clickhouseCloudApiKey",
        file_context: None,
        regex: r###"\b4b1d[A-Za-z0-9]{38}\b"###,
    },
    SecretPattern {
        name: "neonDatabaseConnectionString",
        file_context: None,
        regex: r###"(?i)\bpostgres:\/\/[^:]+:[^@]+@[^/\s]*neon\.tech[^?\s]*\b"###,
    },
    SecretPattern {
        name: "tursoDatabaseToken",
        file_context: None,
        regex: r###"(?i)\b['"]?(?:turso|libsql)(?:[\s\w.-]{0,20})(?:token|auth)['"]?\s*(?::|=>|=)\s*['"]?[a-zA-Z0-9._-]{50,}['"]?\b"###,
    },
    SecretPattern {
        name: "upstashRedisToken",
        file_context: None,
        regex: r###"(?i)\b['"]?(?:upstash)(?:[\s\w.-]{0,20})(?:token|key)['"]?\s*(?::|=>|=)\s*['"]?[a-zA-Z0-9=]{40,}['"]?\b"###,
    },
    SecretPattern {
        name: "supabaseJwtKey",
        file_context: None,
        regex: r###"\b['"]?(?:SUPABASE|supabase)_?(?:ANON|SERVICE_ROLE|anon|service_role)?_?(?:KEY|key)['"]?\s*(?::|=>|=)\s*['"]?(eyJ[a-zA-Z0-9_-]{100,})['"]?\b"###,
    },
    SecretPattern {
        name: "cockroachdbConnectionString",
        file_context: None,
        regex: r###"(?i)\bpostgresql:\/\/[^:]+:[^@]+@[^/\s]*cockroachlabs\.cloud[^?\s]*\b"###,
    },
    SecretPattern {
        name: "npmAccessToken",
        file_context: None,
        regex: r###"\bnpm_[a-zA-Z0-9]{36}\b"###,
    },
    SecretPattern {
        name: "nugetApiKey",
        file_context: None,
        regex: r###"\boy2[a-z0-9]{43}\b"###,
    },
    SecretPattern {
        name: "artifactoryApiKey",
        file_context: None,
        regex: r###"\bAKCp[A-Za-z0-9]{69}\b"###,
    },
    SecretPattern {
        name: "herokuApiKey",
        file_context: None,
        regex: r###"(?i)\bheroku.*[0-9A-F]{8}-[0-9A-F]{4}-[0-9A-F]{4}-[0-9A-F]{4}-[0-9A-F]{12}\b"###,
    },
    SecretPattern {
        name: "terraformCloudToken",
        file_context: None,
        regex: r###"\b[a-zA-Z0-9]{14}\.[a-zA-Z0-9]{6}\.[a-zA-Z0-9]{16}\b"###,
    },
    SecretPattern {
        name: "pulumiAccessToken",
        file_context: None,
        regex: r###"\bpul-[a-f0-9]{40}\b"###,
    },
    SecretPattern {
        name: "atlassianApiToken",
        file_context: None,
        regex: r###"\bATATT3[A-Za-z0-9_\-=]{186}\b"###,
    },
    SecretPattern {
        name: "sourcegraphApiKey",
        file_context: None,
        regex: r###"\bsgp_[a-zA-Z0-9]{32}\b"###,
    },
    SecretPattern {
        name: "linearApiKey",
        file_context: None,
        regex: r###"\blin_api_[0-9A-Za-z]{40}\b"###,
    },
    SecretPattern {
        name: "notionIntegrationToken",
        file_context: None,
        regex: r###"\bntn_[a-zA-Z0-9_-]{43}\b"###,
    },
    SecretPattern {
        name: "notionIntegrationTokenLegacy",
        file_context: None,
        regex: r###"\bsecret_[a-zA-Z0-9]{43}\b"###,
    },
    SecretPattern {
        name: "stackhawkApiKey",
        file_context: None,
        regex: r###"\bhawk\.[0-9A-Za-z\-_]{20}\.[0-9A-Za-z\-_]{20}\b"###,
    },
    SecretPattern {
        name: "sentryAuthToken",
        file_context: None,
        regex: r###"(?i)\bsentry[\s\w]*(?:auth|token)[\s:=]*["']?[a-f0-9]{64}["']?\b"###,
    },
    SecretPattern {
        name: "bugsnagApiKey",
        file_context: None,
        regex: r###"(?i)\bbugsnag[\s\w]*(?:api|key)[\s:=]*["']?[a-f0-9]{32}["']?\b"###,
    },
    SecretPattern {
        name: "rollbarAccessToken",
        file_context: None,
        regex: r###"(?i)\brollbar[\s\w]*(?:access|token)[\s:=]*["']?[a-f0-9]{32}["']?\b"###,
    },
    SecretPattern {
        name: "postmanApiToken",
        file_context: None,
        regex: r###"(?i)\bPMAK-[a-f0-9]{24}-[a-f0-9]{34}\b"###,
    },
    SecretPattern {
        name: "prefectApiToken",
        file_context: None,
        regex: r###"\bpnu_[a-zA-Z0-9]{36}\b"###,
    },
    SecretPattern {
        name: "readmeApiToken",
        file_context: None,
        regex: r###"\brdme_[a-z0-9]{70}\b"###,
    },
    SecretPattern {
        name: "rubygemsApiToken",
        file_context: None,
        regex: r###"\brubygems_[a-f0-9]{48}\b"###,
    },
    SecretPattern {
        name: "clojarsApiToken",
        file_context: None,
        regex: r###"(?i)\bCLOJARS_[a-z0-9]{60}\b"###,
    },
    SecretPattern {
        name: "snykApiToken",
        file_context: None,
        regex: r###"(?i)\b['"]?(?:snyk[_.-]?(?:(?:api|oauth)[_.-]?)?(?:key|token))['"]?\s*(?::|=>|=)\s*['"]?[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}['"]?\b"###,
    },
    SecretPattern {
        name: "sonarqubeToken",
        file_context: None,
        regex: r###"(?i)\b(?:squ_|sqp_|sqa_)[a-z0-9=_-]{40}\b"###,
    },
    SecretPattern {
        name: "travisciAccessToken",
        file_context: None,
        regex: r###"(?i)\b['"]?(?:travis)(?:[\s\w.-]{0,20})['"]?\s*(?::|=>|=)\s*['"]?[a-z0-9]{22}['"]?\b"###,
    },
    SecretPattern {
        name: "codecovAccessToken",
        file_context: None,
        regex: r###"(?i)\b['"]?(?:codecov)(?:[\s\w.-]{0,20})['"]?\s*(?::|=>|=)\s*['"]?[a-z0-9]{32}['"]?\b"###,
    },
    SecretPattern {
        name: "droneCiAccessToken",
        file_context: None,
        regex: r###"(?i)\b['"]?(?:droneci|drone)(?:[\s\w.-]{0,20})['"]?\s*(?::|=>|=)\s*['"]?[a-z0-9]{32}['"]?\b"###,
    },
    SecretPattern {
        name: "octopusDeployApiKey",
        file_context: None,
        regex: r###"\bAPI-[A-Z0-9]{26}\b"###,
    },
    SecretPattern {
        name: "circleciToken",
        file_context: None,
        regex: r###"(?i)\b['"]?(?:circleci|circle)(?:[\s\w.-]{0,20})['"]?\s*(?::|=>|=)\s*['"]?[a-f0-9]{40}['"]?\b"###,
    },
    SecretPattern {
        name: "buildkiteAgentToken",
        file_context: None,
        regex: r###"\bbkagent_[a-f0-9]{40}\b"###,
    },
    SecretPattern {
        name: "launchdarklyAccessToken",
        file_context: None,
        regex: r###"(?i)\b['"]?(?:launchdarkly)(?:[\s\w.-]{0,20})['"]?\s*(?::|=>|=)\s*['"]?[a-z0-9=_-]{40}['"]?\b"###,
    },
    SecretPattern {
        name: "algoliaApiKey",
        file_context: None,
        regex: r###"(?i)\b['"]?(?:algolia)(?:[\s\w.-]{0,20})['"]?\s*(?::|=>|=)\s*['"]?[a-z0-9]{32}['"]?\b"###,
    },
    SecretPattern {
        name: "clerkSecretKey",
        file_context: None,
        regex: r###"\bsk_(?:live|test)_[a-zA-Z0-9]{24,}\b"###,
    },
    SecretPattern {
        name: "clerkPublishableKey",
        file_context: None,
        regex: r###"\bpk_(?:live|test)_[a-zA-Z0-9]{24,}\b"###,
    },
    SecretPattern {
        name: "launchdarklySdkKey",
        file_context: None,
        regex: r###"\bsdk-[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}\b"###,
    },
    SecretPattern {
        name: "vercelOidcToken",
        file_context: None,
        regex: r###"\b['"]?(?:VERCEL_OIDC_TOKEN)['"]?\s*(?::|=>|=)\s*['"]?eyJ[a-zA-Z0-9_-]{100,}['"]?\b"###,
    },
    SecretPattern {
        name: "novuApiKey",
        file_context: None,
        regex: r###"(?i)\b['"]?(?:NOVU|novu)_?(?:API|api)?_?(?:KEY|key)['"]?\s*(?::|=>|=)\s*['"]?[a-zA-Z0-9]{32,}['"]?\b"###,
    },
    SecretPattern {
        name: "triggerDevApiKey",
        file_context: None,
        regex: r###"\btr_(?:dev|prod)_[a-zA-Z0-9]{20,}\b"###,
    },
    SecretPattern {
        name: "nxCloudAccessToken",
        file_context: None,
        regex: r###"\b['"]?(?:NX_CLOUD_ACCESS_TOKEN|nxCloudAccessToken)['"]?\s*(?::|=>|=)\s*['"]?[a-zA-Z0-9]{36,}['"]?\b"###,
    },
    SecretPattern {
        name: "depotToken",
        file_context: None,
        regex: r###"\bdpt_[a-zA-Z0-9]{40,}\b"###,
    },
    SecretPattern {
        name: "grafbaseApiKey",
        file_context: None,
        regex: r###"(?i)\b['"]?(?:GRAFBASE|grafbase)_?(?:API|api)?_?(?:KEY|key)['"]?\s*(?::|=>|=)\s*['"]?eyJ[a-zA-Z0-9_-]{50,}['"]?\b"###,
    },
    SecretPattern {
        name: "shopifyStorefrontAccessToken",
        file_context: None,
        regex: r###"\bshpatf_[0-9a-f]{32}\b"###,
    },
    SecretPattern {
        name: "woocommerceConsumerKey",
        file_context: None,
        regex: r###"\bck_[a-f0-9]{40}\b"###,
    },
    SecretPattern {
        name: "woocommerceConsumerSecret",
        file_context: None,
        regex: r###"\bcs_[a-f0-9]{40}\b"###,
    },
    SecretPattern {
        name: "contentfulAccessToken",
        file_context: None,
        regex: r###"\bCFPAT-[0-9a-zA-Z]{20}\b"###,
    },
    SecretPattern {
        name: "mailchimpEcommerceApiKey",
        file_context: None,
        regex: r###"\b[0-9a-f]{32}-[a-z]{2,3}[0-9]{1,2}\b"###,
    },
    SecretPattern {
        name: "credentialsInUrl",
        file_context: None,
        regex: r###"\b[a-zA-Z]{3,10}:\/\/[^\\/\s:@]{3,20}:[^\\/\s:@]{3,20}@[^\s'"]+\b"###,
    },
    SecretPattern {
        name: "envVarSecrets",
        file_context: None,
        regex: r###"(?i)\b(?:\w+_)?(?:SECRET|secret|password|key|token|jwt_secret)(?:_\w+)?\s*=\s*["'](?P<secret>[^"']{16,})["']"###,
    },
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
        regex: r###"(?i)\b['"]?(?:sumo)(?:[\s\w.-]{0,20})['"]?\s*(?::|=>|=)\s*['"]?su[a-zA-Z0-9]{12}['"]?\b"###,
    },
    SecretPattern {
        name: "splunkApiToken",
        file_context: None,
        regex: r###"(?i)\b['"]?(?:splunk)(?:[\s\w.-]{0,20})['"]?\s*(?::|=>|=)\s*['"]?[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}['"]?\b"###,
    },
    SecretPattern {
        name: "logdnaApiKey",
        file_context: None,
        regex: r###"(?i)\b['"]?(?:logdna|mezmo)(?:[\s\w.-]{0,20})['"]?\s*(?::|=>|=)\s*['"]?[a-f0-9]{32}['"]?\b"###,
    },
    SecretPattern {
        name: "logglyToken",
        file_context: None,
        regex: r###"(?i)\b['"]?(?:loggly)(?:[\s\w.-]{0,20})['"]?\s*(?::|=>|=)\s*['"]?[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}['"]?\b"###,
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
        regex: r###"(?i)\b['"]?(?:coinbase)(?:[\s\w.-]{0,20})['"]?\s*(?::|=>|=)\s*['"]?[a-z0-9_-]{64}['"]?\b"###,
    },
    SecretPattern {
        name: "krakenAccessToken",
        file_context: None,
        regex: r###"(?i)\b['"]?(?:kraken)(?:[\s\w.-]{0,20})['"]?\s*(?::|=>|=)\s*['"]?[a-z0-9/=_+-]{80,90}['"]?\b"###,
    },
    SecretPattern {
        name: "kucoinAccessToken",
        file_context: None,
        regex: r###"(?i)\b['"]?(?:kucoin)(?:[\s\w.-]{0,20})['"]?\s*(?::|=>|=)\s*['"]?[a-f0-9]{24}['"]?\b"###,
    },
    SecretPattern {
        name: "kucoinSecretKey",
        file_context: None,
        regex: r###"(?i)\b['"]?(?:kucoin)(?:[\s\w.-]{0,20})['"]?\s*(?::|=>|=)\s*['"]?[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}['"]?\b"###,
    },
    SecretPattern {
        name: "bittrexAccessKey",
        file_context: None,
        regex: r###"(?i)\b['"]?(?:bittrex)(?:[\s\w.-]{0,20})['"]?\s*(?::|=>|=)\s*['"]?[a-z0-9]{32}['"]?\b"###,
    },
    SecretPattern {
        name: "binanceApiKey",
        file_context: None,
        regex: r###"(?i)\b['"]?(?:binance)(?:[\s\w.-]{0,20})['"]?\s*(?::|=>|=)\s*['"]?[A-Za-z0-9]{64}['"]?\b"###,
    },
    SecretPattern {
        name: "bybitApiKey",
        file_context: None,
        regex: r###"(?i)\b['"]?(?:bybit)(?:[\s\w.-]{0,20})['"]?\s*(?::|=>|=)\s*['"]?[A-Za-z0-9]{18,24}['"]?\b"###,
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
        regex: r###"(?i)\b['"]?(?:PLAID|plaid)_?(?:CLIENT|client)_?(?:ID|id)['"]?\s*(?::|=>|=)\s*['"]?[a-f0-9]{24}['"]?\b"###,
    },
    SecretPattern {
        name: "lemonSqueezyApiKey",
        file_context: None,
        regex: r###"(?i)\b['"]?(?:LEMONSQUEEZY|LEMON_SQUEEZY|lemonsqueezy)_?(?:API|api)?_?(?:KEY|key)['"]?\s*(?::|=>|=)\s*['"]?eyJ[a-zA-Z0-9_-]{100,}['"]?\b"###,
    },
    SecretPattern {
        name: "paddleApiKey",
        file_context: None,
        regex: r###"(?i)\b['"]?(?:PADDLE|paddle)_?(?:API|api)?_?(?:KEY|key)['"]?\s*(?::|=>|=)\s*['"]?pdl_(?:live|sdbx)_[a-zA-Z0-9]{40,}['"]?\b"###,
    },
    SecretPattern {
        name: "mollieApiKey",
        file_context: Some("mollie"),
        regex: r###"\b(?:live|test)_[a-zA-Z0-9]{30,}\b"###,
    },
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
        regex: r###"(?i)\b['"]?(?:asana)(?:[\s\w.-]{0,20})['"]?\s*(?::|=>|=)\s*['"]?[0-9]{16}['"]?\b"###,
    },
    SecretPattern {
        name: "mondayApiToken",
        file_context: None,
        regex: r###"(?i)\b['"]?(?:monday)(?:[\s\w.-]{0,20})['"]?\s*(?::|=>|=)\s*['"]?eyJ[a-zA-Z0-9_-]{100,}['"]?\b"###,
    },
    SecretPattern {
        name: "trelloApiKey",
        file_context: None,
        regex: r###"(?i)\b['"]?(?:trello)(?:[\s\w.-]{0,20})['"]?\s*(?::|=>|=)\s*['"]?[a-f0-9]{32}['"]?\b"###,
    },
    SecretPattern {
        name: "jiraApiToken",
        file_context: None,
        regex: r###"(?i)\b['"]?(?:jira)(?:[\s\w.-]{0,20})['"]?\s*(?::|=>|=)\s*['"]?[a-zA-Z0-9]{24}['"]?\b"###,
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
        regex: r###"(?i)\b['"]?(?:sendbird)(?:[\s\w.-]{0,20})['"]?\s*(?::|=>|=)\s*['"]?[a-f0-9]{40}['"]?\b"###,
    },
    SecretPattern {
        name: "messagebirdApiToken",
        file_context: None,
        regex: r###"(?i)\b['"]?(?:messagebird|message_bird|message-bird)(?:[\s\w.-]{0,20})['"]?\s*(?::|=>|=)\s*['"]?[a-z0-9]{25}['"]?\b"###,
    },
    SecretPattern {
        name: "mattermostAccessToken",
        file_context: None,
        regex: r###"(?i)\b['"]?(?:mattermost)(?:[\s\w.-]{0,20})['"]?\s*(?::|=>|=)\s*['"]?[a-z0-9]{26}['"]?\b"###,
    },
    SecretPattern {
        name: "zendeskSecretKey",
        file_context: None,
        regex: r###"(?i)\b['"]?(?:zendesk)(?:[\s\w.-]{0,20})['"]?\s*(?::|=>|=)\s*['"]?[a-z0-9]{40}['"]?\b"###,
    },
    SecretPattern {
        name: "freshdeskApiKey",
        file_context: None,
        regex: r###"(?i)\b['"]?(?:freshdesk)(?:[\s\w.-]{0,20})['"]?\s*(?::|=>|=)\s*['"]?[a-zA-Z0-9]{20}['"]?\b"###,
    },
    SecretPattern {
        name: "sendinblueApiToken",
        file_context: None,
        regex: r###"\bxkeysib-[a-f0-9]{64}-[a-z0-9]{16}\b"###,
    },
    SecretPattern {
        name: "pusherAppSecret",
        file_context: None,
        regex: r###"(?i)\b['"]?(?:PUSHER|pusher)_?(?:APP|app)?_?(?:SECRET|secret)['"]?\s*(?::|=>|=)\s*['"]?[a-f0-9]{20}['"]?\b"###,
    },
    SecretPattern {
        name: "streamApiSecret",
        file_context: None,
        regex: r###"(?i)\b['"]?(?:STREAM|stream|GETSTREAM)_?(?:API|api)?_?(?:SECRET|secret|KEY|key)['"]?\s*(?::|=>|=)\s*['"]?[a-z0-9]{40,}['"]?\b"###,
    },
    SecretPattern {
        name: "postmarkServerToken",
        file_context: Some("postmark"),
        regex: r###"\b[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}\b"###,
    },
    SecretPattern {
        name: "vonageApiSecret",
        file_context: None,
        regex: r###"(?i)\b['"]?(?:VONAGE|NEXMO|vonage|nexmo)_?(?:API|api)?_?(?:SECRET|secret)['"]?\s*(?::|=>|=)\s*['"]?[a-zA-Z0-9]{16}['"]?\b"###,
    },
    SecretPattern {
        name: "customerIoApiKey",
        file_context: None,
        regex: r###"(?i)\b['"]?(?:CUSTOMERIO|customer_io|CUSTOMER_IO)_?(?:API|api)?_?(?:KEY|key)['"]?\s*(?::|=>|=)\s*['"]?[a-f0-9]{32,}['"]?\b"###,
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
        regex: r###"(?i)\b['"]?(?:linkedin|linked_in|linked-in)(?:[\s\w.-]{0,20})['"]?\s*(?::|=>|=)\s*['"]?[a-z0-9]{14,16}['"]?\b"###,
    },
    SecretPattern {
        name: "youtubeApiKey",
        file_context: None,
        regex: r###"(?i)\b['"]?(?:youtube)(?:[\s\w.-]{0,20})['"]?\s*(?::|=>|=)\s*['"]?AIza[a-zA-Z0-9_-]{35}['"]?\b"###,
    },
    SecretPattern {
        name: "tiktokApiToken",
        file_context: None,
        regex: r###"(?i)\b['"]?(?:tiktok)(?:[\s\w.-]{0,20})['"]?\s*(?::|=>|=)\s*['"]?[a-zA-Z0-9_-]{40,}['"]?\b"###,
    },
    SecretPattern {
        name: "gitlabPersonalAccessToken",
        file_context: None,
        regex: r###"\bglpat-[A-Za-z0-9_-]{20,}\b"###,
    },
    SecretPattern {
        name: "gitlabDeployToken",
        file_context: None,
        regex: r###"\bgldt-[A-Za-z0-9_-]{20}\b"###,
    },
    SecretPattern {
        name: "gitlabRunnerToken",
        file_context: None,
        regex: r###"\bglrt-[A-Za-z0-9_-]{20}\b"###,
    },
    SecretPattern {
        name: "gitlabCiJobToken",
        file_context: None,
        regex: r###"\bglcbt-[0-9a-zA-Z]{1,5}_[0-9a-zA-Z_-]{20}\b"###,
    },
    SecretPattern {
        name: "gitlabPipelineTriggerToken",
        file_context: None,
        regex: r###"\bglptt-[0-9a-f]{40}\b"###,
    },
    SecretPattern {
        name: "bitbucketAppPassword",
        file_context: None,
        regex: r###"\bATBB[a-zA-Z0-9]{24}\b"###,
    },
    SecretPattern {
        name: "githubTokens",
        file_context: None,
        regex: r###"\b((?:ghp|gho|ghu|ghs|ghr|github_pat)_[a-zA-Z0-9_]{36,255})\b"###,
    },
    SecretPattern {
        name: "githubFineGrainedToken",
        file_context: Some("(?:\\.env|config|settings|secrets)"),
        regex: r###"\bgithub_pat_[A-Za-z0-9_]{82}\b"###,
    },
    SecretPattern {
        name: "githubAppInstallationToken",
        file_context: None,
        regex: r###"\bghs_[0-9a-zA-Z]{37}\b"###,
    },
    SecretPattern {
        name: "gitlabScimToken",
        file_context: None,
        regex: r###"\bglsoat-[0-9a-zA-Z_-]{20}\b"###,
    },
    SecretPattern {
        name: "gitlabFeatureFlagToken",
        file_context: None,
        regex: r###"\bglffct-[0-9a-zA-Z_-]{20}\b"###,
    },
    SecretPattern {
        name: "gitlabFeedToken",
        file_context: None,
        regex: r###"\bglft-[0-9a-zA-Z_-]{20}\b"###,
    },
    SecretPattern {
        name: "gitlabIncomingMailToken",
        file_context: None,
        regex: r###"\bglimt-[0-9a-zA-Z_-]{25}\b"###,
    },
    SecretPattern {
        name: "gitlabK8sAgentToken",
        file_context: None,
        regex: r###"\bglagent-[0-9a-zA-Z_-]{50}\b"###,
    },
    SecretPattern {
        name: "gitlabOAuthAppSecret",
        file_context: None,
        regex: r###"\bgloas-[0-9a-zA-Z_-]{64}\b"###,
    },
    SecretPattern {
        name: "gitlabSessionCookie",
        file_context: None,
        regex: r###"_gitlab_session=[0-9a-z]{32}"###,
    },
    SecretPattern {
        name: "bitbucketRepoToken",
        file_context: None,
        regex: r###"\bATCTT3[a-zA-Z0-9]{24}\b"###,
    },
];

/// Lazily-compiled per-pattern Regex instances (compile on first use per index).
/// Sized from `PATTERNS.len()` — never a hardcoded count — so the cell table
/// can never be shorter than the pattern set it indexes.
static PATTERN_REGEX_CELLS: LazyLock<Vec<OnceLock<Regex>>> =
    LazyLock::new(|| (0..PATTERNS.len()).map(|_| OnceLock::new()).collect());

/// Rewrite Unicode `\b` to ASCII `(?-u:\b)`. A Unicode word boundary makes the
/// regex crate abandon its fast DFA on any non-ASCII haystack (12–20× slower on
/// multi-MB files). The ASCII boundary also fires next to non-ASCII letters, so
/// it only widens redaction around the ASCII token shapes these patterns match.
fn ascii_word_boundaries(pattern: &str) -> String {
    let mut out = String::with_capacity(pattern.len() + 16);
    let mut chars = pattern.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            out.push(ch);
            continue;
        }
        match chars.next() {
            Some('b') => out.push_str("(?-u:\\b)"),
            Some(escaped) => {
                out.push('\\');
                out.push(escaped);
            }
            None => out.push('\\'),
        }
    }
    out
}

/// Get (compiling at most once) the Regex for pattern `idx`.
pub fn pattern_regex(idx: usize) -> &'static Regex {
    // Built-in pattern strings are constants validated by the pattern tests; a
    // compile failure is a build-time bug, not a runtime condition.
    #[allow(clippy::expect_used)]
    PATTERN_REGEX_CELLS[idx].get_or_init(|| {
        Regex::new(&ascii_word_boundaries(PATTERNS[idx].regex)).expect(PATTERNS[idx].name)
    })
}

#[cfg(test)]
mod ascii_boundary_tests {
    use super::*;

    #[test]
    fn rewrites_only_word_boundary_escapes() {
        assert_eq!(
            ascii_word_boundaries(r"\bsk-\d+\b"),
            r"(?-u:\b)sk-\d+(?-u:\b)"
        );
        assert_eq!(ascii_word_boundaries(r"a\\b"), r"a\\b");
    }

    #[test]
    fn every_pattern_compiles_and_still_matches_next_to_non_ascii_text() {
        for idx in 0..PATTERNS.len() {
            let _ = pattern_regex(idx);
        }
        let idx = PATTERNS
            .iter()
            .position(|p| p.regex.starts_with(r"\bsk-proj-"))
            .expect("openai project key pattern");
        let text = "é sk-proj-abcdefghijklmnopqrstuvwx ü";
        assert!(pattern_regex(idx).is_match(text));
    }
}
