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
        "src/f22_window_member.ts",
        "const hook = window.h;\nexport function aa() { hook.scene.mutateElement(1, {}); }\nexport function ab() { window.h.scene.mutateElement(1, {}); }\nexport function ad() { window.h.app.scene.mutateElement(1, {}); }\n",
    ),
    (
        "src/f23_shadowed_window.ts",
        "export function ac(window: any) { window.h.scene.mutateElement(1, {}); }\nexport function ae(window: Foreign) { window.h.scene.mutateElement(1, {}); }\n",
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
        "export class Scene {\n  mutateElement(el: unknown, updates: object) { return el; }\n}\nexport class App {\n  public scene: Scene;\n}\ninterface Window {\n  h: { scene: Scene };\n}\n",
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
    // A `Window` member reaches the receiver through interface merging across
    // the repository: the declarations may sit outside the program that
    // reads them, so the link is `medium`.
    let windowed = |caller: &str| edge(caller, "receiver-type", "medium");
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
            windowed("src/f11_window_global.ts#n"),
            receiver("src/f12_interface_param_field.ts#o"),
            receiver("src/f13_local_class_field_chain.ts#q"),
            receiver("src/f15_imported_interface_member.ts#s"),
            receiver("src/f16_imported_alias_member.ts#t"),
            receiver("src/f17_destructure_from_interface_var.ts#u"),
            receiver("src/f18_contextual_local_alias.ts#v"),
            receiver("src/f19_local_indexed_access.ts#w"),
            receiver("src/f20_indexed_destructured.tsx#y"),
            receiver("src/f21_param_field_chain.tsx#z"),
            windowed("src/f22_window_member.ts#aa"),
            windowed("src/f22_window_member.ts#ab"),
            windowed("src/f22_window_member.ts#ad"),
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

/// A function or object literal passed to a call takes its parameters' types
/// from the callee's declared parameter, as tsserver types them contextually:
/// a type parameter stands for its constraint (`register<T extends Action>`).
/// An overloaded callee, a shadowed name and an untyped parameter type nothing.
#[test]
fn call_arguments_take_the_callee_parameters_declared_types() {
    let temp = fixture(&[
        (
            "src/scene.ts",
            "export class Scene {\n  mutate() { return 1; }\n}\n",
        ),
        (
            "src/action.ts",
            "import type { Scene } from \"./scene\";\nexport interface Props { scene: Scene }\nexport interface Action<D = any> {\n  name: string;\n  perform: (n: number, props: Props) => void;\n}\n",
        ),
        (
            "src/register.ts",
            "import type { Action } from \"./action\";\nimport type { Props } from \"./action\";\nexport const register = <D, T extends Action<D> = Action<D>>(action: T) => action;\nexport function takes(cb: (p: Props) => void) { cb({} as Props); }\nexport function over(cb: (p: Props) => void): void;\nexport function over(cb: number): void;\nexport function over(cb: any) {}\nexport function loose(cb: (p: any) => void) { cb(1); }\n",
        ),
        (
            "src/use.ts",
            "import { register, takes, over, loose } from \"./register\";\nexport function viaObject() {\n  return register<string>({ name: \"x\", perform: (n, props) => { props.scene.mutate(); } });\n}\nexport function viaFunction() { takes((p) => { p.scene.mutate(); }); }\nexport function overloaded() { over((p) => { p.scene.mutate(); }); }\nexport function untyped() { loose((p) => { p.scene.mutate(); }); }\nexport function shadowed(register: any) { register({ perform: (n: number, props: any) => { props.scene.mutate(); } }); }\n",
        ),
    ]);
    let root = ingested(&temp);
    assert_eq!(
        caller_edges(root, "src/scene.ts#Scene.mutate")
            .into_iter()
            .map(|(id, via, _)| (id, via))
            .collect::<Vec<_>>(),
        [
            (
                "src/use.ts#viaFunction".to_owned(),
                "receiver-type".to_owned()
            ),
            (
                "src/use.ts#viaObject".to_owned(),
                "receiver-type".to_owned()
            ),
        ]
    );
}

/// A name merges through the repository's globals only when no scope binds
/// it: an import-bound name (external or unresolved module), a declaration of
/// a CommonJS file (a module, not a global script) and the DOM `window` in a
/// file with its own `Window` all stay off the global target. `window` is the
/// global `Window` whatever the file declares or imports under that name.
#[test]
fn merged_globals_never_reach_bound_names_or_modules() {
    let temp = fixture(&[
        (
            "src/target.ts",
            "export class Target {\n  m() { return 1; }\n}\n",
        ),
        (
            "src/globals.ts",
            "import type { Target } from \"./target\";\ndeclare global {\n  interface Config { x: Target }\n  interface Window { gh: Target }\n}\nexport {};\n",
        ),
        ("src/ok.ts", "export function ok(c: Config) { c.x.m(); }\n"),
        (
            "src/ext.ts",
            "import type { Config } from \"zod\";\nexport function ext(c: Config) { c.x.m(); }\n",
        ),
        (
            "src/unres.ts",
            "import type { Config } from \"./missing\";\nexport function unres(c: Config) { c.x.m(); }\n",
        ),
        (
            "src/cjs.ts",
            "const t = require(\"./target\");\nclass Wrong { m() { return 2; } }\ninterface Local { x: Wrong }\nmodule.exports = { t };\n",
        ),
        (
            "src/user_local.ts",
            "export function ul(c: Local) { c.x.m(); }\n",
        ),
        (
            "src/win_local.ts",
            "class Wrong2 { m() { return 3; } }\ninterface Window { gh: Wrong2 }\nexport function wl() { window.gh.m(); }\n",
        ),
        (
            "src/winlib.ts",
            "export class Wrong3 { m() { return 4; } }\nexport interface Window { gh: Wrong3 }\n",
        ),
        (
            "src/win_imp.ts",
            "import type { Window } from \"./winlib\";\nexport function wi(w: Window) { return w; }\nexport function wj() { window.gh.m(); }\n",
        ),
    ]);
    let root = ingested(&temp);
    let callers = |target: &str| {
        caller_edges(root, target)
            .into_iter()
            .map(|(id, _, _)| id)
            .collect::<Vec<_>>()
    };
    assert_eq!(
        callers("src/target.ts#Target.m"),
        ["src/ok.ts#ok", "src/win_imp.ts#wj", "src/win_local.ts#wl"]
    );
    for decoy in [
        "src/cjs.ts#Wrong.m",
        "src/win_local.ts#Wrong2.m",
        "src/winlib.ts#Wrong3.m",
    ] {
        assert_eq!(callers(decoy), Vec::<String>::new(), "{decoy}");
    }
}
