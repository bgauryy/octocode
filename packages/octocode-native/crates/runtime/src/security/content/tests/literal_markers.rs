//! A private-key armor written as a string literal in code is no key: only an
//! actual key block span is replaced, and every other byte of the file, the
//! hashes, base64 and SRI values after the literal among them, reads as
//! written.

use super::*;

const SHA256: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
const SRI: &str =
    "sha512-Z2VuZXJhdGVkRmFrZUludGVncml0eUhhc2hGb3JUZXN0c09ubHlOb3RBS2V5Tm90QVNlY3JldEF0QWxs";
const PNG: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==";

fn read(source: &str) -> String {
    ContentSecurity::new()
        .sanitize_text(source, None)
        .content
        .into_owned()
}

#[test]
fn a_lone_begin_literal_hides_no_later_hash_or_base64() {
    let source = format!(
        "const HEADER = \"-----BEGIN RSA PRIVATE KEY-----\";\nexport const sha = \"{SHA256}\";\nexport const lock = {{ integrity: \"{SRI}\" }};\nexport const img = \"{PNG}\"; // logo\n"
    );
    assert_eq!(read(&source), source);
    assert!(
        private_key_block_line_ranges(&source)
            .iter()
            .all(|&(from, to)| to == from)
    );
}

#[test]
fn a_begin_literal_far_above_a_hash_hides_nothing_of_it() {
    let filler = "let x = compute(1);\n".repeat(40);
    let source = format!(
        "const HEADER = \"-----BEGIN RSA PRIVATE KEY-----\";\n{filler}export const sha = \"{SHA256}\";\nexport const lock = {{ integrity: \"{SRI}\" }};\n"
    );
    assert_eq!(read(&source), source);
}

#[test]
fn begin_and_end_in_separate_literals_keep_the_code_between() {
    let source = "const HEADER = \"-----BEGIN RSA PRIVATE KEY-----\";\nconst FOOTER = \"-----END RSA PRIVATE KEY-----\";\nconst MID = 1;\n";
    assert_eq!(read(source), source);
    let between = "const HEADER = \"-----BEGIN RSA PRIVATE KEY-----\";\nconst FOOTER = \"\";\nconst X = \"-----END RSA PRIVATE KEY-----\";\n";
    assert_eq!(read(between), between);
    let same_line = "const A = \"-----BEGIN RSA PRIVATE KEY-----\"; const B = 2; const C = \"-----END RSA PRIVATE KEY-----\";\n";
    assert_eq!(read(same_line), same_line);
}

#[test]
fn a_real_key_block_is_still_replaced_span_only() {
    let body = "MIIEpQIBAAKCAQEAunterminatedKeyBodyAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
    let source = format!(
        "const a = 1;\nconst k = \"-----BEGIN RSA PRIVATE KEY-----\\n\" +\n  \"{body}\\n\" +\n  \"-----END RSA PRIVATE KEY-----\";\nexport const sha = \"{SHA256}\";\n"
    );
    let out = read(&source);
    assert!(!out.contains(body), "{out}");
    assert!(out.contains("const a = 1;"), "{out}");
    assert!(
        out.contains(&format!("export const sha = \"{SHA256}\";")),
        "{out}"
    );
}

#[test]
fn the_symbols_outline_view_of_a_pem_literal_file_keeps_later_hashes() {
    // An outline keeps the BEGIN row and elides the rest: the text it hands
    // the leaf judge holds a BEGIN with no END and a hash after it.
    let outline = format!(
        "1\tconst HEADER = \"-----BEGIN RSA PRIVATE KEY-----\";\n5\texport const sha = \"{SHA256}\";\n6\texport const img = \"{PNG}\";\n"
    );
    let policy = ContentSecurity::new();
    assert_eq!(policy.sanitize_leaf(&outline), None);
    let (hidden, _) = split_private_key_hidden(&outline, None).unzip();
    assert_eq!(hidden, None);
}

#[test]
fn an_unterminated_key_closes_where_its_body_ends() {
    let body = "MIIEpAIBAAKCAQEAazSbx/LFFsNj0jn5+XC2fxG1hsULQ7Fbo6ACNNDSWAdMYqYy8zqWNA5vY9raTQXY6IYJnKGTko2W6YBMpEuGgNud";
    let source = format!(
        "Your key file starts like this:\n\n    -----BEGIN RSA PRIVATE KEY-----\n    {body}\n    ...\n\n## Downloads\n\n| file | sha256 |\n|------|--------|\n| app.tar.gz | {SHA256} |\n\nRelease notes follow.\n"
    );
    let out = read(&source);
    assert!(!out.contains(body), "{out}");
    assert!(out.contains("## Downloads"), "{out}");
    assert!(out.contains(&format!("| app.tar.gz | {SHA256} |")), "{out}");
    assert!(out.contains("Release notes follow."), "{out}");
}
