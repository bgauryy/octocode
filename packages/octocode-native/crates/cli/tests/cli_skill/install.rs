use super::*;

#[test]
fn embedded_catalog_and_workflows_need_no_node_or_installed_skills() {
    let workspace = Workspace::new();
    let catalog = report(&run(&workspace, &["list"]), 0);
    assert!(catalog["count"].as_u64().unwrap() > 10);
    assert_eq!(catalog["installedCount"], 0);
    let served = catalog["skills"]
        .as_array()
        .unwrap()
        .iter()
        .map(|skill| {
            let name = skill["name"].as_str().unwrap();
            let info = report(&run(&workspace, &["info", name]), 0);
            assert_eq!(info["skill"]["name"], name);
            let instructions = info["skill"]["skillMd"].as_str().unwrap().to_owned();
            (name.to_owned(), instructions)
        })
        .collect::<Vec<_>>();
    assert!(!workspace.home.join("skills").exists());
    // Every relative link the served instructions hold, a neighbor skill's
    // file included, names a file the embedded bundle ships.
    report(
        &run(
            &workspace,
            &[
                "install",
                "--all",
                "--platform",
                "codex",
                "--global",
                "--mode",
                "copy",
            ],
        ),
        0,
    );
    let mut links = 0;
    for (name, instructions) in &served {
        let root = workspace.home.join("skills").join(name);
        assert_eq!(
            &fs::read_to_string(root.join("SKILL.md")).unwrap(),
            instructions,
            "{name}"
        );
        for target in relative_links(instructions) {
            links += 1;
            assert!(root.join(target).is_file(), "{name} links {target}");
        }
    }
    assert!(links > 0);
}
/// Relative Markdown link targets, without their fragment.
fn relative_links(markdown: &str) -> impl Iterator<Item = &str> {
    markdown.split("](").skip(1).filter_map(|rest| {
        let target = rest.split(')').next()?.split('#').next()?;
        (!target.is_empty() && !target.contains(':') && !target.contains(char::is_whitespace))
            .then_some(target)
    })
}
#[test]
fn install_check_repair_and_remove_use_the_embedded_bundle() {
    let workspace = Workspace::new();
    let installed = report(
        &run(
            &workspace,
            &[
                "install",
                SKILL,
                "--platform",
                "codex",
                "--global",
                "--mode",
                "copy",
            ],
        ),
        0,
    );
    assert_eq!(installed["summary"]["installed"], 1);
    assert_eq!(installed["summary"]["copied"], 1);
    let canonical = workspace.home.join("skills").join(SKILL);
    let destination = workspace.home.join(".agents/skills").join(SKILL);
    for reference in [
        "runtime-surgery.md",
        "single-actor-traversal.md",
        "ast-lsp-workflows.md",
        "verification.md",
    ] {
        assert_eq!(
            fs::read(canonical.join("references").join(reference)).unwrap(),
            fs::read(destination.join("references").join(reference)).unwrap()
        );
    }
    let repeated = report(
        &run(
            &workspace,
            &[
                "install",
                SKILL,
                "--platform",
                "codex",
                "--global",
                "--mode",
                "copy",
            ],
        ),
        0,
    );
    assert_eq!(repeated["summary"]["unchanged"], 2);
    fs::write(
        destination.join("references/single-actor-traversal.md"),
        "old",
    )
    .unwrap();
    let stale = report(
        &run(
            &workspace,
            &["check", SKILL, "--platform", "codex", "--no-env"],
        ),
        1,
    );
    assert_eq!(stale["skills"][0]["installStatus"], "stale");
    report(
        &run(
            &workspace,
            &["check", SKILL, "--platform", "codex", "--no-env", "--fix"],
        ),
        0,
    );
    assert_eq!(
        fs::read(canonical.join("references/single-actor-traversal.md")).unwrap(),
        fs::read(destination.join("references/single-actor-traversal.md")).unwrap()
    );
    report(&run(&workspace, &["remove", SKILL]), 0);
    assert!(!canonical.exists());
    assert!(!destination.exists());
}
#[test]
fn installed_architect_delegates_moved_procedures_to_workflows() {
    let workspace = Workspace::new();
    report(
        &run(
            &workspace,
            &[
                "install",
                ENGINEER,
                "--platform",
                "codex",
                "--global",
                "--mode",
                "copy",
            ],
        ),
        0,
    );
    for root in [
        workspace.home.join("skills").join(ENGINEER),
        workspace.home.join(".agents/skills").join(ENGINEER),
    ] {
        for reference in [
            "runtime-surgery.md",
            "solo-traversal.md",
            "ast-lsp-workflows.md",
            "verification.md",
        ] {
            assert!(!root.join("references").join(reference).exists());
        }
    }
}
#[test]
fn dry_run_and_invalid_requests_do_not_install_anything() {
    let workspace = Workspace::new();
    report(
        &run(
            &workspace,
            &[
                "install",
                "--all",
                "--platform",
                "all",
                "--global",
                "--dry-run",
            ],
        ),
        0,
    );
    assert!(!workspace.home.join("skills").exists());
    assert!(!workspace.home.join(".agents").exists());
    for args in [
        vec!["install", SKILL, "missing-skill"],
        vec!["install", SKILL, "--platform", "claude"],
        vec!["install", SKILL, "--global"],
        vec!["install", SKILL, "--platform", "not-a-platform", "--global"],
        vec!["install", SKILL, "--all"],
        vec!["remove", "../escape", "--force"],
        vec!["list", "--force"],
    ] {
        report(&run(&workspace, &args), 2);
        assert!(!workspace.home.join("skills").exists(), "{args:?}");
    }
}
#[test]
fn local_add_upgrades_managed_copies_and_preserves_edited_copies() {
    let workspace = Workspace::new();
    let local = workspace.workspace.join("custom");
    local_skill(&local, "custom", "v1");
    let args = [
        "install",
        "--add",
        local.to_str().unwrap(),
        "--platform",
        "claude,cursor",
        "--global",
        "--mode",
        "copy",
    ];
    report(&run(&workspace, &args), 0);
    let edited = workspace
        .home
        .join(".cursor/skills/custom/references/guide.md");
    fs::write(&edited, "user edit").unwrap();
    local_skill(&local, "custom", "v2");
    let mut upgrade = args.to_vec();
    upgrade.push("--upgrade");
    let result = report(&run(&workspace, &upgrade), 1);
    assert_eq!(result["summary"]["conflicts"], 1);
    assert_eq!(fs::read_to_string(&edited).unwrap(), "user edit");
    assert_eq!(
        fs::read_to_string(
            workspace
                .home
                .join(".claude/skills/custom/references/guide.md")
        )
        .unwrap(),
        "v2"
    );
    let failed_remove = report(&run(&workspace, &["remove", "custom"]), 1);
    assert_eq!(failed_remove["summary"]["failed"], 1);
    assert!(edited.exists());
    report(&run(&workspace, &["remove", "custom", "--force"]), 0);
    assert!(!edited.exists());
}
