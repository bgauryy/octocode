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
        // A text naming the kind. `detector::document` hides the `data` and
        // `stringData` blocks of the mapping that says so.
        regex: r###"(?im)\bkind:[ \t]*["']?Secret\b"###,
    },
    SecretPattern {
        name: "kubernetesSecretJson",
        file_context: None,
        // A text naming the kind, as `kubectl -o json` writes it or held escaped in
        // an annotation. It names its own kind beside its data, in the same object,
        // so no path has to say what it is, and `detector::document` hides that
        // object's `data` and `stringData`: a ConfigMap beside it in a List stays.
        regex: r###"\\*"kind\\*"\s*:\s*\\*"Secret\\*""###,
    },
    SecretPattern {
        name: "dockerComposeSecrets",
        file_context: Some("docker-compose\\.ya?ml$"),
        // Compose spells values bare too (`- POSTGRES_PASSWORD=value`); a bare
        // `${NAME}` only names one.
        regex: r###"(?i)\b(?:MYSQL_ROOT_PASSWORD|POSTGRES_PASSWORD|REDIS_PASSWORD|MONGODB_PASSWORD)['"]?\s*[:=]\s*(?:['"](?P<secret>[^'"\r\n]{4,})['"]|(?P<secret_bare>[^'"\s$][^'"\s]{3,}))"###,
    },
    SecretPattern {
        name: "springBootSecrets",
        file_context: Some("(?:application|bootstrap)(?:-\\w+)?\\.(?:properties|ya?ml)$"),
        // A `.properties` value is bare; a bare `${NAME}` only names one.
        regex: r###"(?i)\b(?:spring\.datasource\.password|spring\.security\.oauth2\.client\.registration\..*\.client-secret)\s*[:=]\s*(?:['"](?P<secret>[^'"\r\n]{4,})['"]|(?P<secret_bare>[^'"\s$][^'"\s]{3,}))"###,
    },
    SecretPattern {
        name: "dotnetConnectionStrings",
        // ASP.NET names them `Web.config`, `Web.Release.config`, `App.config`
        // and `appsettings.Production.json`. A connection string sits under
        // any key (`"Default": "…"`), so its own `Password=` is what marks it.
        file_context: Some(
            "(?i)(?:appsettings[^/\\\\]*\\.json|(?:web|app)(?:\\.[^/\\\\]*)?\\.config)$",
        ),
        regex: r###"(?i)['"][^'"\r\n]*\b(?:password|pwd)\s*=\s*(?P<secret>[^;'"\r\n]{4,})"###,
    },
    SecretPattern {
        name: "base64PemPrivateKey",
        file_context: None,
        // Base64 of a `-----BEGIN … PRIVATE KEY` armor line (Kubernetes `data:`,
        // Helm values, JSON): the armor rules never see the decoded markers.
        // Each alternative spells only whole 3-byte groups of the header, so
        // the encoded prefix is fixed; a line-wrapped body continues the match.
        // RFC 4716 spells its own dashes (`---- BEGIN SSH2 …`).
        regex: r###"(?:LS0tLS1CRUdJTiB(?:QUklWQVRFIEtFWS|SU0EgUFJJVkFURSBLRVkt|FQyBQUklWQVRFIEtFWS|EU0EgUFJJVkFURSBLRVkt|PUEVOU1NIIFBSSVZBVEUgS0VZ|FTkNSWVBURUQgUFJJVkFURSBLRVkt|QR1AgUFJJVkFURSBLRVkgQkxPQ0st|TU0gyIEVOQ1JZUFRFRCBQUklWQVRFIEtFWS|TU0gyIFBSSVZBVEUgS0VZ)|LS0tLSBCRUdJTiBTU0gyI(?:EVOQ1JZUFRFRCBQUklWQVRFIEtF|FBSSVZBVEUgS0VZ))[A-Za-z0-9+/]*={0,2}(?:\r?\n[ \t]*[A-Za-z0-9+/]{16,}={0,2})*"###,
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
        name: "ssh2PrivateKey",
        file_context: None,
        // RFC 4716 armor: four dashes and a space, where PEM has five. A run
        // of more dashes still ends in one of these prefixes, which gate the
        // pattern: `-{4,}` alone would make every Markdown rule a candidate.
        regex: r###"-----?[ \t]?BEGIN[ \t]+SSH2(?:[ \t]+ENCRYPTED)?[ \t]+PRIVATE[ \t]+KEY[ \t]*-----?[\s\S]*?-----?[ \t]?END[ \t]+SSH2(?:[ \t]+ENCRYPTED)?[ \t]+PRIVATE[ \t]+KEY[ \t]*-----?"###,
    },
    SecretPattern {
        name: "puttyPrivateKey",
        file_context: None,
        regex: r###"\bPuTTY-User-Key-File-[23]:[\s\S]*?Private-MAC:[ \t]*[0-9A-Fa-f]*"###,
    },
    SecretPattern {
        name: "firebaseServiceAccountPrivateKey",
        file_context: None,
        regex: r###""private_key":\s*"(?P<secret>-----BEGIN PRIVATE KEY-----\\n[a-zA-Z0-9+/=\\n]+\\n-----END PRIVATE KEY-----)""###,
    },
    SecretPattern {
        name: "openvpnClientPrivateKey",
        file_context: None,
        regex: r###"<key>\s*-----BEGIN[^<]*-----END[^<]*<\/key>"###,
    },
    SecretPattern {
        name: "secretKeyArmor",
        file_context: None,
        // Armors that say `SECRET KEY`, as older PGP tools do, or OpenVPN's
        // shared key. PGP's `PUBLIC KEY BLOCK` and `AGE ENCRYPTED FILE` are not.
        regex: r###"-----BEGIN\s+(?:PGP\s+)?SECRET\s+KEY(?:\s+BLOCK)?-----[\s\S]*?-----END\s+(?:PGP\s+)?SECRET\s+KEY(?:\s+BLOCK)?-----|-----BEGIN\s+OpenVPN\s+Static\s+key\s+V\d+-----[\s\S]*?-----END\s+OpenVPN\s+Static\s+key\s+V\d+-----"###,
    },
    SecretPattern {
        name: "urlEncodedPrivateKey",
        file_context: None,
        // A private-key armor percent- or form-encoded once or twice, whose one value
        // runs from BEGIN to where the URL does. A public armor has no such label.
        regex: r###"(?:-{4,}|(?:%2[dD]){4,}|(?:%252[dD]){4,})(?:%20|%2520|\+)?BEGIN(?:(?:%20|%2520|\+)[A-Za-z0-9]+)*?(?:%20PRIVATE|%2520PRIVATE|\+PRIVATE)(?:%20|%2520|\+)KEY[A-Za-z0-9%+/=_.~-]*"###,
    },
    SecretPattern {
        name: "wireguardPrivateKey",
        file_context: None,
        // A key named as a private or preshared one, holding exactly what WireGuard
        // keys are: 32 bytes of base64. `PublicKey =` is the same shape, under its own name.
        regex: r###"(?m)^[ \t#;>*/-]*(?:PrivateKey|privateKey|privatekey|private_key|private-key|PRIVATE_KEY|PRIVATEKEY|PresharedKey|presharedKey|presharedkey|preshared_key|preshared-key|PRESHARED_KEY|PRESHAREDKEY)[ \t]*[=:][ \t]*["']?(?P<secret>[A-Za-z0-9+/]{42}[AEIMQUYcgkosw048]=)["']?[ \t;,]*\r?$"###,
    },
    SecretPattern {
        name: "jwkPrivateMembers",
        file_context: None,
        // A text naming a JSON Web Key. `detector::document` hides the private
        // members of the object with `kty` in it, on either side, and keeps the
        // public ones.
        regex: r###"\bkty\\*["']?[ \t\r\n]*:"###,
    },
    SecretPattern {
        name: "minisignSecretKey",
        file_context: None,
        // A minisign, rsign or signify secret key: named by its comment, or by the
        // header its base64 opens with (`Ed` and its KDF and checksum, which a public
        // key or signature lacks), or base64 of the whole file as Tauri takes it. A
        // signature's comment says `signature from … secret key`.
        regex: r###"untrusted comment:[ \t]*(?:[\w.-]+[ \t]+)?(?:(?:un)?encrypted[ \t]+)?secret key[^\r\n\\]*(?:\\n|\r?\n)[ \t]*(?P<secret>[A-Za-z0-9+/]{40,}={0,2})|\b(?:RWRTY0Iy|RWQAAEIy|RWRCSw)[A-Za-z0-9+/]{60,}={0,2}|\bdW50cnVzdGVkIGNvbW1lbnQ6I(?:G1pbmlzaWduIGVuY3J5cHRlZCBzZWNyZXQga2V5|G1pbmlzaWduIHNlY3JldCBrZXk|HNpZ25pZnkgc2VjcmV0IGtleQ|HJzaWduIGVuY3J5cHRlZCBzZWNyZXQga2V5)[A-Za-z0-9+/]*={0,2}"###,
    },
    SecretPattern {
        name: "nostrSecretKey",
        file_context: None,
        // Bech32 of a Nostr secret key; the public form is `npub1`.
        regex: r###"\bnsec1[02-9ac-hj-np-z]{58}\b"###,
    },
    SecretPattern {
        name: "bip32ExtendedPrivateKey",
        file_context: None,
        // Base58 of a BIP-32 extended private key; the public forms read `pub`.
        regex: r###"\b[xyztuvYZUV]prv[1-9A-HJ-NP-Za-km-z]{100,}\b"###,
    },
    SecretPattern {
        name: "ageSecretKey",
        file_context: None,
        // X25519 and post-quantum identities, and a plugin's. Recipients read `age1`.
        regex: r###"\bAGE-SECRET-KEY-(?:PQ-)?1[QPZRY9X8GF2TVDW0S3JN54KHCE6MUA7L]{58,}\b|\bAGE-PLUGIN-[A-Z0-9-]+?-1[QPZRY9X8GF2TVDW0S3JN54KHCE6MUA7L]{20,}\b"###,
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
        regex: r###"(?i)\b(?:priv(?:ate)?[_-]?key|secret[_-]?key)['"]?\s*[:=]\s*["'](?P<secret>[A-Za-z0-9+/]{64,}={0,2})["']"###,
    },
    SecretPattern {
        name: "hexEncodedKey",
        file_context: None,
        regex: r###"(?i)\b(?:key|secret)['"]?\s*[:=]\s*["'](?P<secret>[a-fA-F0-9]{32,})["']"###,
    },
    SecretPattern {
        name: "postgresqlConnectionString",
        file_context: None,
        regex: r###"(?i)\bpostgresql:\/\/[^:]+:(?P<secret>[^@]+)@[^/\s]+\/[^?\s]+\b"###,
    },
    SecretPattern {
        name: "mysqlConnectionString",
        file_context: None,
        regex: r###"(?i)\bmysql:\/\/[^:]+:(?P<secret>[^@]+)@[^/\s]+\/[^?\s]+\b"###,
    },
    SecretPattern {
        name: "jdbcConnectionStringWithCredentials",
        file_context: Some("(?:\\.env|config|settings|secrets)"),
        regex: r###"(?i)\bjdbc:(?:postgresql|mysql):\/\/[^:]+:(?P<secret>[^@]+)@[^/\s]+\b"###,
    },
    SecretPattern {
        name: "mongodbConnectionString",
        file_context: None,
        regex: r###"\bmongodb(?:\+srv)?:\/\/[a-zA-Z0-9._%-]+:(?P<secret>[a-zA-Z0-9._%-]+)@[a-zA-Z0-9._-]+(?::[0-9]+)?(?:\/[a-zA-Z0-9._-]*)?\b"###,
    },
    SecretPattern {
        name: "redisConnectionString",
        file_context: None,
        regex: r###"\brediss?:\/\/[a-zA-Z0-9._%-]+:(?P<secret>[a-zA-Z0-9._%-]+)@[a-zA-Z0-9._-]+:[0-9]+\b"###,
    },
    SecretPattern {
        name: "redisAuthPassword",
        file_context: None,
        regex: r###"(?m)^[ \t]*(?:(?:redis-cli(?:[ \t]+|>)|[a-zA-Z0-9_.-]+:\d+>[ \t]*))?AUTH[ \t]+(?:[a-zA-Z0-9_.@-]+[ \t]+)?[a-zA-Z0-9_-]{8,}\b"###,
    },
    SecretPattern {
        name: "elasticsearchCredentials",
        file_context: None,
        regex: r###"(?i)\bhttps?:\/\/[^:]+:(?P<secret>[^@]+)@[^/\s]+:9200\b"###,
    },
    SecretPattern {
        name: "couchdbCredentials",
        file_context: None,
        regex: r###"(?i)\bhttp[s]?:\/\/[^:]+:(?P<secret>[^@]+)@[^/\s]+:5984\b"###,
    },
    SecretPattern {
        name: "neo4jCredentials",
        file_context: None,
        regex: r###"(?i)\bbolt[s]?:\/\/[^:]+:(?P<secret>[^@]+)@[^/\s]+:7687\b"###,
    },
    SecretPattern {
        name: "timescaledbConnectionString",
        file_context: None,
        regex: r###"(?i)\btimescaledb:\/\/[^:]+:(?P<secret>[^@]+)@[^/\s]+\/[^?\s]+\b"###,
    },
    SecretPattern {
        name: "clickhouseCredentials",
        file_context: None,
        regex: r###"(?i)\bclickhouse:\/\/[^:]+:(?P<secret>[^@]+)@[^/\s]+:8123\b"###,
    },
    SecretPattern {
        name: "cassandraConnectionString",
        file_context: None,
        regex: r###"(?i)\bcassandra:\/\/[^:]+:(?P<secret>[^@]+)@[^/\s]+:9042\b"###,
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
        regex: r###"(?i)\b(?:postgres|mysql|mongodb|redis):\/\/[^:]+:(?P<secret>[^@]+)@[^/\s]+\b"###,
    },
    SecretPattern {
        name: "clickhouseCloudApiKey",
        file_context: None,
        regex: r###"\b4b1d[A-Za-z0-9]{38}\b"###,
    },
    SecretPattern {
        name: "neonDatabaseConnectionString",
        file_context: None,
        regex: r###"(?i)\bpostgres:\/\/[^:]+:(?P<secret>[^@]+)@[^/\s]*neon\.tech[^?\s]*\b"###,
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
        regex: r###"(?i)\bpostgresql:\/\/[^:]+:(?P<secret>[^@]+)@[^/\s]*cockroachlabs\.cloud[^?\s]*\b"###,
    },
];
