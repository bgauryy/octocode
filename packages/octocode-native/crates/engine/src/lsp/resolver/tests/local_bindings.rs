use super::super::local_binding_lines;

/// The `num.rs` shape: three functions, each with its own `value` parameter.
const SAME_SPELLED_PARAMETERS: &str = "\
pub fn saturating_u32(value: impl TryInto<u32>) -> u32 {
    value.try_into().unwrap_or(u32::MAX)
}

pub fn saturating_i64(value: impl TryInto<i64>) -> i64 {
    value.try_into().unwrap_or(i64::MAX)
}
";

#[test]
fn a_parameter_and_its_use_name_only_their_own_function() {
    // The parameter itself, and its use on the next line.
    assert_eq!(
        local_binding_lines("num.rs", SAME_SPELLED_PARAMETERS, "value", 0, 22),
        Some((0, 2))
    );
    assert_eq!(
        local_binding_lines("num.rs", SAME_SPELLED_PARAMETERS, "value", 5, 4),
        Some((4, 6))
    );
}

#[test]
fn a_let_binding_is_scoped_to_its_block() {
    let source = "fn outer() {\n    let total = 1;\n    use_it(total);\n}\nfn total() {}\n";
    assert_eq!(
        local_binding_lines("a.rs", source, "total", 2, 11),
        Some((0, 3))
    );
}

/// A `let` anchored on its own name is that binding, as a use of it is;
/// its initializer still names the outer one it shadows.
#[test]
fn a_let_anchored_on_its_own_name_is_local() {
    let source = "fn outer() {\n    let total = 1;\n    use_it(total);\n}\nfn total() {}\n";
    assert_eq!(
        local_binding_lines("a.rs", source, "total", 1, 8),
        Some((0, 3))
    );
    let shadow = "fn f(x: u32) -> u32 {\n    let x = x + 1;\n    x\n}\n";
    assert_eq!(local_binding_lines("a.rs", shadow, "x", 1, 8), Some((0, 3)));
    assert_eq!(
        local_binding_lines("a.rs", shadow, "x", 1, 12),
        Some((0, 3))
    );
}

#[test]
fn items_fields_and_module_names_are_not_local() {
    let source = "pub struct Entry {\n    pub value: u32,\n}\nfn read(entry: Entry) -> u32 {\n    entry.value\n}\n";
    // A struct field, its use through an instance, and an item name.
    assert_eq!(local_binding_lines("a.rs", source, "value", 1, 8), None);
    assert_eq!(local_binding_lines("a.rs", source, "value", 4, 10), None);
    assert_eq!(local_binding_lines("a.rs", source, "Entry", 0, 11), None);
    // A module-level Python name is importable elsewhere; a variable bound
    // in a function body is not.
    let python = "value = 1\n\ndef f():\n    value = 2\n    return value\n";
    assert_eq!(local_binding_lines("a.py", python, "value", 0, 0), None);
    assert_eq!(
        local_binding_lines("a.py", python, "value", 4, 11),
        Some((2, 4))
    );
}

/// A call site names a Python or Scala parameter by keyword (`f(value=1)`,
/// `f(value = 1)`), in any file that reaches the function: such a
/// parameter has no local scope.
#[test]
fn a_parameter_callers_name_by_keyword_is_not_local() {
    let python = "def f(value):\n    return value\n\nf(value=1)\n";
    assert_eq!(local_binding_lines("a.py", python, "value", 0, 6), None);
    assert_eq!(local_binding_lines("a.py", python, "value", 1, 11), None);
    let scala = "object A {\n  def f(value: Int): Int = value\n}\n";
    assert_eq!(local_binding_lines("A.scala", scala, "value", 1, 8), None);
    // A Rust parameter has no keyword form: it stays local.
    assert_eq!(
        local_binding_lines(
            "a.rs",
            "fn f(value: u32) -> u32 {\n    value\n}\n",
            "value",
            1,
            4
        ),
        Some((0, 2))
    );
}

#[test]
fn a_java_field_is_no_local_but_a_method_variable_is() {
    let source =
        "class A {\n  int count;\n  void f() {\n    int total = count;\n    g(total);\n  }\n}\n";
    assert_eq!(local_binding_lines("A.java", source, "count", 1, 6), None);
    assert_eq!(
        local_binding_lines("A.java", source, "total", 4, 6),
        Some((2, 5))
    );
}
