//! Session and OAuth tokens, auth headers, and cloud provider (AWS, Google,
//! Azure) credentials.
//!
//! A contiguous slice of [`super::PATTERNS`]: its order is part of the
//! evaluation order.

use super::SecretPattern;

pub(super) const PATTERNS: &[SecretPattern] = &[
    SecretPattern {
        name: "jwtToken",
        regex: r###"\b(ey[a-zA-Z0-9]{17,}\.ey[a-zA-Z0-9/_-]{17,}\.(?:[a-zA-Z0-9/_-]{10,}={0,2})?)\b"###,
    },
    SecretPattern {
        name: "sessionIds",
        regex: r###"(?i)(?:JSESSIONID|PHPSESSID|ASP\.NET_SessionId|connect\.sid)=(?P<secret>[a-zA-Z0-9%:._-]+)"###,
    },
    SecretPattern {
        name: "googleOauthToken",
        regex: r###"ya29\.[a-zA-Z0-9_-]+\b"###,
    },
    SecretPattern {
        name: "googleOauthRefreshToken",
        // The tag `1//0` marks the token wherever it stands, an assignment
        // or not; it ends on a token character, never a sentence's period.
        regex: r###"\b1//0[A-Za-z0-9._-]{39,}[A-Za-z0-9_-]"###,
    },
    SecretPattern {
        name: "onePasswordSecretKey",
        regex: r###"\bA3-[A-Z0-9]{6}-[A-Z0-9]{5}-[A-Z0-9]{5}-[A-Z0-9]{5}-[A-Z0-9]{5}\b"###,
    },
    SecretPattern {
        name: "onePasswordServiceAccountToken",
        regex: r###"ops_eyJ[a-zA-Z0-9+/]+={0,2}"###,
    },
    SecretPattern {
        name: "jsonWebTokenEnhanced",
        // A signed token, or any compact serialization a JOSE header opens
        // (`eyJ` is `{"`): an encrypted token's parts after it are not JSON.
        regex: r###"\bey[a-zA-Z0-9]+\.ey[a-zA-Z0-9/_-]+\.(?:[a-zA-Z0-9/_-]+={0,2})?\b|\beyJ[A-Za-z0-9_-]{14,}(?:\.[A-Za-z0-9_-]{10,}){2,4}"###,
    },
    SecretPattern {
        name: "authressServiceClientAccessKey",
        regex: r###"(?i)\b(?:sc|ext|scauth|authress)_[a-z0-9]+\.[a-z0-9]+\.acc[_-][a-z0-9-]+\.[a-z0-9+/_=-]+\b"###,
    },
    SecretPattern {
        name: "auth0ClientSecret",
        regex: r###"(?i)\b['"]?(?:AUTH0|auth0)_?(?:CLIENT|client)?_?(?:SECRET|secret)['"]?\s*(?::|=>|=)\s*['"]?(?P<secret>[a-zA-Z0-9_-]{32,64})['"]?\b"###,
    },
    SecretPattern {
        name: "auth0ManagementToken",
        regex: r###"(?i)\b['"]?(?:AUTH0|auth0)_?(?:MANAGEMENT|management|MGMT)?_?(?:API)?_?(?:TOKEN|token)['"]?\s*(?::|=>|=)\s*['"]?(?P<secret>eyJ[a-zA-Z0-9_-]{50,})['"]?\b"###,
    },
    SecretPattern {
        name: "supertokensApiKey",
        regex: r###"(?i)\b['"]?(?:SUPERTOKENS|supertokens)_?(?:API|api)?_?(?:KEY|key)['"]?\s*(?::|=>|=)\s*['"]?(?P<secret>[a-zA-Z0-9_-]{30,})['"]?\b"###,
    },
    SecretPattern {
        name: "basicAuthHeader",
        // Only the credential goes: the scheme stays, and the encoding's
        // `=` padding is the credential's own.
        regex: r###"(?i)\bBasic\s+(?P<secret>[A-Za-z0-9+/]{20,}(?:={1,2}|\b))"###,
    },
    SecretPattern {
        name: "bearerAuthHeader",
        // The token after the `Bearer` scheme, in whatever layout holds it
        // (a header line, `headers["Authorization"] = "Bearer …"`, a
        // member, a template, a sentence), as `basicAuthHeader` reads its
        // scheme's: the scheme places a credential, whatever its
        // characters. Only the token goes: the header's name and its scheme
        // stay. The token ends with a word character, its `=` padding after
        // it included; a mark after that closes the sentence.
        regex: r###"(?i)\bBearer\s+(?P<secret>[A-Za-z0-9._~+/=-]{20,}\b=*)"###,
    },
    SecretPattern {
        name: "awsAccessKeyId",
        regex: r###"((?:AKIA|ABIA|ACCA|ASIA)[A-Z0-9]{16})\b"###,
    },
    SecretPattern {
        name: "awsAppSyncApiKey",
        regex: r###"\bda2-[a-z0-9]{26}\b"###,
    },
    SecretPattern {
        name: "awsMwsAuthToken",
        regex: r###"amzn\.mws\.[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}"###,
    },
    SecretPattern {
        name: "alibabaAccessKeyId",
        regex: r###"\bLTAI[a-zA-Z0-9]{20}\b"###,
    },
    SecretPattern {
        name: "awsSecretAccessKey",
        // The value is the 40 base64 bytes after the operator, behind any
        // punctuation or symbol that touches it (a quote of any kind, a
        // backtick, emphasis, a bracket, an ellipsis), and it ends at the
        // next byte that is neither base64 nor a word character, so a value
        // that ends in `+` or `/` is matched whole. A placeholder's opener
        // (`$`, `{`, `<`, `%`, `[`) is no such touch: a template names the
        // value and holds none of it.
        regex: r###"\b['"]?(?:AWS|aws|Aws)?_?(?:SECRET|secret|Secret)_?(?:ACCESS|access|Access)_?(?:KEY|key|Key)['"]?\s*(?::|=>|=)\s*[^\s\w$%<\[{+/=]*(?P<secret>[A-Za-z0-9/+=]{40})(?:[^A-Za-z0-9_+/=]|\z)"###,
    },
    SecretPattern {
        name: "awsSessionToken",
        // Its value is touched and ended as `awsSecretAccessKey`'s is.
        regex: r###"\b['"]?(?:AWS|aws|Aws)?_?(?:SESSION|session|Session)_?(?:TOKEN|token|Token)['"]?\s*(?::|=>|=)\s*[^\s\w$%<\[{+/=]*(?P<secret>[A-Za-z0-9/+=]{200,})(?:[^A-Za-z0-9_+/=]|\z)"###,
    },
    SecretPattern {
        name: "googleApiKey",
        regex: r###"AIza[a-zA-Z0-9_-]{30,}\b"###,
    },
    SecretPattern {
        name: "googleOAuthClientSecret",
        regex: r###""client_secret":\s*"(?P<secret>[a-zA-Z0-9-_]{24})""###,
    },
    SecretPattern {
        name: "azureStorageConnectionString",
        regex: r###"\bDefaultEndpointsProtocol=https?;AccountName=[a-z0-9]+;AccountKey=[a-zA-Z0-9+/]+={0,2};EndpointSuffix=core\.windows\.net\b"###,
    },
    SecretPattern {
        name: "azureCosmosDbConnectionString",
        regex: r###"\bAccountEndpoint=https:\/\/[a-z0-9-]+\.documents\.azure\.com:443\/;AccountKey=[a-zA-Z0-9+/]+={0,2}\b"###,
    },
    SecretPattern {
        name: "azureServiceBusConnectionString",
        regex: r###"\bEndpoint=sb:\/\/[a-z0-9-]+\.servicebus\.windows\.net\/;SharedAccessKeyName=[a-zA-Z0-9]+;SharedAccessKey=[a-zA-Z0-9+/]+={0,2}\b"###,
    },
    SecretPattern {
        name: "dropboxAccessToken",
        regex: r###"\bsl\.[a-zA-Z0-9_-]{64}\b"###,
    },
    SecretPattern {
        name: "supabaseServiceKey",
        regex: r###"sbp_[a-f0-9]{40}"###,
    },
    SecretPattern {
        name: "supabaseSecretKey",
        regex: r###"sb_secret_[a-zA-Z0-9_-]{22}_[a-fA-F0-9]{8}"###,
    },
    SecretPattern {
        name: "planetScaleConnectionString",
        regex: r###"\bmysql:\/\/[a-zA-Z0-9_-]+:(?P<secret>[a-zA-Z0-9_=-]+)@[a-z0-9.-]+\.psdb\.cloud\/[a-zA-Z0-9_-]+\?sslaccept=strict\b"###,
    },
    SecretPattern {
        name: "planetScaleToken",
        regex: r###"pscale_tkn_[a-zA-Z0-9_-]{38,43}"###,
    },
];
