//! Discovery finds no server for code Octocode fetched: a file or workspace
//! root resolving inside `fetched_code_roots`, by whatever path it is named.
use super::super::{
    FETCHED_CODE_REFUSAL, LspDiscoveryOptions, default_server_for_file,
    default_server_for_workspace_root, route_problems, typescript_server_fallback,
    typescript_server_fallback_for_workspace_root,
};
use crate::test_support::TmpDir;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};

struct Layout {
    _dir: TmpDir,
    fetched: PathBuf,
    checkout: PathBuf,
}

fn layout() -> Layout {
    let dir = TmpDir::new();
    let root = PathBuf::from(dir.path()).canonicalize().expect("canonical");
    let (fetched, checkout) = (root.join("home/clones"), root.join("work"));
    for project in [fetched.join("o/r"), checkout.clone()] {
        std::fs::create_dir_all(project.join("src")).expect("project");
        std::fs::write(project.join("tsconfig.json"), "{}").expect("marker");
        std::fs::write(project.join("src/a.ts"), "export {};\n").expect("source");
    }
    Layout {
        _dir: dir,
        fetched,
        checkout,
    }
}

fn text(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn refusing(root: &Path) -> LspDiscoveryOptions {
    LspDiscoveryOptions {
        fetched_code_roots: vec![root.to_path_buf()],
        ..LspDiscoveryOptions::default()
    }
}

#[test]
fn no_route_starts_a_server_inside_a_fetched_root() {
    let at = layout();
    let project = at.fetched.join("o/r");
    let (file, root) = (text(&project.join("src/a.ts")), text(&project));
    let options = refusing(&at.fetched);
    assert!(default_server_for_file(&file, &root, &options).is_none());
    assert!(typescript_server_fallback(&file, &root, &options).is_none());
    assert!(default_server_for_workspace_root(&root, &options).is_none());
    assert!(typescript_server_fallback_for_workspace_root(&root, &options).is_none());
    assert_eq!(
        route_problems(&file, None, &root, &options),
        [FETCHED_CODE_REFUSAL]
    );
    // The opt-in is an empty list: the same calls route again.
    let trusted = LspDiscoveryOptions::default();
    assert!(default_server_for_file(&file, &root, &trusted).is_some());
    assert!(default_server_for_workspace_root(&root, &trusted).is_some());
}

#[test]
fn a_symlink_is_judged_by_where_it_resolves() {
    let at = layout();
    let options = refusing(&at.fetched);
    let target = at.fetched.join("o/r");
    let linked_dir = at.checkout.join("linked");
    let linked_file = at.checkout.join("linked.ts");
    symlink(&target, &linked_dir).expect("directory link");
    symlink(target.join("src/a.ts"), &linked_file).expect("file link");
    let checkout = text(&at.checkout);
    assert!(options.opens_fetched_code(&text(&linked_file), &checkout));
    assert!(options.opens_fetched_code(&text(&linked_dir.join("src/a.ts")), &checkout));
    assert!(options.opens_fetched_code(&text(&at.checkout.join("src/a.ts")), &text(&linked_dir)));
    // A root named through a symlink of its own still holds what it held.
    let aliased = at.checkout.join("aliased-home");
    symlink(&at.fetched, &aliased).expect("root link");
    let through_alias = refusing(&aliased);
    assert!(through_alias.opens_fetched_code(&text(&target.join("src/a.ts")), &checkout));
}

/// A name no file holds yet (a workspace root's representative source) sits
/// where its existing ancestors resolve, and a root that does not exist yet
/// refuses what later appears in it.
#[test]
fn names_that_do_not_exist_resolve_through_their_ancestors() {
    let at = layout();
    let options = refusing(&at.fetched);
    let absent = text(&at.fetched.join("o/r/src/absent.ts"));
    assert!(options.opens_fetched_code(&absent, &text(&at.checkout)));
    let later = at.checkout.join("later");
    let before = refusing(&later);
    std::fs::create_dir_all(later.join("x")).expect("later");
    assert!(before.opens_fetched_code(&text(&later.join("x/a.ts")), &text(&later.join("x"))));
}

#[test]
fn an_ordinary_checkout_is_not_fetched_code() {
    let at = layout();
    let options = refusing(&at.fetched);
    let (file, root) = (text(&at.checkout.join("src/a.ts")), text(&at.checkout));
    assert!(!options.opens_fetched_code(&file, &root));
    assert!(default_server_for_file(&file, &root, &options).is_some());
    // A sibling whose name extends the root's is outside it.
    let sibling = format!("{}-other/a.ts", at.fetched.display());
    assert!(!options.opens_fetched_code(&sibling, &root));
}

/// A root above fetched code opens it too, whichever way the root is named, but
/// only once there is something there to open.
#[test]
fn a_root_holding_fetched_code_counts_once_it_holds_any() {
    let at = layout();
    let options = refusing(&at.fetched);
    let home_dir = at.fetched.parent().expect("home").parent().expect("parent");
    std::fs::create_dir_all(home_dir.join("own/src")).expect("own");
    let own = text(&home_dir.join("own/src/a.ts"));
    let above = text(home_dir);
    assert!(options.opens_fetched_code(&own, &above));
    assert!(options.opens_fetched_code(&text(&at.fetched), &above));
    // A narrower root beside it is not.
    assert!(!options.opens_fetched_code(&own, &text(&home_dir.join("own"))));
    let alias = at.checkout.join("above");
    symlink(home_dir, &alias).expect("root link");
    assert!(options.opens_fetched_code(&own, &text(&alias)));

    let empty = at.checkout.join("empty/clones");
    std::fs::create_dir_all(&empty).expect("empty");
    let before = refusing(&empty);
    let checkout = text(&at.checkout);
    let file = text(&at.checkout.join("src/a.ts"));
    assert!(
        !before.opens_fetched_code(&file, &checkout),
        "an empty root"
    );
    let absent = refusing(&at.checkout.join("absent/clones"));
    assert!(
        !absent.opens_fetched_code(&file, &checkout),
        "an absent root"
    );
}

/// One request's options answer every case as options that resolve on each call,
/// a symlink named any way and a name no file holds included, the second asking
/// too. A later request sees what changed since.
#[test]
fn one_request_judges_as_every_call_does() {
    let at = layout();
    let target = at.fetched.join("o/r");
    let (linked_dir, linked_file) = (at.checkout.join("linked"), at.checkout.join("linked.ts"));
    symlink(&target, &linked_dir).expect("directory link");
    symlink(target.join("src/a.ts"), &linked_file).expect("file link");
    let aliased = at.checkout.join("aliased-home");
    symlink(&at.fetched, &aliased).expect("root link");
    let checkout = text(&at.checkout);
    let above = text(at.fetched.parent().expect("home"));
    let own = text(&at.checkout.join("src/a.ts"));
    let cases = [
        (text(&linked_file), checkout.clone(), true),
        (text(&linked_dir.join("src/a.ts")), checkout.clone(), true),
        (own.clone(), text(&linked_dir), true),
        (text(&target.join("src/absent.ts")), checkout.clone(), true),
        (own.clone(), above, true),
        (own.clone(), checkout.clone(), false),
        (
            format!("{}-other/a.ts", at.fetched.display()),
            checkout.clone(),
            false,
        ),
    ];
    for root in [&at.fetched, &aliased] {
        let each_call = refusing(root);
        let one_request = refusing(root).for_one_request();
        for (file, workspace, verdict) in &cases {
            assert_eq!(
                each_call.opens_fetched_code(file, workspace),
                *verdict,
                "{file}"
            );
            for _ in 0..2 {
                assert_eq!(
                    one_request.opens_fetched_code(file, workspace),
                    *verdict,
                    "{file}"
                );
            }
        }
    }
    // What this request saw stands for it; the next one resolves again.
    let flipped = at.checkout.join("flipped.ts");
    std::fs::write(&flipped, "export {};\n").expect("plain file");
    let first = refusing(&at.fetched).for_one_request();
    assert!(!first.opens_fetched_code(&text(&flipped), &checkout));
    std::fs::remove_file(&flipped).expect("remove");
    symlink(target.join("src/a.ts"), &flipped).expect("now a link");
    assert!(!first.opens_fetched_code(&text(&flipped), &checkout));
    let next = refusing(&at.fetched).for_one_request();
    assert!(next.opens_fetched_code(&text(&flipped), &checkout));
}
