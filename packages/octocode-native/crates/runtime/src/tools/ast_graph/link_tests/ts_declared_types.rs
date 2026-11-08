//! TS receivers typed by declared types across files, as tsserver reads them
//! (B154 fixture): a class field or an imported interface or alias member
//! (`x.f.m()` with `x: T`), an indexed access `T["k"]`, a parameter of a
//! function its declarator annotates with an alias (`const f: A = (p) => …`),
//! and a property destructured from a typed value. Each name resolves in the
//! file that writes it; a type depending on a type argument, a `window`
//! global and an `import("…")` type stay unlinked.
use super::*;

const B154: &[(&str, &str)] = &[
    (
        "src/app.ts",
        "import { Scene } from \"./scene\";\nexport default class App {\n  public scene: Scene;\n  constructor() { this.scene = new Scene(); }\n}\ndeclare global {\n  interface Window {\n    h: { scene: Scene; app: InstanceType<typeof App> };\n  }\n}\n",
    ),
    (
        "src/cb.ts",
        "import type { Scene } from \"./scene\";\nexport type CB<P> = (props: { scene: Scene; property: P }) => void;\n",
    ),
    (
        "src/f01_param.ts",
        "import { Scene } from \"./scene\";\nexport function a(s: Scene) { s.mutateElement(1, {}); }\n",
    ),
    (
        "src/f02_param_field_chain.ts",
        "import type App from \"./app\";\nexport function b(app: App) { app.scene.mutateElement(1, {}); }\n",
    ),
    (
        "src/f03_destructured_inline_chain.ts",
        "import type App from \"./app\";\nexport const c = ({ app }: { app: App }) => { app.scene.mutateElement(1, {}); };\n",
    ),
    (
        "src/f04_destructured_inline.ts",
        "import type { Scene } from \"./scene\";\nexport const d = ({ scene }: { scene: Scene }) => { scene.mutateElement(1, {}); };\n",
    ),
    (
        "src/f05_param_property_chain.ts",
        "import type App from \"./app\";\nexport class E { constructor(private app: App) {} run() { this.app.scene.mutateElement(1, {}); } }\n",
    ),
    (
        "src/f06_param_property.ts",
        "import type { Scene } from \"./scene\";\nexport class F { constructor(private scene: Scene) {} run() { this.scene.mutateElement(1, {}); } }\n",
    ),
    (
        "src/f07_indexed_access.ts",
        "import type { AppProps } from \"./props\";\nexport function g(p: AppProps) { p.scene.mutateElement(1, {}); }\n",
    ),
    (
        "src/f08_alias_object_direct.ts",
        "import type { Scene } from \"./scene\";\ntype Local = { scene: Scene };\nexport function h(p: Local) { p.scene.mutateElement(1, {}); }\n",
    ),
    (
        "src/f09_contextual_alias.ts",
        "import type { CB } from \"./cb\";\nexport const k: CB<\"x\"> = ({ scene }) => { scene.mutateElement(1, {}); };\n",
    ),
    (
        "src/f10_destructure_from_typed.ts",
        "import type App from \"./app\";\nexport function m(app: App) { const { scene } = app; scene.mutateElement(1, {}); }\n",
    ),
    (
        "src/f11_window_global.ts",
        "const { h } = window;\nexport function n() { h.scene.mutateElement(1, {}); h.app.scene.mutateElement(1, {}); }\n",
    ),
    (
        "src/f12_interface_param_field.ts",
        "import type { Scene } from \"./scene\";\ninterface Holder { scene: Scene }\nexport function o(x: Holder) { x.scene.mutateElement(1, {}); }\n",
    ),
    (
        "src/f13_local_class_field_chain.ts",
        "import { Scene } from \"./scene\";\nclass Local2 { public scene: Scene = new Scene(); }\nexport function q(x: Local2) { x.scene.mutateElement(1, {}); }\n",
    ),
    (
        "src/f14_imported_alias_direct.ts",
        "import type { AppPropsDirect } from \"./props\";\nexport function r(p: AppPropsDirect) { p.scene.mutateElement(1, {}); }\n",
    ),
    (
        "src/f15_imported_interface_member.ts",
        "import type { HolderI } from \"./holder\";\nexport function s(x: HolderI) { x.scene.mutateElement(1, {}); }\n",
    ),
    (
        "src/f16_imported_alias_member.ts",
        "import type { HolderT } from \"./holder\";\nexport function t(x: HolderT) { x.scene.mutateElement(1, {}); }\n",
    ),
    (
        "src/f17_destructure_from_interface_var.ts",
        "import type { Scene } from \"./scene\";\ninterface H2 { scene: Scene }\nexport function u(x: H2) { const { scene } = x; scene.mutateElement(1, {}); }\n",
    ),
    (
        "src/f18_contextual_local_alias.ts",
        "import type { Scene } from \"./scene\";\ntype CBL = (p: { scene: Scene }) => void;\nexport const v: CBL = ({ scene }) => { scene.mutateElement(1, {}); };\n",
    ),
    (
        "src/f19_local_indexed_access.ts",
        "import type { Scene } from \"./scene\";\ninterface H3 { scene: Scene }\ntype P3 = { scene: H3[\"scene\"] };\nexport function w(x: P3) { x.scene.mutateElement(1, {}); }\n",
    ),
    (
        "src/f20_indexed_destructured.tsx",
        "import type App from \"./app\";\ntype AP = { scene: App[\"scene\"] };\nexport const y = ({ app }: { app: AP }) => { app.scene.mutateElement(1, {}); };\n",
    ),
    (
        "src/f21_param_field_chain.tsx",
        "import type App from \"./app\";\nexport function z(app: App) { app.scene.mutateElement(1, {}); }\n",
    ),
    (
        "src/holder.ts",
        "import type { Scene } from \"./scene\";\nexport interface HolderI { scene: Scene }\nexport type HolderT = { scene: Scene };\n",
    ),
    (
        "src/props.ts",
        "import type App from \"./app\";\nexport type AppProps = { scene: App[\"scene\"] };\nexport type AppPropsDirect = { scene: import(\"./scene\").Scene };\n",
    ),
    (
        "src/scene.ts",
        "export class Scene {\n  mutateElement(el: unknown, updates: object) { return el; }\n}\n",
    ),
    (
        "src/decoy.ts",
        "export class Scene {\n  mutateElement(el: unknown, updates: object) { return el; }\n}\nexport class App {\n  public scene: Scene;\n}\n",
    ),
    (
        "src/generic.ts",
        "import type { Scene } from \"./scene\";\ninterface Box<T> { scene: T }\nexport function gen(x: Box<Scene>) { x.scene.mutateElement(1, {}); }\ntype Loose = { scene: any };\nexport function loose(x: Loose) { x.scene.mutateElement(1, {}); }\n",
    ),
];

#[test]
fn declared_types_type_receivers_across_files() {
    let temp = fixture(B154);
    let root = ingested(&temp);
    let receiver = |caller: &str| edge(caller, "receiver-type", "high");
    assert_eq!(
        caller_edges(root, "src/scene.ts#Scene.mutateElement"),
        [
            receiver("src/f01_param.ts#a"),
            receiver("src/f02_param_field_chain.ts#b"),
            receiver("src/f03_destructured_inline_chain.ts#c"),
            receiver("src/f04_destructured_inline.ts#d"),
            receiver("src/f05_param_property_chain.ts#E.run"),
            receiver("src/f06_param_property.ts#F.run"),
            receiver("src/f07_indexed_access.ts#g"),
            receiver("src/f08_alias_object_direct.ts#h"),
            receiver("src/f09_contextual_alias.ts#k"),
            receiver("src/f10_destructure_from_typed.ts#m"),
            receiver("src/f12_interface_param_field.ts#o"),
            receiver("src/f13_local_class_field_chain.ts#q"),
            receiver("src/f15_imported_interface_member.ts#s"),
            receiver("src/f16_imported_alias_member.ts#t"),
            receiver("src/f17_destructure_from_interface_var.ts#u"),
            receiver("src/f18_contextual_local_alias.ts#v"),
            receiver("src/f19_local_indexed_access.ts#w"),
            receiver("src/f20_indexed_destructured.tsx#y"),
            receiver("src/f21_param_field_chain.tsx#z"),
        ]
    );
    // Same names in another file share only the spelling.
    assert_eq!(caller_edges(root, "src/decoy.ts#Scene.mutateElement"), []);
}

/// The cycle guard holds per path, not per walk: a chain may pass the same
/// class (through an inherited member) or the same alias again.
#[test]
fn a_declared_type_chain_may_meet_a_class_or_alias_again() {
    let temp = fixture(&[
        (
            "src/scene.ts",
            "export class Scene {\n  mutate() { return 1; }\n}\n",
        ),
        (
            "src/node.ts",
            "import type { Scene } from \"./scene\";\nexport class Base {\n  parent: Node;\n  scene: Scene;\n}\nexport class Node extends Base {}\n",
        ),
        (
            "src/app.ts",
            "import type { Scene } from \"./scene\";\nexport class App {\n  self: App;\n  scene: Scene;\n}\nexport type AppAlias = App;\n",
        ),
        (
            "src/use.ts",
            "import type { Node } from \"./node\";\nimport type { AppAlias } from \"./app\";\nexport function one(a: Node) { a.scene.mutate(); }\nexport function two(a: Node) { a.parent.scene.mutate(); }\nexport function three(a: Node) { a.parent.parent.scene.mutate(); }\nexport function alias(a: AppAlias) { a.self.self.scene.mutate(); }\n",
        ),
        (
            "src/cycle.ts",
            "export type A = B;\nexport type B = A;\nexport function loop(x: A) { x.scene.mutate(); }\n",
        ),
    ]);
    let root = ingested(&temp);
    let callers = caller_edges(root, "src/scene.ts#Scene.mutate")
        .into_iter()
        .map(|(id, _, _)| id)
        .collect::<Vec<_>>();
    assert_eq!(
        callers,
        [
            "src/use.ts#alias",
            "src/use.ts#one",
            "src/use.ts#three",
            "src/use.ts#two"
        ]
    );
}
