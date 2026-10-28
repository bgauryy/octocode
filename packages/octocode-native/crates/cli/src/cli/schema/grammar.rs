use serde_json::Value;

/// The runtime grammar inventory line, stating the common feature set once.
pub(super) fn grammar_instruction(capabilities: &Value) -> Option<String> {
    let features_of = |capability: &Value| {
        [
            (capability["structuralSearch"] == true, "structural"),
            (capability["syntaxTree"] == true, "syntaxTree"),
            (capability["signatureOutline"] == true, "outline"),
            (capability["facts"]["declarations"] == true, "declarations"),
            (capability["graphFacts"] == true, "graph"),
        ]
        .into_iter()
        .filter_map(|(supported, name)| supported.then_some(name))
        .collect::<Vec<_>>()
        .join("+")
    };
    let supported: Vec<(&Value, String)> = capabilities
        .as_array()?
        .iter()
        .filter_map(|capability| {
            let features = features_of(capability);
            (!features.is_empty()).then_some((capability, features))
        })
        .collect();
    if supported.is_empty() {
        return None;
    }
    // Most common feature set; the first one seen wins a tie.
    let mut counts: Vec<(String, usize)> = Vec::new();
    for (_, features) in &supported {
        match counts.iter_mut().find(|(seen, _)| seen == features) {
            Some((_, count)) => *count += 1,
            None => counts.push((features.clone(), 1)),
        }
    }
    let mut common = &counts[0];
    for entry in &counts {
        if entry.1 > common.1 {
            common = entry;
        }
    }
    let common = common.0.clone();
    let strings = |value: &Value| -> Vec<String> {
        value
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect()
    };
    let inventory = supported
        .iter()
        .map(|(capability, features)| {
            let mut ids: Vec<String> = capability["languageId"]
                .as_str()
                .filter(|id| !id.is_empty())
                .map(str::to_owned)
                .into_iter()
                .collect();
            ids.extend(strings(&capability["selectorAliases"]));
            let ids = ids.join(",");
            let extensions = strings(&capability["extensions"])
                .iter()
                .map(|extension| format!(".{extension}"))
                .collect::<Vec<_>>()
                .join(" ");
            let tag = if *features == common {
                String::new()
            } else {
                format!("; {features}")
            };
            let selector = match (ids.is_empty(), extensions.is_empty()) {
                (false, false) => format!("{ids}: {extensions}"),
                (false, true) => ids,
                (true, _) => extensions,
            };
            format!(
                "{} ({selector}{tag})",
                capability["language"].as_str().unwrap_or_default()
            )
        })
        .collect::<Vec<_>>()
        .join("; ");
    Some(format!(
        "Runtime grammar inventory ({common} unless noted): {inventory}. Parser availability does not imply an installed language server or complete cross-file linking."
    ))
}
