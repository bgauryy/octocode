//! Package registries, CI and developer SaaS tokens, and credential-named
//! assignments.
//!
//! A contiguous slice of [`super::PATTERNS`]: its order is part of the
//! evaluation order.

use super::SecretPattern;

/// A read that a fallback literal backs up: a dotted, indexed or called path
/// (`process.env.NAME`, `os.environ["NAME"]`, `os.getenv("NAME")`).
macro_rules! env_read {
    () => {
        r###"[A-Za-z_$][A-Za-z0-9_$]*(?:\.[A-Za-z0-9_$]+|\[[^\]\r\n]*\]|\([^()\r\n]*\))*"###
    };
}

/// The characters of a literal's body: an escape, or anything but its own
/// closing quote.
macro_rules! body {
    (dq) => {
        r###"(?:\\[^\r\n]|[^"\\\r\n])*"###
    };
    (sq) => {
        r###"(?:\\[^\r\n]|[^'\\\r\n])*"###
    };
    (bt) => {
        r###"(?:\\[^\r\n]|[^`\\\r\n])*"###
    };
}

/// A whole literal of any quote type, in a piece list.
macro_rules! any_piece {
    () => {
        concat!(
            r###"(?:""###,
            body!(dq),
            r###"""###,
            r###"|'"###,
            body!(sq),
            r###"'"###,
            r###"|`"###,
            body!(bt),
            r###"`)"###
        )
    };
}

/// What follows a literal's last piece: not another piece. Alternatives are
/// tried in order, so without it the one that stops at a shorter chain
/// (`'a' "b"` before `'a' "b" 'c'`) would win and leave the rest readable.
macro_rules! piece_end {
    () => {
        r###"(?:[ \t]*[^ \t"'`+]|[ \t]*\+[ \t]*[^ \t\r\n"'`]|[ \t]*\z)"###
    };
}

/// What sits between two pieces of one literal: a `+`, or nothing
/// (`"a" "b"` is one string in Python and C), with what source code writes
/// around either (`code_gap`: blanks, line breaks, comments).
macro_rules! piece_seam {
    () => {
        concat!(code_gap!(), r###"(?:\+"###, code_gap!(), ")?")
    };
}

/// One alternative of the split literals: the pieces from an opening quote
/// `$open` (first body `$first`, closed by `$q1`) to the last piece (opened
/// by `$q2`, body `$last`, closed by `$close`), in a group `$name` that holds
/// the text between the first opening quote and the last closing one.
macro_rules! split_literal {
    ($open:literal, $first:expr, $q1:literal, $name:literal, $q2:literal, $last:expr, $close:literal) => {
        concat!(
            $open,
            "(?P<",
            $name,
            ">",
            $first,
            $q1,
            piece_seam!(),
            "(?:",
            any_piece!(),
            piece_seam!(),
            ")*",
            $q2,
            $last,
            ")",
            $close,
            piece_end!()
        )
    };
}

/// The string literals a credential assignment may end in, the secret in a
/// `secret…` group: double-quoted, single-quoted or backtick, each with an
/// optional prefix (`r#"…"#`, `b"…"`, `r'…'`, `@"…"`), read with escapes and,
/// where that read finds no close, as raw text, and, for the pieces of one literal
/// joined with `+` or only written side by side (`"Fake0" + 'Moo9Zed'`,
/// `"Fake0" "Moo9Zed"`), the pieces, of any quote types, as one secret. A
/// single literal is eight characters or more; a joined one is judged on its
/// joined value (`ValueTest::Quoted`).
macro_rules! literals {
    () => {
        concat!(
            r###"(?:"###,
            split_literal!(
                r###"[A-Za-z]{0,2}#*""###,
                body!(dq),
                r###"""###,
                "secret_split_dd",
                r###"""###,
                body!(dq),
                r###"""###
            ),
            "|",
            split_literal!(
                r###"[A-Za-z]{0,2}#*""###,
                body!(dq),
                r###"""###,
                "secret_split_ds",
                "'",
                body!(sq),
                "'"
            ),
            "|",
            split_literal!(
                r###"[A-Za-z]{0,2}#*""###,
                body!(dq),
                r###"""###,
                "secret_split_db",
                "`",
                body!(bt),
                "`"
            ),
            "|",
            split_literal!(
                "'",
                body!(sq),
                "'",
                "secret_split_sd",
                r###"""###,
                body!(dq),
                r###"""###
            ),
            "|",
            split_literal!("'", body!(sq), "'", "secret_split_ss", "'", body!(sq), "'"),
            "|",
            split_literal!("'", body!(sq), "'", "secret_split_sb", "`", body!(bt), "`"),
            "|",
            split_literal!(
                "`",
                body!(bt),
                "`",
                "secret_split_bd",
                r###"""###,
                body!(dq),
                r###"""###
            ),
            "|",
            split_literal!("`", body!(bt), "`", "secret_split_bs", "'", body!(sq), "'"),
            "|",
            split_literal!("`", body!(bt), "`", "secret_split_bb", "`", body!(bt), "`"),
            r###"|[A-Za-z@$]{0,2}#*"(?P<secret>(?:\\[^\r\n]|[^"\\\r\n]){8,})""###,
            r###"|[A-Za-z]{0,2}'(?P<secret_single>(?:\\[^\r\n]|[^'\\\r\n]){8,})'"###,
            r###"|`(?P<secret_backtick>(?:\\[^\r\n]|[^`\\\r\n]){8,})`"###,
            // The same literals read with no escape, after the reads above
            // found none that closes: a raw string (`r"…\"`, `@"…\"`), a Go
            // raw string or a shell single-quoted value holds a backslash as
            // text, so a body that ends in one closes at the quote after it.
            r###"|[A-Za-z@$]{0,2}#*"(?P<secret_raw>[^"\r\n]{8,})""###,
            r###"|[A-Za-z]{0,2}'(?P<secret_single_raw>[^'\r\n]{8,})'"###,
            r###"|`(?P<secret_backtick_raw>[^`\r\n]{8,})`"###,
            // A literal written inside another literal's text, its quotes
            // escaped (`"jwt_secret = \"…\";"`): the escaped quotes are its own.
            r###"|\\"(?P<secret_escaped>(?:[^\\\r\n]|\\[^"\r\n])+?)\\")"###
        )
    };
}

pub(super) const PATTERNS: &[SecretPattern] = &[
    SecretPattern {
        name: "npmAccessToken",
        file_context: None,
        regex: r###"npm_[a-zA-Z0-9]{36}"###,
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
        regex: r###"sgp_[a-zA-Z0-9]{32}"###,
    },
    SecretPattern {
        name: "linearApiKey",
        file_context: None,
        regex: r###"lin_api_[0-9A-Za-z]{40}"###,
    },
    SecretPattern {
        name: "notionIntegrationToken",
        file_context: None,
        regex: r###"ntn_[a-zA-Z0-9_-]{43}"###,
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
        regex: r###"(?i)PMAK-[a-f0-9]{24}-[a-f0-9]{34}"###,
    },
    SecretPattern {
        name: "prefectApiToken",
        file_context: None,
        regex: r###"pnu_[a-zA-Z0-9]{36}"###,
    },
    SecretPattern {
        name: "readmeApiToken",
        file_context: None,
        regex: r###"rdme_[a-z0-9]{70}"###,
    },
    SecretPattern {
        name: "rubygemsApiToken",
        file_context: None,
        regex: r###"rubygems_[a-f0-9]{48}"###,
    },
    SecretPattern {
        name: "clojarsApiToken",
        file_context: None,
        regex: r###"(?i)CLOJARS_[a-z0-9]{60}"###,
    },
    SecretPattern {
        name: "snykApiToken",
        file_context: None,
        regex: r###"(?i)\b['"]?(?:snyk[_.-]?(?:(?:api|oauth)[_.-]?)?(?:key|token))['"]?\s*(?::|=>|=)\s*['"]?(?P<secret>[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12})['"]?\b"###,
    },
    SecretPattern {
        name: "sonarqubeToken",
        file_context: None,
        regex: r###"(?i)(?:squ_|sqp_|sqa_)[a-z0-9=_-]{40}"###,
    },
    SecretPattern {
        name: "travisciAccessToken",
        file_context: None,
        regex: r###"(?i)\b['"]?(?:travis)(?:[\s\w.-]{0,20})['"]?\s*(?::|=>|=)\s*['"]?(?P<secret>[a-z0-9]{22})['"]?\b"###,
    },
    SecretPattern {
        name: "codecovAccessToken",
        file_context: None,
        regex: r###"(?i)\b['"]?(?:codecov)(?:[\s\w.-]{0,20})['"]?\s*(?::|=>|=)\s*['"]?(?P<secret>[a-z0-9]{32})['"]?\b"###,
    },
    SecretPattern {
        name: "droneCiAccessToken",
        file_context: None,
        regex: r###"(?i)\b['"]?(?:droneci|drone)(?:[\s\w.-]{0,20})['"]?\s*(?::|=>|=)\s*['"]?(?P<secret>[a-z0-9]{32})['"]?\b"###,
    },
    SecretPattern {
        name: "octopusDeployApiKey",
        file_context: None,
        regex: r###"\bAPI-[A-Z0-9]{26}\b"###,
    },
    SecretPattern {
        name: "circleciToken",
        file_context: None,
        regex: r###"(?i)\b['"]?(?:circleci|circle)(?:[\s\w.-]{0,20})['"]?\s*(?::|=>|=)\s*['"]?(?P<secret>[a-f0-9]{40})['"]?\b"###,
    },
    SecretPattern {
        name: "buildkiteAgentToken",
        file_context: None,
        regex: r###"bkagent_[a-f0-9]{40}"###,
    },
    SecretPattern {
        name: "launchdarklyAccessToken",
        file_context: None,
        regex: r###"(?i)\b['"]?(?:launchdarkly)(?:[\s\w.-]{0,20})['"]?\s*(?::|=>|=)\s*['"]?(?P<secret>[a-z0-9=_-]{40})['"]?\b"###,
    },
    SecretPattern {
        name: "algoliaApiKey",
        file_context: None,
        regex: r###"(?i)\b['"]?(?:algolia)(?:[\s\w.-]{0,20})['"]?\s*(?::|=>|=)\s*['"]?(?P<secret>[a-z0-9]{32})['"]?\b"###,
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
        regex: r###"\b['"]?(?:VERCEL_OIDC_TOKEN)['"]?\s*(?::|=>|=)\s*['"]?(?P<secret>eyJ[a-zA-Z0-9_-]{100,})['"]?\b"###,
    },
    SecretPattern {
        name: "novuApiKey",
        file_context: None,
        regex: r###"(?i)\b['"]?(?:NOVU|novu)_?(?:API|api)?_?(?:KEY|key)['"]?\s*(?::|=>|=)\s*['"]?(?P<secret>[a-zA-Z0-9]{32,})['"]?\b"###,
    },
    SecretPattern {
        name: "triggerDevApiKey",
        file_context: None,
        regex: r###"\btr_(?:dev|prod)_[a-zA-Z0-9]{20,}\b"###,
    },
    SecretPattern {
        name: "nxCloudAccessToken",
        file_context: None,
        regex: r###"\b['"]?(?:NX_CLOUD_ACCESS_TOKEN|nxCloudAccessToken)['"]?\s*(?::|=>|=)\s*['"]?(?P<secret>[a-zA-Z0-9]{36,})['"]?\b"###,
    },
    SecretPattern {
        name: "depotToken",
        file_context: None,
        regex: r###"dpt_[a-zA-Z0-9]{40,}\b"###,
    },
    SecretPattern {
        name: "grafbaseApiKey",
        file_context: None,
        regex: r###"(?i)\b['"]?(?:GRAFBASE|grafbase)_?(?:API|api)?_?(?:KEY|key)['"]?\s*(?::|=>|=)\s*['"]?(?P<secret>eyJ[a-zA-Z0-9_-]{50,})['"]?\b"###,
    },
    SecretPattern {
        name: "shopifyStorefrontAccessToken",
        file_context: None,
        regex: r###"shpatf_[0-9a-f]{32}"###,
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
        regex: r###"CFPAT-[0-9a-zA-Z]{20}"###,
    },
    SecretPattern {
        name: "mailchimpEcommerceApiKey",
        file_context: None,
        regex: r###"\b[0-9a-f]{32}-[a-z]{2,3}[0-9]{1,2}\b"###,
    },
    SecretPattern {
        name: "credentialsInUrl",
        file_context: None,
        // Any scheme's userinfo password, whatever the user: empty
        // (`redis://:pw@host`) or short (`amqp://u:pw@host`). A host holds
        // no backslash: a literal's `\n` or `\t` escape ends it, so the next
        // URL is a match of its own.
        regex: r###"\b[a-zA-Z]{3,10}:\/\/[^\\/\s:@]{0,64}:(?P<secret>[^\\/\s:@]{1,128})@[^\s'"\\]+\b"###,
    },
    SecretPattern {
        name: "envVarSecrets",
        file_context: None,
        // A quoted value assigned to a credential-named identifier: snake,
        // kebab or dotted (`API_KEY`, `client-secret`, `db.password`) or
        // camelCase (`awsSecret`, `clientSecretKey`), optionally quoted as a
        // JSON/YAML key, with `=`, `:=`, `:` or `=>`. Which values count, by
        // length and key, is the quoted value test's (`ValueTest::Quoted`). A
        // literal is double-quoted, single-quoted or a backtick string; the
        // name may carry a type (`PASSWORD: &str = "…"`) or a number
        // (`apiKey2`), and a fallback operator (`env.DB_PASSWORD || "…"`,
        // `??`) assigns like `=`. Between the name, the operator and the
        // literal source code may write blanks, line breaks and comments
        // (`code_gap`): the value is read through them.
        regex: concat!(
            r###"(?:(?:\b(?:[A-Za-z0-9]+[_.-])*(?i:secret|password|passwd|passphrase|key|token|credential)s?"###,
            name_number!(),
            r###"(?:[_.-][A-Za-z0-9]+)*|\b[a-z][a-z0-9]*(?:[A-Z][a-z0-9]*)*(?:Secret|Password|Passwd|Passphrase|Key|Token|Credential)s?"###,
            name_number!(),
            r###"(?:[A-Z][a-z0-9]*)*)["']?"###,
            code_gap!(),
            r###"(?::[ \t]*[^\s=:"'`,;(){}](?:'[A-Za-z_][A-Za-z0-9_]*|[^=\r\n"'`;(){}])*?\s*)?"###,
            // After the name: an assignment operator, the same followed by a read
            // the literal backs up (`= process.env.X || "…"`, `= env() or "…"`), or
            // a fallback operator right after a name.
            r###"(?:[A-Za-z_][A-Za-z0-9_.*]*(?:\[[^\]\r\n]*\])?\s*(?::=|=)|(?:=>|:=|<-|=|:)\s*(?:"###,
            env_read!(),
            r###"\s*(?:\|\||\?\?|\bor\b)\s*)+|=>|:=|<-|=|:|\|\||\?\?)"###,
            r###"|\b(?:process\.env|import\.meta\.env)(?:\.[A-Za-z0-9_]*(?i:"###,
            credential_words!(),
            r###")[A-Za-z0-9_]*|\[\s*["'][A-Za-z0-9_]*(?i:"###,
            credential_words!(),
            r###")[A-Za-z0-9_]*["']\s*\])\s*(?:\|\||\?\?)|\b(?:os\.environ\.get|os\.getenv|getenv|environ\.get|env\.get)\(\s*["'][A-Za-z0-9_]*(?i:"###,
            credential_words!(),
            r###")[A-Za-z0-9_]*["']\s*(?:,|\)\s*or\b))"###,
            code_gap!(),
            // A constructor call may wrap the literal (`Some("…")`,
            // `String::from("…")`): the name still holds its value.
            r###"(?:(?:[A-Za-z_][A-Za-z0-9_]*(?:::[A-Za-z_][A-Za-z0-9_]*)*!?)?\("###,
            code_gap!(),
            ")?",
            literals!()
        ),
    },
    SecretPattern {
        name: "base64EncodedSecrets",
        file_context: None,
        // A base64 value under a credential-named key: evaluated after
        // `envVarSecrets`, which names the quoted assignments both match, so
        // this one adds only the keys it alone spells (`secretbase64`).
        regex: r###"(?i)\b(?:secret|password|key|token)[_-]?(?:base64|encoded)?['"]?\s*[:=]\s*['"](?P<secret>[A-Za-z0-9+/]{32,}={0,3})['"]"###,
    },
    SecretPattern {
        name: "dotenvSecretAssignment",
        file_context: None,
        // An unquoted dotenv/INI/shell line `NAME=value` whose name holds a
        // credential word. The value must run to the end of the line (or a
        // comment), so code such as `SECRET = os.environ["X"]` is left alone.
        // A list item (`- NAME=value`, a compose or Kubernetes `environment:`
        // entry) is judged as its bare `NAME=value` form: the marker only
        // moves where the line starts. Outside a dotenv or properties file
        // (see `dotenvFileCredential` and `propertiesFileCredential`) a short
        // value is too like any setting to call a credential: it has 12+
        // characters.
        // A diff line (`+` or `-` prefix, inside a hunk) is read as the line
        // itself: see the detector's `DiffLines`.
        regex: concat!(
            r###"(?m)^(?P<diff>[+\-])?[ \t]*"###,
            comment_lead!(),
            r###"(?:-[ \t]+["']?)?(?:export[ \t]+)?(?:[A-Za-z0-9]+[_.])*(?i:"###,
            credential_words!(),
            ")s?",
            name_number!(),
            r###"(?:[_.][A-Za-z0-9]+)*[ \t]*=[ \t]*(?P<secret>[^\s"'$#(){}\[\]:][^\s"'(){}\[\]:]{11,})["']?[ \t]*(?:#[^\r\n]*)?\r?$"###
        ),
    },
    SecretPattern {
        name: "dotenvFileCredential",
        // A dotenv file (`.env`, `.env.*`, `*.env`) holds credentials, so a
        // credential name there says its value is one, however short.
        file_context: Some(super::DOTENV_FILE_CONTEXT),
        // `NAME=value` (a diff line inside a hunk too) whose name
        // holds a credential word; which names count, and which values stay
        // readable, is the file-class value test's (`ValueTest::FileClass`).
        // The value is what a dotenv parser reads: double-quoted (escapes, line
        // breaks), single-quoted (literal, line breaks), a quoted value with
        // text after its closing quote (all of it), or bare up to a ` #`
        // comment, spaces and brackets included.
        regex: concat!(
            r###"(?m)^(?P<diff>[+\-])?[ \t]*"###,
            comment_lead!(),
            r###"(?:-[ \t]+)?(?:export[ \t]+)?[A-Za-z0-9_.\-]*?(?i:"###,
            credential_words!(),
            r###")[A-Za-z0-9_.\-]*[ \t]*=(?:[ \t]+(?P<secret_empty>)#[^\r\n]*\r?$|[ \t]*(?:"(?P<secret>(?:\\[\s\S]|[^"\\])*)"[ \t]*(?:#[^\r\n]*)?\r?$|'(?P<secret_single>[^']*)'[ \t]*(?:#[^\r\n]*)?\r?$|(?P<secret_tail>["'][^\r\n]*?)(?:[ \t]+#[^\r\n]*)?[ \t]*\r?$|(?P<secret_bare>[^\r\n]*?)(?:[ \t]+#[^\r\n]*)?[ \t]*\r?$))"###
        ),
    },
    SecretPattern {
        name: "yamlSecretValue",
        file_context: None,
        // A YAML scalar of any length under a credential-named key: quoted, or
        // plain, which runs to the end of the line less a trailing comment
        // (`password: correct horse 9`) and holds a colon that no blank
        // follows (`password: Zq7x:mPlanted`). The key may sit under a slash-separated path
        // (`secret/data/app/api_token:`). A bare `key:` is too common in YAML
        // (label selectors, tolerations) to count, and the key's credential
        // word ends its name (`secretName:` is no credential), a number
        // after it aside (`api_key2:`). A value in a block comment ends
        // before the comment's closer (`comment_close`). Which values
        // count is the value test's (`ValueTest::Scalar`).
        regex: concat!(
            r###"(?m)^[ \t]*"###,
            comment_lead!(),
            r###"(?:-[ \t]+)?(?:[A-Za-z0-9_.-]+/)*(?:(?:[A-Za-z0-9]+[_.-])*(?i:secret|password|passwd|passphrase|token|api[_-]?key|access[_-]?key|private[_-]?key|auth[_-]?key|credential)s?"###,
            name_number!(),
            r###"(?:[_.-][A-Za-z0-9]+)*|[a-z][a-z0-9]*(?:[A-Z][a-z0-9]*)*(?:Secret|Password|Passwd|Passphrase|Token|ApiKey|AccessKey|PrivateKey|AuthKey|Credential)s?"###,
            name_number!(),
            r###"(?:[A-Z][a-z0-9]*)*)[ \t]*:(?:[ \t]+(?:"(?P<secret>[^"\\\s$\{](?:\\[^\r\n]|[^"\\\r\n])*)"|'(?P<secret_single>[^'\s$\{][^'\r\n]*)'|(?P<secret_bare>[^\s"'#$&*!|>{}\[\]:%@`](?:[^\r\n:#]|:[^\s#])*?))"###,
            // A plain scalar may start on the line after its key, indented
            // under it: one word there (a list item, a nested key or a
            // sentence of a definition list is none).
            r###"|[ \t]*(?:#[^\r\n]*)?\r?\n[ \t]+(?P<secret_next>[^\s"'#$&*!|>{}\[\]:%@`\-](?:[^\s:#]|:[^\s#])*))"###,
            comment_close!(),
            r###"[ \t]*(?:#.*)?\r?$"###
        ),
    },
];
