//! A row cut from a private-key block: a key pasted across string literals
//! has no marker in the piece a pattern selects.

use super::*;

fn key_file() -> (String, Vec<String>) {
    let body = (0..4)
        .map(|at| format!("{at}Qx7Zk2Vb9Ls4Tm6Yp1Wn8Rd3Hc5Gj0Fa7Eu2Si9Oq4Lk6Xv1Bz3Nm8Pt5Dy{at}"))
        .collect::<Vec<_>>();
    let mut lines = vec!["const k = \"-----BEGIN RSA PRIVATE KEY-----\\n\" +".to_owned()];
    lines.extend(body.iter().map(|line| format!("  \"{line}\\n\" +")));
    lines.push("  \"-----END RSA PRIVATE KEY-----\";".to_owned());
    lines.push("const other = \"plain text\";".to_owned());
    (format!("{}\n", lines.join("\n")), body)
}

#[test]
fn no_row_or_capture_shows_a_line_of_a_key_block() {
    let (text, body) = key_file();
    let root = Fixture::new();
    let path = root.write("k.js", text);
    let rows = [
        json!({"operation":"match","path":path,"rule":{"kind":"string"}}),
        json!({"operation":"match","path":path,"rule":{"kind":"string"},"includeCaptures":true}),
        json!({"operation":"match","path":path,"rule":{"pattern":"$A + $B"},"includeCaptures":true}),
        json!({"operation":"match","path":root.0,"parser":"javascript","rule":{"kind":"string"}}),
    ];
    for row in rows {
        let out = run(&root.0, row.clone()).expect("match");
        let shown = out.to_string();
        // The block's lines are what a read withholds: no match is made of
        // them, and nothing of them is shown.
        for line in &body {
            assert!(!shown.contains(line.as_str()), "{row}: {shown}");
        }
    }
    let out = run(
        &root.0,
        json!({"operation":"match","path":path,"rule":{"kind":"string"}}),
    )
    .expect("match");
    assert!(out.to_string().contains("plain text"), "{out}");
}

/// A file with no key keeps its rows as they are.
#[test]
fn a_file_with_no_key_block_is_not_touched() {
    let root = Fixture::new();
    let path = root.write(
        "a.js",
        "const a = \"MIIEpQIBAAKCAQEAplainBase64LookingStringValue00\";\n",
    );
    let out = run(
        &root.0,
        json!({"operation":"match","path":path,"rule":{"kind":"string"}}),
    )
    .expect("match");
    assert!(
        out.to_string().contains("plainBase64LookingStringValue00"),
        "{out}"
    );
}

/// A string cut from a keyed secret has nothing of its key in it: the line it
/// came from says what it is. The other members stay readable.
#[test]
fn no_row_shows_a_keyed_secret_cut_from_its_line() {
    let scalar = "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY";
    let public = "Public0Coordinate9Stays8Readable7Always6Ab";
    let root = Fixture::new();
    let path = root.write(
        "k.ts",
        format!(
            "export const k = {{\n  region: \"eu\",\n  x: \"{public}\",\n  aws_secret_access_key: \"{scalar}\",\n}};\n"
        ),
    );
    let out = run(
        &root.0,
        json!({"operation":"match","path":path,"rule":{"kind":"string"}}),
    )
    .expect("match");
    let shown = out.to_string();
    assert!(!shown.contains(scalar), "{shown}");
    assert!(shown.contains(public), "{shown}");
}
