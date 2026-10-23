use serde_json::Value;

/// The runtime grammar inventory line, stating the common feature set once.
pub(super) fn grammar_instruction(capabilities: &Value) -> Option<String> {
    let structural: Vec<&Value> = capabilities
        .as_array()?
        .iter()
        .filter(|capability| {
            capability["structuralSearch"] == true
                && capability["extensions"]
                    .as_array()
                    .is_some_and(|extensions| !extensions.is_empty())
        })
        .collect();
    if structural.is_empty() {
        return None;
    }
    let features_of = |capability: &Value| {
        let mut features = vec!["structural"];
        if capability["signatureOutline"] == true {
            features.push("outline");
        }
        if capability["graphFacts"] == true {
            features.push("graph");
        }
        features.join("+")
    };
    // Most common feature set; the first one seen wins a tie.
    let mut counts: Vec<(String, usize)> = Vec::new();
    for capability in &structural {
        let features = features_of(capability);
        match counts.iter_mut().find(|(seen, _)| *seen == features) {
            Some((_, count)) => *count += 1,
            None => counts.push((features, 1)),
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
    let inventory = structural
        .iter()
        .map(|capability| {
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
            let features = features_of(capability);
            let tag = if features == common {
                String::new()
            } else {
                format!("; {features}")
            };
            let ids = if ids.is_empty() {
                String::new()
            } else {
                format!("{ids}: ")
            };
            format!(
                "{} ({ids}{extensions}{tag})",
                capability["language"].as_str().unwrap_or_default()
            )
        })
        .collect::<Vec<_>>()
        .join("; ");
    Some(format!(
        "Runtime grammar inventory ({common} unless noted): {inventory}. Parser availability does not imply an installed language server or complete cross-file linking."
    ))
}
