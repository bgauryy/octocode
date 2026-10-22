//! Tool identity codegen: `ToolId`, policy, continuation channels and error codes
//! rendered from the tool contract into `tool_ids.rs`, `query_limits` into
//! `query_limits.rs`, and listing field roles into `listing_roles.rs`.

use super::{
    bool_value, invalid, rust_enum_variant, rust_ident, string, to_screaming_snake, to_snake,
};
use serde_json::{Map, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;

/// Contract `continuationChannels`: which continuation kinds are pages
/// (`next`) and which are leads (`hints`).
fn render_continuation_channels(contract: &Value) -> Result<String, Box<dyn Error>> {
    let channels = contract
        .get("continuationChannels")
        .ok_or_else(|| invalid("tool contract has no continuationChannels"))?;
    let text = |key: &str| {
        channels
            .get(key)
            .and_then(Value::as_str)
            .ok_or_else(|| invalid(format!("continuationChannels.{key} must be a string")))
    };
    let list = |pointer: &str| -> Result<String, Box<dyn Error>> {
        let items = channels
            .pointer(pointer)
            .and_then(Value::as_array)
            .ok_or_else(|| invalid(format!("continuationChannels{pointer} must be a list")))?
            .iter()
            .map(|item| {
                item.as_str()
                    .map(|name| format!("{name:?}"))
                    .ok_or_else(|| {
                        invalid(format!("continuationChannels{pointer} holds a non-string"))
                    })
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(format!("&[{}]", items.join(", ")))
    };
    Ok(format!(
        "/// Contract `continuationChannels`: pages stay in `next`, leads and\n\
         /// prose tips go to `hints`.\n\
         pub mod continuation_channels {{\n\
         \x20   pub const PAGES_KEY: &str = {pages:?};\n\
         \x20   pub const HINTS_KEY: &str = {hints:?};\n\
         \x20   pub const HINT_TEXT_KEY: &str = {text:?};\n\
         \x20   pub const PAGE_PREFIXES: &[&str] = {prefixes};\n\
         \x20   pub const RESUME_PREFIXES: &[&str] = {resume};\n\
         \x20   pub const RESTART_PREFIXES: &[&str] = {restart};\n\
         \x20   pub const ENRICHMENT_NAMES: &[&str] = {enrichment};\n\
         \x20   pub const PAGE_NAMES: &[&str] = {names};\n\
         \x20   pub const RECOVERY_LEADS: &[&str] = {recovery};\n\
         \x20   pub const DEFAULT_LEADS: &[&str] = {default_leads};\n\
         \x20   pub const PAGE_KINDS: &[&str] = {page_kinds};\n\
         \x20   pub const LEAD_KINDS: &[&str] = {lead_kinds};\n\
         \x20   pub const LEAD_PRIORITY: &[&str] = {lead_priority};\n\
         }}\n\n",
        pages = text("pagesKey")?,
        hints = text("hintsKey")?,
        text = text("hintTextKey")?,
        prefixes = list("/pagePrefixes")?,
        resume = list("/resumePrefixes")?,
        restart = list("/restartPrefixes")?,
        enrichment = list("/enrichmentNames")?,
        names = list("/pageNames")?,
        recovery = list("/recoveryLeads")?,
        default_leads = list("/defaultLeads")?,
        page_kinds = list("/kinds/pages")?,
        lead_kinds = list("/kinds/leads")?,
        lead_priority = list("/leadPriority")?,
    ))
}

/// Contract `errorCodes`: every declared `errorCode` with its exit class and
/// wire spelling, keyed by the generated `ErrorCode` enum (one variant per
/// declared code: an exhaustive match, so a code the enum and the table
/// disagree on fails to compile) so no runtime matches on code spelling.
fn render_error_codes(contract: &Value) -> Result<String, Box<dyn Error>> {
    let codes = contract
        .get("errorCodes")
        .and_then(Value::as_object)
        .ok_or_else(|| invalid("tool contract has no errorCodes"))?;
    let mut all = String::new();
    let mut classes = String::new();
    let mut spellings = String::new();
    for (code, class) in codes {
        let mut chars = code.chars();
        let variant = match chars.next() {
            Some(first)
                if first.is_ascii_lowercase()
                    && code.chars().all(|c| c.is_ascii_alphanumeric()) =>
            {
                format!("{}{}", first.to_ascii_uppercase(), chars.as_str())
            }
            _ => {
                return Err(invalid(format!(
                    "errorCodes.{code} is not a camelCase name"
                )));
            }
        };
        let class = class
            .as_str()
            .ok_or_else(|| invalid(format!("errorCodes.{code} must name a class")))?;
        let class = match class {
            "invalidInput" => "InvalidInput",
            "notFound" => "NotFound",
            "authentication" => "Authentication",
            "permission" => "Permission",
            "rateLimited" => "RateLimited",
            "execution" => "Execution",
            other => return Err(invalid(format!("errorCodes.{code}: unknown class {other}"))),
        };
        all.push_str(&format!("        ErrorCode::{variant},\n"));
        classes.push_str(&format!(
            "            ErrorCode::{variant} => ErrorClass::{class},\n"
        ));
        spellings.push_str(&format!("            ErrorCode::{variant} => {code:?},\n"));
    }
    Ok(format!(
        "/// Contract `errorCodes`: every declared `errorCode` and how it ends a call.\n\
         pub mod error_codes {{\n\
         \x20   use octocode_contract_types::ErrorCode;\n\n\
         \x20   /// The exit class of an `errorCode`.\n\
         \x20   #[derive(Clone, Copy, Debug, PartialEq, Eq)]\n\
         \x20   pub enum ErrorClass {{\n\
         \x20       InvalidInput,\n\
         \x20       NotFound,\n\
         \x20       Authentication,\n\
         \x20       Permission,\n\
         \x20       RateLimited,\n\
         \x20       Execution,\n\
         \x20   }}\n\n\
         \x20   /// Every declared code, in contract order.\n\
         \x20   pub const ALL: &[ErrorCode] = &[\n{all}    ];\n\n\
         \x20   /// The exit class of `code`.\n\
         \x20   #[must_use]\n\
         \x20   pub const fn class(code: ErrorCode) -> ErrorClass {{\n\
         \x20       match code {{\n{classes}        }}\n\
         \x20   }}\n\n\
         \x20   /// The wire spelling of `code`.\n\
         \x20   #[must_use]\n\
         \x20   pub const fn as_str(code: ErrorCode) -> &'static str {{\n\
         \x20       match code {{\n{spellings}        }}\n\
         \x20   }}\n\n\
         \x20   /// The declared code `spelling` names; `None` for an undeclared one.\n\
         \x20   #[must_use]\n\
         \x20   pub fn parse(spelling: &str) -> Option<ErrorCode> {{\n\
         \x20       spelling.parse().ok()\n\
         \x20   }}\n\
         }}\n\n"
    ))
}

/// `keyword` of one field schema as an unsigned integer: declared on the
/// field itself, else the one value its `anyOf`/`oneOf` members declare (a
/// `string | string[]` field's `maxItems` lives on the array member).
fn field_fact(schema: &Value, defs: &Value, keyword: &str) -> Option<u64> {
    if let Some(value) = schema.get(keyword) {
        return value.as_u64();
    }
    let mut found = None;
    for member in ["anyOf", "oneOf"]
        .iter()
        .filter_map(|key| schema.get(*key).and_then(Value::as_array))
        .flatten()
    {
        let Some(value) = resolve_schema_ref(member, defs).get(keyword) else {
            continue;
        };
        match (found, value.as_u64()) {
            (_, None) => return None,
            (Some(previous), Some(value)) if previous != value => return None,
            (_, value) => found = value,
        }
    }
    found
}

/// One `oneOf`/`anyOf` branch of a query schema: the `operation` values it
/// admits (`None` without a discriminant) and its top-level properties.
struct QueryVariant<'a> {
    operations: Option<Vec<&'a str>>,
    fields: &'a Map<String, Value>,
}

/// The `oneOf`/`anyOf` branches of a tool's query schema (the schema itself
/// when it has none) that declare properties.
fn query_variants(schema: &Value) -> Vec<QueryVariant<'_>> {
    let defs = &schema["$defs"];
    let branches = ["oneOf", "anyOf"]
        .iter()
        .find_map(|key| schema.get(*key).and_then(Value::as_array))
        .map_or_else(|| vec![schema], |branches| branches.iter().collect());
    let mut variants = Vec::new();
    for branch in branches {
        let branch = resolve_schema_ref(branch, defs);
        let Some(fields) = branch.get("properties").and_then(Value::as_object) else {
            continue;
        };
        let operations = fields.get("operation").map(|operation| {
            let operation = resolve_schema_ref(operation, defs);
            operation["const"].as_str().map_or_else(
                || {
                    operation["enum"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(Value::as_str)
                        .collect()
                },
                |single| vec![single],
            )
        });
        variants.push(QueryVariant { operations, fields });
    }
    variants
}

/// Integer `default`/`maximum`/`maxItems`/`maxLength` facts of one tool's
/// top-level query fields, keyed `FIELD_KEYWORD`, over the query-schema
/// variants `admits` selects. A fact is emitted only when every selected
/// variant that has the field declares the same value (the agreement rule of
/// `contracts::query_schema_value`).
fn query_facts(
    variants: &[QueryVariant<'_>],
    defs: &Value,
    admits: impl Fn(&Option<Vec<&str>>) -> bool,
) -> BTreeMap<String, u64> {
    const KEYWORDS: [&str; 4] = ["default", "maximum", "maxItems", "maxLength"];
    let mut facts: BTreeMap<String, Option<u64>> = BTreeMap::new();
    for variant in variants
        .iter()
        .filter(|variant| admits(&variant.operations))
    {
        for (field, schema) in variant.fields {
            let schema = resolve_schema_ref(schema, defs);
            for keyword in KEYWORDS {
                let value = field_fact(schema, defs, keyword);
                let name = format!(
                    "{}_{}",
                    to_screaming_snake(field),
                    to_screaming_snake(keyword)
                );
                facts
                    .entry(name)
                    .and_modify(|previous| {
                        if *previous != value {
                            *previous = None;
                        }
                    })
                    .or_insert(value);
            }
        }
    }
    facts
        .into_iter()
        .filter_map(|(name, value)| Some((name, value?)))
        .collect()
}

fn resolve_schema_ref<'a>(schema: &'a Value, defs: &'a Value) -> &'a Value {
    schema
        .get("$ref")
        .and_then(Value::as_str)
        .and_then(|reference| reference.strip_prefix("#/$defs/"))
        .and_then(|name| defs.get(name))
        .unwrap_or(schema)
}

/// `contracts::query_limits::<tool>`: every agreed integer query-schema fact
/// as a `usize` constant, plus `<tool>::<operation>` for the variants one
/// operation admits, so tools never re-spell a contract bound.
pub(super) fn render_query_limits(contract: &Value) -> Result<String, Box<dyn Error>> {
    let tools = contract
        .get("tools")
        .and_then(Value::as_array)
        .ok_or_else(|| invalid("tool contract has no tools[]"))?;
    let render = |facts: &BTreeMap<String, u64>, indent: &str| {
        facts
            .iter()
            .map(|(name, value)| format!("{indent}pub const {name}: usize = {value};\n"))
            .collect::<String>()
    };
    let mut output = String::from(
        "// @generated by crates/runtime/build.rs from packages/octocode-config/contract/tool-contract.json.\n",
    );
    for tool in tools {
        let name = tool.get("name").and_then(Value::as_str).unwrap_or_default();
        let schema = tool
            .get("querySchema")
            .ok_or_else(|| invalid(format!("{name} has no querySchema")))?;
        let defs = &schema["$defs"];
        let variants = query_variants(schema);
        output.push_str(&format!(
            "pub mod {} {{\n{}",
            rust_ident(name),
            render(&query_facts(&variants, defs, |_| true), "    ")
        ));
        let operations = variants
            .iter()
            .filter_map(|variant| variant.operations.as_ref())
            .flatten()
            .copied()
            .collect::<BTreeSet<_>>();
        for operation in operations {
            let facts = query_facts(&variants, defs, |admitted| {
                admitted
                    .as_ref()
                    .is_none_or(|admitted| admitted.contains(&operation))
            });
            output.push_str(&format!(
                "    pub mod {} {{\n{}    }}\n",
                rust_ident(operation),
                render(&facts, "        ")
            ));
        }
        output.push_str("}\n");
    }
    Ok(output)
}

/// `contracts::listing_roles`: each listing tool's query fields that carry
/// an `x-listing` role (core `listing()`), by name over its query variants,
/// so no listing re-spells its filter lists. A field must carry
/// one role in every variant that declares it, and an unknown role fails the
/// build until native handles it.
pub(super) fn render_listing_roles(contract: &Value) -> Result<String, Box<dyn Error>> {
    let tools = contract
        .get("tools")
        .and_then(Value::as_array)
        .ok_or_else(|| invalid("tool contract has no tools[]"))?;
    let mut output = String::from(
        "// @generated by crates/runtime/build.rs from packages/octocode-config/contract/tool-contract.json.\n\
         /// What a listing query field does to the walk (contract `x-listing`).\n\
         #[derive(Clone, Copy, PartialEq, Eq, Debug)]\n\
         pub enum ListingRole {\n\
         \x20   /// Keeps entries by name or path.\n\
         \x20   Name,\n\
         \x20   /// Keeps entries by type, emptiness, mode, size or time.\n\
         \x20   Metadata,\n\
         \x20   /// Skips subtrees or levels.\n\
         \x20   Prune,\n\
         \x20   /// Sets how deep the walk goes and what it enters.\n\
         \x20   Scope,\n\
         }\n",
    );
    for tool in tools {
        let name = tool.get("name").and_then(Value::as_str).unwrap_or_default();
        let schema = tool
            .get("querySchema")
            .ok_or_else(|| invalid(format!("{name} has no querySchema")))?;
        let defs = &schema["$defs"];
        // Every field with its role (`None`: none).
        let mut declared: BTreeMap<&str, Option<&str>> = BTreeMap::new();
        for variant in query_variants(schema) {
            for (field, field_schema) in variant.fields {
                let role = resolve_schema_ref(field_schema, defs)
                    .get("x-listing")
                    .or_else(|| field_schema.get("x-listing"))
                    .map(|role| string(role, &format!("{name}.{field}.x-listing")))
                    .transpose()?;
                match declared.get(field.as_str()) {
                    Some(known) if *known != role => {
                        return Err(invalid(format!(
                            "{name}.{field}: x-listing {role:?} in one variant, {known:?} in another"
                        )));
                    }
                    Some(_) => {}
                    None => {
                        declared.insert(field, role);
                    }
                }
            }
        }
        let roles = declared
            .into_iter()
            .filter_map(|(field, role)| Some((field, role?)))
            .collect::<Vec<_>>();
        if roles.is_empty() {
            continue;
        }
        let mut fields = String::new();
        for (field, role) in roles {
            let variant = match role {
                "name" => "Name",
                "metadata" => "Metadata",
                "prune" => "Prune",
                "scope" => "Scope",
                other => {
                    return Err(invalid(format!(
                        "{name}.{field}: x-listing {other:?} is not name|metadata|prune|scope"
                    )));
                }
            };
            fields.push_str(&format!("        ({field:?}, ListingRole::{variant}),\n"));
        }
        output.push_str(&format!(
            "\n/// `{name}`'s listing fields with their roles, by name.\n\
             pub mod {module} {{\n\
             \x20   use super::ListingRole;\n\
             \x20   pub const FIELDS: &[(&str, ListingRole)] = &[\n{fields}    ];\n\
             }}\n",
            module = rust_ident(name),
        ));
    }
    Ok(output)
}

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
    policy.push_str(&format!(
        "/// Contract `limits.maxInputArrayItems`: native input-security array ceiling.\n\
         pub const MAX_INPUT_ARRAY_ITEMS: usize = {max_items};\n\n\
         /// Contract `limits.defaultReadContextLines`: lines around each\n\
         /// `matchString` hit when `contextLines` is omitted.\n\
         pub const DEFAULT_READ_CONTEXT_LINES: usize = {read_context};\n\n"
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
