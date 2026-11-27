//! Symlinks read through the path policy by the link's own name, its target
//! and its ancestors.

use super::{Expect, TOOLS, denial, fixture, runtime};

/// `(link, target, expectation)`, both relative to the workspace: symlinks
/// are read through the policy by the link's own name, its target and its
/// ancestors. They live in their own fixture because walks never follow them.
/// An empty target is a path reached through an earlier link.
#[cfg(unix)]
const LINKS: &[(&str, &str, Expect)] = &[
    // A credential-named link to a normal file.
    (
        "links/id_rsa",
        "src/credentials/hasher.ts",
        Expect::WithheldByFilename,
    ),
    // An ordinary-named link to a credential file.
    (
        "links/ordinary.ts",
        "src/credentials/deploy.pem",
        Expect::WithheldByFilename,
    ),
    // A `credentials/` link to a source folder stays a folder...
    ("links/credentials", "src/credentials", Expect::Visible),
    // ...and the credential-named files inside it are still withheld.
    ("links/credentials/hasher.ts", "", Expect::Visible),
    (
        "links/credentials/deploy.pem",
        "",
        Expect::WithheldByFilename,
    ),
];

/// A symlink is judged by its own name, its target and the folders above
/// it: a credential name or target withholds it, a `credentials/` folder link
/// does not.
#[cfg(unix)]
#[tokio::test]
async fn symlinks_are_judged_by_name_target_and_ancestors() {
    let workspace = fixture();
    std::fs::create_dir(workspace.workspace.join("links")).expect("links dir");
    for (link, target, _) in LINKS.iter().filter(|(_, target, _)| !target.is_empty()) {
        std::os::unix::fs::symlink(
            workspace.workspace.join(target),
            workspace.workspace.join(link),
        )
        .unwrap_or_else(|error| panic!("{link}: {error}"));
    }
    let runtime = runtime(&workspace);
    for (link, _, expect) in LINKS {
        let path = workspace.workspace.join(link);
        for tool in TOOLS {
            let denied = denial(&runtime, tool, &path).await;
            match expect {
                Expect::Visible => assert_eq!(denied, None, "{tool} must read {link}"),
                Expect::WithheldByFilename => {
                    assert!(denied.is_some(), "{tool} must withhold {link}");
                }
            }
        }
    }
    runtime.close().await;
}
