//! Every private-key text format the guard must hold, built from generated
//! fake key bytes, and the lines around them that must not start a block.

pub(crate) struct KeyFormat {
    pub(crate) name: &'static str,
    /// The whole source, with readable lines before and after the key.
    pub(crate) text: String,
    /// The key's body lines, none of which may be shown.
    pub(crate) secrets: Vec<String>,
}

/// Fake base64 body lines, unique per `seed`: never a real key.
fn body(seed: u64) -> Vec<String> {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut state = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
    let mut line = |len: usize| {
        (0..len)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                char::from(ALPHABET[usize::try_from(state % 64).expect("small")])
            })
            .collect::<String>()
    };
    let mut lines: Vec<String> = (0..5).map(|_| line(64)).collect();
    lines.push(format!("{}==", line(10)));
    lines
}

fn around(lines: &[String]) -> String {
    format!("intro line\n{}\ntail line\n", lines.join("\n"))
}

/// A format of `open`, `headers`, the body and `close`, each line under `wrap`.
fn block(
    name: &'static str,
    seed: u64,
    open: &str,
    headers: &[&str],
    close: &str,
    wrap: impl Fn(&str) -> String,
) -> KeyFormat {
    let secrets = body(seed);
    let lines: Vec<String> = std::iter::once(open.to_owned())
        .chain(headers.iter().map(|header| (*header).to_owned()))
        .chain(secrets.iter().cloned())
        .chain((!close.is_empty()).then(|| close.to_owned()))
        .map(|line| wrap(&line))
        .collect();
    KeyFormat {
        name,
        text: around(&lines),
        secrets,
    }
}

fn pem(name: &'static str, seed: u64, label: &str) -> KeyFormat {
    block(
        name,
        seed,
        &format!("-----BEGIN {label}-----"),
        &[],
        &format!("-----END {label}-----"),
        str::to_owned,
    )
}

fn rsa(name: &'static str, seed: u64, wrap: impl Fn(&str) -> String) -> KeyFormat {
    block(
        name,
        seed,
        "-----BEGIN RSA PRIVATE KEY-----",
        &[],
        "-----END RSA PRIVATE KEY-----",
        wrap,
    )
}

fn ssh2(name: &'static str, seed: u64, headers: &[&str]) -> KeyFormat {
    block(
        name,
        seed,
        "---- BEGIN SSH2 ENCRYPTED PRIVATE KEY ----",
        headers,
        "---- END SSH2 ENCRYPTED PRIVATE KEY ----",
        str::to_owned,
    )
}

fn putty(name: &'static str, seed: u64, version: u8, before_private: &[&str]) -> KeyFormat {
    let public = body(seed + 100);
    let secrets = body(seed);
    let mut lines = vec![
        format!("PuTTY-User-Key-File-{version}: ssh-rsa"),
        "Encryption: none".to_owned(),
        "Comment: fake".to_owned(),
        format!("Public-Lines: {}", public.len()),
    ];
    lines.extend(public);
    lines.extend(before_private.iter().map(|line| (*line).to_owned()));
    lines.push(format!("Private-Lines: {}", secrets.len()));
    lines.extend(secrets.iter().cloned());
    lines.push("Private-MAC: 0123456789abcdef0123456789abcdef01234567".to_owned());
    KeyFormat {
        name,
        text: around(&lines),
        secrets,
    }
}

/// One line holding the whole key, its newlines escaped.
fn one_line(name: &'static str, seed: u64, prefix: &str, open: &str, close: &str) -> KeyFormat {
    let secrets = body(seed);
    let key = secrets.join("\\n");
    KeyFormat {
        name,
        text: format!("{{\n{prefix}\"{open}\\n{key}\\n{close}\\n\",\n\"tail line\": 1\n}}\n"),
        secrets,
    }
}

/// The PEM, PGP and RFC 4716 armors, with and without headers.
fn armored() -> Vec<KeyFormat> {
    vec![
        ssh2("rfc4716 encrypted", 1, &["Comment: \"fake test key\""]),
        block(
            "rfc4716 plain",
            2,
            "---- BEGIN SSH2 PRIVATE KEY ----",
            &["Comment: fake"],
            "---- END SSH2 PRIVATE KEY ----",
            str::to_owned,
        ),
        ssh2(
            "rfc4716 continued header",
            3,
            &[
                "Comment: a long comment \\",
                "that continues here",
                "x-tag_name: /bin/true",
            ],
        ),
        block(
            "rfc4716 no header",
            4,
            "---- BEGIN SSH2 ENCRYPTED PRIVATE KEY ----",
            &[],
            "---- END SSH2 ENCRYPTED PRIVATE KEY ----",
            str::to_owned,
        ),
        block(
            "rfc4716 no end",
            5,
            "---- BEGIN SSH2 ENCRYPTED PRIVATE KEY ----",
            &["Comment: fake"],
            "",
            str::to_owned,
        ),
        pem("openssh", 6, "OPENSSH PRIVATE KEY"),
        pem("rsa", 7, "RSA PRIVATE KEY"),
        pem("dsa", 8, "DSA PRIVATE KEY"),
        pem("ec", 9, "EC PRIVATE KEY"),
        pem("encrypted", 10, "ENCRYPTED PRIVATE KEY"),
        pem("pkcs8", 11, "PRIVATE KEY"),
        pem("any label", 12, "ANY PRIVATE KEY"),
        pem("dashed label", 13, "XYZ-1 PRIVATE KEY"),
        block(
            "pem armor headers",
            14,
            "-----BEGIN RSA PRIVATE KEY-----",
            &[
                "Proc-Type: 4,ENCRYPTED",
                "DEK-Info: AES-128-CBC,0123ABCD",
                "",
            ],
            "-----END RSA PRIVATE KEY-----",
            str::to_owned,
        ),
        block(
            "pgp armor headers",
            15,
            "-----BEGIN PGP PRIVATE KEY BLOCK-----",
            &["Version: GnuPG v2", "Comment: fake key", ""],
            "-----END PGP PRIVATE KEY BLOCK-----",
            str::to_owned,
        ),
    ]
}

/// PuTTY keys, wrapped lines, JSON strings and marker spellings.
fn wrapped() -> Vec<KeyFormat> {
    let padding = "x".repeat(300);
    vec![
        putty("ppk v2", 16, 2, &[]),
        putty(
            "ppk v3",
            17,
            3,
            &[
                "Key-Derivation: Argon2id",
                "Argon2-Memory: 8192",
                "Argon2-Salt: 0123456789abcdef",
            ],
        ),
        rsa("leading whitespace", 18, |line| format!("    {line}")),
        rsa("quote prefix", 19, |line| format!("\"{line}\\n\" +")),
        rsa("mail quote", 20, |line| format!("> {line}")),
        rsa("hash comment", 21, |line| format!("# {line}")),
        rsa("slash comment", 22, |line| format!("// {line}")),
        rsa("star comment", 23, |line| format!(" * {line}")),
        rsa("bytes literal", 24, |line| format!("  b\"{line}\\n\"")),
        rsa("double spaced", 25, |line| format!("{line}\n")),
        one_line(
            "service account json",
            26,
            "\"private_key\": ",
            "-----BEGIN PRIVATE KEY-----",
            "-----END PRIVATE KEY-----",
        ),
        one_line(
            "json any label",
            27,
            "\"k\": ",
            "-----BEGIN ANY PRIVATE KEY-----",
            "-----END ANY PRIVATE KEY-----",
        ),
        one_line(
            "json rfc4716",
            28,
            "\"k\": ",
            "---- BEGIN SSH2 ENCRYPTED PRIVATE KEY ----",
            "---- END SSH2 ENCRYPTED PRIVATE KEY ----",
        ),
        one_line(
            "json past a long prefix",
            29,
            &format!("\"pad\": \"{padding}\", \"private_key\": "),
            "-----BEGIN PRIVATE KEY-----",
            "-----END PRIVATE KEY-----",
        ),
        block(
            "tab after begin",
            30,
            "-----BEGIN\tRSA PRIVATE KEY-----",
            &[],
            "-----END\tRSA PRIVATE KEY-----",
            str::to_owned,
        ),
        block(
            "tabs throughout",
            31,
            "-----BEGIN\tRSA\tPRIVATE\tKEY-----",
            &[],
            "-----END\tRSA\tPRIVATE\tKEY-----",
            str::to_owned,
        ),
        block(
            "marker past 256 bytes",
            32,
            &format!("{padding} -----BEGIN RSA PRIVATE KEY-----"),
            &[],
            "-----END RSA PRIVATE KEY-----",
            str::to_owned,
        ),
        block(
            "rfc4716 past 256 bytes",
            33,
            &format!("{padding} ---- BEGIN SSH2 PRIVATE KEY ----"),
            &[],
            "---- END SSH2 PRIVATE KEY ----",
            str::to_owned,
        ),
        block(
            "no end marker",
            34,
            "-----BEGIN OPENSSH PRIVATE KEY-----",
            &[],
            "",
            str::to_owned,
        ),
        block(
            "marker with no closing dashes",
            35,
            "-----BEGIN RSA PRIVATE KEY",
            &[],
            "-----END RSA PRIVATE KEY",
            str::to_owned,
        ),
    ]
}

/// A key whose body `lines` arranges.
fn arranged(name: &'static str, seed: u64, lines: impl Fn(&[String]) -> Vec<String>) -> KeyFormat {
    let secrets = body(seed);
    KeyFormat {
        name,
        text: around(&lines(&secrets)),
        secrets,
    }
}

/// One line holding both edges, and lines of a body that are not key text.
fn interrupted() -> Vec<KeyFormat> {
    const BEGIN: &str = "-----BEGIN RSA PRIVATE KEY-----";
    const END: &str = "-----END RSA PRIVATE KEY-----";
    let with = |first: &[&str], rest: &[String], last: &[&str]| {
        first
            .iter()
            .map(|line| (*line).to_owned())
            .chain(rest.iter().cloned())
            .chain(last.iter().map(|line| (*line).to_owned()))
            .collect::<Vec<_>>()
    };
    let joined = format!("{END}{BEGIN}");
    let spaced = format!("{END} {BEGIN}");
    let worded = format!("first {END} x {BEGIN}");
    let split = |secrets: &[String], line: &str, close: &[&str]| {
        let mut lines = with(&[BEGIN], &secrets[..2], &[line]);
        lines.extend(with(&[], &secrets[2..], close));
        lines
    };
    vec![
        arranged("end then begin joined", 40, |secrets| {
            with(&[joined.as_str()], secrets, &[END])
        }),
        arranged("end then begin spaced", 41, |secrets| {
            with(&[spaced.as_str()], secrets, &[END])
        }),
        arranged("end then begin with no later end", 42, |secrets| {
            with(&[worded.as_str()], secrets, &[])
        }),
        arranged("comment in a body", 43, |secrets| {
            split(secrets, "# rotated key follows", &[END])
        }),
        arranged("comment in a body with no end", 44, |secrets| {
            split(secrets, "# rotated key follows", &[])
        }),
        arranged("prose before a body", 45, |secrets| {
            with(&[BEGIN, "this key rotates weekly"], secrets, &[END])
        }),
        arranged("prose before a body with no end", 46, |secrets| {
            with(&[BEGIN, "this key rotates weekly"], secrets, &[])
        }),
        arranged("leading plus concatenation", 47, |secrets| {
            let mut lines = vec![format!("const k = \"{BEGIN}\\n\"")];
            lines.extend(secrets.iter().map(|line| format!("  + \"{line}\\n\"")));
            lines.push(format!("  + \"{END}\";"));
            lines
        }),
    ]
}

pub(crate) fn formats() -> Vec<KeyFormat> {
    let mut all = armored();
    all.extend(wrapped());
    all.extend(interrupted());
    let mut crlf = pem("crlf", 36, "RSA PRIVATE KEY");
    crlf.text = crlf.text.replace('\n', "\r\n");
    all.push(crlf);
    let mut crlf = ssh2("rfc4716 crlf", 37, &["Comment: fake"]);
    crlf.text = crlf.text.replace('\n', "\r\n");
    all.push(crlf);
    let mut bom = pem("bom", 38, "RSA PRIVATE KEY");
    bom.text = format!("\u{feff}{}", bom.text.replacen("intro line\n", "", 1));
    all.push(bom);
    let mut truncated = pem("truncated body", 39, "OPENSSH PRIVATE KEY");
    let cut = truncated.text.find(&truncated.secrets[2]).expect("a line");
    truncated.text.truncate(cut);
    truncated.text.push_str("tail line\n");
    truncated.secrets.truncate(2);
    all.push(truncated);
    all
}

/// Lines that look a little like an armor marker, and must not start a block
/// over the base64 run that follows them.
pub(crate) const NOT_MARKERS: [&str; 16] = [
    "----",
    "---- ----",
    "----------------",
    "| ---- | ---- |",
    "| BEGIN | PRIVATE KEY |",
    "| ---- | BEGIN PRIVATE KEY | ---- |",
    "---- text ----",
    "---- BEGIN SECTION ----",
    "---- END OF FILE ----",
    "---- BEGIN SSH2 PUBLIC KEY ----",
    "-----BEGIN CERTIFICATE-----",
    "--- BEGIN PRIVATE KEY ---",
    "-- BEGIN PRIVATE KEY --",
    "BEGIN PRIVATE KEY",
    "See the Private-Lines: field of PuTTY-User-Key-File-3: below",
    "check(line.startsWith(\"-----BEGIN RSA PRIVATE KEY\"));",
];
