//! A stand-in `rustup` for tests of toolchain-component selection: the
//! `rust-analyzer` proxy on a fake `PATH` links to a script that answers
//! `rustup which rust-analyzer` as rustup does (a path on stdout, or a failure
//! when the selected toolchain lacks the component).
use crate::test_support::TmpDir;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

/// The script fails unless `RUSTUP_AUTO_INSTALL=0` is set (rustup 1.29 would
/// otherwise download a missing toolchain), and records the directory it ran
/// in. Its answer is the content of the `selected` file next to it.
const SCRIPT: &str = r#"#!/bin/sh
dir=$(dirname "$0")
pwd -P > "$dir/ran-in"
[ "$1" = which ] && [ "$2" = rust-analyzer ] || { echo "unexpected arguments: $*" >&2; exit 2; }
[ "$RUSTUP_AUTO_INSTALL" = 0 ] || { echo "rustup would auto-install" >&2; exit 3; }
[ -f "$dir/selected" ] || { echo "error: unknown binary 'rust-analyzer' in toolchain 'bare'" >&2; exit 1; }
cat "$dir/selected"
"#;

pub(super) struct FakeRustup {
    bin: PathBuf,
}

impl FakeRustup {
    /// `<root>/cargo/bin/{rustup, rust-analyzer -> rustup}`; no toolchain ships
    /// the component until [`Self::ships`] is called.
    pub(super) fn install(root: &TmpDir) -> Self {
        let bin = root.join("cargo/bin");
        let rustup = root.write("cargo/bin/rustup", SCRIPT);
        std::fs::set_permissions(&rustup, std::fs::Permissions::from_mode(0o755)).expect("chmod");
        std::os::unix::fs::symlink("rustup", bin.join("rust-analyzer")).expect("proxy");
        Self { bin }
    }

    /// The directory to put on `PATH`.
    pub(super) fn bin(&self) -> &Path {
        &self.bin
    }

    pub(super) fn proxy(&self) -> PathBuf {
        self.bin.join("rust-analyzer")
    }

    /// The selected toolchain ships `rust-analyzer` at `binary`.
    pub(super) fn ships(&self, binary: &Path) {
        std::fs::write(self.bin.join("selected"), format!("{}\n", binary.display()))
            .expect("selected");
    }

    /// The selected toolchain lacks the component.
    pub(super) fn lacks_component(&self) {
        let _ = std::fs::remove_file(self.bin.join("selected"));
    }

    /// The directory the last `rustup which` ran in.
    pub(super) fn ran_in(&self) -> PathBuf {
        PathBuf::from(
            std::fs::read_to_string(self.bin.join("ran-in"))
                .expect("rustup was not run")
                .trim(),
        )
    }
}

/// An executable file at `root/relative`, standing for a toolchain's binary.
pub(super) fn toolchain_binary(root: &TmpDir, relative: &str) -> PathBuf {
    let path = root.write(relative, "#!/bin/sh\n");
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    path
}
