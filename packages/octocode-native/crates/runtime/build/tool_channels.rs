use super::super::invalid;
use serde_json::Value;
use std::error::Error;

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
