//! `permissions` is the exact mode; `access` is what the caller can do.
#![cfg(unix)]

use super::*;
use std::os::unix::fs::{PermissionsExt, symlink};

fn names(options: FileSystemQueryOptions) -> Vec<String> {
    let mut names = query_file_system_inner(options)
        .expect("query")
        .entries
        .into_iter()
        .map(|entry| entry.name)
        .collect::<Vec<_>>();
    names.sort_unstable();
    names
}

fn file(root: &Path, name: &str, mode: u32) {
    fs::write(root.join(name), "x").expect("file");
    fs::set_permissions(root.join(name), fs::Permissions::from_mode(mode)).expect("mode");
}

fn is_root_user() -> bool {
    // SAFETY: `geteuid` has no preconditions.
    unsafe { libc::geteuid() == 0 }
}

/// A four-digit mode names the special bits too: it matches exactly the
/// entries with them, and a three-digit one only those without.
#[test]
fn permissions_match_the_exact_mode_special_bits_included() {
    let root = TmpDir::new();
    file(&root, "plain", 0o755);
    file(&root, "setuid", 0o4755);
    let of = |mode: &str| {
        names(FileSystemQueryOptions {
            path: root.to_string_lossy().into_owned(),
            permissions: Some(mode.to_owned()),
            ..Default::default()
        })
    };
    assert_eq!(of("4755"), ["setuid"]);
    assert_eq!(of("755"), ["plain"]);
    assert_eq!(of("0755"), ["plain"]);
}

/// `access` asks what the caller holds, not whether anyone holds it: a mode
/// only others have does not count, and a link answers for its target.
#[test]
fn access_is_what_the_caller_can_do() {
    if is_root_user() {
        return;
    }
    let root = TmpDir::new();
    file(&root, "others_only", 0o007);
    file(&root, "mine", 0o700);
    file(&root, "plain", 0o600);
    symlink(root.join("mine"), root.join("link_to_mine")).expect("link");
    symlink(root.join("plain"), root.join("link_to_plain")).expect("link");
    symlink(root.join("missing"), root.join("link_broken")).expect("link");
    let of = |set: fn(&mut FileSystemQueryOptions)| {
        let mut options = FileSystemQueryOptions {
            path: root.to_string_lossy().into_owned(),
            ..Default::default()
        };
        set(&mut options);
        names(options)
    };
    assert_eq!(
        of(|options| options.executable = Some(true)),
        ["link_to_mine", "mine"]
    );
    assert_eq!(
        of(|options| options.readable = Some(true)),
        ["link_to_mine", "link_to_plain", "mine", "plain"]
    );
    assert_eq!(
        of(|options| options.writable = Some(true)),
        ["link_to_mine", "link_to_plain", "mine", "plain"]
    );
}

/// A size filter is about content: a link's own length (its target's
/// spelling) is none, as a directory's entry table is none.
#[test]
fn a_size_filter_never_matches_a_link() {
    let root = TmpDir::new();
    fs::write(root.join("target"), "12345").expect("file");
    symlink(root.join("target"), root.join("link")).expect("link");
    for (size_min, size_max) in [(Some(1), None), (None, Some(u64::MAX))] {
        let found = names(FileSystemQueryOptions {
            path: root.to_string_lossy().into_owned(),
            size_min,
            size_max,
            ..Default::default()
        });
        assert_eq!(found, ["target"], "{size_min:?}..={size_max:?}");
    }
}
