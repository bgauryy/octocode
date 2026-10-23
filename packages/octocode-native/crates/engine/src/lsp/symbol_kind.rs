//! The LSP `SymbolKind` codes (spec 3.17) and their names: the one table
//! every producer (the oxc outline lane) and consumer (lspSearch decoding a
//! server's reply) of those numbers shares.

pub const FILE: u8 = 1;
pub const MODULE: u8 = 2;
pub const NAMESPACE: u8 = 3;
pub const PACKAGE: u8 = 4;
pub const CLASS: u8 = 5;
pub const METHOD: u8 = 6;
pub const PROPERTY: u8 = 7;
pub const FIELD: u8 = 8;
pub const CONSTRUCTOR: u8 = 9;
pub const ENUM: u8 = 10;
pub const INTERFACE: u8 = 11;
pub const FUNCTION: u8 = 12;
pub const VARIABLE: u8 = 13;
pub const CONSTANT: u8 = 14;
pub const STRING: u8 = 15;
pub const NUMBER: u8 = 16;
pub const BOOLEAN: u8 = 17;
pub const ARRAY: u8 = 18;
pub const OBJECT: u8 = 19;
pub const KEY: u8 = 20;
pub const NULL: u8 = 21;
pub const ENUM_MEMBER: u8 = 22;
pub const STRUCT: u8 = 23;
pub const EVENT: u8 = 24;
pub const OPERATOR: u8 = 25;
pub const TYPE_PARAMETER: u8 = 26;

/// The name a kind code carries in rows; `None` for a code outside the
/// spec's `1..=26`.
pub fn name(code: u64) -> Option<&'static str> {
    Some(match u8::try_from(code).ok()? {
        FILE => "file",
        MODULE => "module",
        NAMESPACE => "namespace",
        PACKAGE => "package",
        CLASS => "class",
        METHOD => "method",
        PROPERTY => "property",
        FIELD => "field",
        CONSTRUCTOR => "constructor",
        ENUM => "enum",
        INTERFACE => "interface",
        FUNCTION => "function",
        VARIABLE => "variable",
        CONSTANT => "constant",
        STRING => "string",
        NUMBER => "number",
        BOOLEAN => "boolean",
        ARRAY => "array",
        OBJECT => "object",
        KEY => "key",
        NULL => "null",
        ENUM_MEMBER => "enumMember",
        STRUCT => "struct",
        EVENT => "event",
        OPERATOR => "operator",
        TYPE_PARAMETER => "typeParameter",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_spec_code_has_one_name_and_others_have_none() {
        let names: Vec<_> = (1..=26)
            .map(|code| name(code).expect("spec kind"))
            .collect();
        let mut distinct = names.clone();
        distinct.sort_unstable();
        distinct.dedup();
        assert_eq!(distinct.len(), 26);
        assert_eq!(
            (names[0], names[11], names[21]),
            ("file", "function", "enumMember")
        );
        assert_eq!(name(0), None);
        assert_eq!(name(27), None);
        assert_eq!(name(u64::MAX), None);
    }
}
