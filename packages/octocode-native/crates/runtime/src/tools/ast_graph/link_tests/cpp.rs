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

/// `(*this).x()` is `this->x()`, and `this->B::x()` names `B`'s member, as
/// `B::x()` does.
#[test]
fn spelled_out_this_receivers_bind_the_member() {
    let temp = fixture(&[(
        "src/box.cpp",
        "class Base {\npublic:\n  int bm() { return 0; }\n};\nclass Box : public Base {\n  int helper() { return 2; }\n  int deref() { return (*this).helper(); }\n  int scoped() { return this->Base::bm(); }\n};\n",
    )]);
    let root = ingested(&temp);
    assert_eq!(
        callees(root, "src/box.cpp#Box.deref"),
        [to("src/box.cpp#Box.helper", "local")]
    );
    assert_eq!(
        callees(root, "src/box.cpp#Box.scoped"),
        [to("src/box.cpp#Base.bm", "qualified")]
    );
}

/// `p->B::x()` and `(*p).B::x()` name `B`'s member, whatever `p`'s type
/// declares: the written class settles it, so `D`, which hides `x`, gets no
/// edge, and an inherited member is still found through the written class.
/// The same holds when a class defines its members out of line in one file.
#[test]
fn a_qualified_member_call_links_the_class_it_names_not_the_receivers() {
    let temp = fixture(&[
        (
            "inline.cpp",
            "struct A { int z() { return 0; } };\n\
             struct B : A { int x() { return 1; } };\n\
             struct D : B { int x() { return 2; } };\n\
             int arrow(D *p) { return p->B::x(); }\n\
             int deref(D *p) { return (*p).B::x(); }\n\
             int dot(D d) { return d.B::x(); }\n\
             int inherited(D *p) { return p->B::z(); }\n\
             int unwritten(D *p) { return p->x(); }\n",
        ),
        (
            "out.cpp",
            "struct A2 { int z(); };\nint A2::z() { return 0; }\n\
             struct B2 : A2 { int x(); };\nint B2::x() { return 1; }\n\
             struct D2 : B2 { int x(); };\nint D2::x() { return 2; }\n\
             int arrow(D2 *p) { return p->B2::x(); }\n\
             int deref(D2 *p) { return (*p).B2::x(); }\n\
             int dot(D2 d) { return d.B2::x(); }\n\
             int inherited(D2 *p) { return p->B2::z(); }\n\
             int unwritten(D2 *p) { return p->x(); }\n\
             int plain() { return B2::x(); }\n\
             struct C2 { int u(); int v(); int w() { return u(); } };\n\
             int C2::u() { return 3; }\nint C2::v() { return u(); }\n",
        ),
    ]);
    let root = ingested(&temp);
    for caller in ["arrow", "deref", "dot"] {
        assert_eq!(
            callees(root, &format!("inline.cpp#{caller}")),
            [to("inline.cpp#B.x", "qualified")],
            "{caller}"
        );
        assert_eq!(
            callees(root, &format!("out.cpp#{caller}")),
            [to("out.cpp#B2.x@4", "qualified")],
            "out of line: {caller}"
        );
    }
    assert_eq!(
        callees(root, "out.cpp#plain"),
        [to("out.cpp#B2.x@4", "qualified")]
    );
    for caller in ["out.cpp#C2.w", "out.cpp#C2.v@15"] {
        let targets = call_edges(root, caller)
            .into_iter()
            .map(|(id, _, _)| id)
            .collect::<Vec<_>>();
        assert_eq!(targets, ["out.cpp#C2.u@14"], "bare member call: {caller}");
    }
    for (caller, target) in [
        ("inline.cpp#inherited", "inline.cpp#A.z"),
        ("out.cpp#inherited", "out.cpp#A2.z@2"),
    ] {
        assert_eq!(
            call_edges(root, caller)
                .into_iter()
                .map(|(id, _, _)| id)
                .collect::<Vec<_>>(),
            [target],
            "{caller}"
        );
    }
    for (caller, target) in [
        ("inline.cpp#unwritten", "inline.cpp#D.x"),
        ("out.cpp#unwritten", "out.cpp#D2.x@6"),
    ] {
        assert_eq!(
            callees(root, caller),
            [to(target, "receiver-type")],
            "{caller}"
        );
    }
}
