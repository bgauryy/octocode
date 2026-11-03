//! The research references an MCP agent can read on demand. Each is
//! embedded from its one skill source file; the tool-flow graphs link the
//! others by relative path, so every file they reach is served under a URI
//! that resolves the same relative link (`octocode://research/<file>`).
//! The instructions name only the tool-flow URI, so a session that never
//! reads these pays nothing for them.
use rmcp::model::{Resource, ResourceContents};

const MARKDOWN: &str = "text/markdown";

struct Reference {
    uri: &'static str,
    name: &'static str,
    title: &'static str,
    description: &'static str,
    text: &'static str,
}

macro_rules! reference_file {
    ($file:literal) => {
        include_str!(concat!(
            "../../../../../../../skills/octocode-research/references/",
            $file
        ))
    };
}

const REFERENCES: [Reference; 5] = [
    Reference {
        uri: "octocode://research/tool-flow",
        name: "tool-flow",
        title: "Octocode research tool flow",
        description: "Read when the next call is unclear: which tool, how little to ask for, when to judge instead of read, how to carry evidence across, recovery and measured cost.",
        text: reference_file!("tool-flow.md"),
    },
    Reference {
        uri: "octocode://research/octocode-usage.md",
        name: "octocode-usage",
        title: "Octocode shared usage",
        description: "Envelopes, paging and absence rules shared by every tool.",
        text: reference_file!("octocode-usage.md"),
    },
    Reference {
        uri: "octocode://research/evidence.md",
        name: "evidence",
        title: "Octocode evidence rules",
        description: "What counts as proof for a research claim.",
        text: reference_file!("evidence.md"),
    },
    Reference {
        uri: "octocode://research/protocol-catalog.md",
        name: "protocol-catalog",
        title: "Octocode protocol catalog",
        description: "Communication protocols rated by fit.",
        text: reference_file!("protocol-catalog.md"),
    },
    Reference {
        uri: "octocode://research/configuration.md",
        name: "configuration",
        title: "Octocode configuration",
        description: "Home, environment and workspace configuration for research tools.",
        text: reference_file!("configuration.md"),
    },
];

/// Every served reference, for `resources/list`.
pub(super) fn list() -> Vec<Resource> {
    REFERENCES
        .iter()
        .map(|reference| {
            Resource::new(reference.uri, reference.name)
                .with_title(reference.title)
                .with_description(reference.description)
                .with_mime_type(MARKDOWN)
        })
        .collect()
}

/// The contents of one served reference, or `None` for an unknown URI.
pub(super) fn read(uri: &str) -> Option<ResourceContents> {
    let reference = REFERENCES.iter().find(|reference| reference.uri == uri)?;
    Some(ResourceContents::text(reference.text, reference.uri).with_mime_type(MARKDOWN))
}
