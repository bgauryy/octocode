use super::*;
use crate::policy::path::PathPolicyConfig;

fn policy(allow: bool) -> PathPolicy {
    PathPolicy::new(PathPolicyConfig {
        allow_credential_files: allow,
        ..PathPolicyConfig::default()
    })
    .expect("policy")
}

/// Every name that identifies a file is judged by the one decision: the path,
/// a rename's old path and a link's target.
#[test]
fn any_name_of_a_file_withholds_its_text() {
    let table: &[(&str, Option<&str>, Option<&str>, bool)] = &[
        ("src/lib.rs", None, None, false),
        (".pgpass", None, None, true),
        ("notes.txt", Some(".netrc"), None, true),
        ("env", None, Some(".env"), true),
        ("env", None, Some("config/defaults.txt"), false),
        ("a.txt", Some("b.txt"), Some("c.txt"), false),
        ("a.txt", Some("b.txt"), Some(".ssh/config"), true),
    ];
    for &(path, previous, target, withheld) in table {
        assert_eq!(
            policy(false).withheld_text(path, previous, target),
            withheld,
            "{path} {previous:?} {target:?}"
        );
        assert_eq!(
            withheld_text_by_default(path, previous, target),
            withheld,
            "default: {path} {previous:?} {target:?}"
        );
    }
}

/// The user's opt-in lifts credential file names, never a sensitive
/// directory, for any of the names; the default stays withheld.
#[test]
fn the_opt_in_lifts_names_but_not_directories() {
    let allowing = policy(true);
    assert!(!allowing.withheld_text(".env", None, None));
    assert!(!allowing.withheld_text("env", None, Some(".env")));
    assert!(allowing.withheld_text("env", None, Some(".ssh/id_rsa")));
    assert!(withheld_text_by_default(".env", None, None));
}
