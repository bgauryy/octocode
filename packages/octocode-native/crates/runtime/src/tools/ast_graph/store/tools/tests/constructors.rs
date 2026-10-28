use super::*;

fn rows(dir: &Path, operation: &str, target: &str) -> Value {
    let mut input =
        json!({"operation":operation,"path":"app","target":target,"edges":["calls"],"limit":1000});
    if operation == "witnesses" {
        input["confidence"] = json!("low");
    }
    call(dir, ToolId::GraphQuery, input)
}

#[test]
fn constructor_boundaries_retain_sites_without_borrowing_unrelated_bindings() {
    let dir = Fixture::new();
    let sources = [
        (
            "boundary.cs",
            "class C { C(): base(handler) {} void Run(){ C x = new(handler); } }",
        ),
        (
            "boundary.java",
            "class C { C(){ this(null); } C(Object x){ super(); } }",
        ),
        (
            "boundary.cpp",
            "struct Box {}; void run(){ new (place) Box(handler); new Box[3]; new int(1); new typename T::Box(handler); }",
        ),
        (
            "unrelated.py",
            "def new(): pass\ndef base(): pass\ndef super(): pass\ndef this(): pass\n",
        ),
    ];
    for (file, source) in sources {
        dir.write(&format!("app/{file}"), source);
    }
    let built = call(&dir.0, ToolId::GraphBuild, json!({"path":"app"}));
    assert_eq!(built["linkage"]["linked"], 0, "{built}");
    assert_eq!(built["linkage"]["eligible"], 0, "{built}");
    assert_eq!(built["linkage"]["excluded"], 8, "{built}");
    assert!(built["linkage"]["ratio"].is_null());
    let mut reasons = BTreeMap::<String, usize>::new();
    let mut original = Vec::new();
    for (file, source) in sources.into_iter().take(3) {
        let out = rows(&dir.0, "unresolved", file);
        let found = out["results"].as_array().unwrap();
        assert!(!found.is_empty(), "{out}");
        for row in found {
            *reasons
                .entry(row["reason"].as_str().unwrap().into())
                .or_default() += 1;
            assert!(row.get("candidates").is_none());
            assert!(row.get("to").is_none());
            let start = row["sourceRange"]["startByte"].as_u64().unwrap() as usize;
            let end = row["sourceRange"]["endByte"].as_u64().unwrap() as usize;
            assert!(!source[start..end].is_empty());
            assert_eq!(
                row["sourceDigest"],
                octocode_engine::digest::sha256(source.as_bytes())
            );
        }
        assert_eq!(rows(&dir.0, "witnesses", file)["total"], 0);
        original.push((file, out["results"].clone()));
    }
    assert_eq!(
        reasons,
        BTreeMap::from([
            ("constructor-target-unavailable".into(), 1),
            ("constructor-binding-not-assessed".into(), 3),
            ("unsupported-constructor-form".into(), 4),
        ])
    );
    let reused = call(&dir.0, ToolId::GraphBuild, json!({"path":"app"}));
    assert_eq!(reused["id"], built["id"]);
    let fresh = call(
        &dir.0,
        ToolId::GraphBuild,
        json!({"path":"app","force":true}),
    );
    assert_eq!(fresh["linkage"], built["linkage"]);
    for (file, expected) in original {
        assert_eq!(rows(&dir.0, "unresolved", file)["results"], expected);
    }
}

#[test]
fn named_constructions_only_link_type_candidates_and_oxc_keeps_callable_constructors() {
    let dir = Fixture::new();
    dir.write(
        "app/named.cpp",
        "struct Box {}; void Box() {} void run(){ new Box(); new Box; }",
    );
    dir.write(
        "app/factory.js",
        "function Factory() {} function run(){ new Factory(); Factory(); }",
    );
    call(&dir.0, ToolId::GraphBuild, json!({"path":"app"}));
    let named = rows(&dir.0, "witnesses", "named.cpp");
    assert_eq!(named["total"], 2, "{named}");
    for row in named["results"].as_array().unwrap() {
        let target = call(
            &dir.0,
            ToolId::GraphQuery,
            json!({"operation":"node","path":"app","target":row["to"]}),
        );
        assert_eq!(target["node"]["symbolKind"], "struct", "{target}");
        assert_eq!(row["detail"], "constructs:local");
    }
    let js = rows(&dir.0, "witnesses", "factory.js");
    assert_eq!(js["total"], 2, "{js}");
    assert_eq!(
        js["results"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["detail"].as_str().unwrap())
            .collect::<BTreeSet<_>>(),
        BTreeSet::from(["constructs:local", "local"])
    );
}

#[test]
fn recovered_constructor_stays_unresolved_while_healthy_neighbor_links() {
    let dir = Fixture::new();
    dir.write(
        "app/recovered.java",
        "class Box {} class C { void broken(){ new Box(,); } void healthy(){ new Box(); } }",
    );
    let built = call(&dir.0, ToolId::GraphBuild, json!({"path":"app"}));
    assert_eq!(built["linkage"]["linked"], 1, "{built}");
    assert_eq!(built["linkage"]["eligible"], 1);
    assert_eq!(built["linkage"]["excluded"], 1);
    let out = rows(&dir.0, "unresolved", "recovered.java");
    assert_eq!(out["total"], 1, "{out}");
    assert_eq!(out["results"][0]["reason"], "recovered-declaration");
    let healthy = rows(&dir.0, "witnesses", "recovered.java");
    assert_eq!(healthy["total"], 1, "{healthy}");
    assert!(
        healthy["results"][0]["from"]
            .as_str()
            .unwrap()
            .ends_with(".healthy")
    );
}

#[test]
fn leading_global_constructor_does_not_bind_a_nested_type() {
    let dir = Fixture::new();
    dir.write(
        "app/global.cpp",
        "struct Box {}; namespace inner { struct Box {}; void run(){ new ::Box(); } }",
    );
    call(&dir.0, ToolId::GraphBuild, json!({"path":"app"}));
    let out = rows(&dir.0, "witnesses", "global.cpp");
    assert_eq!(out["total"], 1, "{out}");
    assert_eq!(out["results"][0]["to"], "global.cpp#Box");
    dir.write(
        "app/global.cpp",
        "namespace inner { struct Box {}; void run(){ new ::Box(); } }",
    );
    call(&dir.0, ToolId::GraphBuild, json!({"path":"app"}));
    assert_eq!(rows(&dir.0, "witnesses", "global.cpp")["total"], 0);
    let unresolved = rows(&dir.0, "unresolved", "global.cpp");
    assert_eq!(unresolved["total"], 1, "{unresolved}");
}

#[test]
fn constructors_require_the_full_written_container_and_lexical_visibility() {
    for (source, expected) in [
        (
            "namespace Wrong { struct Outer { struct Box {}; }; } void run(){ new Missing::Outer::Box(); new ::Outer::Box(); }",
            2,
        ),
        (
            "namespace Hidden { struct Box {}; } void run(){ new Box(); }",
            1,
        ),
    ] {
        let dir = Fixture::new();
        dir.write("app/scoped.cpp", source);
        call(&dir.0, ToolId::GraphBuild, json!({"path":"app"}));
        let out = rows(&dir.0, "witnesses", "scoped.cpp");
        assert_eq!(out["total"], 0, "{source}: {out}");
        let unresolved = rows(&dir.0, "unresolved", "scoped.cpp");
        assert_eq!(unresolved["total"], expected, "{unresolved}");
        let mut ids = BTreeSet::new();
        for row in unresolved["results"].as_array().unwrap() {
            assert!(ids.insert(row["id"].as_str().unwrap()));
            let start = row["sourceRange"]["startByte"].as_u64().unwrap() as usize;
            let end = row["sourceRange"]["endByte"].as_u64().unwrap() as usize;
            assert!(source[start..end].starts_with("new "));
        }
    }
    let dir = Fixture::new();
    dir.write(
        "app/scoped.cpp",
        "struct Outer { struct Box {}; void run(){ new Box(); new Outer::Box(); } };",
    );
    call(&dir.0, ToolId::GraphBuild, json!({"path":"app"}));
    let out = rows(&dir.0, "witnesses", "scoped.cpp");
    assert_eq!(out["total"], 2, "{out}");
    assert!(
        out["results"]
            .as_array()
            .unwrap()
            .iter()
            .all(|row| row["to"] == "scoped.cpp#Outer.Box")
    );
}

#[test]
fn exact_namespace_constructor_paths_preserve_ambiguity_without_casing_heuristics() {
    let dir = Fixture::new();
    dir.write("app/namespaces.cpp","namespace lower { struct Box {}; } struct Root {}; void run(){ new lower::Box(); new ::lower::Box(); new ::Root(); }");
    call(&dir.0, ToolId::GraphBuild, json!({"path":"app"}));
    let out = rows(&dir.0, "witnesses", "namespaces.cpp");
    assert_eq!(out["total"], 3, "{out}");
    assert!(
        out["results"]
            .as_array()
            .unwrap()
            .iter()
            .all(|row| row["confidence"] == "high")
    );
    assert_eq!(
        out["results"]
            .as_array()
            .unwrap()
            .iter()
            .map(|row| row["to"].as_str().unwrap())
            .collect::<BTreeSet<_>>(),
        BTreeSet::from(["namespaces.cpp#lower.Box", "namespaces.cpp#Root"])
    );
    dir.write("app/namespaces.cpp","namespace lower { struct Box {}; }\nnamespace lower { struct Box {}; }\nvoid run(){ new lower::Box(); }");
    call(&dir.0, ToolId::GraphBuild, json!({"path":"app"}));
    let ambiguous = rows(&dir.0, "witnesses", "namespaces.cpp");
    assert_eq!(ambiguous["total"], 2, "{ambiguous}");
    assert!(
        ambiguous["results"]
            .as_array()
            .unwrap()
            .iter()
            .all(|row| row["confidence"] == "medium")
    );
    let strong = call(
        &dir.0,
        ToolId::GraphQuery,
        json!({"operation":"witnesses","path":"app","target":"namespaces.cpp","edges":["calls"],"confidence":"high"}),
    );
    assert_eq!(strong["total"], 0);
}

#[test]
fn anonymous_namespace_occurrences_remain_distinct_and_do_not_flatten_to_root() {
    let dir = Fixture::new();
    let source = "namespace { struct Box {}; } namespace { struct Other {}; } void run(){ new Box(); new Other(); }";
    dir.write("app/anonymous.cpp", source);
    call(&dir.0, ToolId::GraphBuild, json!({"path":"app"}));
    let scopes = call(
        &dir.0,
        ToolId::GraphQuery,
        json!({"operation":"candidates","path":"app","target":"(anonymous namespace)","kind":"symbol"}),
    );
    assert_eq!(scopes["total"], 2, "{scopes}");
    assert_eq!(
        scopes["results"]
            .as_array()
            .unwrap()
            .iter()
            .map(|row| row["id"].as_str().unwrap())
            .collect::<BTreeSet<_>>()
            .len(),
        2
    );
    for row in scopes["results"].as_array().unwrap() {
        assert_eq!(row["symbolKind"], "namespace");
        let start = row["sourceRange"]["startByte"].as_u64().unwrap() as usize;
        let end = row["sourceRange"]["endByte"].as_u64().unwrap() as usize;
        assert!(source[start..end].starts_with("namespace {"));
    }
    assert_eq!(rows(&dir.0, "witnesses", "anonymous.cpp")["total"], 0);
    assert_eq!(rows(&dir.0, "unresolved", "anonymous.cpp")["total"], 2);
}

#[test]
fn unproven_namespace_reopening_keeps_explicit_unassessed_sites_and_counts() {
    for source in [
        "namespace lower { struct Box {}; } namespace lower { void run(){ new Box(); } }",
        "namespace outer::inner { struct Box {}; } namespace outer { namespace inner { void run(){ new Box(); } } }",
    ] {
        let dir = Fixture::new();
        dir.write("app/reopened.cpp", source);
        let built = call(&dir.0, ToolId::GraphBuild, json!({"path":"app"}));
        assert_eq!(built["linkage"]["eligible"], 0, "{built}");
        assert_eq!(built["linkage"]["excluded"], 1, "{built}");
        assert_eq!(rows(&dir.0, "witnesses", "reopened.cpp")["total"], 0);
        let out = rows(&dir.0, "unresolved", "reopened.cpp");
        assert_eq!(out["total"], 1, "{out}");
        assert_eq!(
            out["results"][0]["reason"],
            "constructor-binding-not-assessed"
        );
        assert_eq!(out["results"][0]["targetName"], "Box");
        assert!(out["results"][0].get("candidates").is_none());
    }
}
