//! Java and Go receiver types.

use super::*;

#[test]
fn java_receiver_types_come_from_locals_params_and_fields() {
    let source = r#"
class Svc {
    private Store store;
    private final Map<String, Item> items = new HashMap<>();
    void run(Repo repo, List<String> names, int[] raw) {
        var s = new Store();
        Cache c = new Cache();
        a.b.Client cl = null;
        s.saveA();
        c.getB();
        cl.callC();
        repo.findD();
        names.sizeE();
        store.putF();
        this.store.putG();
        items.getH();
        for (Item it : all) { it.useI(); }
        xs.forEach(s2 -> s2.lambdaJ());
        var q = factory();
        q.goK();
        raw.cloneL();
        Unknown.staticM();
    }
}
"#;
    assert_receivers(
        source,
        "src/Svc.java",
        &[
            ("saveA", Some("Store")),
            ("getB", Some("Cache")),
            ("callC", Some("a.b.Client")),
            ("findD", Some("Repo")),
            ("sizeE", Some("List")),
            ("putF", Some("Store")),
            ("putG", Some("Store")),
            ("getH", Some("Map")),
            ("useI", Some("Item")),
            ("lambdaJ", None),
            ("goK", None),
            ("cloneL", None),
            ("staticM", None),
        ],
    );
}

#[test]
fn java_local_shadows_a_field_with_an_unknown_type() {
    let source = "class A { Store store; void f() { var store = make(); store.saveA(); } }";
    assert_receivers(source, "A.java", &[("saveA", None)]);
}

#[test]
fn go_receiver_types_come_from_params_short_vars_constructors_and_struct_fields() {
    let source = r#"package p

type Server struct {
	store *store.Store
	cache Cache
}

func (s *Server) Run(ctx context.Context, n *Node, xs []Item) error {
	x := NewStore()
	y, err := pkg.NewThing()
	z := &Conn{}
	var w Writer
	var v = Value{}
	q := new(Queue)
	x.SaveA()
	y.DoB()
	z.OpenC()
	w.WriteD()
	v.GetE()
	q.PushF()
	ctx.DoneG()
	n.VisitH()
	s.store.GetI()
	s.cache.PutJ()
	err.ErrorK()
	for _, x := range xs {
		x.LoopL()
	}
	s.OwnM()
	xs.LenN()
	fmt.PrintlnO()
	return nil
}
"#;
    assert_receivers(
        source,
        "server.go",
        &[
            ("SaveA", Some("Store")),
            ("DoB", Some("pkg.Thing")),
            ("OpenC", Some("Conn")),
            ("WriteD", Some("Writer")),
            ("GetE", Some("Value")),
            ("PushF", Some("Queue")),
            ("DoneG", Some("context.Context")),
            ("VisitH", Some("Node")),
            ("GetI", Some("store.Store")),
            ("PutJ", Some("Cache")),
            ("ErrorK", None),
            ("LoopL", None),
            ("OwnM", Some("Server")),
            ("LenN", None),
            ("PrintlnO", None),
        ],
    );
}
