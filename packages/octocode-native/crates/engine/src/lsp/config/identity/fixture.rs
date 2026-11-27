//! A stand-in rustup with two installed toolchains, for tests of what a
//! language server's launch resolves to.
use crate::test_support::TmpDir;
use std::collections::HashMap;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

/// Answers `rustup which <name>` as rustup does: the binary of the toolchain
/// `RUSTUP_TOOLCHAIN` names (`stable` when unset) that ships `<name>`.
const RUSTUP: &str = r#"#!/bin/sh
[ "$1" = which ] || exit 2
dir=$(cd "$(dirname "$0")" && pwd -P)
bin="$dir/../toolchains/${RUSTUP_TOOLCHAIN:-stable}/bin/$2"
[ -x "$bin" ] || exit 1
echo "$(cd "$(dirname "$bin")" && pwd -P)/$2"
"#;

const PROXIED: [&str; 3] = ["rustc", "cargo", "rust-analyzer"];

pub(crate) struct FakeToolchains {
    pub(crate) root: TmpDir,
}

impl FakeToolchains {
    /// `<root>/cargo/bin/{rustup, rustc, cargo, rust-analyzer}` (the last
    /// three link to rustup) and the toolchains `stable` and `nightly`, each
    /// shipping all three.
    pub(crate) fn install() -> Self {
        let root = TmpDir::new();
        let bin = root.join("cargo/bin");
        let rustup = root.write("cargo/bin/rustup", RUSTUP);
        std::fs::set_permissions(&rustup, std::fs::Permissions::from_mode(0o755)).expect("chmod");
        for name in PROXIED {
            std::os::unix::fs::symlink("rustup", bin.join(name)).expect("proxy");
            for toolchain in ["stable", "nightly"] {
                let binary = root.write(
                    &format!("cargo/toolchains/{toolchain}/bin/{name}"),
                    format!("#!/bin/sh\n# {toolchain}\n"),
                );
                std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o755))
                    .expect("chmod");
            }
        }
        std::fs::create_dir_all(root.join("rustup-home")).expect("rustup home");
        Self { root }
    }

    pub(crate) fn bin(&self) -> PathBuf {
        self.root.join("cargo/bin")
    }

    /// A call environment whose `PATH` is `path` with the proxies' directory
    /// where `{bin}` stands, and `extra` laid over its rustup home.
    pub(crate) fn env(&self, path: &str, extra: &[(&str, &str)]) -> HashMap<String, String> {
        let path = path.replace("{bin}", &self.bin().to_string_lossy());
        let mut env = HashMap::from([
            ("PATH".to_owned(), path),
            (
                "RUSTUP_HOME".to_owned(),
                self.root.join("rustup-home").to_string_lossy().into_owned(),
            ),
            (
                "HOME".to_owned(),
                self.root.join("home").to_string_lossy().into_owned(),
            ),
        ]);
        env.extend(
            extra
                .iter()
                .map(|(n, v)| ((*n).to_owned(), (*v).to_owned())),
        );
        env
    }
}
