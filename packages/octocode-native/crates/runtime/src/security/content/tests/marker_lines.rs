//! Generated lines around every private-key marker form, to prove the
//! marker scan decides exactly as the plain rule does. A fixed seed keeps a
//! failure reproducible.

/// Every BEGIN/END armor the content guard or the engine's key patterns name,
/// in both dash forms.
const ARMORS: [&str; 12] = [
    "PRIVATE KEY",
    "RSA PRIVATE KEY",
    "DSA PRIVATE KEY",
    "EC PRIVATE KEY",
    "OPENSSH PRIVATE KEY",
    "ENCRYPTED PRIVATE KEY",
    "SSH2 ENCRYPTED PRIVATE KEY",
    "PGP PRIVATE KEY BLOCK",
    "CERTIFICATE",
    "PUBLIC KEY",
    "DH PARAMETERS",
    "private key",
];

/// Forms close to a marker that either check could get wrong.
const NEAR_MISSES: [&[u8]; 42] = [
    b"",
    b"-",
    b"----",
    b"-----",
    b"------",
    b"----------",
    b"-----BEGIN",
    b"-----BEGIN ",
    b"-----END",
    b"-----END ",
    b"-----BEGINRSA PRIVATE KEY-----",
    b"-----BEGIN\tRSA PRIVATE KEY-----",
    b"-----BEGIN  RSA PRIVATE  KEY-----",
    b"-----ENDRSA PRIVATE KEY-----",
    b"-----BEGIN RSA PRIVATE KEY",
    b"-----BEGIN RSA PRIVATE KEY----",
    b"-----BEGIN RSA PRIVATE\nKEY-----",
    b"----BEGIN RSA PRIVATE KEY-----",
    b"---- BEGIN SSH2 ENCRYPTED PRIVATE KEY ----",
    b"---- END SSH2 ENCRYPTED PRIVATE KEY ----",
    b"-- ---BEGIN RSA PRIVATE KEY-----",
    b"PRIVATE KEY",
    b"PRIVATE KEY-----",
    b"-----BEGIN CERTIFICATE----- PRIVATE KEY",
    b"-----begin rsa private key-----",
    b"-----Begin Rsa Private Key-----",
    b"-----BEG\xFFIN RSA PRIVATE KEY-----",
    b"-----\xFFBEGIN RSA PRIVATE KEY-----",
    b"-----BEGIN RSA PRIV\xC3ATE KEY-----",
    "\u{2014}\u{2014}BEGIN RSA PRIVATE KEY\u{2014}\u{2014}".as_bytes(),
    b"----\tBEGIN\tRSA\tPRIVATE\tKEY\t----",
    b"-----BEGIN RSA PRIVATE\tKEY-----",
    b"---- ----BEGIN RSA PRIVATE KEY----",
    b"----BE-----BEGIN RSA PRIVATE KEY-----",
    b"-----BEGIN X-----END Y PRIVATE KEY-----",
    b"-----BEGIN RSA PRIVATE KEY---",
    b"-----END RSA PRIVATE KEY  ",
    b"---BEGIN RSA PRIVATE KEY---",
    b"| ---- | BEGIN PRIVATE KEY | ---- |",
    b"Private-Lines: 14",
    b"Private-MAC: 0123abcd",
    b"PuTTY-User-Key-File-3: ssh-ed25519",
];

const LEADS: [&[u8]; 12] = [
    b"",
    b" ",
    b"\t \t",
    b"\r",
    b"\x0b\x0c",
    b"\xEF\xBB\xBF",
    b"\xEF\xBB\xBF  ",
    b"\xFF",
    b"\xC3",
    b"\xE2\x82",
    "\u{a0}\u{85}\u{2003}".as_bytes(),
    b"const key = `",
];

const TAILS: [&[u8]; 8] = [
    b"",
    b"\n",
    b"\r\n",
    b" ",
    b"\xFF",
    b"\xE2\x82",
    b"`;",
    "\u{fffd}".as_bytes(),
];

/// A long run before or after a marker: where a check that reads only a
/// line's head cuts it, a multi-byte char included.
fn padded(line: &[u8]) -> [Vec<u8>; 4] {
    let run = |bytes: &[u8]| [bytes, line].concat();
    [
        run(&[b' '; 240]),
        run(&[b'x'; 300]),
        [&[b' '; 255][..], "\u{e9}".as_bytes(), line].concat(),
        [line, &[b'x'; 300]].concat(),
    ]
}

/// Every marker form and near miss under every lead and tail, the same again
/// past a head's cut, each marker with one byte changed, then seeded splices
/// of all of those pieces and arbitrary bytes.
pub(crate) fn lines() -> Vec<Vec<u8>> {
    let markers: Vec<Vec<u8>> = ARMORS
        .iter()
        .flat_map(|armor| {
            ["BEGIN", "END"].into_iter().flat_map(move |edge| {
                [
                    format!("-----{edge} {armor}-----").into_bytes(),
                    format!("---- {edge} {armor} ----").into_bytes(),
                ]
            })
        })
        .collect();
    let forms: Vec<Vec<u8>> = markers
        .iter()
        .cloned()
        .chain(NEAR_MISSES.iter().map(|form| form.to_vec()))
        .collect();

    let mut lines = Vec::new();
    for form in &forms {
        for lead in LEADS {
            for tail in TAILS {
                lines.push([lead, form.as_slice(), tail].concat());
            }
        }
        lines.extend(padded(form));
    }
    for marker in &markers {
        for at in 0..marker.len() {
            let mut changed = marker.clone();
            changed[at] = 0xFF;
            lines.push(changed);
            let mut removed = marker.clone();
            removed.remove(at);
            lines.push(removed);
            lines.push(marker[at..].to_vec());
            lines.push(marker[..at].to_vec());
        }
    }

    let mut state = 0x9E37_79B9_7F4A_7C15_u64;
    let mut next = |below: usize| {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        usize::try_from(state % u64::try_from(below).expect("small")).expect("small")
    };
    for _ in 0..20_000 {
        let mut line = Vec::new();
        for _ in 0..=next(4) {
            match next(4) {
                0 => line.extend_from_slice(LEADS[next(LEADS.len())]),
                1 => line.extend_from_slice(TAILS[next(TAILS.len())]),
                2 => line.extend_from_slice(&forms[next(forms.len())]),
                _ => line.push(u8::try_from(next(256)).expect("a byte")),
            }
        }
        lines.push(line);
    }
    lines
}
