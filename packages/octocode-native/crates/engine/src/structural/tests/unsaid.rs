//! A declaration pattern matches the parts it spells and leaves the rest
//! unsaid: modifiers, type parameters, a return type, a `where` clause. What
//! it spells stays exact, and calls and expressions keep every part.

use super::*;

mod rules;

fn lines(src: &str, ext: &str, pattern: &str) -> Vec<u32> {
    search(src, ext, Some(pattern), None)
        .unwrap_or_else(|error| panic!("{ext} {pattern}: {error}"))
        .iter()
        .map(|found| found.start_line)
        .collect()
}

/// Every line of `src` is a declaration the pattern means.
fn assert_all_lines(ext: &str, pattern: &str, src: &str) {
    let expected = (1..=u32::try_from(src.lines().count()).expect("small")).collect::<Vec<_>>();
    assert_eq!(
        lines(src, ext, pattern),
        expected,
        "{ext}: {pattern}\n{src}"
    );
}

#[test]
fn a_function_pattern_matches_whatever_it_leaves_unsaid() {
    let cases: &[(&str, &str, &str)] = &[
        (
            "ts",
            "function $N($$$) { $$$ }",
            "function a() {}\nfunction b(x: number): void {}\nfunction c<T>(x: T): T { return x; }\nfunction d(x): x is string { return true; }\nfunction e(x): asserts x {}\nfunction* f() {}\nasync function g(): Promise<void> {}\n",
        ),
        (
            "tsx",
            "function $N($$$) { $$$ }",
            "function a() {}\nfunction b(): JSX.Element { return <div />; }\nfunction c<T,>(x: T): T { return x; }\n",
        ),
        (
            "js",
            "function $N($$$) { $$$ }",
            "function a() {}\nfunction* b() {}\nasync function c() {}\n",
        ),
        (
            "rs",
            "fn $N($$$) { $$$ }",
            "fn a() {}\npub fn b() {}\npub(crate) fn c() -> u8 { 1 }\nasync fn d() {}\nunsafe fn e() {}\npub async unsafe fn f<T>(x: T) -> T where T: Copy { x }\nconst fn g() {}\n",
        ),
        (
            "py",
            "def $N($$$):\n    $$$",
            "def a(): pass\ndef b(x) -> int: return x\nasync def c(): pass\ndef d[T](x: T) -> T: return x\n",
        ),
    ];
    for (ext, pattern, src) in cases {
        assert_all_lines(ext, pattern, src);
    }
    let go = "package p\nfunc a() {}\nfunc b(x int) (int, error) { return x, nil }\nfunc c[T any](x T) T { return x }\n";
    assert_eq!(lines(go, "go", "func $N($$$) { $$$ }"), [2, 3, 4]);
}

#[test]
fn a_method_pattern_matches_whatever_it_leaves_unsaid() {
    let java = "class A {\n  void a() {}\n  public void b() {}\n  public static <T> void c(T x) {}\n  @Override protected void d() throws E {}\n  int e() { return 1; }\n}\n";
    assert_eq!(lines(java, "java", "void $N($$$) { $$$ }"), [2, 3, 4, 5]);
    assert_eq!(lines(java, "java", "$T $N($$$) { $$$ }"), [2, 3, 4, 5, 6]);
    let csharp = "class A {\n  void a() {}\n  public void b() {}\n  public static async void c<T>(T x) where T : B {}\n  int d() { return 1; }\n}\n";
    assert_eq!(lines(csharp, "cs", "void $N($$$) { $$$ }"), [2, 3, 4]);
    let cpp = "void a() {}\nstatic void b() {}\ninline void c(int x) noexcept {}\nint d() { return 1; }\n";
    assert_eq!(lines(cpp, "cpp", "void $N($$$) { $$$ }"), [1, 2, 3]);
    let c = "void a(void) {}\nstatic void b(int x) {}\nint c(void) { return 1; }\nvoid *d(void) { return 0; }\n";
    // A type it spells is exact, and a capture takes the pointer's function too.
    assert_eq!(lines(c, "c", "void $N($$$) { $$$ }"), [1, 2]);
    assert_eq!(lines(c, "c", "$T $N($$$) { $$$ }"), [1, 2, 3, 4]);
    let cpp_refs = "int &a() { return x; }\nint b() { return 1; }\nint *c() { return 0; }\n";
    assert_eq!(lines(cpp_refs, "cpp", "int $N($$$) { $$$ }"), [2]);
    assert_eq!(lines(cpp_refs, "cpp", "$T $N($$$) { $$$ }"), [1, 2, 3]);
    assert_eq!(lines(cpp_refs, "cpp", "$M($$$) { $$$ }"), [1, 2, 3]);
    let scala = "object O {\n  def a(x: Int) = x\n  def b(x: Int): Int = x\n  private def c[T](x: T): T = x\n  def d = 1\n}\n";
    assert_eq!(lines(scala, "scala", "def $N($$$) = $B"), [2, 3, 4, 5]);
}

#[test]
fn a_type_pattern_matches_whatever_it_leaves_unsaid() {
    let ts = "class A {}\nclass B<T> {}\nclass C extends D {}\nclass E<T> extends D implements F {}\nabstract class G {}\n@dec class H {}\n";
    assert_eq!(lines(ts, "ts", "class $C { $$$ }"), [1, 2, 3, 4, 5, 6]);
    let rust = "struct A {}\npub struct B<T> { x: T }\npub(crate) struct C where u8: Copy {}\n";
    assert_eq!(lines(rust, "rs", "struct $S { $$$ }"), [1, 2, 3]);
    let imp = "impl A {}\nimpl<T> B<T> {}\nimpl<T> C<T> where T: Copy {}\nimpl D for E {}\nunsafe impl F for G {}\n";
    assert_eq!(lines(imp, "rs", "impl $T { $$$ }"), [1, 2, 3, 4, 5]);
    let java = "class A {}\npublic final class B<T> extends C implements D {}\n";
    assert_eq!(lines(java, "java", "class $C { $$$ }"), [1, 2]);
    let python = "class A: pass\nclass B(C): pass\nclass D[T](E): pass\n";
    assert_eq!(lines(python, "py", "class $C:\n    $$$"), [1, 2, 3]);
}

#[test]
fn a_binding_pattern_matches_whatever_it_leaves_unsaid() {
    let rust = "fn f() {\n  let a = g();\n  let mut b = g();\n  let c: u8 = g();\n}\n";
    assert_eq!(lines(rust, "rs", "let $X = g();"), [2, 3, 4]);
    let constant = "const A: u8 = 1;\npub const B: u8 = 1;\npub(crate) const C: u8 = 1;\n";
    assert_eq!(lines(constant, "rs", "const $N: $T = 1;"), [1, 2, 3]);
    let ts = "const a = g();\nconst b: number = g();\n";
    assert_eq!(lines(ts, "ts", "const $X = g()"), [1, 2]);
    let java = "class A { void f() {\n  int a = 1;\n  final int b = 1;\n} }\n";
    assert_eq!(lines(java, "java", "int $X = 1;"), [2, 3]);
    let c = "int a = 1;\nstatic int b = 1;\nconst int c = 1;\n";
    assert_eq!(lines(c, "c", "int $X = 1;"), [1, 2, 3]);
}

/// A pattern of `($$$)` says any parameters: however the grammar spells them
/// there, or none where its grammar omits the list.
#[test]
fn any_parameters_take_every_spelling_of_the_parameter_slot() {
    let ts = "const a = x => x;\nconst b = (x) => x;\nconst c = async x => x;\nconst d = (x: number): number => x;\nconst e = <T,>(x: T) => x;\nconst f = () => 1;\n";
    assert_eq!(lines(ts, "ts", "($$$) => $B"), [1, 2, 3, 4, 5, 6]);
    let js = "a(x => x);\na((x, y) => x);\n";
    assert_eq!(lines(js, "js", "($$$) => $B"), [1, 2]);
    let java = "class A { void f() {\n  g(x -> x);\n  g((x, y) -> x);\n  g(() -> 1);\n} }\n";
    assert_eq!(lines(java, "java", "($$$) -> $B"), [2, 3, 4]);
    let csharp = "class A { void F() {\n  G(x => x);\n  G((x, y) => x);\n} }\n";
    assert_eq!(lines(csharp, "cs", "G(($$$) => $B)"), [2, 3]);
    let python = "a = lambda: 1\nb = lambda x: x\n";
    assert_eq!(lines(python, "py", "lambda $$$: $B"), [1, 2]);
    // The single parameter is the capture.
    let found = search("const a = x => x;\n", "ts", Some("($$$ARGS) => $B"), None).expect("arrow");
    assert_eq!(found[0].metavars["ARGS"], ["x"]);
}

/// A method spelled alone parses in its class's context in a grammar where it
/// needs one, and its class's other parts stay unsaid.
#[test]
fn a_bare_method_pattern_matches_methods() {
    let ts = "class A {\n  a() {}\n  public b(): void {}\n  private static async c<T>(x: T): Promise<T> { return x; }\n  get d() { return 1; }\n}\nf();\nif (x) {}\n";
    assert_eq!(lines(ts, "ts", "$M($$$) { $$$ }"), [2, 3, 4, 5]);
    assert_eq!(lines(ts, "tsx", "$M($$$) { $$$ }"), [2, 3, 4, 5]);
    assert_eq!(lines(ts, "ts", "a() { $$$ }"), [2]);
    let js = "class A {\n  a() {}\n  static b() {}\n}\nf();\n";
    assert_eq!(lines(js, "js", "$M($$$) { $$$ }"), [2, 3]);
}

/// The members a type's pattern spells are ones it holds, in order; the rest
/// of its members stay unsaid.
#[test]
fn a_type_pattern_holds_the_members_it_spells() {
    let ts = "class A { a() {} }\nclass B { x = 1; a() {} b() {} }\nexport class C<T> extends D { public a(): void {} }\nclass E {}\nclass F { x = 1; }\n";
    assert_eq!(lines(ts, "ts", "class $C { $M($$$) { $$$ } }"), [1, 2, 3]);
    assert_eq!(lines(ts, "ts", "class $C { b() { $$$ } }"), [2]);
    // Order is what the pattern spells.
    assert_eq!(lines(ts, "ts", "class $C { a() { $$$ } b() { $$$ } }"), [2]);
    assert_eq!(
        lines(ts, "ts", "class $C { b() { $$$ } a() { $$$ } }"),
        Vec::<u32>::new()
    );
    // An empty body is spelled.
    assert_eq!(lines(ts, "ts", "class $C {}"), [4]);
    let rust = "impl A { fn a() {} fn b() {} }\nimpl B { const X: u8 = 1; pub fn a() -> u8 { 1 } }\nimpl C { fn c() {} }\n";
    assert_eq!(lines(rust, "rs", "impl $T { fn a($$$) { $$$ } }"), [1, 2]);
    let python = "class A:\n    x = 1\n    def a(self): pass\n    def b(self): pass\n";
    assert_eq!(
        lines(python, "py", "class $C:\n    def b($$$):\n        $$$"),
        [1]
    );
}

/// What a pattern spells is exact: a part it spells must be there and agree,
/// and a function's parameters and body hold what it says and no more.
#[test]
fn a_spelled_part_stays_exact() {
    let rust = "fn a() {}\npub fn b() {}\nfn c(x: u8) {}\nfn d() -> u8 { 1 }\nfn e() { g(); h(); }\nfn f() { g(); }\n";
    assert_eq!(lines(rust, "rs", "pub fn $N($$$) { $$$ }"), [2]);
    assert_eq!(lines(rust, "rs", "fn $N() { $$$ }"), [1, 2, 4, 5, 6]);
    assert_eq!(lines(rust, "rs", "fn $N($$$) -> $R { $$$ }"), [4]);
    assert_eq!(
        lines(rust, "rs", "fn $N($$$) -> u16 { $$$ }"),
        Vec::<u32>::new()
    );
    assert_eq!(lines(rust, "rs", "fn $N($X: u8) { $$$ }"), [3]);
    assert_eq!(lines(rust, "rs", "fn $N() { g(); }"), [6]);
    assert_eq!(lines(rust, "rs", "fn a() { $$$ }"), [1]);
    let java = "class A {\n  public void a() {}\n  private void b() {}\n  void c() {}\n}\n";
    assert_eq!(lines(java, "java", "public void $N($$$) { $$$ }"), [2]);
    assert_eq!(
        lines(java, "java", "int $N($$$) { $$$ }"),
        Vec::<u32>::new()
    );
    let ts = "function a(): void {}\nfunction b(x): x is string { return true; }\nfunction c(y) {}\nfunction d(y, z) {}\nfunction e(x): asserts x {}\n";
    assert_eq!(lines(ts, "ts", "function $N($$$): $R { $$$ }"), [1, 2, 5]);
    assert_eq!(lines(ts, "ts", "function $N($$$): void { $$$ }"), [1]);
    assert_eq!(lines(ts, "ts", "function $N($X) { $$$ }"), [2, 3, 5]);
    // A block body is not an expression body.
    let arrows = "const a = () => { g(); };\nconst b = () => g();\n";
    assert_eq!(lines(arrows, "ts", "() => { $$$ }"), [1]);
    assert_eq!(lines(arrows, "ts", "() => g()"), [2]);
    // A generator's star, spelled, is required.
    let generators = "function a() {}\nfunction* b() {}\n";
    assert_eq!(lines(generators, "ts", "function* $N($$$) { $$$ }"), [2]);
}

/// A function's body holds what the pattern says: a nested declaration is no
/// member list.
#[test]
fn a_function_body_is_not_a_member_list() {
    let src = "function a() { function b() {} g(); }\nfunction c() { function d() {} }\n";
    let found =
        search(src, "ts", Some("function $F() { function $G() {} }"), None).expect("nested");
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].start_line, 2);
}

/// A C# statement around a `$$$` block is that statement, not a method named
/// `if`.
#[test]
fn a_csharp_statement_with_a_capture_body_stays_a_statement() {
    let src = "class A { void F(int[] xs) {\n  if (x) { G(); }\n  while (x) { G(); }\n  foreach (var y in xs) { G(); }\n  for (;;) { G(); }\n  lock (x) { G(); }\n  try { G(); } catch { }\n} }\n";
    assert_eq!(lines(src, "cs", "if ($A) { $$$ }"), [2]);
    assert_eq!(lines(src, "cs", "while ($A) { $$$ }"), [3]);
    assert_eq!(lines(src, "cs", "foreach ($T $X in $Y) { $$$ }"), [4]);
    assert_eq!(lines(src, "cs", "lock ($A) { $$$ }"), [6]);
    assert_eq!(lines(src, "cs", "try { $$$ } catch { }"), [7]);
    // A member-shaped pattern keeps its class context.
    let members = "class A { public int F() { return 1; } A() {} }\n";
    assert_eq!(lines(members, "cs", "public int $N($$$) { $$$ }"), [1]);
    assert_eq!(lines(members, "cs", "A($$$) { $$$ }"), [1]);
}

/// A `$$$` before another member is the pattern's way to say "among others",
/// and reads the same as leaving them unsaid.
#[test]
fn a_capture_between_members_compiles_as_a_member() {
    let ts = "class A { x = 1; a() {} y = 2; }\nclass B { a() {} }\nclass C { b() {} }\n";
    assert_eq!(lines(ts, "ts", "class $C { $$$ a() { $$$ } $$$ }"), [1, 2]);
    let java = "class A { int x = 1; void a() {} int y = 2; }\nclass B { void a() {} }\nclass C { int b() { return 1; } }\n";
    assert_eq!(
        lines(java, "java", "class $C { $$$ void $M($$$) { $$$ } $$$ }"),
        [1, 2]
    );
}

/// A field declared with a modifier or a type in a class reads as its members.
#[test]
fn a_bare_field_pattern_matches_fields() {
    let ts = "class A {\n  private x = 1;\n  public y = 2;\n  z = 3;\n}\n";
    assert_eq!(lines(ts, "ts", "private $F = $V"), [2]);
    assert_eq!(lines(ts, "ts", "public $F = $V"), [3]);
}

/// `struct S` is no definition of `struct $S { $$$ }`: `$$$` fills a body
/// that is there.
#[cfg(feature = "tree-sitter-cpp")]
#[test]
fn any_body_needs_a_body() {
    let src = "struct A {};\nstruct B x;\nvoid f(struct C y);\n";
    assert_eq!(lines(src, "cpp", "struct $S { $$$ }"), [1]);
}

/// Calls and expressions keep every part: the object of a method call, type
/// arguments, attributes, a block's other statements.
#[test]
fn calls_and_expressions_are_unchanged() {
    let java = "class A { void f() {\n  g(1);\n  o.g(1);\n  this.g(1);\n} }\n";
    assert_eq!(lines(java, "java", "g($X)"), [2]);
    assert_eq!(lines(java, "java", "$O.g($X)"), [3, 4]);
    let ts = "g(1);\ng<T>(1);\no.g(1);\nnew A(1);\nnew A<T>(1);\ng(1, 2);\n";
    assert_eq!(lines(ts, "ts", "g($X)"), [1]);
    assert_eq!(lines(ts, "ts", "new A($X)"), [4]);
    assert_eq!(lines(ts, "ts", "g($X, $Y)"), [6]);
    assert_eq!(lines(ts, "ts", "g($$$)"), [1, 6]);
    let tsx = "const a = <A b=\"1\" />;\nconst c = <A />;\n";
    assert_eq!(lines(tsx, "tsx", "<A />"), [2]);
    let rust = "fn f() {\n  g(1);\n  g::<u8>(1);\n  o.g(1);\n  let x = S { a: 1 };\n  let y = S { a: 1, b: 2 };\n}\n";
    assert_eq!(lines(rust, "rs", "g($X)"), [2]);
    assert_eq!(lines(rust, "rs", "let $V = S { a: 1 };"), [5]);
    let go = "package p\nfunc f() {\n  for {\n  }\n  for i := 0; i < 1; i++ {\n  }\n}\n";
    assert_eq!(lines(go, "go", "for { $$$ }"), [3]);
    let ifs = "if (a) { g(); }\nif (a) { g(); h(); }\n";
    assert_eq!(lines(ifs, "ts", "if ($A) { g(); }"), [1]);
    let imports = "import { a } from 'x';\nimport { a, b } from 'x';\n";
    assert_eq!(lines(imports, "ts", "import { a } from 'x'"), [1]);
}

/// A directory of grammars that spell a return type differently reads one
/// pattern text per grammar, with no respelling.
#[test]
fn each_grammar_reads_the_pattern_itself() {
    let dir = TmpDir::new();
    dir.write("a.ts", "function a(): void {}\n");
    dir.write("b.js", "function b() {}\n");
    let mut options = review_file_options(&dir, json!({"pattern":"function $N($$$) { $$$ }"}), 10);
    options.max_files = None;
    let found = search_files_detailed(options).expect("search");
    assert_eq!(matched_paths(&found).len(), 2);
}

/// Text the grammar cannot place is an error, not a pattern that matches
/// nothing.
#[test]
fn a_recovered_pattern_with_stray_text_is_an_error() {
    for (ext, pattern) in [
        ("ts", "if ($A) { $$$ } ]"),
        ("ts", "f() g() {"),
        ("rs", "fn $N() { $$$ } }"),
    ] {
        let error = search("", ext, Some(pattern), None).err();
        assert!(
            error
                .as_deref()
                .is_some_and(|e| e.contains("invalid structural pattern")),
            "{ext} {pattern}: {error:?}"
        );
    }
}
