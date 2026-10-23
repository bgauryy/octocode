//! Session and OAuth tokens, auth headers, and cloud provider (AWS, Google,
//! Azure) credentials.
//!
//! A contiguous slice of [`super::PATTERNS`]: its order is part of the
//! evaluation order.

use super::SecretPattern;

pub(super) const PATTERNS: &[SecretPattern] = &[
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
        regex: r###"\b['"]?(?:GOOGLE|google)?_?(?:OAUTH|oauth)?_?(?:REFRESH|refresh)?_?(?:TOKEN|token)['"]?\s*(?::|=>|=)\s*['"]?(?P<secret>(1\/\/0[a-zA-Z0-9._-]{40,}))['"]?\b"###,
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
        regex: r###"(?i)\b['"]?(?:AUTH0|auth0)_?(?:CLIENT|client)?_?(?:SECRET|secret)['"]?\s*(?::|=>|=)\s*['"]?(?P<secret>[a-zA-Z0-9_-]{32,64})['"]?\b"###,
    },
    SecretPattern {
        name: "auth0ManagementToken",
        file_context: None,
        regex: r###"(?i)\b['"]?(?:AUTH0|auth0)_?(?:MANAGEMENT|management|MGMT)?_?(?:API)?_?(?:TOKEN|token)['"]?\s*(?::|=>|=)\s*['"]?(?P<secret>eyJ[a-zA-Z0-9_-]{50,})['"]?\b"###,
    },
    SecretPattern {
        name: "supertokensApiKey",
        file_context: None,
        regex: r###"(?i)\b['"]?(?:SUPERTOKENS|supertokens)_?(?:API|api)?_?(?:KEY|key)['"]?\s*(?::|=>|=)\s*['"]?(?P<secret>[a-zA-Z0-9_-]{30,})['"]?\b"###,
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
        regex: r###"\b['"]?(?:AWS|aws|Aws)?_?(?:ACCOUNT|account|Account)_?(?:ID|id|Id)?['"]?\s*(?::|=>|=)\s*['"]?(?P<secret>[0-9]{12})['"]?\b"###,
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
        regex: r###"\b['"]?(?:AWS|aws|Aws)?_?(?:SECRET|secret|Secret)_?(?:ACCESS|access|Access)_?(?:KEY|key|Key)['"]?\s*(?::|=>|=)\s*['"]?(?P<secret>[A-Za-z0-9/+=]{40})['"]?\b"###,
    },
    SecretPattern {
        name: "awsSessionToken",
        file_context: None,
        regex: r###"\b['"]?(?:AWS|aws|Aws)?_?(?:SESSION|session|Session)_?(?:TOKEN|token|Token)['"]?\s*(?::|=>|=)\s*['"]?(?P<secret>[A-Za-z0-9/+=]{200,})['"]?\b"###,
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
        regex: r###""client_secret":\s*"(?P<secret>[a-zA-Z0-9-_]{24})""###,
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
        regex: r###"(?i)\b['"]?(?:AZURE|azure)?_?(?:SUBSCRIPTION|subscription)_?(?:ID|id)?['"]?\s*(?::|=>|=)\s*['"]?(?P<secret>[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12})['"]?\b"###,
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
];
