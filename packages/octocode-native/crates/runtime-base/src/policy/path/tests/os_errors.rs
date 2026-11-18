use super::*;

fn policy(root: &Path) -> PathPolicy {
    PathPolicy::new(PathPolicyConfig {
        workspace_root: Some(root.to_path_buf()),
        ..Default::default()
    })
    .expect("policy")
}

/// The OS's own errno decides the message, whichever number it has there.
#[cfg(unix)]
#[test]
fn an_os_error_is_named_by_its_own_errno() {
    let root = fixture();
    let policy = policy(&root);
    for (errno, code, text) in [
        (
            libc::ENAMETOOLONG,
            PolicyErrorCode::NameTooLong,
            "Path name too long",
        ),
        (
            libc::ELOOP,
            PolicyErrorCode::SymlinkLoop,
            "Symlink loop detected at path",
        ),
    ] {
        let error = policy.io_error(std::io::Error::from_raw_os_error(errno), Path::new("x"));
        assert_eq!(error.code, code);
        assert!(error.message.starts_with(text), "{}", error.message);
    }
    // Other errnos stay generic, including numbers another OS gives these two.
    let other = policy.io_error(
        std::io::Error::from_raw_os_error(libc::EINVAL),
        Path::new("x"),
    );
    assert_eq!(other.code, PolicyErrorCode::Io);
}

/// A name beyond the OS limit reports its length, not an unknown failure.
#[cfg(unix)]
#[test]
fn a_path_name_beyond_the_os_limit_says_it_is_too_long() {
    let root = fixture();
    let name = "n".repeat(300);
    let error = policy(&root).validate(&name).expect_err("too long");
    assert_eq!(
        error.code,
        PolicyErrorCode::NameTooLong,
        "{}",
        error.message
    );
}
