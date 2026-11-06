use super::*;

#[test]
fn complete_unicode_configuration_and_empty_files_are_preserved() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("Cargo.toml");
    let text = format!("{}\n[package]\nname = '日本語'\n", "# é🚀\n".repeat(20_000));
    std::fs::write(&path, &text).unwrap();
    assert_eq!(read_utf8(&path).unwrap(), text);
    std::fs::write(&path, "").unwrap();
    assert_eq!(read_utf8(&path).unwrap(), "");
}

#[test]
fn missing_nonregular_and_invalid_utf8_inputs_are_errors() {
    let directory = tempfile::tempdir().unwrap();
    assert!(read_utf8(directory.path()).is_err());
    let path = directory.path().join("Cargo.toml");
    assert!(read_utf8(&path).is_err());
    std::fs::write(&path, [0xff]).unwrap();
    assert_eq!(
        read_utf8(&path).unwrap_err().kind(),
        io::ErrorKind::InvalidData
    );
}

#[test]
fn growth_after_the_opened_length_never_returns_a_prefix() {
    use std::io::Write;
    let mut file = tempfile::NamedTempFile::new().unwrap();
    file.write_all(b"[package]\nname = 'before'\n").unwrap();
    let (opened, metadata) = crate::text::open_regular(file.path()).unwrap();
    file.write_all(b"version = '1.0.0'\n").unwrap();
    assert_eq!(
        read_opened(opened, metadata.len()).unwrap_err().kind(),
        io::ErrorKind::InvalidData
    );
}

#[test]
fn a_stopped_caller_does_not_open_configuration() {
    let stopped = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
    crate::deep_stack::while_stoppable(&stopped, || {
        assert_eq!(
            read_utf8(Path::new("a-missing-configuration"))
                .unwrap_err()
                .kind(),
            io::ErrorKind::Interrupted
        );
    });
}

#[cfg(unix)]
#[test]
fn intentional_configuration_symlinks_still_work() {
    let directory = tempfile::tempdir().unwrap();
    let target = directory.path().join("target.toml");
    let link = directory.path().join("Cargo.toml");
    let text = "[package]\nname = 'linked'\n";
    std::fs::write(&target, text).unwrap();
    std::os::unix::fs::symlink(&target, &link).unwrap();
    assert_eq!(read_utf8(&link).unwrap(), text);
}
