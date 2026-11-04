//! Whether the rustup proxy would run a toolchain's server is rustup's to say,
//! once per state of what it reads: a declaration lead asks on every search
//! hit, and a process per hit is the whole cost of that hit.
use super::rustup_fixture::{FakeRustup, toolchain_binary};
use crate::lsp::config::invocation::rustup::{RustupView, with_view};
use crate::lsp::config::{LspDiscoveryOptions, default_server_for_file, server_command_resolves};
use crate::lsp::types::ServerSource;
use crate::stamp::settle_written_so_far;
use crate::test_support::TmpDir;
use std::path::PathBuf;

struct Machine {
    _root: TmpDir,
    rustup: FakeRustup,
    home: PathBuf,
    workspace: PathBuf,
    view: RustupView,
}

/// A machine with a rustup that ships `rust-analyzer` for the workspace, a
/// toolchain directory, and nothing written since it was settled.
fn machine() -> Machine {
    let root = TmpDir::new();
    let rustup = FakeRustup::install(&root);
    rustup.ships(&toolchain_binary(
        &root,
        "rustup-home/toolchains/stable/bin/rust-analyzer",
    ));
    let home = root.join("rustup-home");
    root.write(
        "rustup-home/settings.toml",
        "default_toolchain = \"stable\"\n",
    );
    let workspace = root.join("project/crate");
    std::fs::create_dir_all(&workspace).expect("workspace");
    let view = RustupView {
        path_var: Some(rustup.bin().as_os_str().to_owned()),
        home: Some(home.clone()),
        ..RustupView::default()
    };
    settle_written_so_far();
    Machine {
        _root: root,
        rustup,
        home,
        workspace,
        view,
    }
}

impl Machine {
    /// One lead's whole check, as `tools::leads` makes it; whether it offers.
    fn lead(&self) -> bool {
        with_view(self.view.clone(), || {
            default_server_for_file(
                "src/lib.rs",
                &self.workspace.to_string_lossy(),
                &LspDiscoveryOptions::default(),
            )
            .is_some_and(|config| server_command_resolves(&config))
        })
    }

    /// What a lead costs from here on, with its answer.
    fn counted(&self) -> (bool, usize) {
        let before = self.rustup.runs();
        let offered = self.lead();
        (offered, self.rustup.runs() - before)
    }
}

/// However many hits a search has, rustup answers once.
#[test]
fn repeated_leads_run_rustup_once() {
    let machine = machine();
    assert_eq!(machine.counted(), (true, 1));
    for _ in 0..5 {
        assert_eq!(machine.counted(), (true, 0));
    }
}

/// A workspace's own answer: another directory may select another toolchain.
#[test]
fn another_workspace_asks_rustup_itself() {
    let mut machine = machine();
    assert_eq!(machine.counted().1, 1);
    machine.workspace = machine.workspace.parent().expect("parent").to_path_buf();
    assert_eq!(machine.counted().1, 1);
    assert_eq!(machine.counted().1, 0);
}

/// Each thing rustup reads changes the answer at once, with no time bound
/// and no process until it does: the toolchain file of any ancestor, its
/// settings, a toolchain's binaries, the set of toolchains and the proxy.
#[test]
fn any_input_of_rustup_asks_again() {
    let machine = machine();
    let project = machine.workspace.parent().expect("project");
    let again = |what: &str, change: &dyn Fn()| {
        settle_written_so_far();
        machine.counted();
        assert_eq!(machine.counted().1, 0, "{what}: settled answer served");
        change();
        assert!(machine.counted().1 > 0, "{what}");
    };
    again("a toolchain file above the workspace", &|| {
        std::fs::write(project.join("rust-toolchain.toml"), "[toolchain]\n").expect("file");
    });
    again("that file edited", &|| {
        std::fs::write(
            project.join("rust-toolchain.toml"),
            "[toolchain]\nchannel='1'\n",
        )
        .expect("file");
    });
    again("that file removed", &|| {
        std::fs::remove_file(project.join("rust-toolchain.toml")).expect("removed");
    });
    again("a legacy toolchain file in the workspace", &|| {
        std::fs::write(machine.workspace.join("rust-toolchain"), "stable\n").expect("file");
    });
    again(
        "rustup's settings (a default or a directory override)",
        &|| {
            std::fs::write(
                machine.home.join("settings.toml"),
                "default_toolchain = \"x\"\n",
            )
            .expect("file");
        },
    );
    again("a component added to a toolchain", &|| {
        std::fs::write(machine.home.join("toolchains/stable/bin/other"), "").expect("file");
    });
    again("a toolchain installed", &|| {
        std::fs::create_dir_all(machine.home.join("toolchains/nightly/bin")).expect("dir");
    });
    again("the proxy replaced", &|| {
        let proxy = machine.rustup.proxy();
        std::fs::remove_file(&proxy).expect("removed");
        std::os::unix::fs::symlink("rustup", &proxy).expect("proxy");
    });
}

/// The answer's binary is checked on every lead: a removed component stops the
/// lead with no process, whichever toolchain directory held it.
#[test]
fn a_removed_component_ends_the_lead_at_once() {
    let machine = machine();
    assert_eq!(machine.counted(), (true, 1));
    std::fs::remove_file(machine.home.join("toolchains/stable/bin/rust-analyzer")).expect("gone");
    machine.rustup.lacks_component();
    let (offered, runs) = machine.counted();
    assert!(!offered && runs > 0, "{offered} {runs}");
}

/// An environment value rustup reads is part of the question.
#[test]
fn rustup_toolchain_is_part_of_the_question() {
    let mut machine = machine();
    assert_eq!(machine.counted().1, 1);
    machine.view.toolchain = Some("nightly".into());
    assert_eq!(machine.counted().1, 1);
    assert_eq!(machine.counted().1, 0);
    machine.view.toolchain = None;
    assert_eq!(machine.counted().1, 0, "the first answer still holds");
}

/// A machine written within the settle window could change unseen in one
/// clock tick: its answer is not kept. An unknown rustup home proves nothing
/// either.
#[test]
fn an_unprovable_answer_is_not_kept() {
    let root = TmpDir::new();
    let rustup = FakeRustup::install(&root);
    let binary = toolchain_binary(&root, "rustup-home/toolchains/stable/bin/rust-analyzer");
    rustup.ships(&binary);
    let workspace = root.join("w");
    std::fs::create_dir_all(&workspace).expect("workspace");
    let machine = Machine {
        home: root.join("rustup-home"),
        view: RustupView {
            path_var: Some(rustup.bin().as_os_str().to_owned()),
            home: Some(root.join("rustup-home")),
            ..RustupView::default()
        },
        workspace,
        rustup,
        _root: root,
    };
    assert!(machine.counted().1 > 0);
    assert!(machine.counted().1 > 0, "unsettled");
    settle_written_so_far();
    let mut unknown = machine;
    unknown.view.home = None;
    assert!(unknown.counted().1 > 0);
    assert!(unknown.counted().1 > 0, "no home to stamp");
}

/// The managed-install decision follows rustup's memoized answer: which
/// server a workspace gets, and whether a lead is offered, never disagree.
#[test]
fn the_lead_and_the_server_route_share_one_answer() {
    let machine = machine();
    with_view(machine.view.clone(), || {
        let config = default_server_for_file(
            "src/lib.rs",
            &machine.workspace.to_string_lossy(),
            &LspDiscoveryOptions::default(),
        )
        .expect("rust route");
        assert_eq!(config.source, ServerSource::Path);
        assert!(server_command_resolves(&config));
    });
    assert_eq!(machine.rustup.runs(), 1);
}

/// A kept answer draws on the one process budget, with no cap of its own.
#[test]
fn a_kept_answer_draws_on_the_process_budget() {
    let machine = machine();
    let before = crate::memory::MEMORY.used();
    assert!(machine.lead());
    assert!(crate::memory::MEMORY.used() > before);
}
