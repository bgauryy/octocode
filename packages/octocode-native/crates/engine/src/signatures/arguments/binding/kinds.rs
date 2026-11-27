//! The syntax kinds that open a scope or bind a name, per adapter.

use super::ArgumentAdapter;

pub(super) fn is_callable_scope(kind: &str) -> bool {
    matches!(
        kind,
        "function_definition"
            | "function_item"
            | "function_declaration"
            | "method_declaration"
            | "constructor_declaration"
            | "local_function_statement"
            | "lambda_expression"
            | "closure_expression"
            | "func_literal"
    )
}

pub(super) fn is_block_scope(kind: &str) -> bool {
    matches!(
        kind,
        "block"
            | "compound_statement"
            | "statement_block"
            | "constructor_body"
            | "class_body"
            | "field_declaration_list"
            | "declaration_list"
            | "for_statement"
            | "for_expression"
            | "enhanced_for_statement"
            | "for_each_statement"
            | "catch_clause"
            | "match_arm"
    )
}

pub(super) fn binding_fields(
    adapter: ArgumentAdapter,
    kind: &str,
) -> Option<(&'static [&'static str], bool)> {
    use ArgumentAdapter as A;
    Some(match (adapter, kind) {
        (A::Rust, "parameter" | "self_parameter") => (&["pattern"], true),
        (A::Rust, "let_declaration" | "for_expression" | "let_condition" | "match_arm") => {
            (&["pattern"], false)
        }
        (A::Go, "parameter_declaration" | "variadic_parameter_declaration") => (&["name"], true),
        (A::Go, "var_spec" | "const_spec") => (&["name"], false),
        (A::Go, "short_var_declaration" | "range_clause" | "receive_statement") => {
            (&["left"], false)
        }
        (A::Python, "assignment" | "augmented_assignment" | "for_statement" | "for_in_clause") => {
            (&["left"], false)
        }
        (A::Python, "named_expression") => (&["name"], false),
        (A::Python, "function_definition" | "lambda") => (&["parameters"], true),
        (adapter, "parameter_declaration" | "optional_parameter_declaration")
            if adapter.is_c_family() =>
        {
            (&["declarator"], true)
        }
        (adapter, "declaration" | "init_declarator" | "field_declaration")
            if adapter.is_c_family() =>
        {
            (&["declarator"], false)
        }
        (A::Java, "formal_parameter" | "spread_parameter" | "catch_formal_parameter") => {
            (&["name"], true)
        }
        (A::Java, "variable_declarator" | "enhanced_for_statement") => (&["name"], false),
        #[cfg(feature = "tree-sitter-c-sharp")]
        (A::CSharp, "parameter") => (&["name"], true),
        #[cfg(feature = "tree-sitter-c-sharp")]
        (A::CSharp, "variable_declarator" | "catch_declaration") => (&["name"], false),
        (A::Scala, "parameter" | "class_parameter") => (&["name"], true),
        (A::Scala, "val_definition" | "var_definition") => (&["pattern"], false),
        (A::JavaScript, "variable_declarator") => (&["name"], false),
        _ => return None,
    })
}

pub(super) fn unknown_scope(adapter: ArgumentAdapter, kind: &str) -> bool {
    use ArgumentAdapter as A;
    match adapter {
        // OXC supplies precise binding admission; its recovery fallback cannot.
        A::JavaScript => true,
        A::Python => matches!(
            kind,
            "global_statement"
                | "delete_statement"
                | "nonlocal_statement"
                | "list_comprehension"
                | "dictionary_comprehension"
                | "set_comprehension"
                | "generator_expression"
                | "with_statement"
                | "except_clause"
        ),
        A::Rust => matches!(
            kind,
            "closure_expression" | "macro_invocation" | "match_expression" | "let_chain"
        ),
        #[cfg(any(feature = "tree-sitter-cpp", feature = "tree-sitter-cuda"))]
        A::CFamily => unknown_scope(A::C, kind),
        A::C => matches!(
            kind,
            "lambda_expression" | "preproc_def" | "preproc_function_def" | "for_range_loop"
        ),
        A::Java => matches!(kind, "lambda_expression" | "instanceof_expression"),
        #[cfg(feature = "tree-sitter-c-sharp")]
        A::CSharp => matches!(
            kind,
            "lambda_expression"
                | "anonymous_method_expression"
                | "declaration_expression"
                | "declaration_pattern"
                | "recursive_pattern"
                | "for_each_statement"
        ),
        A::Scala => matches!(kind, "lambda_expression" | "case_clause" | "for_expression"),
        A::Go => kind == "type_switch_statement",
    }
}
