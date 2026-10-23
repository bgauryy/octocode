//! Tool identity codegen: `ToolId`, policy, continuation channels and error codes
//! rendered from the tool contract into `tool_ids.rs`, `query_limits` into
//! `query_limits.rs`, and listing field roles into `listing_roles.rs`.

mod tool_channels;
mod tool_listing_roles;
mod tool_query_limits;
use super::{bool_value, invalid, rust_enum_variant, string, to_screaming_snake, to_snake};
use serde_json::Value;
use std::error::Error;
use tool_channels::{render_continuation_channels, render_error_codes};
pub(super) use tool_listing_roles::render_listing_roles;
pub(super) use tool_query_limits::render_query_limits;
use tool_query_limits::{query_variants, resolve_schema_ref};

/// A contract string array as Rust string literals.
fn string_list(value: &Value, path: &str) -> Result<Vec<String>, Box<dyn Error>> {
    value
        .as_array()
        .ok_or_else(|| invalid(format!("{path} is not an array")))?
        .iter()
        .map(|item| {
            item.as_str()
                .map(|item| format!("{item:?}"))
                .ok_or_else(|| invalid(format!("{path} holds a non-string")))
        })
        .collect()
}

/// `ToolId` identity and policy, generated from the contract's `tools[]`
/// (`name`, `family`, `cliOnly`, `beta`, `disabled` are authored once in core
/// `TOOL_POLICIES`). Nothing in native re-spells a tool list.
pub(super) fn render_tool_ids(contract: &Value) -> Result<String, Box<dyn Error>> {
    let tools = contract
        .get("tools")
        .and_then(Value::as_array)
        .ok_or_else(|| invalid("tool contract has no tools[]"))?;
    let mut variants = Vec::new();
    let (mut names, mut families, mut short, mut cli_only, mut beta, mut disabled) = (
        String::new(),
        String::new(),
        String::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
    );
    let mut extras = String::new();
    let mut sort_keys = String::new();
    let mut scan_totals = String::new();
    let mut github = Vec::new();
    for (index, tool) in tools.iter().enumerate() {
        let path = format!("tools[{index}]");
        let name = string(
            tool.get("name")
                .ok_or_else(|| invalid(format!("{path} has no name")))?,
            &format!("{path}.name"),
        )?;
        let variant = rust_enum_variant(name)?;
        let family = match string(
            tool.get("family")
                .ok_or_else(|| invalid(format!("{path} has no family")))?,
            &format!("{path}.family"),
        )? {
            "local" => "Local",
            "github" => "GitHub",
            "remote" => "Remote",
            other => {
                return Err(invalid(format!(
                    "{path}.family {other:?} is not local|github|remote"
                )));
            }
        };
        if family == "GitHub" {
            github.push(variant.clone());
        }
        names.push_str(&format!("            ToolId::{variant} => {name:?},\n"));
        families.push_str(&format!(
            "            ToolId::{variant} => ToolFamily::{family},\n"
        ));
        let description = tool
            .get("shortDescription")
            .and_then(Value::as_str)
            .unwrap_or_default();
        short.push_str(&format!(
            "            ToolId::{variant} => {description:?},\n"
        ));
        if bool_value(tool.get("cliOnly"), false, &format!("{path}.cliOnly"))? {
            cli_only.push(format!("ToolId::{variant}"));
        }
        if bool_value(tool.get("beta"), false, &format!("{path}.beta"))? {
            beta.push(format!("ToolId::{variant}"));
        }
        // Absent until a contract declares it: no tool is disabled.
        if bool_value(tool.get("disabled"), false, &format!("{path}.disabled"))? {
            disabled.push(format!("ToolId::{variant}"));
        }
        // `scanTotals`: absent for a tool with no scan totals to trim.
        let trim = match tool.get("scanTotals") {
            None => "None",
            Some(value) => match string(value, &format!("{path}.scanTotals"))? {
                "always" => "Some(ScanTotals::Always)",
                "withMatchCount" => "Some(ScanTotals::WithMatchCount)",
                other => {
                    return Err(invalid(format!(
                        "{path}.scanTotals {other:?} is not always|withMatchCount"
                    )));
                }
            },
        };
        scan_totals.push_str(&format!("            ToolId::{variant} => {trim},\n"));
        // `extraPaths`: opt-in output paths by `extras` kind.
        let mut kinds = Vec::new();
        for (kind, paths) in tool
            .get("extraPaths")
            .and_then(Value::as_object)
            .ok_or_else(|| invalid(format!("{path} has no extraPaths{{}}")))?
        {
            let paths = string_list(paths, &format!("{path}.extraPaths.{kind}"))?;
            kinds.push(format!("({kind:?}, &[{}])", paths.join(", ")));
        }
        extras.push_str(&format!(
            "            ToolId::{variant} => &[{}],\n",
            kinds.join(", ")
        ));
        // `sortKeyPaths`: absent for a tool whose rows sort by no extra field.
        let mut sorts = Vec::new();
        if let Some(entries) = tool.get("sortKeyPaths") {
            let entries = entries
                .as_object()
                .ok_or_else(|| invalid(format!("{path}.sortKeyPaths is not an object")))?;
            for (sort, paths) in entries {
                let paths = string_list(paths, &format!("{path}.sortKeyPaths.{sort}"))?;
                sorts.push(format!("({sort:?}, &[{}])", paths.join(", ")));
            }
        }
        sort_keys.push_str(&format!(
            "            ToolId::{variant} => &[{}],\n",
            sorts.join(", ")
        ));
        variants.push(variant);
    }
    let set = |members: &[String]| {
        if members.is_empty() {
            "false".to_owned()
        } else {
            format!("matches!(self, {})", members.join(" | "))
        }
    };
    let mut policy = String::new();
    let max_items = contract
        .pointer("/limits/maxInputArrayItems")
        .and_then(Value::as_u64)
        .ok_or_else(|| invalid("tool contract has no limits.maxInputArrayItems"))?;
    let read_context = contract
        .pointer("/limits/defaultReadContextLines")
        .and_then(Value::as_u64)
        .ok_or_else(|| invalid("tool contract has no limits.defaultReadContextLines"))?;
    let clasify_context = contract
        .pointer("/limits/clasifyContextLines")
        .and_then(Value::as_u64)
        .ok_or_else(|| invalid("tool contract has no limits.clasifyContextLines"))?;
    policy.push_str(&format!(
        "/// Contract `limits.maxInputArrayItems`: native input-security array ceiling.\n\
         pub const MAX_INPUT_ARRAY_ITEMS: usize = {max_items};\n\n\
         /// Contract `limits.defaultReadContextLines`: lines around each\n\
         /// `matchString` hit when `contextLines` is omitted.\n\
         pub const DEFAULT_READ_CONTEXT_LINES: usize = {read_context};\n\n\
         /// Contract `limits.clasifyContextLines`: lines each side of a\n\
         /// localSearch hit clasifyContext judges when no context field is set.\n\
         pub const CLASIFY_CONTEXT_LINES: u32 = {clasify_context};\n\n"
    ));
    for tool in tools {
        let Some(entries) = tool.get("semanticPolicy").and_then(Value::as_object) else {
            continue;
        };
        let name = tool.get("name").and_then(Value::as_str).unwrap_or_default();
        let module = to_snake(name);
        policy.push_str(&format!(
            "/// Contract `{name}.semanticPolicy`: tool sets and limits enforced outside its schema.\n\
             pub mod {module}_policy {{\n"
        ));
        if entries.values().any(Value::is_array) {
            policy.push_str("    use super::ToolId;\n");
        }
        for (key, value) in entries {
            let constant = to_screaming_snake(key);
            match value {
                Value::Array(items) => {
                    let members = items
                        .iter()
                        .map(|item| {
                            let member = item.as_str().filter(|member| {
                                tools.iter().any(|tool| {
                                    tool.get("name").and_then(Value::as_str) == Some(member)
                                })
                            });
                            member
                                .map(|member| {
                                    rust_enum_variant(member).map(|v| format!("ToolId::{v}"))
                                })
                                .unwrap_or_else(|| {
                                    Err(invalid(format!(
                                        "{name}.semanticPolicy.{key} names an unknown tool {item}"
                                    )))
                                })
                        })
                        .collect::<Result<Vec<_>, _>>()?;
                    policy.push_str(&format!(
                        "    pub const {constant}: &[ToolId] = &[{}];\n",
                        members.join(", ")
                    ));
                }
                Value::Number(number) if number.is_u64() => {
                    policy.push_str(&format!("    pub const {constant}: usize = {number};\n"));
                }
                other => {
                    return Err(invalid(format!(
                        "{name}.semanticPolicy.{key} must be a tool list or unsigned integer, got {other}"
                    )));
                }
            }
        }
        policy.push_str("}\n\n");
    }
    policy.push_str(&render_continuation_channels(contract)?);
    policy.push_str(&render_error_codes(contract)?);
    Ok(format!(
        "// @generated by crates/runtime/build.rs from packages/octocode-config/contract/tool-contract.json.\n\
         {policy}\
         /// Every tool the native runtime can execute, in contract order.\n\
         #[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]\n\
         pub enum ToolId {{\n{variants_decl}}}\n\n\
         impl ToolId {{\n\
         \x20   /// All tool identities in contract order.\n\
         \x20   pub const ALL: [ToolId; {count}] = [{all}];\n\n\
         \x20   /// This tool's position in [`ToolId::ALL`]: the variants are declared\n\
         \x20   /// in that order, so a per-tool table indexes by it.\n\
         \x20   #[must_use]\n\
         \x20   pub const fn index(self) -> usize {{\n        self as usize\n    }}\n\n\
         \x20   /// The wire name exactly as it appears in the contract and in tool-call envelopes.\n\
         \x20   #[must_use]\n\
         \x20   pub const fn as_str(self) -> &'static str {{\n        match self {{\n{names}        }}\n    }}\n\n\
         \x20   /// One-line summary (contract `shortDescription`), available without\n\
         \x20   /// parsing the embedded contract.\n\
         \x20   #[must_use]\n\
         \x20   pub const fn short_description(self) -> &'static str {{\n        match self {{\n{short}        }}\n    }}\n\n\
         \x20   /// Cursor-scope / policy family (contract `family`).\n\
         \x20   #[must_use]\n\
         \x20   pub const fn family(self) -> ToolFamily {{\n        match self {{\n{families}        }}\n    }}\n\n\
         \x20   /// Mutating tools the CLI runs on the user's machine; MCP never lists or\n\
         \x20   /// executes them (contract `cliOnly`).\n\
         \x20   #[must_use]\n\
         \x20   pub const fn is_cli_only(self) -> bool {{\n        {cli_only}\n    }}\n\n\
         \x20   /// Tools hidden and refused unless the shared beta gate is enabled (contract `beta`).\n\
         \x20   #[must_use]\n\
         \x20   pub const fn is_beta(self) -> bool {{\n        {beta}\n    }}\n\n\
         \x20   /// Tools parked in this build: unavailable on every surface, whatever\n\
         \x20   /// the configuration (contract `disabled`).\n\
         \x20   #[must_use]\n\
         \x20   pub const fn is_disabled(self) -> bool {{\n        {disabled}\n    }}\n\n\
         \x20   /// Output fields core classes `extra`, by `extras` kind (contract\n\
         \x20   /// `extraPaths`, envelope-rooted: `results[].data.x`); dropped unless\n\
         \x20   /// the row's `extras` names the kind.\n\
         \x20   #[must_use]\n\
         \x20   pub const fn extra_paths(self) -> &'static [(&'static str, &'static [&'static str])] {{\n        match self {{\n{extras}        }}\n    }}\n\n\
         \x20   /// Extra paths a row sorted by a `sort` value keeps, by value (contract\n\
         \x20   /// `sortKeyPaths`): the field rows are ordered by is default output.\n\
         \x20   #[must_use]\n\
         \x20   pub const fn sort_key_paths(self) -> &'static [(&'static str, &'static [&'static str])] {{\n        match self {{\n{sort_keys}        }}\n    }}\n\n\
         \x20   /// Which rows of this tool carry scan totals that default output trims\n\
         \x20   /// (contract `scanTotals`); `None`: the tool has none.\n\
         \x20   #[must_use]\n\
         \x20   pub const fn scan_totals(self) -> Option<ScanTotals> {{\n        match self {{\n{scan_totals}        }}\n    }}\n\n\
         \x20   /// The GitHub-family identity, so GitHub routing matches only its own tools.\n\
         \x20   #[must_use]\n\
         \x20   pub const fn github(self) -> Option<GitHubTool> {{\n        match self {{\n{github_arms}            _ => None,\n        }}\n    }}\n\
         }}\n\n\
         /// Which rows carry scan totals (contract `scanTotals`): every row, or\n\
         /// only a row that has `matchCount`.\n\
         #[derive(Clone, Copy, PartialEq, Eq, Debug)]\n\
         pub enum ScanTotals {{\n    Always,\n    WithMatchCount,\n}}\n\n\
         /// A tool of the contract's `github` family.\n\
         #[derive(Clone, Copy, PartialEq, Eq, Debug)]\n\
         pub enum GitHubTool {{\n{github_decl}}}\n\n\
         #[cfg(test)]\n\
         mod tool_index_tests {{\n\
         \x20   #[test]\n\
         \x20   fn each_tool_indexes_its_position_in_all() {{\n\
         \x20       for (position, tool) in super::ToolId::ALL.into_iter().enumerate() {{\n\
         \x20           assert_eq!(tool.index(), position, \"{{}}\", tool.as_str());\n\
         \x20       }}\n\
         \x20   }}\n\
         }}\n",
        variants_decl = variants
            .iter()
            .map(|v| format!("    {v},\n"))
            .collect::<String>(),
        count = variants.len(),
        all = variants
            .iter()
            .map(|v| format!("ToolId::{v}"))
            .collect::<Vec<_>>()
            .join(", "),
        cli_only = set(&cli_only),
        beta = set(&beta),
        disabled = set(&disabled),
        github_arms = github
            .iter()
            .map(|v| format!("            ToolId::{v} => Some(GitHubTool::{v}),\n"))
            .collect::<String>(),
        github_decl = github
            .iter()
            .map(|v| format!("    {v},\n"))
            .collect::<String>(),
    ))
}
