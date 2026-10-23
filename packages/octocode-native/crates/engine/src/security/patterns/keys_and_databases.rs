//! Secrets in config files, private keys, Vault tokens and database
//! connection strings.
//!
//! A contiguous slice of [`super::PATTERNS`]: its order is part of the
//! evaluation order.

use super::SecretPattern;

pub(super) const PATTERNS: &[SecretPattern] = &[
    SecretPattern {
        name: "jwtSecrets",
        file_context: None,
        regex: r###"(?i)\bjwt[_-]?secret['"]?\s*[:=]\s*['"](?P<secret>[^'"\r\n]{16,})['"]"###,
    },
    SecretPattern {
        name: "kubernetesSecrets",
        file_context: Some("\\.ya?ml$"),
        regex: r###"(?im)\bkind:[ \t]*["']?Secret["']?[\s\S]{0,2000}?^[ \t]*(?:data|stringData):[ \t]*\r?\n(?P<secret>(?:[ \t]+[^\s:#]+:[ \t]*\S.*(?:\r?\n|$))+)|^[ \t]*(?:data|stringData):[ \t]*\r?\n(?P<secret_before_kind>(?:[ \t]+[^\s:#]+:[ \t]*\S.*(?:\r?\n|$))+)[\s\S]{0,2000}?\bkind:[ \t]*["']?Secret\b"###,
    },
    SecretPattern {
        name: "dockerComposeSecrets",
        file_context: Some("docker-compose\\.ya?ml$"),
        regex: r###"(?i)\b(?:MYSQL_ROOT_PASSWORD|POSTGRES_PASSWORD|REDIS_PASSWORD|MONGODB_PASSWORD)['"]?\s*[:=]\s*['"](?P<secret>[^'"\r\n]{4,})['"]"###,
    },
    SecretPattern {
        name: "springBootSecrets",
        file_context: Some("(?:application|bootstrap)(?:-\\w+)?\\.(?:properties|ya?ml)$"),
        regex: r###"(?i)\b(?:spring\.datasource\.password|spring\.security\.oauth2\.client\.registration\..*\.client-secret)\s*[:=]\s*['"](?P<secret>[^'"\r\n]{4,})['"]"###,
    },
    SecretPattern {
        name: "dotnetConnectionStrings",
        file_context: Some("(?:appsettings|web\\.config).*\\.(?:json|config)$"),
        regex: r###"(?i)\b(?:ConnectionStrings?|connectionString)\s*[:=]\s*['"][^'"]*(?:password|pwd)\s*=\s*(?P<secret>[^;'"\r\n]{4,})[^'"]*['"]"###,
    },
    SecretPattern {
        name: "base64EncodedSecrets",
        file_context: None,
        regex: r###"(?i)\b(?:secret|password|key|token)[_-]?(?:base64|encoded)?['"]?\s*[:=]\s*['"](?P<secret>[A-Za-z0-9+/]{32,}={0,3})['"]"###,
    },
    SecretPattern {
        name: "base64PemPrivateKey",
        file_context: None,
        // Base64 of a `-----BEGIN … PRIVATE KEY` armor line (Kubernetes `data:`,
        // Helm values, JSON): the armor rules never see the decoded markers.
        // Each alternative spells only whole 3-byte groups of the header, so
        // the encoded prefix is fixed; a line-wrapped body continues the match.
        regex: r###"LS0tLS1CRUdJTiB(?:QUklWQVRFIEtFWS|SU0EgUFJJVkFURSBLRVkt|FQyBQUklWQVRFIEtFWS|EU0EgUFJJVkFURSBLRVkt|PUEVOU1NIIFBSSVZBVEUgS0VZ|FTkNSWVBURUQgUFJJVkFURSBLRVkt|QR1AgUFJJVkFURSBLRVkgQkxPQ0st|TU0gyIEVOQ1JZUFRFRCBQUklWQVRFIEtFWS)[A-Za-z0-9+/]*={0,2}(?:\r?\n[ \t]*[A-Za-z0-9+/]{16,}={0,2})*"###,
    },
    SecretPattern {
        name: "rsaPrivateKey",
        file_context: None,
        regex: r###"-----BEGIN\s+(?:RSA\s+)?PRIVATE\s+KEY-----[\s\S]*?-----END\s+(?:RSA\s+)?PRIVATE\s+KEY-----"###,
    },
    SecretPattern {
        name: "opensshPrivateKey",
        file_context: None,
        regex: r###"-----BEGIN\s+OPENSSH\s+PRIVATE\s+KEY-----[\s\S]*?-----END\s+OPENSSH\s+PRIVATE\s+KEY-----"###,
    },
    SecretPattern {
        name: "sshPrivateKeyEncrypted",
        file_context: None,
        regex: r###"-----BEGIN SSH2 ENCRYPTED PRIVATE KEY-----[\s\S]*?-----END SSH2 ENCRYPTED PRIVATE KEY-----"###,
    },
    SecretPattern {
        name: "puttyPrivateKey",
        file_context: None,
        regex: r###"\bPuTTY-User-Key-File-[23]:\s*[\s\S]*?Private-MAC:\b"###,
    },
    SecretPattern {
        name: "firebaseServiceAccountPrivateKey",
        file_context: None,
        regex: r###""private_key":\s*"(?P<secret>-----BEGIN PRIVATE KEY-----\\n[a-zA-Z0-9+/=\\n]+\\n-----END PRIVATE KEY-----)""###,
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
        regex: r###"(?i)\b(?:private[_-]?key|secret[_-]?key)['"]?\s*[:=]\s*["'](?P<secret>[A-Za-z0-9+/]{64,}={0,2})["']"###,
    },
    SecretPattern {
        name: "hexEncodedKey",
        file_context: None,
        regex: r###"(?i)\b(?:key|secret)['"]?\s*[:=]\s*["'](?P<secret>[a-fA-F0-9]{32,})["']"###,
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
        regex: r###"(?i)\b['"]?(?:turso|libsql)(?:[\s\w.-]{0,20})(?:token|auth)['"]?\s*(?::|=>|=)\s*['"]?(?P<secret>[a-zA-Z0-9._-]{50,})['"]?\b"###,
    },
    SecretPattern {
        name: "upstashRedisToken",
        file_context: None,
        regex: r###"(?i)\b['"]?(?:upstash)(?:[\s\w.-]{0,20})(?:token|key)['"]?\s*(?::|=>|=)\s*['"]?(?P<secret>[a-zA-Z0-9=]{40,})['"]?\b"###,
    },
    SecretPattern {
        name: "supabaseJwtKey",
        file_context: None,
        regex: r###"\b['"]?(?:SUPABASE|supabase)_?(?:ANON|SERVICE_ROLE|anon|service_role)?_?(?:KEY|key)['"]?\s*(?::|=>|=)\s*['"]?(?P<secret>(eyJ[a-zA-Z0-9_-]{100,}))['"]?\b"###,
    },
    SecretPattern {
        name: "cockroachdbConnectionString",
        file_context: None,
        regex: r###"(?i)\bpostgresql:\/\/[^:]+:[^@]+@[^/\s]*cockroachlabs\.cloud[^?\s]*\b"###,
    },
];
