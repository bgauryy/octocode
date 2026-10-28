//! Registry-owned argument syntax adapters. Shared graph consumers see only
//! normalized observations and their binding admission, never grammar names.

use crate::graph::GraphArgumentForm;
use crate::types::GrammarArgumentCapabilities;

pub(super) mod binding;
pub(super) mod syntax;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ArgumentAdapter {
    JavaScript,
    Python,
    C,
    #[cfg(any(feature = "tree-sitter-cpp", feature = "tree-sitter-cuda"))]
    CFamily,
    Go,
    Java,
    Rust,
    #[cfg(feature = "tree-sitter-c-sharp")]
    CSharp,
    Scala,
}

impl ArgumentAdapter {
    pub(super) fn is_c_family(self) -> bool {
        match self {
            Self::C => true,
            #[cfg(any(feature = "tree-sitter-cpp", feature = "tree-sitter-cuda"))]
            Self::CFamily => true,
            _ => false,
        }
    }

    pub(super) fn static_module_imports(self) -> bool {
        match self {
            Self::Go | Self::Java | Self::Rust => true,
            #[cfg(feature = "tree-sitter-c-sharp")]
            Self::CSharp => true,
            _ => false,
        }
    }

    pub(super) fn implicit_members(self) -> bool {
        match self {
            Self::Java | Self::Scala => true,
            #[cfg(any(feature = "tree-sitter-cpp", feature = "tree-sitter-cuda"))]
            Self::CFamily => true,
            #[cfg(feature = "tree-sitter-c-sharp")]
            Self::CSharp => true,
            _ => false,
        }
    }

    pub(super) fn binding_visible_after_name(self) -> bool {
        self.is_c_family() || self.implicit_members()
    }

    pub(crate) fn capabilities(self) -> GrammarArgumentCapabilities {
        use GraphArgumentForm as F;
        let mut forms = vec![F::Identifier, F::Member];
        let limits: &[&str] = match self {
            Self::JavaScript => &[
                "spread",
                "computed or optional selections",
                "tagged templates",
            ],
            Self::Python => &[
                "splat",
                "generator groups",
                "comprehension and directive scopes",
                "import visibility and rebinding",
            ],
            #[cfg(any(feature = "tree-sitter-cpp", feature = "tree-sitter-cuda"))]
            Self::CFamily => {
                forms.extend([F::Qualified, F::AddressOf]);
                &[
                    "macro expansion",
                    "packs",
                    "placement and initializer groups",
                ]
            }
            Self::C => {
                forms.push(F::AddressOf);
                &["macro expansion", "computed receivers"]
            }
            Self::Go => {
                forms.push(F::AddressOf);
                &["variadic arguments", "call versus conversion semantics"]
            }
            Self::Java => {
                forms.push(F::MethodReference);
                &["constructor references", "computed receivers"]
            }
            Self::Rust => {
                forms.extend([F::Qualified, F::Borrow]);
                &[
                    "macro token trees",
                    "expansion",
                    "capture and pattern scope gaps",
                ]
            }
            #[cfg(feature = "tree-sitter-c-sharp")]
            Self::CSharp => &[
                "out-var declarations",
                "conditional, alias-qualified or computed selections",
            ],
            Self::Scala => &[
                "block, case and colon groups",
                "varargs",
                "named argument forms",
                "import visibility and rebinding",
            ],
        };
        GrammarArgumentCapabilities {
            supported: true,
            forms,
            unsupported_constructs: limits.iter().map(|s| (*s).to_owned()).collect(),
        }
    }

    pub(super) fn is_recipient(self, kind: &str) -> bool {
        match self {
            Self::JavaScript => matches!(kind, "call_expression" | "new_expression"),
            Self::Python => kind == "call",
            Self::Java => matches!(
                kind,
                "method_invocation"
                    | "object_creation_expression"
                    | "explicit_constructor_invocation"
            ),
            #[cfg(feature = "tree-sitter-c-sharp")]
            Self::CSharp => matches!(
                kind,
                "invocation_expression"
                    | "object_creation_expression"
                    | "implicit_object_creation_expression"
                    | "constructor_initializer"
            ),
            Self::Rust => matches!(kind, "call_expression" | "macro_invocation"),
            #[cfg(any(feature = "tree-sitter-cpp", feature = "tree-sitter-cuda"))]
            Self::CFamily => matches!(kind, "call_expression" | "new_expression"),
            Self::C | Self::Go | Self::Scala => kind == "call_expression",
        }
    }
}

#[cfg(test)]
mod tests;
