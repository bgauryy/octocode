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
