//! A listed line whose secrecy only the whole blob shows.
use super::*;

/// A listed line of a private key's body, its armor further than any window
/// from it, is judged with the whole blob: the line alone says nothing of the
/// block it sits in. The lines around the block stay.
#[test]
fn a_listed_key_body_line_is_judged_with_the_whole_blob() {
    let security = crate::security::ContentSecurity::new();
    let body = "MIIEowIBAAKCAQEAu1SU1LfVLPHCozMxH2Mo4lgOEePzNm0tRgeLezV6ffAt0gun";
    let text = format!(
        "key: |\n  -----BEGIN RSA PRIVATE KEY-----\n{}  {body}\n  -----END RSA PRIVATE KEY-----\nafter: plain\n",
        format!("  {body}\n").repeat(8)
    );
    let hits = scan(
        text.as_bytes(),
        "deploy/key.yaml",
        &[body[40..52].to_owned(), "after".to_owned()],
        "",
        Window::default(),
        &security,
    );
    let FileHits::Lines { lines, .. } = hits else {
        panic!("the blob is read");
    };
    let shown = lines.join("\n");
    assert!(!shown.contains(&body[8..40]), "{lines:?}");
    assert!(shown.contains("after: plain"), "{lines:?}");
}

/// A secret whose name and value sit on different lines is judged with the
/// whole blob, as localFetch judges it: every listed line is that line of the
/// shared sealed view (`source_view::prepare::sealed_view`), at any path.
#[test]
fn a_secret_across_a_line_edge_lists_as_the_sealed_view_shows_it() {
    let security = crate::security::ContentSecurity::new();
    let blobs = [
        "needle = 1\nAWS_SECRET_ACCESS_KEY=\nwJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY\n",
        "needle = 1\nheaders = {\"Authorization\":\n  \"Bearer eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.dozjgNryP4J3jVmNHl0w5N_XgL0n3I9PlFUP0THsR8U\"}\n",
        "needle = 1\napi_key: >\n  sk_live_4eC39HqLyjWDarjtT1zdp7dcXq\n",
    ];
    // Each blob's fake secret value, which no listed line may show.
    let secrets = [
        "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY",
        "dozjgNryP4J3jVmNHl0w5N_XgL0n3I9PlFUP0THsR8U",
        "sk_live_4eC39HqLyjWDarjtT1zdp7dcXq",
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
