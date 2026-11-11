use super::*;

fn fixture() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("dir");
    let root = dir.path().canonicalize().expect("canonical");
    (dir, root)
}

fn write(root: &Path, name: &str, text: &str) -> String {
    let path = root.join(name);
    std::fs::create_dir_all(path.parent().expect("parent")).expect("dirs");
    std::fs::write(&path, text).expect("write");
    path.to_string_lossy().into_owned()
}

const ANCHOR: &str =
    "export function errorMessage(error: unknown): string {\n  return String(error);\n}\n";

/// The filter applies only to a symbol the anchor exports itself.
#[test]
fn only_an_anchor_that_exports_the_symbol_filters() {
    let (_dir, root) = fixture();
    let anchor = write(&root, "a.ts", ANCHOR);
    assert!(AnchorModule::of(&anchor, ANCHOR, "errorMessage").is_some());
    let default = "export default function errorMessage() {}\n";
    assert!(AnchorModule::of(&anchor, default, "errorMessage").is_some());
    let listed = "function errorMessage() {}\nexport { errorMessage };\n";
    assert!(AnchorModule::of(&anchor, listed, "errorMessage").is_some());
    // A script's global, a CommonJS export or a member: no import names it.
    assert!(AnchorModule::of(&anchor, "function errorMessage() {}\n", "errorMessage").is_none());
    let commonjs = "function errorMessage() {}\nmodule.exports = { errorMessage };\n";
    assert!(AnchorModule::of(&anchor, commonjs, "errorMessage").is_none());
    let member = "export class A { errorMessage() {} }\n";
    assert!(AnchorModule::of(&anchor, member, "errorMessage").is_none());
}

/// A file reaches the anchor through a relative import that resolves to
/// it (a compiled `.js` name, a directory index) or to a barrel that
/// relays it; one importing a same-named declaration from another module,
/// or naming no module, does not.
#[test]
fn relative_imports_reach_the_anchor_or_a_relay_only() {
    let (_dir, root) = fixture();
    let anchor = write(&root, "cli/native.ts", ANCHOR);
    let module = AnchorModule::of(&anchor, ANCHOR, "errorMessage").expect("module");
    write(
        &root,
        "shared/util.ts",
        "export function errorMessage() { return ''; }\n",
    );
    write(&root, "cli/index.ts", "export * from './native.js';\n");
    write(
        &root,
        "relay.ts",
        "import { errorMessage } from './cli/native';\nexport { errorMessage };\n",
    );
    let reached = |name: &str, text: &str| {
        let file = write(&root, name, text);
        module.reached_by(&file, text, &mut HashMap::new())
    };
    assert!(reached(
        "b.ts",
        "import { errorMessage } from './cli/native.js';\n"
    ));
    assert!(reached(
        "c.ts",
        "import * as n from './cli/native';\nn.errorMessage(1);\n"
    ));
    assert!(reached("d.ts", "import { errorMessage } from './cli';\n"));
    assert!(reached("e.ts", "import { errorMessage } from './relay';\n"));
    // A module declaring its own name relays nothing, whatever types it imports.
    write(
        &root,
        "shared/typed.ts",
        "export function errorMessage() {}\nexport type Db = typeof import('node:sqlite');\n",
    );
    assert!(!reached(
        "e2.ts",
        "import { errorMessage } from './shared/typed';\n"
    ));
    assert!(reached(
        "f.cjs",
        "const { errorMessage } = require('./cli/native');\n"
    ));
    assert!(reached(
        "g.ts",
        "export { errorMessage } from './cli/native';\n"
    ));
    // Another module's declaration of the same name.
    assert!(!reached(
        "h.ts",
        "import { errorMessage } from './shared/util.js';\n"
    ));
    assert!(!reached(
        "i.cjs",
        "const { errorMessage } = require('./shared/util');\n"
    ));
    // Its own declaration, and no module at all.
    assert!(!reached(
        "j.ts",
        "function errorMessage() {}\nerrorMessage();\n"
    ));
    // A relative module that names no file binds nothing.
    assert!(!reached(
        "k.ts",
        "import { errorMessage } from './missing';\n"
    ));
}

/// A bare or aliased specifier is the project's to resolve: it may reach
/// the anchor when it binds the name or a member of it; computed loads,
/// dynamic imports and unparsable files stay candidates.
#[test]
fn bare_and_undecidable_references_stay_candidates() {
    let (_dir, root) = fixture();
    let anchor = write(&root, "a.ts", ANCHOR);
    let module = AnchorModule::of(&anchor, ANCHOR, "errorMessage").expect("module");
    let reached = |name: &str, text: &str| {
        let file = write(&root, name, text);
        module.reached_by(&file, text, &mut HashMap::new())
    };
    assert!(reached(
        "b.ts",
        "import { errorMessage } from '@scope/cli';\n"
    ));
    assert!(reached(
        "c.ts",
        "import * as cli from '@/cli';\ncli.errorMessage(1);\n"
    ));
    assert!(reached(
        "d.cjs",
        "const { errorMessage } = require('pkg');\n"
    ));
    assert!(!reached(
        "e.ts",
        "import fs from 'node:fs';\nconst errorMessage = fs.readFileSync('x', 'utf8');\n"
    ));
    assert!(reached(
        "f.js",
        "const m = require(name);\nm.errorMessage();\n"
    ));
    assert!(reached(
        "g.ts",
        "const m = await import('./a');\nm.errorMessage();\n"
    ));
    assert!(reached("h.ts", "import { errorMessage from './a';\n"));
    assert!(reached("g2.ts", "const m = await import(name);\n"));
    // A literal dynamic import is resolved like a static one.
    assert!(!reached(
        "g3.ts",
        "type Db = typeof import('node:sqlite');\nconst errorMessage = 1;\n"
    ));
    // A namespace or default binding reaches the name only qualified by it.
    assert!(!reached(
        "i.ts",
        "import path from 'node:path';\nconst x = error.errorMessage;\n"
    ));
    assert!(!reached(
        "j.ts",
        "import * as ns from 'pkg';\nconst x = xns.errorMessage;\n"
    ));
}

/// The split keeps every candidate without an anchor module, and sets
/// aside the files that cannot reach it otherwise.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn split_sets_aside_unreached_candidates() {
    let (_dir, root) = fixture();
    let anchor = write(&root, "a.ts", ANCHOR);
    let importer = write(&root, "b.ts", "import { errorMessage } from './a';\n");
    let local = write(&root, "c.ts", "const errorMessage = 'x';\n");
    let policy = crate::tools::test_support::workspace_policy(&root);
    let cancel = &crate::execution::cancel::NeverCancel;
    let candidates = vec![importer.clone(), local.clone()];
    let (kept, unreached) = split_reachable(candidates.clone(), None, &policy, cancel)
        .await
        .expect("split");
    assert_eq!((kept, unreached), (candidates.clone(), Vec::new()));
    let module = AnchorModule::of(&anchor, ANCHOR, "errorMessage").map(Arc::new);
    let (kept, unreached) = split_reachable(candidates, module, &policy, cancel)
        .await
        .expect("split");
    assert_eq!((kept, unreached), (vec![importer], vec![local]));
}

/// Gitignored files that import the name from another module are no
/// evidence of a hidden importer; one importing the anchor is.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ignored_evidence_must_reach_the_anchor() {
    let (_dir, root) = fixture();
    let anchor = write(&root, "a.ts", ANCHOR);
    write(&root, "gen/util.ts", "export function errorMessage() {}\n");
    let hidden = write(&root, "gen/b.ts", "import { errorMessage } from '../a';\n");
    let other = write(
        &root,
        "gen/c.ts",
        "import { errorMessage } from './util';\n",
    );
    let destructured = write(
        &root,
        "gen/f.ts",
        "import { errorMessage as e } from 'pkg';\nconst { errorMessage } = await import('../a');\n",
    );
    // Its dynamic load cannot bind the name it imports from another module.
    let loader = write(
        &root,
        "gen/d.ts",
        "import { errorMessage } from './util';\nawait import(process.env.HELPER);\n",
    );
    // Recovered facts cannot prove that the visible import is the only binding.
    let recovered_source = "import { errorMessage } from './util';\nreturn;\n";
    let recovered = write(&root, "gen/recovered.ts", recovered_source);
    let recovered_facts =
        octocode_engine::signatures::extract_graph_facts(recovered_source, &recovered)
            .expect("recovered facts");
    assert!(!facts_are_clean(&recovered_facts));
    assert!(super::super::imports_name(
        &recovered_facts,
        recovered_source,
        "errorMessage"
    ));
    let mut ignored = IgnoredMentions {
        files: vec![
            hidden.clone(),
            other,
            loader,
            destructured.clone(),
            recovered.clone(),
        ],
        ..IgnoredMentions::default()
    };
    let policy = crate::tools::test_support::workspace_policy(&root);
    super::super::search::check_imports(
        &mut ignored,
        &policy,
        &crate::execution::cancel::NeverCancel,
        "errorMessage",
        AnchorModule::of(&anchor, ANCHOR, "errorMessage").map(Arc::new),
    )
    .await
    .expect("check");
    assert_eq!(ignored.importing, [hidden, destructured, recovered]);
    assert!(ignored.unchecked.is_empty(), "{ignored:?}");
}

/// A project of one workspace: a `paths` alias and a workspace package that
/// reach the anchor's package, and an installed package outside the listing.
fn project_fixture(root: &Path) -> String {
    write(
        root,
        "tsconfig.json",
        r#"{"compilerOptions":{"baseUrl":".","paths":{"@lib/*":["packages/lib/src/*"]}}}"#,
    );
    write(
        root,
        "package.json",
        r#"{"name":"mono","private":true,"workspaces":["packages/*"]}"#,
    );
    write(
        root,
        "packages/lib/package.json",
        r#"{"name":"@acme/lib","main":"src/index.ts"}"#,
    );
    write(
        root,
        "packages/lib/src/index.ts",
        "export * from './errors';\n",
    );
    write(
        root,
        "packages/lib/src/other.ts",
        "export function errorMessage() { return ''; }\n",
    );
    write(root, ".gitignore", "node_modules/\n");
    write(
        root,
        "node_modules/left-pad/package.json",
        r#"{"name":"left-pad","main":"index.js"}"#,
    );
    write(
        root,
        "node_modules/left-pad/index.js",
        "exports.errorMessage = () => '';\n",
    );
    write(root, "packages/lib/src/errors.ts", ANCHOR)
}

/// The importers of the project fixture, and whether each may reach the
/// anchor: a specifier the project places in another module (a Node
/// builtin, an installed package, an alias of another file) does not; one
/// it places at the anchor or a relay (a `paths` alias, a workspace package
/// name, a `baseUrl` module) does, as does project code it cannot link.
const PROJECT_IMPORTERS: [(&str, &str, bool); 9] = [
    (
        "app/builtin.ts",
        "import { errorMessage } from 'node:child_process';\n",
        false,
    ),
    (
        "app/installed.ts",
        "import { errorMessage } from 'left-pad';\n",
        false,
    ),
    (
        "app/required.cjs",
        "const { errorMessage } = require('left-pad');\n",
        false,
    ),
    (
        "app/other.ts",
        "import { errorMessage } from '@lib/other';\n",
        false,
    ),
    (
        "app/alias.ts",
        "import { errorMessage } from '@lib/errors';\n",
        true,
    ),
    (
        "app/package.ts",
        "import { errorMessage } from '@acme/lib';\n",
        true,
    ),
    (
        "app/base.ts",
        "import { errorMessage } from 'packages/lib/src/errors';\n",
        true,
    ),
    (
        "app/renamed.ts",
        "import render from '@lib/errors';\nrender(1);\nconst errorMessage = 1;\n",
        true,
    ),
    (
        "app/unlinked.ts",
        "import { errorMessage } from '@lib/missing';\n",
        true,
    ),
];

/// With the scope's listed files a bare specifier is resolved as the
/// project resolves it: a candidate stays only when the module it names is
/// the anchor's, may relay it, or is project code no file was found for.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_bare_specifier_is_placed_by_the_project() {
    let (_dir, root) = fixture();
    let anchor = project_fixture(&root);
    let candidates = PROJECT_IMPORTERS
        .iter()
        .map(|(name, text, _)| write(&root, name, text))
        .collect::<Vec<_>>();
    let policy = crate::tools::test_support::workspace_policy(&root);
    let cancel = &crate::execution::cancel::NeverCancel;
    // The listing is the request's own text scan of the scope.
    let scope = Scope::new(
        root.to_string_lossy().into_owned(),
        vec!["*.ts".to_owned(), "*.cjs".to_owned()],
    );
    let spelled = scope
        .text_files("errorMessage", &policy, cancel)
        .await
        .expect("scan")
        .expect("files");
    assert!(
        !spelled.iter().any(|file| file.contains("node_modules")),
        "{spelled:?}"
    );
    let listed = scope.listed_files("errorMessage").expect("listed");
    let module = AnchorModule::of(&anchor, ANCHOR, "errorMessage")
        .expect("module")
        .within(&root, &listed, &policy);
    for ((name, text, reaches), file) in PROJECT_IMPORTERS.iter().zip(&candidates) {
        assert_eq!(
            module.reached_by(file, text, &mut HashMap::new()),
            *reaches,
            "{name}"
        );
    }
    let expected = |reaches: bool| {
        let mut files = PROJECT_IMPORTERS
            .iter()
            .zip(&candidates)
            .filter(|((_, _, reached), _)| *reached == reaches)
            .map(|(_, file)| file.clone())
            .collect::<Vec<_>>();
        files.sort();
        files
    };
    let mut sorted = candidates.clone();
    sorted.sort();
    let (kept, unreached) = split_reachable(sorted, Some(Arc::new(module)), &policy, cancel)
        .await
        .expect("split");
    assert_eq!((kept, unreached), (expected(true), expected(false)));
}

/// A relay names the alias it exports the symbol under: re-exported from a
/// module that may be the anchor's, or exported from a binding that imports
/// it. Another module's same-named export, a default and a namespace name
/// none.
#[test]
fn a_relay_names_the_aliases_it_exports_the_symbol_under() {
    let (_dir, root) = fixture();
    let anchor = write(&root, "a/lib.ts", ANCHOR);
    let module = AnchorModule::of(&anchor, ANCHOR, "errorMessage").expect("module");
    write(
        &root,
        "other.ts",
        "export function errorMessage() { return ''; }\n",
    );
    let aliases = |name: &str, text: &str| {
        let file = write(&root, name, text);
        module
            .renamed_by(&file, text, &mut HashMap::new())
            .into_iter()
            .collect::<Vec<_>>()
    };
    assert_eq!(
        aliases(
            "c/re.ts",
            "export { errorMessage as viaC } from '../a/lib';\n"
        ),
        ["viaC"]
    );
    assert_eq!(
        aliases(
            "c/local.ts",
            "import { errorMessage as inner } from '../a/lib';\nexport { inner as viaG, inner };\n"
        ),
        ["inner", "viaG"]
    );
    assert_eq!(
        aliases(
            "c/same.ts",
            "import { errorMessage } from '../a/lib';\nexport { errorMessage as viaS, errorMessage };\n"
        ),
        ["viaS"]
    );
    // The same name of another module, and names no importer must spell.
    assert!(
        aliases(
            "c/far.ts",
            "export { errorMessage as viaC } from '../other';\n"
        )
        .is_empty()
    );
    assert!(
        aliases(
            "c/shapes.ts",
            "export { errorMessage as default } from '../a/lib';\nexport * as ns from '../a/lib';\nexport * from '../a/lib';\n"
        )
        .is_empty()
    );
    // The alias is the name its importers reach the relay by.
    let relay = module.relay(&root.join("c/re.ts").to_string_lossy(), "viaC".to_owned());
    let user = "import { viaC } from '../c/re';\nviaC(1);\n";
    let file = write(&root, "d/user.ts", user);
    assert!(relay.reached_by(&file, user, &mut HashMap::new()));
    let stranger = "import { viaC } from '../other';\nviaC(1);\n";
    let file = write(&root, "d/stranger.ts", stranger);
    assert!(!relay.reached_by(&file, stranger, &mut HashMap::new()));
}
