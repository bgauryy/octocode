use super::ContractValidationError;
use serde_json::Value;

/// A data mismatch is false; an invalid contract is never a negative match.
pub(in crate::contracts::validate) fn probe(
    result: Result<(), ContractValidationError>,
) -> Result<bool, ContractValidationError> {
    match result {
        Ok(()) => Ok(true),
        Err(error) if error.is_contract_error() => Err(error),
        Err(_) => Ok(false),
    }
}

/// Exported predicate documents carry their own definitions; ordinary subschemas share the root.
pub(super) fn predicate_root<'a>(root: &'a Value, predicate: &'a Value) -> &'a Value {
    if predicate.get("$schema").is_some() {
        predicate
    } else {
        root
    }
}
