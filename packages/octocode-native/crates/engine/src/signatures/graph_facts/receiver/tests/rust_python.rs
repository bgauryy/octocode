//! Rust and Python receiver types.

use super::*;

#[test]
fn rust_receiver_types_come_from_declarations_constructors_and_fields() {
    let source = r#"
struct Svc { store: Arc<Store>, cache: crate::cache::Cache, name: &'static str }
impl Svc {
    fn run(&self, param: &mut Repo, boxed: Box<dyn Sink>, list: &[Item]) {
        let mut a: Vec<Item> = Vec::new();
        let s = Store::new();
        let t = Other::from_config(c)?;
        let u = crate::store::Store { x: 1 };
        let d = Self::default();
        s.save_a();
        t.load_b();
        u.flush_c();
        d.reset_d();
        a.push_e(1);
        param.find_f();
        boxed.write_g();
        self.store.save_h();
        self.cache.get_i();
        self.helper_j();
        list.iter_k();
        unknown.call_l();
        let q = |p: Pool| p.take_m();
        if let Some(v) = maybe { v.opt_n(); }
        {
            let t = Thing::new();
            t.inner_o();
        }
        t.outer_p();
        x.turbo_q::<u8>();
        let s = make();
        s.shadow_r();
        let fresh = s.fresh_s();
    }
}
"#;
    assert_receivers(
        source,
        "src/svc.rs",
        &[
            ("save_a", Some("Store")),
            ("load_b", Some("Other")),
            ("flush_c", Some("crate::store::Store")),
            ("reset_d", Some("Svc")),
            ("push_e", Some("Vec")),
            ("find_f", Some("Repo")),
            ("write_g", Some("Sink")),
            ("save_h", Some("Store")),
            ("get_i", Some("crate::cache::Cache")),
            ("helper_j", None),
            ("iter_k", None),
            ("call_l", None),
            ("take_m", Some("Pool")),
            ("opt_n", None),
            ("inner_o", Some("Thing")),
            ("outer_p", Some("Other")),
            ("turbo_q", None),
            ("shadow_r", None),
            ("fresh_s", None),
        ],
    );
}

#[test]
fn callee_stays_as_written_when_a_receiver_type_is_set() {
    let json = crate::signatures::graph_facts::tests::extract_graph_facts(
        "struct S { store: Store }\nimpl S { fn f(&self) { self.store.save(); } }",
        "src/lib.rs",
    )
    .unwrap();
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    let call = &value["calls"][0];
    assert_eq!(call["callee"], "self.store.save");
    assert_eq!(call["receiverType"], "Store");
}

#[test]
fn rust_let_initializer_sees_the_previous_binding() {
    let source = "fn f(x: Repo) { let x = x.open_a(); x.use_b(); }";
    assert_receivers(
        source,
        "src/lib.rs",
        &[("open_a", Some("Repo")), ("use_b", None)],
    );
}

#[test]
fn python_receiver_types_come_from_annotations_constructors_and_init_fields() {
    let source = r#"
class Svc:
    repo: Repo

    def __init__(self, store: Store, cache=None):
        self.store = store
        self.helper = Helper()
        self.cache = cache
        self.conn: "Conn" = connect()

    def run(self, db: models.Database, opt: Optional[Client] = None, *rest):
        s = Store()
        m = models.Mapper(1)
        n: Node = build()
        s.save_a()
        m.map_b()
        n.visit_c()
        db.query_d()
        opt.send_e()
        self.store.put_f()
        self.helper.help_g()
        self.repo.find_h()
        self.cache.get_i()
        self.conn.open_j()
        low = factory()
        low.lower_k()
        for item in items:
            item.loop_l()
        s = make()
        s.shadow_m()
        self.own_n()
"#;
    assert_receivers(
        source,
        "pkg/svc.py",
        &[
            ("save_a", Some("Store")),
            ("map_b", Some("models.Mapper")),
            ("visit_c", Some("Node")),
            ("query_d", Some("models.Database")),
            ("send_e", Some("Client")),
            ("put_f", Some("Store")),
            ("help_g", Some("Helper")),
            ("find_h", Some("Repo")),
            ("get_i", None),
            ("open_j", Some("Conn")),
            ("lower_k", None),
            ("loop_l", None),
            ("shadow_m", None),
            ("own_n", None),
        ],
    );
}
