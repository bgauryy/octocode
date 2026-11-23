use super::GraphParseAssessment;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Deserialize, Serialize, Eq, PartialEq, Ord, PartialOrd)]
pub struct GraphPosition {
    pub line: u32,
    pub character: u32,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, Eq, PartialEq, Ord, PartialOrd)]
pub struct GraphRange {
    pub start: GraphPosition,
    pub end: GraphPosition,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, Eq, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct GraphFactDeclaration {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub line: u32,
    pub range: GraphRange,
    pub selection_range: GraphRange,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_range: Option<crate::text::analysis::ObservedSpan>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selection_byte_range: Option<crate::text::analysis::ObservedSpan>,
    pub parse_assessment: GraphParseAssessment,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub test_identity: Option<super::GraphTestIdentity>,
    pub exported: bool,
    /// Public names when an exported local binding is exported under another
    /// name (`export { foo as bar }`, named default exports).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub exported_as: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    /// The type a method is declared on outside that type's body: a Go
    /// method's receiver base type (`T` for `func (r *T[U]) M()`), the
    /// written scope of a C++ out-of-line definition (`A::B` for
    /// `void A::B::m() {}`; a class or a namespace).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner: Option<String>,
    /// 0-based first line of the comment block directly above.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub doc_line: Option<u32>,
    /// A C/C++ function declared without its body (a prototype, an in-class
    /// member declaration): the function's definition is another declaration.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub prototype: bool,
    /// A Rust function's declared return type reduced to a type name as
    /// [`GraphFactCall::receiver_type`] spells one, `Self` read as the
    /// enclosing impl's type: the type a method call on its result sees. For
    /// an `async fn` ([`Self::asynchronous`]) it is what the future yields,
    /// which only an awaited call sees.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub return_type: Option<String>,
    /// The type `?` yields from that return type: the first type argument of
    /// a `Result<T, ..>` or `Option<T>`, spelled the same way.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub try_type: Option<String>,
    /// A Rust return type that is a tuple: each element's type, spelled as
    /// [`Self::return_type`] is (`None` for one no type name spells), for a
    /// `let (a, b) = f()` that destructures it.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub return_elements: Vec<Option<String>>,
    /// A Rust `async fn`: its return types are what its future yields.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub asynchronous: bool,
    /// A data member whose written type is a function type (Scala
    /// `A => B`, C# `Action`, C++ `std::function`): a call of it invokes it.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub function_typed: bool,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, Eq, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct GraphFactImport {
    pub id: String,
    pub specifier: String,
    pub line: u32,
    pub import_kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub local_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub imported_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub imported_range: Option<GraphRange>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub local_range: Option<GraphRange>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolution_hint: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub module_scope: Option<Vec<String>>,
    /// Declaration id of the innermost declaration whose body holds the
    /// import: a Scala import is visible only in its enclosing block. Absent
    /// at file level and for other languages.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope_id: Option<String>,
    /// A TypeScript value import whose binding the file reads only in type
    /// positions (annotations, `implements`, `typeof` queries) or not at
    /// all. TypeScript's emit elides such a binding unless the project keeps
    /// value imports (`verbatimModuleSyntax`, decorator metadata), so the
    /// import alone establishes no module load. A JSX tag, an export clause
    /// and any expression read are value uses.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub type_use_only: bool,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, Eq, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct GraphFactExport {
    pub id: String,
    pub name: String,
    pub line: u32,
    pub export_kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub local_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

/// A Rust qualified path outside call and `use` position: a constant or
/// type (`crate::a::LIMIT`), a function passed as a value, or a path inside
/// a macro token tree. Its module prefix is a dependency, as a call's is.
#[derive(Clone, Debug, Default, Deserialize, Serialize, Eq, PartialEq, Ord, PartialOrd)]
#[serde(default, rename_all = "camelCase")]
pub struct GraphFactPath {
    pub line: u32,
    /// The path as written, whitespace removed (`crate::a::LIMIT`).
    pub path: String,
    /// Declaration id of the enclosing item; absent for module-level code.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub caller_id: Option<String>,
    /// Number of inline `mod` items enclosing the path: their `super`s
    /// stay in the file.
    #[serde(skip_serializing_if = "is_zero")]
    pub inline_depth: u32,
}

/// A bare Rust type name a file writes in type or struct-literal position
/// (`fn f(_: &Saved)`, `Saved { .. }`, `impl Saved`), at its first line. A
/// module that glob-imports an ancestor may bind it there.
#[derive(Clone, Debug, Default, Deserialize, Serialize, Eq, PartialEq, Ord, PartialOrd)]
#[serde(default, rename_all = "camelCase")]
pub struct GraphFactTypeName {
    pub name: String,
    pub line: u32,
}

fn is_zero(value: &u32) -> bool {
    *value == 0
}

/// The lookup premise established by the call syntax adapter.
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, Eq, PartialEq)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum GraphCallBinding {
    #[default]
    SyntaxLookup,
    /// The source names a constructed type, not a selected overload.
    ConstructedType,
    Unassessed {
        reason: GraphCallBoundary,
    },
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum GraphCallBoundary {
    TargetTypeUnavailable,
    ConstructorDelegation,
    UnsupportedConstructorForm,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, Eq, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct GraphFactCall {
    pub binding: GraphCallBinding,
    pub id: String,
    pub caller: String,
    /// Declaration id of the enclosing caller; absent for module-level code.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub caller_id: Option<String>,
    pub callee: String,
    pub line: u32,
    pub range: GraphRange,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_range: Option<crate::text::analysis::ObservedSpan>,
    pub parse_assessment: GraphParseAssessment,
    pub kind: String,
    /// Syntactic type of a member call's receiver (`x` in `x.m()`) as written,
    /// without references, pointers or generic arguments; absent when the
    /// parser cannot read it from a declaration, constructor or field.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub receiver_type: Option<String>,
    /// The callee's head names a term the parser saw bound in scope, not a
    /// graph declaration: a bare call `f()` of a parameter, a local variable
    /// or a closure calls that value, never a declaration named `f`
    /// elsewhere. Scala (from its own scope reading) also sets it for the
    /// head of `x.m()` (a lambda or pattern variable, an untyped or abstract
    /// `val`) when `x` has no usable type. JS/TS reads it from oxc semantic
    /// bindings, Python, Go, Rust, C/C++/CUDA and C# from the argument
    /// adapter's lexical bindings; Java never sets it (a bare call names a
    /// method only), nor does a scope a lane cannot settle.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub term_binding: bool,
    /// The id of the declaration a bare callee's name binds, where the lane's
    /// semantics settle it, as an argument's `Declaration` binding does: oxc
    /// for JS/TS (a nested `const f = () => ..` shadows a top-level `f`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bound_declaration: Option<String>,
    /// The receiver is a self alias (`self =>`): the name of the enclosing
    /// template that declares it (for an anonymous class, the declaration
    /// it is the value of). Scala only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub self_template: Option<String>,
    /// The Rust expression macros whose arguments hold this call, outermost
    /// first, as written (`ready`, `std::task::ready`); empty for a call in
    /// plain code.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub macros: Vec<String>,
    /// A Rust member call whose receiver is the value of another call: the
    /// receiver expression itself (`T::open()?.m()`) or a `let` local bound
    /// to it (`let x = c.f(); x.m()`). Recorded beside `receiver_type`: a
    /// written type types the receiver first, but `T::new(..).unwrap()` needs
    /// the declared return type of `T::new`, which outranks the
    /// constructor-name guess.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub receiver_call: Option<GraphReceiverCall>,
    /// A JS/TS member call whose receiver the file types only through a
    /// declared type another declaration owns (`x.f.m()` with `x: T`, where
    /// `T` is a class, interface or alias that types `f`; a parameter typed
    /// by the alias annotating its function): that type and the steps to the
    /// receiver, for the linker to follow across files. Absent when
    /// `receiver_type` already names the receiver's type.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub receiver_declared: Option<GraphDeclaredType>,
}

/// A TS type as a declaration or annotation writes it, kept for the linker
/// to follow across files. Only declared types: no inference, and a type
/// that mentions a type parameter of its declaration is not one of these.
#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum GraphDeclaredType {
    /// The type `name` names in the scope of the file that writes it (`T`,
    /// `ns.T`), then each step: `T["k"]` is `T` then member `k`.
    Named {
        name: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        steps: Vec<GraphTypeStep>,
    },
    /// An object type: a type literal `{ k: T }`, an interface or a class's
    /// declared fields.
    Object { members: Vec<GraphTypeMember> },
    /// A function type `(a: A, b: B) => R`: each parameter's declared type.
    Function { parameters: Vec<Option<Self>> },
}

/// One step from a type to a part of it.
#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(tag = "kind", content = "at", rename_all = "camelCase")]
pub enum GraphTypeStep {
    /// The declared type of member `k`.
    Member(String),
    /// The declared type of the function type's parameter at this index.
    Parameter(u32),
}

/// A member of a [`GraphDeclaredType::Object`]; `ty` is `None` when the
/// member is declared without one of the types above, so it shadows nothing.
#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GraphTypeMember {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ty: Option<GraphDeclaredType>,
}

/// The declared type of one class, interface or type alias of the file, by
/// its declaration id.
#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GraphFactDeclaredType {
    pub declaration_id: String,
    pub ty: GraphDeclaredType,
    /// Declared in the global scope (`declare global { .. }`, or a script
    /// file): an interface of the same name in any other file merges into it.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub global: bool,
}

/// The call whose value a member call's receiver is: the bytes of that call
/// expression, the `source_range` of its own [`GraphFactCall`] in the same
/// file, whether `?` unwraps it on the way, whether it is awaited, and the
/// tuple element a destructuring `let` takes (`let (a, b) = f()`: `b` is 1).
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, Eq, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct GraphReceiverCall {
    pub start_byte: u32,
    pub end_byte: u32,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub unwrap: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub awaited: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub element: Option<u32>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, Eq, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct GraphFactEdge {
    pub id: String,
    pub from: String,
    pub to: String,
    pub relation: String,
    pub source: String,
    pub line: u32,
    pub resolution: String,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, Eq, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct GraphFactRustModule {
    pub name: String,
    pub line: u32,
    pub scope: Vec<String>,
    pub inline: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// The file `#[cfg_attr(test, path = "...")]` mounts it from, under test only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub test_path: Option<String>,
    /// The file `#[cfg_attr(not(test), path = "...")]` mounts it from: a build
    /// under test mounts the module's own name instead.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plain_path: Option<String>,
    pub unsupported: bool,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, Eq, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct GraphFactCommonJs {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub specifier: Option<String>,
    pub line: u32,
    pub kind: String,
    pub binding: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// Names this load binds at module scope, only where oxc semantics prove
    /// the name holds the loaded module (or one property of it) for the whole
    /// file. Every other load keeps its file edge and binds nothing.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub bindings: Vec<GraphFactCommonJsBinding>,
}

/// The public name of a CommonJS module's whole `module.exports` value:
/// distinct from the ESM `default`, so the two module shapes never conflate.
pub const COMMONJS_WHOLE_EXPORT: &str = "module.exports";

/// One proven `require` binding: `const local = require(x)` (`imported` is
/// `None`: the whole `module.exports`), `const local = require(x).imported`
/// or `const { imported: local } = require(x)`. An inline
/// `require(x).k()` binds the receiver as the call is spelled
/// (`require()`) over that one call.
#[derive(Clone, Debug, Default, Deserialize, Serialize, Eq, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct GraphFactCommonJsBinding {
    pub local: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub imported: Option<String>,
    /// The declaration the outline emits for `local`: a variable that holds
    /// the module, never a call target of its own. `None` when the outline
    /// lists no declaration for it (a function-local variable, an inline load).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub declaration_id: Option<String>,
    /// The source extent of the scope that holds the binding: only call sites
    /// inside it read `local` as the module. `None`: the whole file.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<crate::text::analysis::ObservedSpan>,
}

/// The `import_kind`/`export_kind` of a binding whose module load a parse
/// cannot settle: a JS/TS file only the tree-sitter fallback lane read. Not
/// `value`, which the graph reads as eager.
pub const UNKNOWN_LOAD: &str = "unknown";

#[derive(Clone, Debug, Default, Deserialize, Serialize, Eq, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct GraphFactsDocument {
    pub kind: String,
    pub schema_version: u32,
    pub source: String,
    pub language: String,
    pub file: String,
    /// SHA-256 of the UTF-8 text supplied to the parser, before extraction.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub source_digest: String,
    pub source_templates: crate::text::analysis::SourceTemplates,
    pub declarations: Vec<GraphFactDeclaration>,
    pub imports: Vec<GraphFactImport>,
    pub exports: Vec<GraphFactExport>,
    pub calls: Vec<GraphFactCall>,
    pub argument_sites: Vec<super::GraphFactArgumentSite>,
    pub common_js: Vec<GraphFactCommonJs>,
    pub edges: Vec<GraphFactEdge>,
    pub diagnostics: Vec<super::GraphFactDiagnostic>,
    /// 1-based `[start, end]` line spans of the syntax errors a recovered
    /// parse skipped (merged); empty for a clean parse.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub error_lines: Vec<[u32; 2]>,
    pub modules: Vec<GraphFactRustModule>,
    /// 1-based `[start, end]` line spans of items compiled only under test
    /// (Rust `#[cfg(test)]`, `#![cfg(test)]`), outermost spans only.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub test_lines: Vec<[u32; 2]>,
    /// Rust qualified paths outside call and `use` position.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub paths: Vec<GraphFactPath>,
    /// The distinct bare Rust type names written in type or struct-literal
    /// position, by first line, in name order.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub type_names: Vec<GraphFactTypeName>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rust_root_unsupported: Option<bool>,
    /// A Go file's package clause: the name a plain import of it binds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub go_package: Option<String>,
    /// A build constraint keeps this file out of every build: it holds no
    /// graph facts, and no package import reaches it.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub build_excluded: bool,
    /// The declared types of the file's JS/TS classes, interfaces and type
    /// aliases (see [`GraphFactCall::receiver_declared`]).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub declared_types: Vec<GraphFactDeclaredType>,
    /// The file's CommonJS exports hold only on some runs: the whole export
    /// is written in a branch (a UMD wrapper's CommonJS branch), or the
    /// exported value escapes to code that may change its members.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub conditional_exports: bool,
}

impl GraphFactsDocument {
    pub fn observed_spans(&self) -> impl Iterator<Item = crate::text::analysis::ObservedSpan> + '_ {
        self.declarations
            .iter()
            .flat_map(|d| {
                [d.source_range, d.selection_byte_range]
                    .into_iter()
                    .flatten()
            })
            .chain(self.calls.iter().filter_map(|c| c.source_range))
            .chain(
                self.argument_sites
                    .iter()
                    .flat_map(super::GraphFactArgumentSite::observed_spans),
            )
    }
}

#[derive(Clone, Debug)]
pub struct GraphFactsTypedEntry {
    pub relative_path: String,
    pub content_digest: String,
    /// The stamp of the bytes `content_digest` hashed, when settled.
    pub source_stamp: Option<crate::stamp::SourceStamp>,
    pub facts: GraphFactsDocument,
    pub reference_counts: Vec<GraphReferenceCount>,
}

#[cfg(test)]
mod tests;

#[derive(Debug, Clone)]
/// Syntax-aware value references to one declaration, excluding its own name,
/// export clauses and call-callee positions (those are call edges). Comments
/// and string literals never count.
pub struct GraphReferenceCount {
    pub declaration_id: String,
    pub count: u32,
}
