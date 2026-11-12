//! The literal check and the walks of a source that passes in pieces. Every value
//! is fake.
use super::super::source_names::{
    Named, Objects, SecretForm, SecretSpans, SourceNames, SourceWalk,
};
use super::super::{EveryValueScan, every_value_spans, objects, yaml_secret};

/// What `text` names, fed in pieces of `size` bytes.
fn named_in_pieces(path: &str, text: &[u8], size: usize) -> Named {
    let mut names = SourceNames::for_path(path);
    for piece in text.chunks(size) {
        names.feed(piece);
    }
    names.finish()
}

fn named(path: &str, text: &str) -> Named {
    let whole = named_in_pieces(path, text.as_bytes(), text.len().max(1));
    for size in [1, 2, 3, 7, 63, 64, 65, 136, 137, 200] {
        assert_eq!(
            named_in_pieces(path, text.as_bytes(), size),
            whole,
            "{text:?} in pieces of {size}"
        );
    }
    whole
}

/// A source naming every family a walk of objects reads: what a walk is built from.
const OBJECTS: Named = Named {
    objects: Objects::ALL,
    yaml: false,
};

const fn only(objects: Objects) -> Named {
    Named {
        objects,
        yaml: false,
    }
}

const KEY: Named = only(Objects {
    secret: false,
    key: true,
    registry: false,
});
const SECRET: Named = only(Objects {
    secret: true,
    key: false,
    registry: false,
});
const REGISTRY: Named = only(Objects {
    secret: false,
    key: false,
    registry: true,
});

/// Only a spelling a walk would act on counts.
#[test]
fn a_name_is_a_structural_spelling() {
    for text in [
        r#"{"kty": "EC"}"#,
        r#"{ "kty" : "EC" }"#,
        "{\"kty\"\n\n:\n\"EC\"}",
        r#"{"note":"{\"kty\":\"EC\"}"}"#,
        r#"{"a":"{\\\"kty\\\":\\\"EC\\\"}"}"#,
        "kty: EC",
        "'kty': EC",
        "kty:",
    ] {
        assert_eq!(named("x.json", text), KEY, "{text}");
    }
    for text in [
        r#"{"kind":"Secret"}"#,
        r#"{ "kind" : "Secret" }"#,
        "{\"kind\"\n:\n\"Secret\"}",
        r#"{"note":"{\"kind\":\"Secret\"}"}"#,
        r#"{"a":"{\\\"kind\\\":\\\"Secret\\\"}"}"#,
    ] {
        assert_eq!(named("x.json", text), SECRET, "{text}");
    }
    for text in [
        "QUJDkty0REVG",
        "QUJD+kty/REVG",
        "the kty field",
        "x-kty: EC",
        "my_kty: EC",
        "Kty::Rsa and kty::Rsa",
        r#"{"kty","x"}"#,
        r#""kty" is a word"#,
        r#"{"type":"Secret"}"#,
        r#"{"kind":"ConfigMap","name":"Secret"}"#,
        r#"{"kind":"SecretList"}"#,
        r#"{"kind":"Secret\n"}"#,
        r#"{"subkind":"Secret"}"#,
        "a Secret, and \"Secret\" alone",
    ] {
        assert_eq!(named("x.json", text), Named::default(), "{text}");
    }
    // A YAML manifest's kind names its document in any file of text (D2,
    // round 13), a `.json` path's too.
    assert_eq!(
        named("x.json", "kind: \"Secret\""),
        Named {
            objects: Objects::default(),
            yaml: true,
        }
    );
}

/// Blanks that outrun the reach make the spelling count: the walk then decides.
#[test]
fn a_name_past_any_blanks_still_counts() {
    let blanks = " ".repeat(300);
    assert_eq!(
        named("x.json", &format!("{{\"kty\"{blanks}: \"EC\"}}")),
        KEY
    );
    assert_eq!(
        named("x.json", &format!("{{\"kind\":{blanks}\"Secret\"}}")),
        SECRET
    );
}

/// `kind: Secret` counts at every path of text, its YAML judged by content
/// (D2, round 13: a `.txt` copy of a manifest is the manifest); source code
/// holds no manifest.
#[test]
fn a_yaml_kind_counts_at_a_yaml_path() {
    let yaml = Named {
        objects: Objects::default(),
        yaml: true,
    };
    for text in [
        "kind: Secret\n",
        "kind: \"Secret\"\n",
        "KIND: 'secret'\n",
        "- kind: Secret\r\n",
        "kind: Secret # note\n",
        "kind: Secret",
    ] {
        assert_eq!(named("a/b.yaml", text), yaml, "{text}");
        assert_eq!(named("a/b.yml", text), yaml, "{text}");
        assert_eq!(named("a/b.txt", text), yaml, "{text}");
        assert_eq!(named("a/b.rs", text), Named::default(), "{text}");
    }
    for text in [
        "kind: ConfigMap\n",
        "kind: SecretList\n",
        "subkind: Secret\n",
        "kind: \n",
    ] {
        assert_eq!(named("a/b.yaml", text), Named::default(), "{text}");
    }
}

fn walk_in_pieces(text: &str, size: usize) -> Option<objects::Found> {
    let mut walk = objects::Walk::new();
    for piece in text.as_bytes().chunks(size) {
        walk.feed(piece);
    }
    walk.finish()
}

const DOCUMENTS: [&str; 6] = [
    "{\n \"data\": {\n  \"blob\": \"QUJD\"\n },\n \"note\": \"x\",\n \"kind\": \"Secret\"\n}\n",
    "{\"kind\":\"Secret\",\"stringData\":{\"db\":\"plain words\"}}",
    "{\"a\":\"{\\\"kind\\\":\\\"Secret\\\",\\\"data\\\":{\\\"z\\\":\\\"v\\\"}}\"}",
    "{\"kty\":\"RSA\",\"d\":\"QUJDREVGR0hJSktMTU5PUFFSU1RVVldYWVo\",\"oth\":[{\"r\":\"QUJD\",\"t\":\"REVG\"}]}",
    "{\"oth\":[{\"r\":\"QUJD\"}],\"d\": 'QUJDREVGR0hJSktMTU5PUFFSU1RVVldYWVo=', \"kty\" :\n\"EC\"}",
    "d: QUJDREVGR0hJSktMTU5PUFFSU1RVVldYWVo==\nkty: EC\n",
];

/// A comment between a member's name and its value, cut anywhere, keeps the
/// value's role as the whole text does.
#[test]
fn a_comment_before_a_value_reads_the_same_in_any_pieces() {
    let value = "QUJD".repeat(10);
    for text in [
        format!("{{\"kind\": /* a*b **/ \"Secret\", \"data\": // c\n{{\"p\": \"{value}\"}}}}"),
        format!("{{\"data\":\n# c\n\n{{\"p\": \"{value}\"}}, \"kind\": \"Secret\"}}"),
    ] {
        let whole = objects::walk(&text);
        assert!(!whole.secret_values.is_empty(), "{text}");
        for size in 1..=text.len() {
            let parts = walk_in_pieces(&text, size).expect("a short text");
            assert_eq!(parts.secret_values, whole.secret_values, "{text} by {size}");
        }
    }
}

/// However a source is cut into pieces, a walk finds what one over all of it does.
#[test]
fn a_walk_finds_the_same_in_any_pieces() {
    for text in DOCUMENTS {
        let whole = objects::walk(text);
        assert!(
            !whole.secret_values.is_empty() || !whole.private_members.is_empty(),
            "{text}"
        );
        for size in 1..=text.len() {
            let parts = walk_in_pieces(text, size).expect("a short text");
            assert_eq!(parts.secret_values, whole.secret_values, "{text} by {size}");
            assert_eq!(
                parts.private_members, whole.private_members,
                "{text} by {size}"
            );
        }
    }
}

#[test]
fn yaml_blocks_are_the_same_in_any_pieces() {
    for text in [
        "apiVersion: v1\nstringData:\n  blob: plain words\n  other: x\nkind: Secret\n---\ndata:\n  a: b\nkind: ConfigMap\n",
        "kind: Secret\r\ndata:\r\n  a: QUJD\r\n",
        "items:\n- kind: Secret\n  data:\n    a: b\n- kind: ConfigMap\n  data:\n    c: d\n",
    ] {
        let whole = yaml_secret::secret_blocks(text);
        assert_eq!(whole.len(), 1, "{text}");
        for size in 1..=text.len() {
            let mut blocks = yaml_secret::Blocks::default();
            for piece in text.as_bytes().chunks(size) {
                blocks.feed(piece);
            }
            assert_eq!(blocks.finish().as_ref(), Some(&whole), "{text} by {size}");
        }
    }
}

/// A value of any length passes with nothing kept of it but what its end asks.
#[test]
fn a_long_token_keeps_nothing_and_still_counts() {
    let long = "QUJD".repeat(1_000_000);
    let text = format!("{{\"kty\":\"EC\",\"d\":\"{long}\",\"x\":\"{long}\",\"{long}\":1}}");
    let mut walk = objects::Walk::new();
    let mut most = 0;
    for piece in text.as_bytes().chunks(64 * 1024) {
        walk.feed(piece);
        most = most.max(walk.weight());
    }
    assert!(most < 64 * 1024, "{most} bytes held");
    let found = walk.finish().expect("kept");
    assert_eq!(found.private_members.len(), 1);
    assert_eq!(
        &text[found.private_members[0].clone()],
        long,
        "the value, to the byte"
    );
}

/// A long line holds no secret outside its head, so it passes the same way.
#[test]
fn a_long_yaml_line_keeps_only_its_head() {
    let long = "x".repeat(4_000_000);
    let text = format!("note: {long}\nkind: Secret\ndata: {long}\nname: y\n");
    let mut blocks = yaml_secret::Blocks::default();
    let mut most = 0;
    for piece in text.as_bytes().chunks(64 * 1024) {
        blocks.feed(piece);
        most = most.max(blocks.weight());
    }
    assert!(most < 64 * 1024, "{most} bytes held");
    let found = blocks.finish().expect("kept");
    assert_eq!(found.len(), 1);
    assert_eq!(&text[found[0].clone()], long);
}

/// Past its budget a walk says it cannot tell, and never that nothing is secret.
#[test]
fn a_walk_over_its_budget_says_so() {
    let open = "{\n".repeat(100_000);
    let text = format!("{{\"kind\":\"Secret\",\"data\":{{\"a\":\"b\"}}}}\n{open}");
    let walked = |budget: usize| {
        let mut walk = SourceWalk::new(OBJECTS, budget);
        for piece in text.as_bytes().chunks(4096) {
            walk.feed(piece);
        }
        walk.finish()
    };
    assert_eq!(walked(1024), SecretSpans::TooMany);
    let SecretSpans::Exact(spans) = walked(usize::MAX) else {
        panic!("within its budget");
    };
    assert_eq!(spans.len(), 1);
    assert_eq!(spans[0].form, SecretForm::SecretInJson);
    assert_eq!(&text[spans[0].range.clone()], "\"a\":\"b\"");
}

/// A token too long to decide gives up the same way: a token waits only so long.
#[test]
fn a_token_that_never_decides_gives_up() {
    let mut walk = objects::Walk::new();
    walk.feed(b"{\"d\":");
    for _ in 0..20 {
        walk.feed(&vec![b' '; 128 * 1024]);
    }
    assert!(walk.finish().is_none());
}

/// Spans come out in source order and never overlap: one inside another goes with it.
#[test]
fn spans_are_ordered_and_apart() {
    let value = "QUJDREVGR0hJSktMTU5PUFFSU1RVVldYWVo";
    let text = format!(
        "{{\"kty\":\"EC\",\"kind\":\"Secret\",\"data\":{{\"kty\":\"EC\",\"d\":\"{value}\"}},\"k\":\"{value}\"}}"
    );
    let mut walk = SourceWalk::new(OBJECTS, usize::MAX);
    walk.feed(text.as_bytes());
    let SecretSpans::Exact(spans) = walk.finish() else {
        panic!("within its budget");
    };
    assert_eq!(spans.len(), 2, "the `d` goes with its block: {spans:?}");
    assert!(spans[0].range.end <= spans[1].range.start);
    assert_eq!(spans[0].form, SecretForm::SecretInJson);
    assert_eq!(spans[1].form, SecretForm::JsonWebKey);
}

/// An escape can skip the first byte of a character: a walk reads bytes, so what
/// follows is a name like any other, where a slice of the text would have panicked.
#[test]
fn an_escape_before_a_multibyte_character_is_read_past() {
    let text = "{\"kty\":\"EC\",\\é: 1,\"d\":\"QUJDREVGR0hJSktMTU5PUFFSU1RVVldYWVo\"}";
    assert_eq!(objects::walk(text).private_members.len(), 1);
}

/// Past the bound a text was once replaced whole; now only a secret's bytes go.
#[test]
fn a_long_text_keeps_all_but_its_secrets() {
    use crate::security::sanitizer::{sanitize_content, sanitize_leaf};
    let token = "ghp_aB3dE5gH7jK9mN1pQ3sT5vX7zA9cE1gI3kM5";
    let words = "word ".repeat(crate::security::MAX_CONTENT_SIZE / 5 + 1);
    let plain = sanitize_content(&words, None).expect("scanned");
    assert!(!plain.has_secrets, "{:?}", plain.secrets_detected);
    assert_eq!(plain.content.len(), words.len());
    let text = format!("{words} token={token} {words}\n");
    let scan = sanitize_content(&text, Some("x.txt")).expect("scanned");
    assert!(!scan.content.contains(&token[4..20]));
    assert!(scan.content.len() > 2 * words.len() - 16, "the words stay");
    assert!(scan.content.contains("[REDACTED-"));
    let leaf = sanitize_leaf(&text, None)
        .expect("scanned")
        .expect("redacted");
    assert!(!leaf.contains(&token[4..20]));
    assert!(leaf.len() > 2 * words.len() - 16, "the words stay");
    assert_eq!(sanitize_leaf(&words, None).expect("scanned"), None);
}

/// A Secret past the old bound is judged as one under it.
#[test]
fn a_secret_past_any_size_is_hidden_by_its_object() {
    use crate::security::sanitizer::sanitize_content;
    let value = "c2VjcmV0LXZhbHVlLW9mLXRoZS1maXh0dXJl";
    let words = "ordinary words\n".repeat(crate::security::MAX_CONTENT_SIZE / 15 + 1);
    let text = format!("{{\"data\":{{\"k\":\"{value}\"}},\n{words}\"kind\":\"Secret\"}}\n");
    let scan = sanitize_content(&text, Some("dump.json")).expect("scanned");
    assert!(!scan.content.contains(value));
    assert!(scan.content.contains("ordinary words"));
}

/// A source records each family it names, and keeps reading for the others: a
/// cut of it is labelled by the family that named it.
#[test]
fn a_source_names_every_family_it_holds() {
    assert_eq!(named("x.json", r#"{"auths":{}}"#), REGISTRY);
    assert_eq!(named("x.json", r#"{"Auths":{}}"#), REGISTRY);
    let kty_then_secret = format!(r#"{{"kty":"EC"}}{}{{"kind":"Secret"}}"#, " ".repeat(500));
    let both = named("x.json", &kty_then_secret);
    assert!(both.objects.key && both.objects.secret && !both.objects.registry);
    assert_eq!(
        named("x.json", r#"{"kty":"EC","auths":{}}"#).objects.form(),
        SecretForm::SecretInJson
    );
    assert_eq!(
        named(
            "x.json",
            &format!(r#"{{"kty":"EC"}}{}{{"auths":{{}}}}"#, " ".repeat(500))
        )
        .objects,
        Objects {
            secret: false,
            key: true,
            registry: true
        }
    );
}

/// A walk past its budget hides every value of the cut under the pattern of the
/// family that named it, never another family's.
#[test]
fn every_value_is_labelled_by_the_family_that_named_the_source() {
    let text = "{\"a\":\"v\",\"b\":{\"c\":\"w\"}}";
    let several = Named {
        objects: Objects {
            secret: true,
            key: true,
            registry: false,
        },
        yaml: false,
    };
    for (named, form, pattern) in [
        (KEY, SecretForm::JsonWebKey, "jwkPrivateMembers"),
        (REGISTRY, SecretForm::DockerAuths, "dockerRegistryAuth"),
        (SECRET, SecretForm::SecretInJson, "kubernetesSecretJson"),
        (several, SecretForm::SecretInJson, "kubernetesSecretJson"),
    ] {
        let spans = every_value_spans(text.as_bytes(), named);
        assert!(!spans.is_empty(), "{named:?}");
        for span in &spans {
            assert_eq!(span.form, form, "{named:?}");
            assert_eq!(span.form.pattern(), pattern);
        }
    }
}

/// The same through a walk that gave up: the fallback is the walk's own.
#[test]
fn a_walk_that_gave_up_falls_back_to_its_family() {
    let open = "{\n".repeat(100_000);
    for (named, form) in [
        (KEY, SecretForm::JsonWebKey),
        (REGISTRY, SecretForm::DockerAuths),
    ] {
        let text = format!("{{\"a\":\"value\"}}\n{open}");
        let mut walk = SourceWalk::new(named, 1024);
        for piece in text.as_bytes().chunks(4096) {
            walk.feed(piece);
        }
        assert_eq!(walk.finish(), SecretSpans::TooMany);
        let spans = every_value_spans(text.as_bytes(), named);
        assert!(spans.iter().all(|span| span.form == form), "{named:?}");
    }
}

/// Whatever the pieces a document is fed in, the scan finds the spans one read of
/// all of it finds (a string, a name's colon or a YAML line cut by a piece's end
/// included), and they are the values the document's structure leaves visible.
#[test]
fn every_value_scan_is_the_same_in_any_pieces() {
    let documents = [
        "{\"a\": \"plain words\", \"b\" : [\"c\", 1], \"n\": 7,\n  \"esc\": \"x\\\"y\"\n}\n",
        "\"a\": \"{\\\"data\\\":\\\"x\\\"}\"\n",
        "\"a\": \"open\n\"b\": \"cut",
        "\"a\"\n :\n\"b\": \"x\" : :\"c\" \"d\":\"e\\",
        "---\nstringData:\n  blob: plain words\n  block: |\n    more\n- item: x\n-  :a\n- \n...\r\nk:v: w\n\n:\n- -\nlast: x",
    ];
    for text in documents {
        for named in [
            Named {
                objects: Objects::ALL,
                yaml: false,
            },
            Named {
                objects: Objects::default(),
                yaml: true,
            },
            Named {
                objects: Objects::ALL,
                yaml: true,
            },
        ] {
            let whole = every_value_spans(text.as_bytes(), named);
            for size in [1, 2, 3, 5, 8] {
                let mut scan = EveryValueScan::new(named);
                for piece in text.as_bytes().chunks(size) {
                    scan.feed(piece);
                }
                assert_eq!(
                    scan.finish(),
                    whole,
                    "{text:?} {named:?} in pieces of {size}"
                );
            }
        }
    }
}

/// A scan resumed at a point the first was quiet at finds the spans the whole
/// scan finds from there on: a window of a long source can start there.
#[test]
fn every_value_scan_resumes_at_a_quiet_point() {
    let text = "{\"a\": \"one\", \"b\": [\"two\", \"three\"],\n\"c\" : \"four\", \"d\": \"fi\\\nve\"}\nk: v\n- x: y\n\"z\": \"s\"\n";
    for named in [
        Named {
            objects: Objects::ALL,
            yaml: false,
        },
        Named {
            objects: Objects::ALL,
            yaml: true,
        },
    ] {
        let whole = every_value_spans(text.as_bytes(), named);
        let mut scan = EveryValueScan::new(named);
        let mut quiet = Vec::new();
        for (at, byte) in text.bytes().enumerate() {
            scan.feed(&[byte]);
            if scan.quiet() {
                quiet.push(at + 1);
            }
        }
        assert!(quiet.len() > 2, "{named:?}: {quiet:?}");
        for &point in &quiet {
            let mut head = EveryValueScan::new(named);
            head.feed(&text.as_bytes()[..point]);
            let mut spans = head.take();
            let mut tail = EveryValueScan::resume(named, point);
            assert_eq!(tail.at(), point);
            tail.feed(&text.as_bytes()[point..]);
            spans.extend(tail.finish());
            spans.sort_by_key(|span| (span.range.start, span.range.end));
            assert_eq!(spans, whole, "{named:?} resumed at {point}");
        }
        // Fed to the first quiet byte at or past a point, it stops there.
        let mut scan = EveryValueScan::new(named);
        let taken = scan.feed_until_quiet(text.as_bytes(), quiet[1]);
        assert_eq!(taken, quiet[1]);
    }
}
