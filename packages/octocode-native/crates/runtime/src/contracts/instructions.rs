use serde_json::Value;

/// Select generated canonical instructions. No prose or workflow policy lives here.
pub fn mcp_instructions(
    contract: &Value,
    enabled: impl Fn(&str) -> bool,
) -> Result<String, &'static str> {
    let tools = contract["tools"].as_array().ok_or("Missing tools")?;
    let mut mask = 0_usize;
    for (index, tool) in tools.iter().enumerate() {
        let name = tool["name"].as_str().ok_or("Missing tool name")?;
        if enabled(name) {
            mask |= 1_usize
                .checked_shl(index as u32)
                .ok_or("Tool mask overflow")?;
        }
    }
    let table = &contract["mcpInstructionTable"];
    let lines = table["lines"]
        .as_array()
        .ok_or("Missing instruction lines")?;
    let selected = table["sets"]
        .get(mask)
        .and_then(Value::as_array)
        .ok_or("Missing enabled-set instructions")?;
    let text = selected
        .iter()
        .map(|id| {
            let index = id
                .as_u64()
                .and_then(|v| usize::try_from(v).ok())
                .ok_or("Invalid instruction index")?;
            lines
                .get(index)
                .and_then(Value::as_str)
                .ok_or("Invalid instruction line")
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(text.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn selects_empty_single_and_full_sets_without_advertising_disabled_tools() {
        let contract = crate::contracts::parsed_contract().expect("embedded contract");
        assert_eq!(
            mcp_instructions(contract, |_| true).expect("full set"),
            contract["mcpInstructions"]
        );
        assert!(
            mcp_instructions(contract, |_| false)
                .expect("empty set")
                .contains("No Octocode tools are enabled")
        );
        let read = mcp_instructions(contract, |name| name == "localFetch").expect("read set");
        assert!(read.contains("Readers return exact original source"));
        assert!(!read.contains("ghSearch"));
    }
    #[test]
    fn corrupt_generated_index_fails_closed() {
        let invalid =
            serde_json::json!({"tools": [], "mcpInstructionTable":{"lines":[],"sets":[[1]]}});
        assert!(mcp_instructions(&invalid, |_| false).is_err());
    }
}
