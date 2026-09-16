pub fn from_lsp_code(kind: Option<u32>) -> &'static str {
    match kind {
        Some(1) | Some(2) | Some(4) => "module",
        Some(3) => "namespace",
        Some(5) | Some(19) | Some(23) => "class",
        Some(6) | Some(9) => "method",
        Some(7) | Some(8) | Some(20) => "property",
        Some(10) => "enum",
        Some(11) => "interface",
        Some(12) => "function",
        Some(13) => "variable",
        Some(14) | Some(22) => "constant",
        Some(26) => "type",
        _ => "unknown",
    }
}

pub fn to_lsp_code(kind: &str) -> u32 {
    match kind {
        "function" => 12,
        "method" => 6,
        "class" => 5,
        "interface" => 11,
        "type" => 26,
        "variable" => 13,
        "constant" => 14,
        "property" => 7,
        "enum" => 10,
        "module" => 2,
        "namespace" => 3,
        _ => 13,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_lsp_codes_to_octocode_tags() {
        assert_eq!(from_lsp_code(Some(12)), "function");
        assert_eq!(from_lsp_code(Some(6)), "method");
        assert_eq!(from_lsp_code(Some(5)), "class");
        assert_eq!(from_lsp_code(Some(99)), "unknown");
        assert_eq!(from_lsp_code(None), "unknown");
    }

    #[test]
    fn maps_octocode_tags_to_lsp_codes() {
        assert_eq!(to_lsp_code("function"), 12);
        assert_eq!(to_lsp_code("method"), 6);
        assert_eq!(to_lsp_code("class"), 5);
        assert_eq!(to_lsp_code("unknown-kind"), 13);
    }
}
