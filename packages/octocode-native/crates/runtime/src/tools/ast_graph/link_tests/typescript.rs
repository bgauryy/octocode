//! TypeScript/TSX ES-module idioms, linked the way TypeScript's module
//! resolution and tsserver bind them: each call reaches the declaration its
//! binding names, and nothing else.
use super::*;

const LIB: (&str, &str) = (
    "src/lib.ts",
    "export function named() {}\nexport default function primary() {}\nexport const arrow = () => 1;\nexport function useThing() { return 1; }\n",
);

#[test]
fn named_default_aliased_and_namespace_imports_reach_their_declarations() {
    let temp = fixture(&[
        LIB,
        (
            "src/main.ts",
            "import primaryAlias, { named, named as renamed, arrow, useThing } from './lib';\nimport * as lib from './lib';\nexport function run() { named(); renamed(); primaryAlias(); arrow(); useThing(); }\nexport function viaNamespace() { lib.named(); lib.default(); }\n",
        ),
    ]);
    let root = ingested(&temp);
    assert_eq!(
        caller_edges(root, "src/lib.ts#named"),
        [
            edge("src/main.ts#run", "import", "high"),
            edge("src/main.ts#viaNamespace", "namespace", "high"),
        ]
    );
    assert_eq!(
        caller_edges(root, "src/lib.ts#primary"),
        [
            edge("src/main.ts#run", "import", "high"),
            edge("src/main.ts#viaNamespace", "namespace", "high"),
        ]
    );
    for target in ["src/lib.ts#arrow", "src/lib.ts#useThing"] {
        assert_eq!(
            caller_edges(root, target),
            [edge("src/main.ts#run", "import", "high")],
            "{target}"
        );
    }
}

/// `export *` and `export { x as y } from` chains, several hops deep, keep
/// the identity of the declaration they forward.
#[test]
fn multi_hop_barrels_forward_star_and_renamed_reexports() {
    let temp = fixture(&[
        (
            "src/impl/math.ts",
            "export function add() {}\nexport default function sum() {}\n",
        ),
        ("src/impl/index.ts", "export * from './math';\n"),
        (
            "src/mid.ts",
            "export * from './impl';\nexport { add as plus } from './impl/math';\nexport { default as total, default } from './impl/math';\n",
        ),
        (
            "src/index.ts",
            "export * from './mid';\nexport { plus as more } from './mid';\n",
        ),
        (
            "app/use.ts",
            "import { add, plus, more, total } from '../src';\nimport sum from '../src/mid';\nexport function run() { add(); plus(); more(); total(); sum(); }\n",
        ),
        // Same-named decoys beside the caller.
        (
            "app/add.ts",
            "export function add() {}\nexport function plus() {}\n",
        ),
    ]);
    let root = ingested(&temp);
    assert_eq!(
        caller_edges(root, "src/impl/math.ts#add"),
        [edge("app/use.ts#run", "import", "high")]
    );
    assert_eq!(
        caller_edges(root, "src/impl/math.ts#sum"),
        [edge("app/use.ts#run", "import", "high")]
    );
    assert_eq!(caller_edges(root, "app/add.ts#add"), []);
    assert_eq!(caller_edges(root, "app/add.ts#plus"), []);
}

/// `import type` binds a type: a parameter annotated with it types the
/// receiver of its member calls.
#[test]
fn type_only_imports_type_receivers() {
    let temp = fixture(&[
        (
            "src/service.ts",
            "export class Service {\n  start() {}\n  static create() { return new Service(); }\n}\nexport class Other { start() {} }\n",
        ),
        (
            "src/main.ts",
            "import type { Service } from './service';\nimport { type Service as S2 } from './service';\nexport function run(s: Service, t: S2 | null) { s.start(); t?.start(); }\n",
        ),
    ]);
    let root = ingested(&temp);
    let rows = caller_edges(root, "src/service.ts#Service.start");
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert_eq!(rows[0].0, "src/main.ts#run");
    assert_eq!(caller_edges(root, "src/service.ts#Other.start"), []);
}

/// tsconfig `paths` and `baseUrl`, workspace packages and directory
/// `index.ts` imports resolve like TypeScript's module resolution.
#[test]
fn tsconfig_paths_base_url_workspaces_and_directory_imports_resolve() {
    let temp = fixture(&[
        (
            "tsconfig.json",
            r#"{"compilerOptions":{"baseUrl":".","paths":{"@app/*":["src/*"],"@core":["packages/core/src/index.ts"]}}}"#,
        ),
        ("src/util/strings.ts", "export function pad() {}\n"),
        ("src/dir/index.ts", "export function fromDir() {}\n"),
        ("src/base.ts", "export function fromBase() {}\n"),
        (
            "packages/core/package.json",
            r#"{"name":"@core","main":"src/index.ts"}"#,
        ),
        ("packages/core/src/index.ts", "export function core() {}\n"),
        (
            "packages/ui/package.json",
            r#"{"name":"@scope/ui","types":"src/index.ts"}"#,
        ),
        ("packages/ui/src/index.ts", "export function button() {}\n"),
        (
            "src/main.ts",
            "import { pad } from '@app/util/strings';\nimport { fromDir } from './dir';\nimport { fromBase } from 'src/base';\nimport { core } from '@core';\nimport { button } from '@scope/ui';\nexport function run() { pad(); fromDir(); fromBase(); core(); button(); }\n",
        ),
    ]);
    let root = ingested(&temp);
    for target in [
        "src/util/strings.ts#pad",
        "src/dir/index.ts#fromDir",
        "src/base.ts#fromBase",
        "packages/core/src/index.ts#core",
        "packages/ui/src/index.ts#button",
    ] {
        assert_eq!(
            caller_edges(root, target),
            [edge("src/main.ts#run", "import", "high")],
            "{target}"
        );
    }
    assert_eq!(unresolved_internal(root), 0);
}

/// A package importing itself by its own name (`@x/element` inside
/// `packages/element`) through tsconfig `paths` reaches its own barrel.
#[test]
fn a_package_self_import_through_paths_reaches_its_barrel() {
    let temp = fixture(&[
        (
            "tsconfig.json",
            r#"{"compilerOptions":{"paths":{"@x/element":["./packages/element/src/index.ts"],"@x/element/*":["./packages/element/src/*"]}}}"#,
        ),
        ("packages/element/package.json", r#"{"name":"@x/element"}"#),
        (
            "packages/element/src/index.ts",
            "export { mutate } from './mutate';\nexport { Scene } from './Scene';\n",
        ),
        (
            "packages/element/src/mutate.ts",
            "export function mutate() {}\n",
        ),
        (
            "packages/element/src/Scene.ts",
            "import { mutate } from '@x/element';\nexport class Scene {\n  mutate() { return mutate(); }\n}\n",
        ),
    ]);
    let root = ingested(&temp);
    assert_eq!(
        caller_edges(root, "packages/element/src/mutate.ts#mutate"),
        [edge(
            "packages/element/src/Scene.ts#Scene.mutate",
            "import",
            "high"
        )]
    );
    assert_eq!(
        caller_edges(root, "packages/element/src/Scene.ts#Scene.mutate"),
        []
    );
}

/// NodeNext `.js` specifiers name the `.ts`/`.tsx` source.
#[test]
fn js_extension_specifiers_resolve_to_typescript_sources() {
    let temp = fixture(&[
        ("src/a.ts", "export function fromTs() {}\n"),
        ("src/b.tsx", "export function fromTsx() {}\n"),
        ("src/c.mts", "export function fromMts() {}\n"),
        (
            "src/main.ts",
            "import { fromTs } from './a.js';\nimport { fromTsx } from './b.js';\nimport { fromMts } from './c.mjs';\nexport function run() { fromTs(); fromTsx(); fromMts(); }\n",
        ),
    ]);
    let root = ingested(&temp);
    for target in ["src/a.ts#fromTs", "src/b.tsx#fromTsx", "src/c.mts#fromMts"] {
        assert_eq!(
            caller_edges(root, target),
            [edge("src/main.ts#run", "import", "high")],
            "{target}"
        );
    }
}

#[test]
fn class_members_constructors_statics_and_object_literal_methods() {
    let temp = fixture(&[
        (
            "src/store.ts",
            "export class Store {\n  static of() { return new Store(); }\n  get() { return this.load(); }\n  load() {}\n}\nexport const api = {\n  fetch() {},\n  post: () => 1,\n};\n",
        ),
        (
            "src/main.ts",
            "import { Store, api } from './store';\nexport function run() {\n  const s = new Store();\n  s.get();\n  Store.of();\n  api.fetch();\n  api.post();\n}\n",
        ),
    ]);
    let root = ingested(&temp);
    assert_eq!(
        caller_edges(root, "src/store.ts#Store.load"),
        [edge("src/store.ts#Store.get", "local", "high")]
    );
    assert_eq!(
        caller_edges(root, "src/store.ts#Store.get"),
        [edge("src/main.ts#run", "receiver-type", "high")]
    );
    for target in [
        "src/store.ts#Store.of",
        "src/store.ts#api.fetch",
        "src/store.ts#api.post",
    ] {
        assert_eq!(
            caller_edges(root, target),
            [edge("src/main.ts#run", "import", "high")],
            "{target}"
        );
    }
}

/// In JS a class member is never in lexical scope: a bare call inside a
/// class names the imported function even when the class declares a
/// same-named member, and `a.b.m()` with an unknown receiver links nothing.
#[test]
fn bare_calls_never_bind_class_members_and_unknown_chains_stay_unlinked() {
    let temp = fixture(&[
        ("src/mutate.ts", "export function mutate() {}\n"),
        (
            "src/App.ts",
            "import { mutate } from './mutate';\nexport class App {\n  public mutate = () => 1;\n  update() {}\n  run() { mutate(); this.scene.mutate(); this.scene.update(); }\n}\n",
        ),
        (
            "src/Fill.ts",
            "import { App } from './App';\nexport class Fill {\n  static cache = new Map();\n  static clear() {}\n  constructor(private app: unknown) {}\n  go() { this.app.scene.mutate(); get().update(); Fill.cache.clear(); App.prototype.update(); }\n}\n",
        ),
    ]);
    let root = ingested(&temp);
    assert_eq!(
        caller_edges(root, "src/mutate.ts#mutate"),
        [edge("src/App.ts#App.run", "import", "high")]
    );
    assert_eq!(caller_edges(root, "src/App.ts#App.mutate"), []);
    assert_eq!(caller_edges(root, "src/App.ts#App.update"), []);
    assert_eq!(caller_edges(root, "src/Fill.ts#Fill.clear"), []);
}

/// A React component destructuring typed props (an interface of its file or
/// an inline type literal) types the receiver of each prop's member calls.
#[test]
fn destructured_typed_props_type_their_member_calls() {
    let temp = fixture(&[
        (
            "src/Scene.ts",
            "export class Scene {\n  mutate() {}\n}\nexport class Other {\n  mutate() {}\n}\n",
        ),
        (
            "src/Stats.tsx",
            "import type { Scene } from './Scene';\ninterface StatsProps { scene: Scene }\nexport const Stats = ({ scene }: StatsProps) => { scene.mutate(); return null; };\nexport function Link({ scene: s }: { scene: Scene }) { s.mutate(); return null; }\n",
        ),
    ]);
    let root = ingested(&temp);
    assert_eq!(
        caller_edges(root, "src/Scene.ts#Scene.mutate"),
        [
            edge("src/Stats.tsx#Link", "receiver-type", "high"),
            edge("src/Stats.tsx#Stats", "receiver-type", "high"),
        ]
    );
    assert_eq!(caller_edges(root, "src/Scene.ts#Other.mutate"), []);
}

/// JSX `<Comp/>` is a `renders` edge to the imported component; hooks are
/// ordinary calls.
#[test]
fn jsx_components_render_and_hooks_call_their_imports() {
    let temp = fixture(&[
        (
            "src/Button.tsx",
            "export function Button() { return <button />; }\nexport default function Card() { return <div />; }\nexport const Icons = { Star: () => <i /> };\n",
        ),
        ("src/hooks.ts", "export function useCount() { return 1; }\n"),
        (
            "src/Panel.tsx",
            "import Card, { Button, Icons } from './Button';\nimport { useCount } from './hooks';\nexport function Panel() { useCount(); return <Card><Button /><Icons.Star /></Card>; }\n",
        ),
    ]);
    let root = ingested(&temp);
    for target in ["src/Button.tsx#Button", "src/Button.tsx#Card"] {
        assert_eq!(
            caller_edges(root, target),
            [edge("src/Panel.tsx#Panel", "renders:import", "high")],
            "{target}"
        );
    }
    assert_eq!(
        caller_edges(root, "src/hooks.ts#useCount"),
        [edge("src/Panel.tsx#Panel", "import", "high")]
    );
}

/// Declarations (`.d.ts`, `declare module`, `declare function`) never shadow
/// the implementation an import names.
#[test]
fn ambient_declarations_do_not_shadow_implementations() {
    let temp = fixture(&[
        ("src/util.ts", "export function helper() {}\n"),
        ("src/util.d.ts", "export declare function helper(): void;\n"),
        (
            "types/global.d.ts",
            "declare function helper(): void;\ndeclare module 'virtual:mod' { export function helper(): void; }\n",
        ),
        (
            "src/main.ts",
            "import { helper } from './util';\nexport function run() { helper(); }\n",
        ),
    ]);
    let root = ingested(&temp);
    assert_eq!(
        caller_edges(root, "src/util.ts#helper"),
        [edge("src/main.ts#run", "import", "high")]
    );
    let deps = ask(root, "deps", Some("src/main.ts"));
    assert_eq!(files(&deps), ["src/util.ts"], "{deps}");
}

/// A literal `import()` is a dependency edge; nothing guesses its members.
#[test]
fn dynamic_import_is_a_dependency() {
    let temp = fixture(&[
        ("src/lazy.ts", "export function load() {}\n"),
        (
            "src/main.ts",
            "export async function run() { const m = await import('./lazy'); m.load(); }\n",
        ),
    ]);
    let root = ingested(&temp);
    let deps = ask(root, "deps", Some("src/main.ts"));
    assert_eq!(files(&deps), ["src/lazy.ts"], "{deps}");
}
