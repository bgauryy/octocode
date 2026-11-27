use super::*;

fn resolve_line(file_name: &str, source: &str, symbol_name: &str, line_hint: Option<u32>) -> u32 {
    resolve_position_in_file_content(file_name, source, &fuzzy(symbol_name, line_hint))
        .unwrap_or_else(|err| panic!("{file_name}: {err}"))
        .found_at_line
}

#[test]
fn uses_inside_class_and_function_bodies_are_not_declarations() {
    // Without a line the declaration wins over earlier uses; a use in a
    // class body, method body or object value must not count as one.
    let cases = [
        (
            "demo.ts",
            "class Service {\n  run() { return helper(); }\n}\nfunction helper() { return 1; }\n",
            4,
        ),
        (
            "demo.js",
            "const routes = { run: () => helper() };\nfunction helper() {}\n",
            2,
        ),
        (
            "demo.tsx",
            "class View {\n  render() { return <div>{helper()}</div>; }\n}\nconst helper = () => 1;\n",
            4,
        ),
        (
            "demo.rs",
            "impl S {\n    fn run(&self) { helper(); }\n}\nfn helper() {}\n",
            4,
        ),
        (
            "demo.py",
            "class A:\n    def run(self):\n        return helper()\n\ndef helper():\n    pass\n",
            5,
        ),
        (
            "demo.go",
            "package main\ntype S struct{}\nfunc (s S) Run() { helper() }\nfunc helper() {}\n",
            4,
        ),
        (
            "demo.java",
            "class A {\n  void run() { helper(); }\n  void helper() {}\n}\n",
            3,
        ),
    ];
    for (file_name, source, expected) in cases {
        assert_eq!(
            resolve_line(file_name, source, "helper", None),
            expected,
            "{file_name}"
        );
    }
}

#[test]
fn class_method_field_and_variable_names_are_declarations() {
    let cases = [
        // method name
        (
            "demo.ts",
            "service.helper();\nclass Service { helper() {} }\n",
            "helper",
            2,
        ),
        // class field
        (
            "demo.ts",
            "use(obj.count);\nclass C { count = 0; }\n",
            "count",
            2,
        ),
        // class name
        (
            "demo.ts",
            "new Service();\nclass Service {}\n",
            "Service",
            2,
        ),
        // interface member
        (
            "demo.ts",
            "x.size();\ninterface Box { size(): number }\n",
            "size",
            2,
        ),
        // object literal key
        (
            "demo.js",
            "use(cfg.port);\nconst cfg = { port: 1 };\n",
            "port",
            2,
        ),
        // Rust struct field + struct name
        (
            "demo.rs",
            "fn f(s: S) -> u8 { s.count }\nstruct S { count: u8 }\n",
            "count",
            2,
        ),
        ("demo.rs", "fn f(s: Store) {}\nstruct Store;\n", "Store", 2),
        // Python method + assignment
        (
            "demo.py",
            "obj.run()\nclass A:\n    def run(self):\n        pass\n",
            "run",
            3,
        ),
        ("demo.py", "print(limit)\nlimit = 3\n", "limit", 2),
        // Go short var declaration through an expression_list
        (
            "demo.go",
            "package main\nfunc f() {\n  use(total)\n  total, n := 1, 2\n}\n",
            "total",
            4,
        ),
        // C function declarator
        (
            "demo.c",
            "int main() { return target(); }\nint target() { return 1; }\n",
            "target",
            2,
        ),
    ];
    for (file_name, source, symbol, expected) in cases {
        assert_eq!(
            resolve_line(file_name, source, symbol, None),
            expected,
            "{file_name}: {symbol}"
        );
    }
}

#[test]
fn line_hint_prefers_the_declaration_over_an_equally_near_use() {
    // Line 3 is the hint; the use (line 2, inside a method body) and the
    // declaration (line 4) are equally near, so the declaration must win.
    let source = "class A {\n  run() { return helper(); }\n}\nfunction helper() {}\n";
    let hit = resolve_position_in_file_content("demo.ts", source, &fuzzy("helper", Some(3)))
        .expect("resolves");
    assert_eq!(hit.found_at_line, 4);
    assert_eq!(hit.position.character, 9);
    // An exact hit on the hinted line still wins over a nearby declaration.
    assert_eq!(resolve_line("demo.ts", source, "helper", Some(2)), 2);
}
