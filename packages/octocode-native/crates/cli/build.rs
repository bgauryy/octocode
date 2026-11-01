use std::{env, path::PathBuf, process::Command};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let crate_dir =
        PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").ok_or("manifest directory is missing")?);
    let repository = crate_dir.join("../../../..");
    let generator = repository.join("packages/octocode-native/scripts/embed-skills.mjs");
    for path in [
        repository.join("skills"),
        repository.join("packages/octocode/scripts/stage-skills.mjs"),
        repository.join("packages/octocode-config/src"),
        generator.clone(),
    ] {
        println!("cargo:rerun-if-changed={}", path.display());
    }
    let status = Command::new("node")
        .arg(generator)
        .arg(env::var_os("OUT_DIR").ok_or("build output directory is missing")?)
        .status()?;
    if !status.success() {
        return Err("embedding the public skills failed".into());
    }
    Ok(())
}
