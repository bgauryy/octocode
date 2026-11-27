//! The platform ids the manifest and npm platform packages are keyed on.

/// The canonical `{os}-{arch}[-musl]` platform id the manifest is keyed on.
pub fn platform_id() -> String {
    platform_id_for(
        std::env::consts::OS,
        std::env::consts::ARCH,
        is_musl_linux(),
    )
}

/// The Node-style `{platform}-{arch}` id (no musl suffix) that npm platform
/// packages are named by, such as `@typescript/native-preview-darwin-arm64`.
pub(crate) fn npm_platform_id() -> String {
    platform_id_for(std::env::consts::OS, std::env::consts::ARCH, false)
}

pub(super) fn platform_id_for(os: &str, arch: &str, musl: bool) -> String {
    // Only the two shipped architectures get their npm-style names; any other
    // keeps its Rust name, so manifest lookup reports it unsupported instead
    // of selecting an arm64 binary.
    let arch = match arch {
        "x86_64" => "x64",
        "aarch64" => "arm64",
        other => other,
    };
    match os {
        "macos" => format!("darwin-{arch}"),
        "windows" => format!("win32-{arch}"),
        _ => {
            let suffix = if musl { "-musl" } else { "" };
            format!("linux-{arch}{suffix}")
        }
    }
}

/// True when the current Linux runtime links musl libc (Alpine etc.).
fn is_musl_linux() -> bool {
    if std::env::consts::OS != "linux" {
        return false;
    }
    std::fs::read_dir("/lib").is_ok_and(|entries| {
        entries.flatten().any(|e| {
            e.file_name()
                .to_str()
                .is_some_and(|n| n.starts_with("ld-musl-"))
        })
    })
}
