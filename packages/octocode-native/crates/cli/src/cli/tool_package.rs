//! Distribution boundary for the two launchers sharing the native engine.
use octocode_runtime_base::id::ToolId;
use serde_json::Value;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum ToolPackage {
    #[default]
    Research,
    Graph,
}

impl ToolPackage {
    pub(super) const fn of(tool: ToolId) -> Self {
        match tool {
            ToolId::GraphBuild | ToolId::GraphQuery => Self::Graph,
            _ => Self::Research,
        }
    }

    pub(super) fn owns(self, name: &str) -> bool {
        ToolId::from_name(name).is_some_and(|id| Self::of(id) == self)
    }

    pub(super) const fn cli(self) -> &'static str {
        match self {
            Self::Research => "octocode",
            Self::Graph => "octocode-graph",
        }
    }

    /// The command that runs this package without installing it.
    pub(super) const fn launch(self) -> &'static str {
        match self {
            Self::Research => "npx -y octocode",
            Self::Graph => "npx -y @octocodeai/octocode-graph",
        }
    }

    /// Where `name` runs when it is a runtime tool this package does not
    /// serve: the package that does and its launch command.
    pub(super) fn elsewhere_note(self, name: &str) -> Option<String> {
        let owner = Self::of(ToolId::from_name(name)?);
        (owner != self).then(|| {
            format!(
                "{name} is served by the {} package: {} schema {name}, or {} {name} '<json>'",
                owner.cli(),
                owner.launch(),
                owner.launch()
            )
        })
    }

    pub(super) fn for_name(name: &str) -> Self {
        ToolId::from_name(name).map(Self::of).unwrap_or_default()
    }

    pub(super) fn select(self, mut catalog: Value) -> Value {
        if let Some(tools) = catalog["tools"].as_array_mut() {
            tools.retain(|row| row["name"].as_str().is_some_and(|name| self.owns(name)));
        }
        catalog
    }
}
