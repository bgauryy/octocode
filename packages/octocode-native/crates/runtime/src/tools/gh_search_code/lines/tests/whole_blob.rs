//! A listed line whose secrecy only the whole blob shows.
use super::*;

/// A listed line of a Kubernetes Secret is judged with the whole blob: the
/// value's own line, and its neighbors, say nothing of the kind above them.
#[test]
fn a_listed_line_is_judged_with_the_whole_blob_when_a_window_cannot_show_it() {
    let security = crate::security::ContentSecurity::new();
    let value = "QUJDREVGR0hJSktMTU5PUFFSU1RVVldYWVowMTIz";
    let text = format!(
        "apiVersion: v1\nkind: Secret\nmetadata:\n  name: x\n  a: 1\n  b: 2\n  c: 3\ndata:\n  tls.crt: {value}\n"
    );
    let hits = scan(
        text.as_bytes(),
        "deploy/secret.yaml",
        &["tls.crt".to_owned()],
        "",
        Window {
            before: 1,
            after: 0,
        },
        &security,
    );
    let FileHits::Lines { lines, .. } = hits else {
        panic!("the blob is read");
    };
    assert!(!lines.join("\n").contains(value), "{lines:?}");
    // No such pattern applies at this path: the bytes are plain text.
    let hits = scan(
        text.as_bytes(),
        "deploy/secret.txt",
        &["tls.crt".to_owned()],
        "",
        Window::default(),
        &security,
    );
    let FileHits::Lines { lines, .. } = hits else {
        panic!("the blob is read");
    };
    assert!(lines.join("\n").contains(value), "{lines:?}");
}

/// A listed member of a private JWK, with `kty` further than any window from
/// it: judged with the whole blob, in any file. Its public members stay.
#[test]
fn a_listed_jwk_member_is_judged_with_the_whole_blob() {
    let security = crate::security::ContentSecurity::new();
    let scalar = "Fake0Scalar9Not4Real8Value7Long6Enough5Xy";
    let public = "Public0Coordinate9Stays8Readable7Always6Ab";
    let text = format!(
        "{{\n  \"crv\": \"P-256\",\n  \"d\": \"{scalar}\",\n  \"kid\": \"a\",\n  \"use\": \"sig\",\n  \"alg\": \"ES256\",\n  \"kty\": \"EC\",\n  \"x\": \"{public}\"\n}}\n"
    );
    let hits = scan(
        text.as_bytes(),
        "keys/signing.json",
        &["\"d\"".to_owned(), "\"x\"".to_owned()],
        "",
        Window::default(),
        &security,
    );
    let FileHits::Lines { lines, .. } = hits else {
        panic!("the blob is read");
    };
    let shown = lines.join("\n");
    assert!(!shown.contains(scalar), "{lines:?}");
    assert!(shown.contains(public), "{lines:?}");
}

/// A secret whose name and value sit on different lines is judged with the
/// whole blob, as localFetch judges it: every listed line is that line of the
/// shared sealed view (`source_view::prepare::sealed_view`), at any path.
#[test]
fn a_secret_across_a_line_edge_lists_as_the_sealed_view_shows_it() {
    let security = crate::security::ContentSecurity::new();
    let blobs = [
        "needle = 1\npassword =\n    \"Xq7Lm2Vb9Rt4Wz6Kp1Hs8Dn3\"\n",
        "needle = 1\nAWS_SECRET_ACCESS_KEY=\nwJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY\n",
        "needle = 1\nheaders = {\"Authorization\":\n  \"Bearer eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.dozjgNryP4J3jVmNHl0w5N_XgL0n3I9PlFUP0THsR8U\"}\n",
        "needle = 1\napi_key: >\n  sk_live_4eC39HqLyjWDarjtT1zdp7dcXq\n",
        "needle = 1\nclient_secret\n= \"a8F3kL9pQ2zX7vB4nM6cR1tY5wE0uI8o\"\n",
    ];
    // Each blob's fake secret value, which no listed line may show.
    let secrets = [
        "Xq7Lm2Vb9Rt4Wz6Kp1Hs8Dn3",
        "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY",
        "dozjgNryP4J3jVmNHl0w5N_XgL0n3I9PlFUP0THsR8U",
        "sk_live_4eC39HqLyjWDarjtT1zdp7dcXq",
        "a8F3kL9pQ2zX7vB4nM6cR1tY5wE0uI8o",
    ];
    let mut differ = Vec::new();
    for (index, text) in blobs.iter().enumerate() {
        let path = "src/settings.py";
        let sealed = crate::tools::source_view::prepare::sealed_view(
            text.as_bytes(),
            std::path::Path::new(path),
            &security,
        )
        .expect("sealed");
        let view: Vec<&str> = sealed.text.lines().collect();
        let hits = scan(
            text.as_bytes(),
            path,
            &["needle".to_owned()],
            "",
            Window {
                before: 0,
                after: 2,
            },
            &security,
        );
        let FileHits::Lines { lines, .. } = hits else {
            panic!("the blob is read");
        };
        assert!(
            !lines.join("\n").contains(secrets[index]),
            "blob {index}: {lines:?}"
        );
        for listed in &lines {
            let (number, shown) = listed
                .split_once(crate::response::numbered::SEPARATOR)
                .expect("numbered");
            let number: usize = number.parse().expect("number");
            if view.get(number - 1) != Some(&shown) {
                differ.push((
                    index,
                    listed.clone(),
                    view.get(number - 1).map(|line| (*line).to_owned()),
                ));
            }
        }
    }
    assert!(differ.is_empty(), "{differ:#?}");
}

/// The same for a Kubernetes Secret written as JSON.
#[test]
fn a_listed_line_of_a_secret_in_json_is_judged_with_the_whole_blob() {
    let security = crate::security::ContentSecurity::new();
    let value = "QUJDREVGR0hJSktMTU5PUFFSU1RVVldYWVowMTIz";
    let text = format!(
        "{{\n  \"data\": {{\n    \"tls.crt\": \"{value}\"\n  }},\n  \"kind\": \"Secret\"\n}}\n"
    );
    let hits = scan(
        text.as_bytes(),
        "deploy/secret.json",
        &["tls.crt".to_owned()],
        "",
        Window::default(),
        &security,
    );
    let FileHits::Lines { lines, .. } = hits else {
        panic!("the blob is read");
    };
    assert!(!lines.join("\n").contains(value), "{lines:?}");
}
