use std::collections::BTreeMap;

fn receivers(source: &str, path: &str) -> BTreeMap<String, Vec<Option<String>>> {
    let json =
        crate::signatures::js_oxc::tests::extract_graph_facts(source, path).expect("graph facts");
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    let mut out: BTreeMap<String, Vec<Option<String>>> = BTreeMap::new();
    for call in value["calls"].as_array().unwrap() {
        let callee = call["callee"].as_str().unwrap();
        let method = callee.rsplit('.').next().unwrap().to_owned();
        let receiver = call["receiverType"].as_str().map(str::to_owned);
        out.entry(method).or_default().push(receiver);
    }
    out
}

fn assert_receivers(source: &str, path: &str, expected: &[(&str, Option<&str>)]) {
    let actual = receivers(source, path);
    for (method, receiver) in expected {
        let found = actual
            .get(*method)
            .map(|items| items.iter().map(Option::as_deref).collect::<Vec<_>>());
        assert_eq!(
            found,
            Some(vec![*receiver]),
            "{path}: receiverType of `{method}` in {actual:?}"
        );
    }
}

#[test]
fn ts_receiver_types_come_from_annotations_constructors_and_class_fields() {
    let source = r#"
class Svc {
  private repo: Repo;
  cache = new Cache();
  #secret: Vault;
  constructor(private readonly store: Store, db: Db, plain) {
this.conn = new Conn();
this.db = db;
this.plain = plain;
  }
  run(param: Param, list: Item[], maybe: Opt | undefined) {
const s = new Store();
let t: Other = make();
const u = new ns.Thing<number>();
s.saveA();
t.loadB();
u.goC();
param.useD();
list.mapE();
this.repo.findF();
this.cache.getG();
this.store.putH();
this.conn.openI();
this.db.queryJ();
this.plain.plainK();
this.#secret.unlockL();
maybe.unionM();
{
  const s = make();
  s.innerN();
}
s.outerO();
function inner() {
  this.repo.detachedP();
}
const arrow = () => this.repo.arrowQ();
const g = (p: Pool) => p.takeR();
unknown.nopeS();
this.ownT();
  }
}
"#;
    assert_receivers(
        source,
        "src/svc.ts",
        &[
            ("saveA", Some("Store")),
            ("loadB", Some("Other")),
            ("goC", Some("ns.Thing")),
            ("useD", Some("Param")),
            ("mapE", None),
            ("findF", Some("Repo")),
            ("getG", Some("Cache")),
            ("putH", Some("Store")),
            ("openI", Some("Conn")),
            ("queryJ", Some("Db")),
            ("plainK", None),
            ("unlockL", Some("Vault")),
            ("unionM", Some("Opt")),
            ("innerN", None),
            ("outerO", Some("Store")),
            ("detachedP", None),
            ("arrowQ", Some("Repo")),
            ("takeR", Some("Pool")),
            ("nopeS", None),
            ("ownT", None),
        ],
    );
}

#[test]
fn ts_callee_stays_as_written() {
    let json = crate::signatures::js_oxc::tests::extract_graph_facts(
        "function f(x: Store) { x.save(); }",
        "a.ts",
    )
    .unwrap();
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    let call = &value["calls"][0];
    assert_eq!(call["callee"], "x.save");
    assert_eq!(call["receiverType"], "Store");
}

#[test]
fn js_receiver_types_come_from_constructors_and_constructor_fields() {
    let source = r#"
class Svc {
  constructor() {
this.store = new Store();
this.twice = new A();
this.twice = new B();
  }
  run(x) {
const s = new Store();
s.saveA();
this.store.putB();
x.useC();
let t = new Thing();
t = make();
t.reboundD();
this.twice.bothE();
  }
}
function free() {
  const m = new Map();
  m.getF();
  let r = new Reader();
  r.readG();
}
const top = new Top();
top.topH();
"#;
    assert_receivers(
        source,
        "src/svc.js",
        &[
            ("saveA", Some("Store")),
            ("putB", Some("Store")),
            ("useC", None),
            ("reboundD", None),
            ("bothE", None),
            ("getF", Some("Map")),
            ("readG", Some("Reader")),
            ("topH", Some("Top")),
        ],
    );
}

#[test]
fn a_constructed_receiver_is_typed_by_its_class() {
    assert_receivers(
        "function f() { new Store().saveA(); new ns.Repo().findB(); make().runC(); }",
        "a.ts",
        &[
            ("saveA", Some("Store")),
            ("findB", Some("ns.Repo")),
            ("runC", None),
        ],
    );
}

#[test]
fn destructured_parameters_take_their_object_type_property_types() {
    let source = r#"
interface Props { scene: Scene; app?: App | null; count: number }
export type Alias = { store: Store };
export const A = ({ scene, app: renamed, count }: Props) => {
  scene.mutateA();
  renamed.stateB();
  count.toFixedC();
};
function b({ store = fallback }: Alias, { repo }: { repo: Repo }, { other }: Unknown) {
  store.putD();
  repo.findE();
  other.nopeF();
}
function c(props: Props) { props.scene.mutateG(); props.missing.nopeH(); }
"#;
    assert_receivers(
        source,
        "src/a.tsx",
        &[
            ("mutateA", Some("Scene")),
            ("stateB", Some("App")),
            ("toFixedC", None),
            ("putD", Some("Store")),
            ("findE", Some("Repo")),
            ("nopeF", None),
            ("mutateG", Some("Scene")),
            ("nopeH", None),
        ],
    );
}
