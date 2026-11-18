//! A file named after a credential is listed with a placeholder in its name;
//! that shown name resolves back to the one entry it stands for.

use super::*;

const TOKEN: &str = "ghp_a1B2c3D4e5F6g7H8i9J0k1L2m3N4o5P6q7R8";
const SHOWN: &str = "deploy-[REDACTED-GITHUBTOKENS].txt";

fn policy(root: &Path) -> PathPolicy {
    PathPolicy::new(PathPolicyConfig {
        workspace_root: Some(root.to_path_buf()),
        ..Default::default()
    })
    .expect("policy")
}

#[test]
fn the_shown_name_of_a_credential_named_file_reaches_the_file() {
    let root = fixture();
    std::fs::create_dir_all(root.join("conf")).expect("dir");
    std::fs::write(
        root.join("conf").join(format!("deploy-{TOKEN}.txt")),
        "hello",
    )
    .expect("file");
    std::fs::write(root.join("conf/other.txt"), "x").expect("file");
    let policy = policy(&root);
    let read = policy
        .validate_read(format!("conf/{SHOWN}"))
        .expect("the shown name resolves");
    assert_eq!(
        std::fs::read_to_string(&read.canonical).expect("read"),
        "hello"
    );
    // The true name resolves as before, and its display is the shown name.
    let direct = policy
        .validate_read(format!("conf/deploy-{TOKEN}.txt"))
        .expect("true name");
    assert_eq!(direct.canonical, read.canonical);
    // A directory named after a credential resolves in the middle of a path.
    std::fs::create_dir_all(root.join(format!("d-{TOKEN}"))).expect("dir");
    std::fs::write(root.join(format!("d-{TOKEN}/in.txt")), "in").expect("file");
    assert!(
        policy
            .validate_read("d-[REDACTED-GITHUBTOKENS]/in.txt")
            .is_ok()
    );
}

#[test]
fn a_shown_name_with_no_entry_or_several_is_an_ordinary_missing_path() {
    let root = fixture();
    let policy = policy(&root);
    let missing = policy.validate_read(SHOWN).expect_err("no entry");
    assert_eq!(missing.code, PolicyErrorCode::NotFound);
    // Two different credentials in one name shape both show the same text.
    let second = "ghp_Z9y8X7w6V5u4T3s2R1q0P9o8N7m6L5k4J3i2";
    std::fs::write(root.join(format!("deploy-{TOKEN}.txt")), "a").expect("file");
    std::fs::write(root.join(format!("deploy-{second}.txt")), "b").expect("file");
    let error = policy.validate_read(SHOWN).expect_err("two entries");
    assert_eq!(error.code, PolicyErrorCode::NotFound);
    assert!(
        error.message.contains("several entries"),
        "{}",
        error.message
    );
}
