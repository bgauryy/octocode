//! C++ and CUDA linking: namespaces, templates, members, out-of-line
//! definitions and kernel launches.
use super::c_family::{callers, callers_at};
use super::*;

// ── C++ ─────────────────────────────────────────────────────────────────────

const UTIL_HPP: &str = "#pragma once\nnamespace util {\nint parse_num(int x);\n\
    class Widget {\n public:\n  void draw();\n  void redraw() { this->draw(); }\n  virtual void run_it();\n  static Widget make_one();\n};\n\
    struct Derived : public Widget {\n  void run_it() override;\n};\n\
    template <typename T> T twice(T x) { return x + x; }\n}\n";

const UTIL_CPP: &str = "#include \"util.hpp\"\nnamespace util {\nint parse_num(int x) { return x; }\n\
    void Widget::draw() {}\nvoid Widget::run_it() {}\nvoid Derived::run_it() {}\n\
    Widget Widget::make_one() { return Widget(); }\n}\n";

fn cpp_fixture(main: &str) -> Fixture {
    fixture(&[
        ("lib/util.hpp", UTIL_HPP),
        ("lib/util.cpp", UTIL_CPP),
        (
            "other/strings.cpp",
            "namespace text {\nint parse_num(int x) { return x; }\n}\n",
        ),
        ("app/main.cpp", main),
    ])
}

/// `ns::f()` names the declaration in namespace `ns`, reaching its
/// definition; the same name in another namespace is never a candidate, nor
/// is a copy of the namespace the caller does not include (a vendored test
/// copy). A namespace match in no file the caller sees is only a name
/// match (low).
#[test]
fn cpp_namespace_qualified_calls_link_the_namespace_member() {
    let temp = fixture(&[
        ("lib/util.hpp", UTIL_HPP),
        ("lib/util.cpp", UTIL_CPP),
        (
            "other/strings.cpp",
            "namespace text {\nint parse_num(int x) { return x; }\n}\n",
        ),
        (
            "tests/abi/util_v1.hpp",
            "namespace util {\ninline int parse_num(int x) { return x; }\n}\n",
        ),
        (
            "app/main.cpp",
            "#include \"../lib/util.hpp\"\nint qualified() { return util::parse_num(1); }\n\
             int global_qualified() { return ::util::parse_num(2); }\nint other() { return text::parse_num(3); }\n",
        ),
    ]);
    let root = ingested(&temp);
    assert_eq!(
        callers_at(root, "lib/util.cpp#util.parse_num", "high"),
        ["app/main.cpp#global_qualified", "app/main.cpp#qualified"]
    );
    let none = Vec::<String>::new();
    assert_eq!(callers(root, "tests/abi/util_v1.hpp#util.parse_num"), none);
    assert_eq!(callers(root, "lib/util.hpp#util.parse_num"), none);
    assert_eq!(
        callers(root, "other/strings.cpp#text.parse_num"),
        ["app/main.cpp#other"]
    );
    assert_eq!(
        callers_at(root, "other/strings.cpp#text.parse_num", "medium"),
        none
    );
}

/// Explicit template arguments select a specialization of the same
/// function: `twice<int>(3)` and `util::twice(4)` call `util::twice`.
#[test]
fn cpp_template_calls_link_without_their_arguments() {
    let temp = cpp_fixture(
        "#include \"../lib/util.hpp\"\nusing namespace util;\nint templ() { return twice<int>(3) + util::twice(4); }\n",
    );
    let root = ingested(&temp);
    assert_eq!(
        callers(root, "lib/util.hpp#util.twice"),
        ["app/main.cpp#templ"]
    );
}

/// `p->m()` is a member call like `w.m()`, and `this->m()` binds the
/// enclosing class's member; each reaches the out-of-line definition
/// (`void Widget::draw() {}`), which keeps its class in its id.
#[test]
fn cpp_arrow_and_this_member_calls_link_the_member() {
    let temp = cpp_fixture(
        "#include \"../lib/util.hpp\"\nusing namespace util;\n\
         void members() { Widget w; w.draw(); Widget* p = new Widget(); p->draw(); }\n",
    );
    let root = ingested(&temp);
    assert_eq!(
        callers(root, "lib/util.cpp#util.Widget.draw"),
        ["app/main.cpp#members", "lib/util.hpp#util.Widget.redraw"]
    );
    assert_eq!(
        callers(root, "lib/util.hpp#util.Widget.draw"),
        Vec::<String>::new()
    );
}

/// `ns::Type::f()` links a static member of that type, at its definition.
#[test]
fn cpp_type_qualified_calls_link_static_members() {
    let temp =
        cpp_fixture("#include \"../lib/util.hpp\"\nvoid make() { util::Widget::make_one(); }\n");
    let root = ingested(&temp);
    assert_eq!(
        callers(root, "lib/util.cpp#util.Widget.make_one"),
        ["app/main.cpp#make"]
    );
}

/// `Derived::f()` reaches a static member `Derived` inherits from its base.
#[test]
fn cpp_type_qualified_calls_reach_inherited_members() {
    let temp = fixture(&[(
        "shapes.hpp",
        "struct Base {\n  static int make_base() { return 1; }\n};\n\
         struct Child : Base {};\nint build() { return Child::make_base(); }\n",
    )]);
    let root = ingested(&temp);
    assert_eq!(
        callers(root, "shapes.hpp#Base.make_base"),
        ["shapes.hpp#build"]
    );
}

// ── CUDA ────────────────────────────────────────────────────────────────────

/// A `<<<grid, block>>>` launch calls its `__global__` kernel, and the
/// kernel calls its `__device__` helper; a kernel in a `.cuh` header links
/// from the `.cu` file that includes it.
#[test]
fn cuda_kernel_launches_and_device_calls_link() {
    let temp = fixture(&[
        (
            "kernels.cuh",
            "#pragma once\n__device__ inline float square_it(float v) { return v * v; }\n\
             __global__ void scale_kernel(float* x) { x[0] = square_it(x[0]); }\n",
        ),
        (
            "launch.cu",
            "#include \"kernels.cuh\"\n__global__ void local_kernel(float* x) {}\n\
             void launch_it(float* x) { scale_kernel<<<1, 1>>>(x); local_kernel<<<2, 32>>>(x); }\n",
        ),
    ]);
    let root = ingested(&temp);
    assert_eq!(
        callers(root, "kernels.cuh#scale_kernel"),
        ["launch.cu#launch_it"]
    );
    assert_eq!(
        callers(root, "launch.cu#local_kernel"),
        ["launch.cu#launch_it"]
    );
    assert_eq!(
        callers(root, "kernels.cuh#square_it"),
        ["kernels.cuh#scale_kernel"]
    );
}
