//! Private keys, Vault tokens and database connection strings.
//!
//! A contiguous slice of [`super::PATTERNS`]: its order is part of the
//! evaluation order.

use super::SecretPattern;

/// A SQL password literal, its capture groups named `secret…$suffix`: quoted
/// (`'…'`, `N'…'`, `"…"`, a doubled quote or a backslash escape inside is the
/// literal's own character) or dollar-quoted (`$$…$$`, `$tag$…$tag$`).
macro_rules! sql_literal {
    ($suffix:literal) => {
        concat!(
            r###"N?'(?P<secret"###,
            $suffix,
            r###">(?:''|\\.|[^'\\\r\n])+)'|N?"(?P<secret_dq"###,
            $suffix,
            r###">(?:""|\\.|[^"\\\r\n])+)"|\$\$(?P<secret_dollar"###,
            $suffix,
            r###">(?:[^$\r\n]|\$[^$\r\n])+)\$\$|\$[A-Za-z_]\w*\$(?P<secret_tag"###,
            $suffix,
            r###">[^$\r\n]+)\$[A-Za-z_]\w*\$"###
        )
    };
}

pub(super) const PATTERNS: &[SecretPattern] = &[
    SecretPattern {
        name: "sqlUserPassword",
        // The password clause of the statement that creates or alters a
        // database user, role or login (`CREATE USER u IDENTIFIED [WITH
        // plugin] BY 'x'`, `ALTER ROLE r PASSWORD 'x'`, `CREATE LOGIN l WITH
        // PASSWORD = 'x'`). The statement's own shape marks the clause, so
        // `password "Enter your password here"` elsewhere is no match. A quoted
        // token (`'u'@'%'`) may sit between the statement's words and the
        // clause, and a `;` ends the statement. The value is a quoted literal
        // (`'…'`, `N'…'`, `"…"`: a doubled quote or a backslash escape is the
        // literal's own character), a dollar-quoted one (`$$…$$`, `$tag$…$tag$`),
        // or the argument of MySQL's `PASSWORD('…')` or the hash after
        // `IDENTIFIED BY PASSWORD`/`VALUES`. An unquoted word is no literal:
        // a keyword (`NULL`, `EXPIRE`) and a bind (`:pw`, `?`) look the same.
        regex: concat!(
            r###"(?i)\b(?:create|alter)\s+(?:user|role|login)\b(?:[^;'"]|'[^'\r\n]*'|"[^"\r\n]*")*?\b(?:"###,
            r###"identified\s+(?:with\s+\w+\s+)?by\b\s*(?:password\b\s*)?\(?\s*(?:values\s+)?(?:"###,
            sql_literal!(""),
            r###")"###,
            r###"|(?:old_)?password\b\s*=?\s*(?:password\b\s*)?\(?\s*(?:"###,
            sql_literal!("_clause"),
            r###"))"###
        ),
    },
    SecretPattern {
        name: "base64PemPrivateKey",
        // Base64 of a `-----BEGIN … PRIVATE KEY` armor line (Kubernetes `data:`,
        // Helm values, JSON): the armor rules never see the decoded markers.
        // Each alternative spells only whole 3-byte groups of the header, so
        // the encoded prefix is fixed; a line-wrapped body continues the match.
        // RFC 4716 spells its own dashes (`---- BEGIN SSH2 …`).
        regex: r###"(?:LS0tLS1CRUdJTiB(?:QUklWQVRFIEtFWS|SU0EgUFJJVkFURSBLRVkt|FQyBQUklWQVRFIEtFWS|EU0EgUFJJVkFURSBLRVkt|PUEVOU1NIIFBSSVZBVEUgS0VZ|FTkNSWVBURUQgUFJJVkFURSBLRVkt|QR1AgUFJJVkFURSBLRVkgQkxPQ0st|TU0gyIEVOQ1JZUFRFRCBQUklWQVRFIEtFWS|TU0gyIFBSSVZBVEUgS0VZ)|LS0tLSBCRUdJTiBTU0gyI(?:EVOQ1JZUFRFRCBQUklWQVRFIEtF|FBSSVZBVEUgS0VZ))[A-Za-z0-9+/]*={0,2}(?:\r?\n[ \t]*[A-Za-z0-9+/]{16,}={0,2})*"###,
    },
    SecretPattern {
        name: "rsaPrivateKey",
        // The armor names the block and holds none of it: only the body
        // (`secret`, without the line breaks that end the armor lines) is
        // hidden. The body is key-shaped lines (`key_body!`): code between two
        // armor literals ends a match, and no match runs across another armor.
        regex: concat!(
            r###"-----BEGIN\s+(?:RSA\s+)?PRIVATE\s+KEY-----[ \t]*(?:\r?\n)?"###,
            armor_headers!(),
            r###"[ \t]*(?P<secret>"###,
            key_body!(),
            r###")(?:\r?\n)?[ \t]*-----END\s+(?:RSA\s+)?PRIVATE\s+KEY-----"###,
        ),
    },
    SecretPattern {
        name: "opensshPrivateKey",
        // The armored block, or its body with no armor around it (a pasted
        // line, a window cut inside the block): the body opens with base64
        // of the format's own header, `openssh-key-v1` and its NUL, which no
        // public key or certificate holds, and each next line that is base64
        // alone continues the match.
        regex: concat!(
            r###"-----BEGIN\s+OPENSSH\s+PRIVATE\s+KEY-----[ \t]*(?:\r?\n)?"###,
            armor_headers!(),
            r###"[ \t]*(?P<secret>"###,
            key_body!(),
            r###")(?:\r?\n)?[ \t]*-----END\s+OPENSSH\s+PRIVATE\s+KEY-----|b3BlbnNzaC1rZXktdjEA[A-Za-z0-9+/]{40,}={0,2}(?:\r?\n[ \t]*[A-Za-z0-9+/]{16,}={0,2}[ \t]*(?m:$))*"###,
        ),
    },
    SecretPattern {
        name: "ssh2PrivateKey",
        // RFC 4716 armor: four dashes and a space, where PEM has five. A run
        // of more dashes still ends in one of these prefixes, which gate the
        // pattern: `-{4,}` alone would make every Markdown rule a candidate.
        regex: concat!(
            r###"-----?[ \t]?BEGIN[ \t]+SSH2(?:[ \t]+ENCRYPTED)?[ \t]+PRIVATE[ \t]+KEY[ \t]*-----?[ \t]*(?:\r?\n)?"###,
            armor_headers!(),
            r###"[ \t]*(?P<secret>"###,
            key_body!(),
            r###")(?:\r?\n)?[ \t]*-----?[ \t]?END[ \t]+SSH2(?:[ \t]+ENCRYPTED)?[ \t]+PRIVATE[ \t]+KEY[ \t]*-----?"###,
        ),
    },
    SecretPattern {
        name: "puttyPrivateKey",
        // A PuTTY key file's private half: the lines after `Private-Lines:`,
        // each base64 alone, and the value of `Private-MAC:`. Its header lines
        // (`Encryption:`, `Comment:`, `Public-Lines:`, a v3 file's key
        // derivation) and its public lines hold no private key and read as
        // written.
        regex: r###"\bPrivate-Lines:[ \t]*[0-9]+[ \t]*\r?\n(?P<secret>(?:[A-Za-z0-9+/=]+\r?\n)*[A-Za-z0-9+/=]+)[ \t]*\r?(?m:$)|\bPrivate-MAC:[ \t]*(?P<secret_mac>[0-9A-Fa-f]+)"###,
    },
    SecretPattern {
        name: "firebaseServiceAccountPrivateKey",
        regex: r###""private_key":\s*"-----BEGIN PRIVATE KEY-----\\n(?P<secret>[a-zA-Z0-9+/=\\n]+?)\\n-----END PRIVATE KEY-----""###,
    },
    SecretPattern {
        name: "openvpnClientPrivateKey",
        regex: concat!(
            r###"<key>\s*-----BEGIN[^<\r\n]*?-----[ \t]*(?:\r?\n)?"###,
            armor_headers!(),
            r###"[ \t]*(?P<secret>[^<]*?)(?:\r?\n)?[ \t]*-----END[^<]*<\/key>"###,
        ),
    },
    SecretPattern {
        name: "secretKeyArmor",
        // Only the body is hidden, as for `rsaPrivateKey`.
        // Armors that say `SECRET KEY`, as older PGP tools do, or OpenVPN's
        // shared key. PGP's `PUBLIC KEY BLOCK` and `AGE ENCRYPTED FILE` are not.
        regex: concat!(
            r###"-----BEGIN\s+(?:PGP\s+)?SECRET\s+KEY(?:\s+BLOCK)?-----[ \t]*(?:\r?\n)?"###,
            armor_headers!(),
            r###"[ \t]*(?P<secret>"###,
            key_body!(),
            r###")(?:\r?\n)?[ \t]*-----END\s+(?:PGP\s+)?SECRET\s+KEY(?:\s+BLOCK)?-----|-----BEGIN\s+OpenVPN\s+Static\s+key\s+V\d+-----[ \t]*(?:\r?\n)?"###,
            armor_headers!(),
            r###"[ \t]*(?P<secret_vpn>"###,
            key_body!(),
            r###")(?:\r?\n)?[ \t]*-----END\s+OpenVPN\s+Static\s+key\s+V\d+-----"###,
        ),
    },
    SecretPattern {
        name: "urlEncodedPrivateKey",
        // A private-key armor percent- or form-encoded once or twice, whose one value
        // runs from BEGIN to where the URL does. A public armor has no such label.
        regex: r###"(?:-{4,}|(?:%2[dD]){4,}|(?:%252[dD]){4,})(?:%20|%2520|\+)?BEGIN(?:(?:%20|%2520|\+)[A-Za-z0-9]+)*?(?:%20PRIVATE|%2520PRIVATE|\+PRIVATE)(?:%20|%2520|\+)KEY[A-Za-z0-9%+/=_.~-]*"###,
    },
    SecretPattern {
        name: "wireguardPrivateKey",
        // A key named as a private or preshared one, holding exactly what WireGuard
        // keys are: 32 bytes of base64. `PublicKey =` is the same shape, under its own name.
        regex: r###"(?m)^[ \t#;>*/-]*(?:PrivateKey|privateKey|privatekey|private_key|private-key|PRIVATE_KEY|PRIVATEKEY|PresharedKey|presharedKey|presharedkey|preshared_key|preshared-key|PRESHARED_KEY|PRESHAREDKEY)[ \t]*[=:][ \t]*["']?(?P<secret>[A-Za-z0-9+/]{42}[AEIMQUYcgkosw048]=)["']?[ \t;,]*\r?$"###,
    },
    SecretPattern {
        name: "minisignSecretKey",
        // A minisign, rsign or signify secret key: named by its comment, or by the
        // header its base64 opens with (`Ed` and its KDF and checksum, which a public
        // key or signature lacks), or base64 of the whole file as Tauri takes it. A
        // signature's comment says `signature from … secret key`.
        regex: r###"untrusted comment:[ \t]*(?:[\w.-]+[ \t]+)?(?:(?:un)?encrypted[ \t]+)?secret key[^\r\n\\]*(?:\\n|\r?\n)[ \t]*(?P<secret>[A-Za-z0-9+/]{40,}={0,2})|\b(?:RWRTY0Iy|RWQAAEIy|RWRCSw)[A-Za-z0-9+/]{60,}={0,2}|\bdW50cnVzdGVkIGNvbW1lbnQ6I(?:G1pbmlzaWduIGVuY3J5cHRlZCBzZWNyZXQga2V5|G1pbmlzaWduIHNlY3JldCBrZXk|HNpZ25pZnkgc2VjcmV0IGtleQ|HJzaWduIGVuY3J5cHRlZCBzZWNyZXQga2V5)[A-Za-z0-9+/]*={0,2}"###,
    },
    SecretPattern {
        name: "nostrSecretKey",
        // Bech32 of a Nostr secret key; the public form is `npub1`.
        regex: r###"nsec1[02-9ac-hj-np-z]{58}"###,
    },
    SecretPattern {
        name: "bip32ExtendedPrivateKey",
        // Base58 of a BIP-32 extended private key; the public forms read `pub`.
        regex: r###"\b[xyztuvYZUV]prv[1-9A-HJ-NP-Za-km-z]{100,}\b"###,
    },
    SecretPattern {
        name: "ageSecretKey",
        // X25519 and post-quantum identities, and a plugin's. Recipients read `age1`.
        // The tag says what follows whatever alphabet a copy keeps.
        regex: r###"AGE-SECRET-KEY-(?:PQ-)?1[A-Z0-9]{58,}\b|AGE-PLUGIN-[A-Z0-9-]+?-1[QPZRY9X8GF2TVDW0S3JN54KHCE6MUA7L]{20,}\b"###,
    },
    SecretPattern {
        name: "vaultBatchToken",
        regex: r###"hvb\.[a-zA-Z0-9_-]{20,}\b"###,
    },
    SecretPattern {
        name: "vaultServiceToken",
        regex: r###"hvs\.[a-zA-Z0-9_-]{20,}\b"###,
    },
    SecretPattern {
        name: "vaultPeriodicToken",
        regex: r###"hvp\.[a-zA-Z0-9_-]{20,}\b"###,
    },
    SecretPattern {
        name: "postgresqlConnectionString",
        regex: concat!(
            r###"(?i)\bpostgresql:\/\/[^:]+:"###,
            url_password!(r###"[^@]+"###),
            r###"[^/\s\\]+\/[^?\s]+\b"###
        ),
    },
    SecretPattern {
        name: "mysqlConnectionString",
        regex: concat!(
            r###"(?i)\bmysql:\/\/[^:]+:"###,
            url_password!(r###"[^@]+"###),
            r###"[^/\s\\]+\/[^?\s]+\b"###
        ),
    },
    SecretPattern {
        name: "jdbcConnectionStringWithCredentials",
        regex: concat!(
            r###"(?i)\bjdbc:(?:postgresql|mysql):\/\/[^:]+:"###,
            url_password!(r###"[^@]+"###),
            r###"[^/\s\\]+\b"###
        ),
    },
    SecretPattern {
        name: "mongodbConnectionString",
        regex: concat!(
            r###"\bmongodb(?:\+srv)?:\/\/[a-zA-Z0-9._%-]+:"###,
            url_password!(r###"[a-zA-Z0-9._%-]+"###),
            r###"[a-zA-Z0-9._-]+(?::[0-9]+)?(?:\/[a-zA-Z0-9._-]*)?\b"###
        ),
    },
    SecretPattern {
        name: "redisConnectionString",
        regex: concat!(
            r###"\brediss?:\/\/[a-zA-Z0-9._%-]+:"###,
            url_password!(r###"[a-zA-Z0-9._%-]+"###),
            r###"[a-zA-Z0-9._-]+:[0-9]+\b"###
        ),
    },
    SecretPattern {
        name: "neo4jCredentials",
        regex: concat!(
            r###"(?i)\bbolt[s]?:\/\/[^:]+:"###,
            url_password!(r###"[^@]+"###),
            r###"[^/\s\\]+:7687\b"###
        ),
    },
    SecretPattern {
        name: "timescaledbConnectionString",
        regex: concat!(
            r###"(?i)\btimescaledb:\/\/[^:]+:"###,
            url_password!(r###"[^@]+"###),
            r###"[^/\s\\]+\/[^?\s]+\b"###
        ),
    },
    SecretPattern {
        name: "clickhouseCredentials",
        regex: concat!(
            r###"(?i)\bclickhouse:\/\/[^:]+:"###,
            url_password!(r###"[^@]+"###),
            r###"[^/\s\\]+:8123\b"###
        ),
    },
    SecretPattern {
        name: "cassandraConnectionString",
        regex: concat!(
            r###"(?i)\bcassandra:\/\/[^:]+:"###,
            url_password!(r###"[^@]+"###),
            r###"[^/\s\\]+:9042\b"###
        ),
    },
    SecretPattern {
        name: "faunadbKey",
        regex: r###"\bfn[a-zA-Z0-9]{40}\b"###,
    },
    SecretPattern {
        name: "databricksApiToken",
        regex: r###"\bdapi[a-f0-9]{32}(?:-\d)?\b"###,
    },
    SecretPattern {
        name: "pineconeApiKey",
        regex: r###"(?i)\bpinecone[\s\w]*(?:api|key)[\s:=]*["']?(?P<secret>[a-z0-9_-]{32})\b"###,
    },
    SecretPattern {
        name: "databaseUrlWithCredentials",
        regex: concat!(
            r###"(?i)\b(?:postgres|mysql|mongodb|redis):\/\/[^:]+:"###,
            url_password!(r###"[^@]+"###),
            r###"[^/\s\\]+\b"###
        ),
    },
    SecretPattern {
        name: "clickhouseCloudApiKey",
        regex: r###"\b4b1d[A-Za-z0-9]{38}\b"###,
    },
    SecretPattern {
        name: "neonDatabaseConnectionString",
        regex: concat!(
            r###"(?i)\bpostgres:\/\/[^:]+:"###,
            url_password!(r###"[^@]+"###),
            r###"[^/\s\\]*neon\.tech[^?\s]*\b"###
        ),
    },
    SecretPattern {
        name: "tursoDatabaseToken",
        regex: r###"(?i)\b['"]?(?:turso|libsql)(?:[\s\w.-]{0,20})(?:token|auth)['"]?\s*(?::|=>|=)\s*['"]?(?P<secret>[a-zA-Z0-9._-]{50,})['"]?\b"###,
    },
    SecretPattern {
        name: "upstashRedisToken",
        regex: r###"(?i)\b['"]?(?:upstash)(?:[\s\w.-]{0,20})(?:token|key)['"]?\s*(?::|=>|=)\s*['"]?(?P<secret>[a-zA-Z0-9=]{40,})['"]?\b"###,
    },
    SecretPattern {
        name: "supabaseJwtKey",
        regex: r###"\b['"]?(?:SUPABASE|supabase)_?(?:ANON|SERVICE_ROLE|anon|service_role)?_?(?:KEY|key)['"]?\s*(?::|=>|=)\s*['"]?(?P<secret>(eyJ[a-zA-Z0-9_-]{100,}))['"]?\b"###,
    },
    SecretPattern {
        name: "cockroachdbConnectionString",
        regex: concat!(
            r###"(?i)\bpostgresql:\/\/[^:]+:"###,
            url_password!(r###"[^@]+"###),
            r###"[^/\s\\]*cockroachlabs\.cloud[^?\s]*\b"###
        ),
    },
];
