use super::*;
use crate::policy::discovery::{CREDENTIAL_FILE_EXTENSIONS, CREDENTIAL_FILE_NAMES};

#[test]
fn credential_filename_rules_admit_directories_but_withhold_files() {
    let root = tempfile::tempdir().expect("fixture");
    let policy = PathPolicy::new(PathPolicyConfig {
        workspace_root: Some(root.path().to_path_buf()),
        ..Default::default()
    })
    .expect("policy");
    let names = CREDENTIAL_FILE_NAMES
        .iter()
        .map(|name| (*name).to_owned())
        .chain(
            CREDENTIAL_FILE_EXTENSIONS
                .iter()
                .map(|ext| format!("source{ext}")),
        )
        .chain(
            [
                ".env",
                ".env.production",
                "id_rsa.old",
                "credentials.json.bak",
                "master.key~",
            ]
            .map(str::to_owned),
        );
    let walk = policy.discovery_walk();
    for name in names {
        let dir = root.path().join("directories").join(&name);
        std::fs::create_dir_all(&dir).expect("directory");
        let child = dir.join("module.ts");
        std::fs::write(&child, "export const value = 1;\n").expect("source");
        assert!(policy.validate(&dir).is_ok(), "directory {name}");
        assert!(policy.validate_read(&child).is_ok(), "child of {name}");
        assert!(
            policy.validate_output(dir.join("new.ts")).is_ok(),
            "output in {name}"
        );
        assert_eq!(
            policy.validate_read(&dir).unwrap_err().code,
            PolicyErrorCode::NotRegular
        );
        for kind in [None, Some(std::fs::metadata(&dir).unwrap().file_type())] {
            assert!(walk.permits_entry(&dir, kind), "walk directory {name}");
        }
        let file = root.path().join("files").join(&name);
        std::fs::create_dir_all(file.parent().unwrap()).expect("file parent");
        assert_eq!(
            policy.validate_output(&file).unwrap_err().code,
            PolicyErrorCode::IgnoredPath
        );
        std::fs::write(&file, "ordinary content\n").expect("credential-named file");
        assert_eq!(
            policy.validate_read(&file).unwrap_err().code,
            PolicyErrorCode::IgnoredPath
        );
        for kind in [None, Some(std::fs::metadata(&file).unwrap().file_type())] {
            assert!(!walk.permits_entry(&file, kind), "walk file {name}");
        }
    }
}

#[test]
fn credential_directory_type_is_not_cached_across_replacement() {
    let root = tempfile::tempdir().expect("fixture");
    let policy = PathPolicy::new(PathPolicyConfig {
        workspace_root: Some(root.path().to_path_buf()),
        ..Default::default()
    })
    .expect("policy");
    let path = root.path().join("credentials");
    let walk = policy.discovery_walk();
    std::fs::write(&path, "ordinary content").unwrap();
    assert!(!walk.permits(&path));
    std::fs::remove_file(&path).unwrap();
    std::fs::create_dir(&path).unwrap();
    assert!(walk.permits(&path));
    std::fs::remove_dir(&path).unwrap();
    std::fs::write(&path, "ordinary content").unwrap();
    assert!(!walk.permits(&path));
}

#[cfg(unix)]
#[test]
fn credential_directory_symlinks_still_check_targets_and_sensitive_ancestors() {
    use std::os::unix::fs::symlink;
    let root = tempfile::tempdir().expect("fixture");
    let policy = PathPolicy::new(PathPolicyConfig {
        workspace_root: Some(root.path().to_path_buf()),
        ..Default::default()
    })
    .expect("policy");
    let source = root.path().join("source");
    std::fs::create_dir(&source).unwrap();
    std::fs::write(source.join("module.ts"), "export const value = 1;\n").unwrap();
    let alias = root.path().join("credentials");
    symlink(&source, &alias).unwrap();
    assert!(policy.validate(&alias).is_ok());
    assert!(policy.validate_read(alias.join("module.ts")).is_ok());
    assert!(policy.validate_output(alias.join("new.ts")).is_ok());
    let secret = root.path().join("private.pem");
    std::fs::write(&secret, "ordinary content").unwrap();
    symlink(&secret, root.path().join("ordinary.ts")).unwrap();
    assert_eq!(
        policy.validate_read("ordinary.ts").unwrap_err().code,
        PolicyErrorCode::IgnoredPath
    );
    symlink(source.join("module.ts"), root.path().join("id_rsa")).unwrap();
    assert_eq!(
        policy.validate_read("id_rsa").unwrap_err().code,
        PolicyErrorCode::IgnoredPath
    );
    for name in crate::policy::discovery::SENSITIVE_DIRECTORY_NAMES {
        let sensitive = root.path().join(name).join("credentials");
        std::fs::create_dir_all(&sensitive).unwrap();
        std::fs::write(sensitive.join("module.ts"), "ordinary content").unwrap();
        assert_eq!(
            policy.validate(&sensitive).unwrap_err().code,
            PolicyErrorCode::IgnoredPath,
            "{name}"
        );
        assert_eq!(
            policy
                .validate_read(sensitive.join("module.ts"))
                .unwrap_err()
                .code,
            PolicyErrorCode::IgnoredPath,
            "{name}"
        );
    }
    symlink(
        root.path().join(".ssh/credentials"),
        root.path().join("linked"),
    )
    .unwrap();
    assert_eq!(
        policy.validate("linked").unwrap_err().code,
        PolicyErrorCode::IgnoredPath
    );
}
