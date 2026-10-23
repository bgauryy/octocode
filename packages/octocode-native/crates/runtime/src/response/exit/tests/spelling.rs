/// Error codes are typed end to end: production source names a contract
/// `errorCode` only as the generated `ErrorCode` enum. Every constructor,
/// row builder and code-returning function takes or returns `ErrorCode`,
/// so the compiler already refuses a string there; this scan covers what
/// it cannot see: JSON bodies (`"errorCode": "x"`,
/// `["errorCode"] = json!("x")`), a row's `errorCode` read and compared
/// with a literal, a `code` compared or matched against one, a `code:`
/// field set to one, and the body of a `fn ..code..() -> &'static str`.
#[test]
fn no_error_code_is_spelled_as_a_string() {
    let crates = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crates dir");
    let mut scanned = 0;
    let mut spelled = Vec::new();
    for krate in ["runtime/src", "cli/src"] {
        let mut pending = vec![crates.join(krate)];
        while let Some(dir) = pending.pop() {
            for entry in std::fs::read_dir(&dir).expect("read crate dir").flatten() {
                let path = entry.path();
                let name = entry.file_name().to_string_lossy().into_owned();
                if path.is_dir() {
                    if !["tests", "test_support", "fixtures"].contains(&name.as_str()) {
                        pending.push(path);
                    }
                    continue;
                }
                if !name.ends_with(".rs") || name == "tests.rs" || name.ends_with("_tests.rs") {
                    continue;
                }
                let source = std::fs::read_to_string(&path).expect("read source");
                scanned += 1;
                for (line, form) in spelled_codes(&source) {
                    spelled.push(format!("{}:{line}: {form}", path.display()));
                }
            }
        }
    }
    assert!(scanned > 300, "the scan reads production source: {scanned}");
    assert!(
        spelled.is_empty(),
        "spell these contract errorCodes as ErrorCode::…:\n{}",
        spelled.join("\n")
    );
}

/// The scan catches each form it guards, and leaves the typed spellings,
/// other vocabularies (`category`, engine diagnostics) and test modules.
#[test]
fn the_spelled_code_scan_finds_each_string_form() {
    let source = r#"
fn a() -> Value { json!({"error": m, "errorCode": "invalidInput"}) }
fn b(row: &mut Value) { row["errorCode"] = json!("notFound"); }
fn c(data: &Value) -> bool { data["errorCode"] == "rateLimited" }
fn d(row: &Value) -> bool { row.pointer("/data/errorCode").and_then(Value::as_str) == Some("staleSnapshot") }
fn e(code: &str) -> bool { code == "timeout" }
fn f(error: &E) -> bool { matches!(error.code.as_str(), "cancelled" | "closed") }
fn g() -> Failure { Failure { code: "requestFailed".into(), message } }
fn local_code(kind: Kind) -> &'static str {
    match kind {
        Kind::A => "fileReadFailed",
    }
}
fn typed(data: &Value) -> Value { json!({"errorCode": ErrorCode::InvalidInput, "category": "noLocations"}) }
fn other(category: &str, diagnostic: &D) -> bool { category == "noDiagnostics" || diagnostic.code == "structural.compile.failed" }
#[cfg(test)]
mod tests {
    fn pinned() -> Value { json!({"errorCode": "invalidInput"}) }
}
"#;
    let found: Vec<usize> = spelled_codes(source)
        .into_iter()
        .map(|(line, _)| line)
        .collect();
    assert_eq!(
        found,
        [2, 3, 4, 5, 6, 7, 8, 11],
        "{:?}",
        spelled_codes(source)
    );
}

/// `(line, text)` of each contract `errorCode` spelled as a string in
/// `source`'s production part (a trailing `#[cfg(test)]` module is not).
fn spelled_codes(source: &str) -> Vec<(usize, String)> {
    use crate::id::error_codes::parse;
    let test_module = regex::Regex::new(r"\n#\[cfg\(test\)\]\s*\nmod ").expect("pattern");
    let source = test_module
        .find(source)
        .map_or(source, |test| &source[..test.start()]);
    // The value of an `errorCode` JSON key is a code whatever it says.
    let json_value = regex::Regex::new(concat!(
        r#""errorCode"\s*:\s*"([^"]*)""#,
        r#"|\["errorCode"\]\s*=\s*json!\(\s*"([^"]*)""#,
    ))
    .expect("pattern");
    // A literal compared with a code: only a declared spelling is one.
    let compared = regex::Regex::new(concat!(
        r#"errorCode"\]?\)?[^;\n]*?[!=]=\s*(?:Some\()?"([A-Za-z]+)""#,
        r#"|\b(?:code|error_code)\s*[!=]=\s*(?:Some\()?"([A-Za-z]+)""#,
        r#"|\bcode:\s*"([A-Za-z]+)""#,
        r#"|matches!\([^,]*\bcode\b[^,]*,\s*"([A-Za-z]+)""#,
    ))
    .expect("pattern");
    let code_fn = regex::Regex::new(r"fn \w*code\w*\([^)]*\)\s*->\s*(?:Option<)?&'static str\s*\{")
        .expect("pattern");
    let literal = regex::Regex::new(r#""([A-Za-z]+)""#).expect("pattern");
    let line_of = |offset: usize| source[..offset].matches('\n').count() + 1;
    let text_of = |offset: usize| {
        let start = source[..offset].rfind('\n').map_or(0, |at| at + 1);
        let end = source[offset..]
            .find('\n')
            .map_or(source.len(), |at| offset + at);
        source[start..end].trim().to_owned()
    };
    let mut found = std::collections::BTreeMap::new();
    for captures in json_value.captures_iter(source) {
        let at = captures.get(0).expect("match").start();
        found.entry(line_of(at)).or_insert_with(|| text_of(at));
    }
    for captures in compared.captures_iter(source) {
        let code = captures.iter().skip(1).flatten().next().expect("group");
        if parse(code.as_str()).is_some() {
            found
                .entry(line_of(code.start()))
                .or_insert_with(|| text_of(code.start()));
        }
    }
    for head in code_fn.find_iter(source) {
        let mut depth = 1;
        let body = &source[head.end()..];
        let end = body
            .char_indices()
            .find(|(_, c)| {
                depth += match c {
                    '{' => 1,
                    '}' => -1,
                    _ => 0,
                };
                depth == 0
            })
            .map_or(body.len(), |(end, _)| end);
        for code in literal
            .captures_iter(&body[..end])
            .filter_map(|c| c.get(1))
            .filter(|code| parse(code.as_str()).is_some())
        {
            let at = head.end() + code.start();
            found.entry(line_of(at)).or_insert_with(|| text_of(at));
        }
    }
    found.into_iter().collect()
}
