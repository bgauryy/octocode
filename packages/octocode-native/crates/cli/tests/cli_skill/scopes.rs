use super::*;

#[test]
fn add_reads_the_name_line_and_a_complex_name_needs_an_override() {
    let workspace = Workspace::new();
    let add = |directory: &Path, extra: &[&str]| {
        let mut args = vec!["install", "--add", directory.to_str().unwrap()];
        args.extend_from_slice(extra);
        run(&workspace, &args)
    };
    let quoted = workspace.workspace.join("quoted");
    local_skill(&quoted, "quoted", "v1");
    fs::write(
        quoted.join("SKILL.md"),
        "---\nname: \"quoted\"\ndescription: Local fixture\n---\nv1\n",
    )
    .unwrap();
    report(&add(&quoted, &[]), 0);
    assert!(workspace.home.join("skills/quoted/SKILL.md").is_file());

    let mismatched = workspace.workspace.join("mismatched");
    local_skill(&mismatched, "other-name", "v1");
    let rejected = add(&mismatched, &[]);
    assert_eq!(rejected.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&rejected.stdout).contains("must match its folder"));

    let block = workspace.workspace.join("block");
    local_skill(&block, "block", "v1");
    fs::write(
        block.join("SKILL.md"),
        "---\nname: |-\n  block\ndescription: Local fixture\n---\nv1\n",
    )
    .unwrap();
    let rejected = add(&block, &[]);
    assert_eq!(rejected.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&rejected.stdout).contains("name override"));
    report(&add(&block, &["block"]), 0);
    assert!(workspace.home.join("skills/block/SKILL.md").is_file());
}
#[test]
fn project_install_and_custom_path_keep_their_scopes() {
    let workspace = Workspace::new();
    report(
        &run(
            &workspace,
            &[
                "install",
                SKILL,
                "--platform",
                "claude",
                "--project-dir",
                workspace.workspace.to_str().unwrap(),
                "--mode",
                "copy",
            ],
        ),
        0,
    );
    assert!(
        workspace
            .workspace
            .join(".claude/skills")
            .join(SKILL)
            .join("SKILL.md")
            .is_file()
    );
    assert!(!workspace.home.join(".claude").exists());
    let destination = workspace.workspace.join("custom-skills");
    report(
        &run(
            &workspace,
            &["install", SKILL, "--path", destination.to_str().unwrap()],
        ),
        0,
    );
    assert!(destination.join(SKILL).join("SKILL.md").is_file());
}
#[test]
fn every_platform_uses_its_declared_destination() {
    let workspace = Workspace::new();
    let result = report(
        &run(
            &workspace,
            &[
                "install",
                SKILL,
                "--platform",
                "all",
                "--global",
                "--mode",
                "copy",
            ],
        ),
        0,
    );
    assert_eq!(result["summary"]["copied"], 7);
    for relative in [
        ".pi/agent/skills",
        ".cursor/skills",
        ".claude/skills",
        ".agents/skills",
        ".config/opencode/skills",
        ".copilot/skills",
        ".gemini/skills",
    ] {
        assert!(
            workspace
                .home
                .join(relative)
                .join(SKILL)
                .join("SKILL.md")
                .is_file(),
            "{relative}"
        );
    }
}
#[cfg(unix)]
#[test]
fn links_target_durable_home_and_external_development_links_are_not_repaired() {
    let workspace = Workspace::new();
    report(
        &run(
            &workspace,
            &["install", SKILL, "--platform", "codex", "--global"],
        ),
        0,
    );
    let destination = workspace.home.join(".agents/skills").join(SKILL);
    assert_eq!(
        fs::read_link(&destination).unwrap(),
        workspace.home.join("skills").join(SKILL)
    );
    fs::remove_file(&destination).unwrap();
    let checkout = workspace.workspace.join("checkout");
    local_skill(&checkout, SKILL, "development");
    std::os::unix::fs::symlink(&checkout, &destination).unwrap();
    report(
        &run(
            &workspace,
            &["check", SKILL, "--platform", "codex", "--fix", "--no-env"],
        ),
        0,
    );
    assert_eq!(fs::read_link(&destination).unwrap(), checkout);
    assert_eq!(
        fs::read_to_string(checkout.join("references/guide.md")).unwrap(),
        "development"
    );
    report(&run(&workspace, &["remove", SKILL]), 0);
    assert!(checkout.join("SKILL.md").exists());
}
#[test]
fn environment_readiness_uses_native_config_without_disclosing_values() {
    let workspace = Workspace::new();
    let output = workspace
        .cli()
        .env("PATH", workspace.home.join("empty"))
        .env("USERPROFILE", &workspace.home)
        .env("GH_TOKEN", "skill-test-secret")
        .args(["skill", "check", "octocode-research", "--json"])
        .output()
        .unwrap();
    let value = report(&output, 0);
    assert_eq!(value["skills"][0]["env"]["readiness"], "ready");
    assert!(!String::from_utf8_lossy(&output.stdout).contains("skill-test-secret"));
}

#[test]
fn remove_dry_run_keeps_installed_files_and_platform_removal_keeps_the_store() {
    let workspace = Workspace::new();
    report(
        &run(
            &workspace,
            &[
                "install",
                SKILL,
                "--platform",
                "claude",
                "--global",
                "--mode",
                "copy",
            ],
        ),
        0,
    );
    let canonical = workspace.home.join("skills").join(SKILL);
    let copy = workspace.home.join(".claude/skills").join(SKILL);
    report(&run(&workspace, &["remove", SKILL, "--dry-run"]), 0);
    assert!(canonical.join("SKILL.md").is_file());
    assert!(copy.join("SKILL.md").is_file());
    report(
        &run(&workspace, &["remove", SKILL, "--platform", "claude"]),
        0,
    );
    assert!(canonical.join("SKILL.md").is_file());
    assert!(!copy.exists());
}

#[cfg(unix)]
#[test]
fn local_source_cannot_install_inside_itself_through_a_directory_alias() {
    let workspace = Workspace::new();
    let local = workspace.workspace.join("custom");
    local_skill(&local, "custom", "v1");
    let alias = workspace.workspace.join("alias");
    std::os::unix::fs::symlink(&local, &alias).unwrap();
    for dry_run in [true, false] {
        let mut args = vec![
            "install",
            "--add",
            local.to_str().unwrap(),
            "--path",
            alias.to_str().unwrap(),
        ];
        if dry_run {
            args.push("--dry-run");
        }
        let result = report(&run(&workspace, &args), 1);
        assert!(
            result["skills"][0]["canonicalError"]
                .as_str()
                .unwrap()
                .contains("inside its source")
        );
        assert!(!local.join("custom").exists());
    }
}

#[cfg(unix)]
#[test]
fn add_rejects_a_symlinked_manifest_even_when_the_manifest_is_the_argument() {
    let workspace = Workspace::new();
    let local = workspace.workspace.join("custom");
    local_skill(&local, "custom", "v1");
    fs::rename(local.join("SKILL.md"), local.join("actual.md")).unwrap();
    std::os::unix::fs::symlink("actual.md", local.join("SKILL.md")).unwrap();
    report(
        &run(
            &workspace,
            &["install", "--add", local.join("SKILL.md").to_str().unwrap()],
        ),
        2,
    );
    assert!(!workspace.home.join("skills").exists());
}

#[test]
fn managed_links_are_idempotent_and_broken_targets_are_repaired() {
    let workspace = Workspace::new();
    let args = ["install", SKILL, "--platform", "codex", "--global"];
    report(&run(&workspace, &args), 0);
    let canonical = workspace.home.join("skills").join(SKILL);
    let destination = workspace.home.join(".agents/skills").join(SKILL);
    #[cfg(windows)]
    assert!(junction::exists(&destination).unwrap());
    let repeat = report(&run(&workspace, &args), 0);
    assert_eq!(repeat["summary"]["unchanged"], 2);
    fs::remove_dir_all(&canonical).unwrap();
    report(
        &run(
            &workspace,
            &["check", SKILL, "--platform", "codex", "--no-env", "--fix"],
        ),
        0,
    );
    assert_eq!(
        destination.canonicalize().unwrap(),
        canonical.canonicalize().unwrap()
    );
    report(
        &run(&workspace, &["remove", SKILL, "--platform", "codex"]),
        0,
    );
    assert!(!destination.exists());
    assert!(canonical.join("SKILL.md").is_file());
}
