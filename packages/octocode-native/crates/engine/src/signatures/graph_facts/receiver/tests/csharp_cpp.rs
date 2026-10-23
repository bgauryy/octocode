//! C# and C++ receiver types.

use super::*;

#[test]
fn csharp_receiver_types_come_from_locals_params_fields_and_properties() {
    let source = r#"
class Svc {
    private Store store;
    public Repo Repo { get; set; }
    void Run(Cache cache, List<int> ids, int? maybe) {
        var s = new Store();
        Client c = new();
        s.SaveA();
        c.CallB();
        cache.GetC();
        ids.AddD();
        store.PutE();
        this.store.PutF();
        this.Repo.FindG();
        foreach (var it in items) { it.UseH(); }
        if (o is Widget w) { w.DrawI(); }
        var q = Make();
        q.GoJ();
        Console.WriteK();
    }
}
"#;
    assert_receivers(
        source,
        "Svc.cs",
        &[
            ("SaveA", Some("Store")),
            ("CallB", Some("Client")),
            ("GetC", Some("Cache")),
            ("AddD", Some("List")),
            ("PutE", Some("Store")),
            ("PutF", Some("Store")),
            ("FindG", Some("Repo")),
            ("UseH", None),
            ("DrawI", Some("Widget")),
            ("GoJ", None),
            ("WriteK", None),
        ],
    );
}

#[test]
fn cpp_receiver_types_come_from_locals_params_and_members() {
    let source = r#"
class Svc {
  Store* store;
  void run(Repo& repo, const Cache* cache) {
    Store s(1);
    auto p = new Pool();
    ns::Client c;
    s.saveA();
    p->takeB();
    c.callC();
    repo.findD();
    cache->getE();
    store->putF();
    this->store->putG();
    auto q = make();
    q.goH();
  }
};
"#;
    assert_receivers(
        source,
        "svc.cpp",
        &[
            ("saveA", Some("Store")),
            ("takeB", Some("Pool")),
            ("callC", Some("ns::Client")),
            ("findD", Some("Repo")),
            ("getE", Some("Cache")),
            ("putF", Some("Store")),
            ("putG", Some("Store")),
            ("goH", None),
        ],
    );
}
