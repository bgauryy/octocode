//! Source-language capability and population recorded in the selected snapshot.
use super::*;
use std::collections::BTreeSet;

fn empty_linkage() -> Value {
    json!({"linked":0,"eligible":0,"excluded":0,"ratio":null,"population":"syntax-call-sites"})
}

fn with_scope(mut coverage: Value, scope: &str) -> Value {
    let warning = coverage["warning"].as_str().unwrap_or("");
    coverage["warning"] = json!(format!("{scope} {warning}").trim());
    coverage
}

impl Graph {
    pub(super) fn coverage(&self, node: u32) -> Value {
        let file = self.t.nodes[node as usize].file;
        let language = if file == NONE {
            ""
        } else {
            self.t.str(self.t.nodes[file as usize].detail)
        };
        self.language_coverage(language)
    }

    fn language_coverage(&self, language: &str) -> Value {
        let linkage = self.manifest["linkageByLanguage"]
            .get(language)
            .cloned()
            .unwrap_or_else(empty_linkage);
        let extraction = match self.manifest["callExtractionByLanguage"][language].as_bool() {
            Some(true) => "supported",
            Some(false) => "unsupported",
            None => "notAssessed",
        };
        let mut coverage = json!({"language": language, "linkage": linkage, "basis": "syntax", "callExtraction":extraction});
        if extraction != "supported" {
            coverage["warning"] = json!(format!(
                "Call extraction is {extraction} for this language in the selected snapshot. Empty call results do not establish absence; another language's linkage is not applicable."
            ));
        } else if linkage["ratio"].as_f64().is_none_or(|ratio| ratio < 0.9) {
            coverage["warning"] = json!(
                "Syntax linkage measures only extracted call sites selected by the stated population. It is not semantic recall. Inspect unresolved sites and confirm identity with lspSearch."
            );
        }
        coverage
    }

    pub(super) fn evidence_coverage(&self, source: Option<u32>) -> Value {
        if let Some(source) = source {
            return with_scope(
                self.coverage(source),
                "Linkage describes the selected snapshot's whole-language population, not only the selected source or matching evidence rows.",
            );
        }
        let languages = self
            .t
            .nodes
            .iter()
            .filter(|node| node.kind == NodeKind::File)
            .map(|node| self.t.str(node.detail))
            .collect::<BTreeSet<_>>();
        let scope = "Coverage describes all source languages in the selected snapshot; destination and evidence filters do not narrow this population.";
        if let Some(language) = languages.first().filter(|_| languages.len() == 1) {
            return with_scope(self.language_coverage(language), scope);
        }
        let mut supported = Vec::new();
        let mut unsupported = Vec::new();
        let mut unassessed = Vec::new();
        for language in &languages {
            let label = if language.is_empty() {
                "<unassigned>"
            } else {
                language
            };
            match self.manifest["callExtractionByLanguage"][*language].as_bool() {
                Some(true) => supported.push(label),
                Some(false) => unsupported.push(label),
                None => unassessed.push(label),
            }
        }
        let extraction = if !languages.is_empty() && supported.len() == languages.len() {
            "supported"
        } else if !languages.is_empty() && unsupported.len() == languages.len() {
            "unsupported"
        } else {
            "notAssessed"
        };
        let linkage = if languages.is_empty() {
            empty_linkage()
        } else {
            self.manifest
                .get("linkage")
                .cloned()
                .unwrap_or_else(empty_linkage)
        };
        json!({
            "language":"", "linkage":linkage, "basis":"syntax", "callExtraction":extraction,
            "warning":format!(
                "{scope} Recorded call extraction: supported=[{}]; unsupported=[{}]; notAssessed=[{}]. Linkage describes only extracted syntax call sites, not semantic recall. Empty results do not establish absence where extraction is unsupported or not assessed.",
                supported.join(", "), unsupported.join(", "), unassessed.join(", "),
            ),
        })
    }
}
