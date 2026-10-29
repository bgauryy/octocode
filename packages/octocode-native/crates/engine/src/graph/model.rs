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
    /// The callee's head (`f` in `f(x)`, `x` in `x.m()`) names a term the
    /// parser saw bound in scope (a parameter, a lambda or pattern variable,
    /// an untyped or abstract `val`) that is not a graph declaration and has
    /// no usable type: it never names a type, an object or a declaration
    /// elsewhere. Scala only.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub term_binding: bool,
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
/// or `const { imported: local } = require(x)`.
#[derive(Clone, Debug, Default, Deserialize, Serialize, Eq, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct GraphFactCommonJsBinding {
    pub local: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub imported: Option<String>,
    /// The declaration the outline emits for `local`: a variable that holds
    /// the module, never a call target of its own.
    pub declaration_id: String,
}

/// What a graph-facts diagnostic says about its file's facts: consumers
/// branch on the code, never on the message text.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, Eq, PartialEq, Hash, Ord, PartialOrd)]
#[serde(rename_all = "camelCase")]
pub enum GraphFactCode {
    /// The static caveat on every tree-sitter file: facts are syntax-only.
    SyntaxOnly,
    /// The parse recovered from syntax errors; facts may be partial.
    RecoveredParse,
    /// An error the JavaScript/TypeScript parser reported (its own text).
    ParseError,
    /// Graph parsing was interrupted by the deadline; facts are incomplete.
    ParseDeadline,
    /// Graph extraction exceeded its deadline; facts are incomplete.
    TraversalDeadline,
    /// Rust macro-generated imports are not linked.
    RustMacroExpansion,
    /// A Rust module carries conditional or custom attributes.
    RustModuleAttributes,
    /// The crate root carries inner conditional or custom attributes.
    RustCrateAttributes,
    /// An observed export-forwarding clause has no admitted member binding.
    UnsupportedExportForwarding,
    /// CommonJS export writes whose final shape is not provable (repeated,
    /// conditional or deferred `module.exports =`, or a rebound `module` or
    /// `exports`): the file exports no CommonJS names.
    UnsupportedCommonJsExports,
}

impl GraphFactCode {
    /// A caveat about import or module linking, which a declaration
    /// listing never does.
    pub fn is_linking_only(self) -> bool {
        matches!(
            self,
            Self::RustMacroExpansion
                | Self::RustModuleAttributes
                | Self::RustCrateAttributes
                | Self::UnsupportedExportForwarding
                | Self::UnsupportedCommonJsExports
        )
    }
}

/// One diagnostic of a graph-facts document.
#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GraphFactDiagnostic {
    pub code: GraphFactCode,
    pub message: String,
    /// The 1-based line the diagnostic is about, when it is about one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
}

impl std::fmt::Display for GraphFactDiagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.line {
            Some(line) => write!(f, "{} at line {line}", self.message),
            None => f.write_str(&self.message),
        }
    }
}

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
    pub diagnostics: Vec<GraphFactDiagnostic>,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rust_root_unsupported: Option<bool>,
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
    pub reference_counts: Vec<crate::types::GraphReferenceCount>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typed_facts_preserve_ranges_modules_edges_and_commonjs() {
        let json = r#"{
          "kind":"graphFacts","schemaVersion":1,"source":"native-ast","language":"rust","file":"lib.rs",
          "declarations":[{"id":"d","name":"run","kind":"function","line":1,"range":{"start":{"line":0,"character":0},"end":{"line":0,"character":8}},"selectionRange":{"start":{"line":0,"character":3},"end":{"line":0,"character":6}},"exported":true}],
          "edges":[{"id":"e","from":"d","to":"c","relation":"calls","source":"native-ast","line":1,"resolution":"unresolved"}],
          "calls":[{"id":"c","caller":"d","callee":"work","line":1,"range":{"start":{"line":0,"character":7},"end":{"line":0,"character":11}},"kind":"direct"}],
          "modules":[{"name":"child","line":2,"scope":[],"inline":false,"path":"child.rs","unsupported":false}],
          "commonJs":[{"specifier":"pkg","line":3,"kind":"commonjs-require","binding":"require"}]
        }"#;
        let facts: GraphFactsDocument = serde_json::from_str(json).expect("typed facts");
        assert_eq!(facts.declarations[0].selection_range.start.character, 3);
        assert_eq!(facts.edges[0].relation, "calls");
        assert_eq!(facts.modules[0].path.as_deref(), Some("child.rs"));
        assert_eq!(facts.common_js[0].specifier.as_deref(), Some("pkg"));
        assert_eq!(facts.common_js[0].kind, "commonjs-require");
    }
}
