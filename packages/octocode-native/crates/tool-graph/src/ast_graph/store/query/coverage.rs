//! Source-language capability and population recorded in the selected snapshot.
use super::*;
use std::collections::BTreeSet;

fn with_scope(mut coverage: Value, scope: &str) -> Value {
    let Some(warning) = coverage["warning"].as_str() else {
        return coverage;
    };
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

    /// The call coverage of a set of files: their language's when they share
    /// one; otherwise no single language's linkage applies, so it names no
    /// language and its warning states each one's extraction.
    pub(super) fn files_coverage(&self, files: &[u32]) -> Value {
        let languages = files
            .iter()
            .map(|file| self.t.str(self.t.nodes[*file as usize].detail))
            .collect::<BTreeSet<_>>();
        match languages.first().filter(|_| languages.len() == 1) {
            Some(one) => self.language_coverage(one),
            None => self.languages_coverage(
                &languages,
                "Coverage describes the languages of the selected files.",
            ),
        }
    }

    /// What the snapshot's call edges rest on: syntax facts, plus a SCIP
    /// index when the build had one.
    fn call_basis(&self) -> &'static str {
        match self.manifest["scip"] == "indexed" {
            true => "syntax+scip",
            false => "syntax",
        }
    }

    /// One language's call coverage; a node of no source language (a
    /// package) names none.
    fn language_coverage(&self, language: &str) -> Value {
        let extraction = match self.manifest["callExtractionByLanguage"][language].as_bool() {
            Some(true) => "supported",
            Some(false) => "unsupported",
            None => "notAssessed",
        };
        let mut coverage = json!({"basis": self.call_basis(), "callExtraction":extraction});
        if !language.is_empty() {
            coverage["language"] = json!(language);
        }
        if extraction != "supported" {
            coverage["warning"] = json!(format!(
                "Call extraction is {extraction} for this language in the selected snapshot. Empty call results do not establish absence; another language's linkage is not applicable."
            ));
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
        self.languages_coverage(&languages, scope)
    }

    /// The call coverage of several languages (or none): no one language's
    /// applies, so it names none, and its warning, unless every one is
    /// supported, names each one's recorded extraction after `scope`.
    fn languages_coverage(&self, languages: &BTreeSet<&str>, scope: &str) -> Value {
        let mut supported = Vec::new();
        let mut unsupported = Vec::new();
        let mut unassessed = Vec::new();
        for language in languages {
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
        let mut coverage = json!({"basis": self.call_basis(), "callExtraction":extraction});
        if extraction != "supported" {
            coverage["warning"] = json!(format!(
                "{scope} Recorded call extraction: supported=[{}]; unsupported=[{}]; notAssessed=[{}]. Linkage describes only extracted syntax call sites, not semantic recall. Empty results do not establish absence where extraction is unsupported or not assessed.",
                supported.join(", "),
                unsupported.join(", "),
                unassessed.join(", "),
            ));
        }
        coverage
    }
}

impl Graph {
    /// The argument-slot coverage of the selected snapshot, limited to the
    /// languages of `sources` (a snapshot-wide answer names `sources` empty
    /// and keeps every language): other languages concern none of the rows.
    pub(super) fn argument_coverage(&self, sources: &[u32]) -> Value {
        let mut coverage = self.manifest["argumentCoverage"].clone();
        if sources.is_empty() {
            return coverage;
        }
        let languages = sources
            .iter()
            .map(|source| {
                let file = self.t.nodes[*source as usize].file;
                if file == NONE {
                    ""
                } else {
                    self.t.str(self.t.nodes[file as usize].detail)
                }
            })
            .collect::<BTreeSet<_>>();
        if let Some(by_language) = coverage["byLanguage"].as_object_mut() {
            by_language.retain(|language, _| languages.contains(language.as_str()));
        }
        coverage
    }
}
