//! Core-declared fields retained on output fragments.
use crate::id::ToolId;
use std::collections::BTreeSet;

pub(crate) struct PageContextFields {
    pub(crate) required: BTreeSet<String>,
    pub(crate) optional: BTreeSet<String>,
}

#[expect(
    clippy::expect_used,
    reason = "missing embedded page context is a contract defect caught for every tool by contract tests"
)]
pub(crate) fn page_context_fields(tool: ToolId) -> &'static PageContextFields {
    static FIELDS: [std::sync::OnceLock<PageContextFields>; ToolId::ALL.len()] =
        [const { std::sync::OnceLock::new() }; ToolId::ALL.len()];
    FIELDS[tool.index()].get_or_init(|| {
        let context = &super::tool_contract(tool)
            .expect("embedded contract contains every tool")["pageContextFields"];
        let names = |kind: &str| {
            context[kind]
                .as_array()
                .expect("core declares each tool's page context")
                .iter()
                .map(|field| field.as_str().expect("page context field name").to_owned())
                .collect()
        };
        PageContextFields { required: names("required"), optional: names("optional") }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_tool_declares_disjoint_page_context_fields() {
        for tool in ToolId::ALL {
            let context = page_context_fields(tool);
            assert!(context.required.is_disjoint(&context.optional), "{tool:?}");
        }
    }
}
