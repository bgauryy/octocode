//! Which paths a graph build's walk reaches beyond the caller's `exclude`:
//! the graphBuild fields `defaultExcludes`, `includeIgnored` and
//! `includeHidden`, as one value every layer that walks the scope shares
//! (extraction, scope proof, build identity, manifest, retry leads).
use octocode_contract_types::GraphBuildQuery;
use octocode_engine::types::IgnorePolicy;
use serde_json::{Value, json};
use std::path::Path;

/// The one admission of a graph build's walk: what the path policy withholds
/// (credential stores, `.env*` files, links, tool artifacts) and the default
/// prune. Extraction and the staleness walk both ask it, so a source the
/// build withholds is none the stale check lists.
pub(crate) fn build_admission<'a>(
    paths: &'a crate::policy::path::PathPolicy,
    root: &Path,
) -> crate::policy::discovery::WalkAdmission<'a> {
    crate::policy::discovery::WalkAdmission::new(
        paths.discovery_walk(),
        root,
        crate::policy::discovery::WalkRules {
            defaults: true,
            include: &[],
            generated: false,
            engine_prunes: true,
        },
    )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct WalkScope {
    /// The one default prune (`node_modules`, build output) applies.
    pub default_excludes: bool,
    /// Ignore files and dot entries the walk admits.
    pub ignore: IgnorePolicy,
}

impl Default for WalkScope {
    fn default() -> Self {
        Self {
            default_excludes: true,
            ignore: IgnorePolicy::default(),
        }
    }
}

impl WalkScope {
    pub(crate) fn of_query(query: &GraphBuildQuery) -> Self {
        use crate::policy::prune::DefaultsFlag;
        Self {
            default_excludes: query.default_excludes.defaults(),
            ignore: IgnorePolicy::new(query.include_ignored, query.include_hidden),
        }
    }

    /// The manifest's `scan.walk` and the build identity's part: only the
    /// flags that differ from the default walk, `None` for the default.
    pub(crate) fn disclosure(self) -> Option<Value> {
        (self != Self::default()).then(|| {
            json!({
                "defaultExcludes": self.default_excludes,
                "includeIgnored": self.ignore.include_ignored,
                "includeHidden": self.ignore.include_hidden,
            })
        })
    }

    /// The walk a manifest's `scan.walk` records; a missing record is the
    /// default walk.
    pub(crate) fn of_manifest(scan: &Value) -> Self {
        let flag = |name: &str, default: bool| scan["walk"][name].as_bool().unwrap_or(default);
        Self {
            default_excludes: flag("defaultExcludes", true),
            ignore: IgnorePolicy {
                include_ignored: flag("includeIgnored", false),
                include_hidden: flag("includeHidden", false),
            },
        }
    }

    /// Sets the walk fields on a graphBuild retry `row`: a lead writes them
    /// through [`super::build_scope::BuildScope::write_row`].
    pub(super) fn write_row(self, row: &mut Value) {
        if let Some(Value::Object(fields)) = self.disclosure() {
            for (name, value) in fields {
                row[name] = value;
            }
        }
    }
}
