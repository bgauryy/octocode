use super::super::invalid;
use serde_json::Value;
use std::error::Error;

/// Contract `continuationChannels`: which continuation kinds are pages
/// (`next`) and which are leads (`hints`).
pub(super) fn render_continuation_channels(contract: &Value) -> Result<String, Box<dyn Error>> {
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
pub(super) fn render_error_codes(contract: &Value) -> Result<String, Box<dyn Error>> {
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
