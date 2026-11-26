//! The research references an MCP agent can read on demand. Each is
//! embedded from its skill source: one file, or for the tool-flow guide its
//! parts joined in order. The guide links the others by relative path, so
//! every file it reaches is served under a URI that resolves the same
//! relative link (`octocode://research/<file>`).
//! The instructions name none of them: a client finds them through
//! `resources/list`, so a session that never reads these pays nothing for them.
use rmcp::model::{Resource, ResourceContents, ResourceTemplate};
use std::sync::LazyLock;

const MARKDOWN: &str = "text/markdown";

struct Reference {
    uri: &'static str,
    name: &'static str,
    title: &'static str,
    text: fn() -> &'static str,
}

/// A reference's description: core's text, by the reference's name
/// (`agentText.resources` in the embedded presentation).
fn description(name: &str) -> String {
    octocode_runtime_base::contracts::cli_presentation()
        .ok()
        .and_then(|presentation| presentation["agentText"]["resources"][name].as_str())
        .unwrap_or_default()
        .to_owned()
}

macro_rules! reference_file {
    ($file:literal) => {
        include_str!(concat!(
            "../../../../../../../skills/octocode-research/references/",
            $file
        ))
    };
}

/// The tool-flow resource; the CLI serves [`tool_flow`] as `schema --view flow`.
pub(in crate::cli) const TOOL_FLOW_URI: &str = "octocode://research/tool-flow";

/// The tool-flow guide's skill source files, in served order: the order the
/// first part's opening line links the others.
const TOOL_FLOW_PARTS: [&str; 4] = [
    reference_file!("tool-flow.md"),
    reference_file!("tool-flow-shape.md"),
    reference_file!("tool-flow-carry.md"),
    reference_file!("tool-flow-recover.md"),
];

/// One guide from its parts. A part opens with one line for a skill reader
/// (when to load the file, links to the sibling files); the guide serves the
/// parts as one text, so it leaves that line out and keeps each part's
/// sections, a blank line between parts.
fn joined(parts: &[&str]) -> String {
    parts
        .iter()
        .map(|part| part.split_once("\n\n").map_or("", |(_, sections)| sections))
        .collect::<Vec<_>>()
        .join("\n")
}

/// The tool-flow guide: one served text from its part files.
pub(in crate::cli) fn tool_flow() -> &'static str {
    static GUIDE: LazyLock<String> = LazyLock::new(|| joined(&TOOL_FLOW_PARTS));
    &GUIDE
}

const REFERENCES: [Reference; 4] = [
    Reference {
        uri: TOOL_FLOW_URI,
        name: "tool-flow",
        title: "Octocode research tool flow",
        text: tool_flow,
    },
    Reference {
        uri: "octocode://research/evidence.md",
        name: "evidence",
        title: "Octocode evidence rules",
        text: || reference_file!("evidence.md"),
    },
    Reference {
        uri: "octocode://research/protocol-catalog.md",
        name: "protocol-catalog",
        title: "Octocode protocol catalog",
        text: || reference_file!("protocol-catalog.md"),
    },
    Reference {
        uri: "octocode://research/configuration.md",
        name: "configuration",
        title: "Octocode configuration",
        text: || reference_file!("configuration.md"),
    },
];

#[cfg(test)]
mod tests;

/// One tool's output view (`schema <tool> --view output`) as a resource.
const OUTPUT_VIEW: &str = "octocode://contract/output/";
const INPUT_VIEW: &str = "octocode://contract/input/";

/// Tools with shorter discovery notes serve their full operation views on demand.
fn has_operation_view(tool: &str) -> bool {
    matches!(tool, "astSearch" | "graphQuery")
}

pub(super) fn templates<'a>(tools: impl Iterator<Item = &'a str>) -> Vec<ResourceTemplate> {
    tools
        .filter(|tool| has_operation_view(tool))
        .map(|tool| {
            ResourceTemplate::new(
                format!("{INPUT_VIEW}{tool}/{{operation}}"),
                format!("{tool} operation schema"),
            )
            .with_mime_type("application/json")
        })
        .collect()
}

/// Every served reference and the output view of each listed tool, for
/// `resources/list`.
pub(super) fn list<'a>(tools: impl Iterator<Item = &'a str>) -> Vec<Resource> {
    let references = REFERENCES.iter().map(|reference| {
        Resource::new(reference.uri, reference.name)
            .with_title(reference.title)
            .with_description(description(reference.name))
            .with_mime_type(MARKDOWN)
    });
    let output_views = tools.map(|tool| {
        Resource::new(format!("{OUTPUT_VIEW}{tool}"), format!("output-{tool}"))
            .with_title(format!("{tool} output fields"))
            .with_description(format!(
                "What {tool}'s output fields and coverage reasons mean."
            ))
            .with_mime_type("application/json")
    });
    references.chain(output_views).collect()
}

/// The contents of one served reference, or `None` for an unknown URI.
pub(super) fn read(uri: &str, enabled: impl Fn(&str) -> bool) -> Option<ResourceContents> {
    if let Some(rest) = uri.strip_prefix(INPUT_VIEW) {
        let (tool, operation) = rest.split_once('/')?;
        if !has_operation_view(tool) || !enabled(tool) || operation.contains(',') {
            return None;
        }
        let catalog = octocode_runtime_base::contracts::cli_presentation().ok()?;
        let entry = super::super::schema::tool_entry(catalog, tool)?;
        let selected = entry["selections"].get(format!("operation={operation}"))?;
        return Some(
            ResourceContents::text(selected.to_string(), uri).with_mime_type("application/json"),
        );
    }
    if let Some(tool) = uri.strip_prefix(OUTPUT_VIEW) {
        if !enabled(tool) {
            return None;
        }
        let view = super::super::schema::output_of(tool)?;
        return Some(
            ResourceContents::text(view.to_string(), uri).with_mime_type("application/json"),
        );
    }
    let reference = REFERENCES.iter().find(|reference| reference.uri == uri)?;
    Some(ResourceContents::text((reference.text)(), reference.uri).with_mime_type(MARKDOWN))
}
