//! The `partialReasons` values each tool can emit. Core authors one
//! dictionary of every reason and what it means; a tool's output reference
//! serves only the reasons that tool emits, so no tool reads another's.

use crate::id::ToolId;
use serde_json::{Map, Value};

use ToolId::{
    ArtifactSearch, AstSearch, GhGetFileContent, GhGetHistoryItem, GhSearchCode, GhSearchHistory,
    GhSearchRepo, GhStructure, GraphBuild, GraphQuery, LocalFetch, LocalSearch, LspSearch,
    SecurityCheck, StructureSearch,
};

/// Each reason with the tools that emit it. A registry test reads the
/// tools' source and fails on a reason a listed tool never emits, and on a
/// tool that emits a reason it is not listed for.
const EMITTERS: &[(&str, &[ToolId])] = &[
    ("aliasRecoveryFailed", &[LspSearch]),
    ("anchorLineError", &[LspSearch]),
    ("binaryQuit", &[LocalSearch]),
    ("buildScriptsOff", &[LspSearch]),
    ("callHierarchyExpansionFailed", &[LspSearch]),
    ("callSitesUnresolved", &[GraphQuery]),
    ("cancelled", &[LocalSearch, LspSearch]),
    ("cfgGatedFiles", &[LspSearch]),
    ("cfgNegatedFeatures", &[LspSearch]),
    ("closingReferenceLimit", &[GhGetHistoryItem]),
    ("contextClipped", &[GhGetHistoryItem]),
    ("deadline", &[AstSearch]),
    ("declarationClipped", &[LspSearch]),
    ("definitionHopFailed", &[LspSearch]),
    ("diagnosticsNotPublished", &[LspSearch]),
    ("documentAnalysisErrors", &[LspSearch]),
    ("dynamicDispatch", &[LspSearch]),
    ("filesSkipped", &[LocalSearch, AstSearch, GraphQuery]),
    ("filesUnevaluated", &[AstSearch, LspSearch, StructureSearch]),
    (
        "fileTooLarge",
        &[LocalSearch, LocalFetch, AstSearch, LspSearch],
    ),
    ("heritageUnread", &[LspSearch]),
    ("hierarchyCandidateLimit", &[LspSearch]),
    ("hierarchyFanOutLimit", &[LspSearch]),
    ("hierarchyNodeLimit", &[LspSearch]),
    ("hierarchyParentUnanchored", &[LspSearch]),
    ("hitLinesNotListed", &[GhSearchCode]),
    ("importerRequestFailed", &[LspSearch]),
    ("importerScanCapped", &[LspSearch]),
    ("importerScanFailed", &[LspSearch]),
    ("importUnresolved", &[LspSearch]),
    ("includeOutsidePackage", &[LspSearch]),
    ("inferredProject", &[LspSearch]),
    ("languageServerIndexing", &[LspSearch]),
    ("locationsWithheld", &[LspSearch]),
    ("materializeDepth", &[GhStructure]),
    ("materializeIncomplete", &[GhStructure]),
    ("noCompileDatabase", &[LspSearch]),
    ("noProject", &[LspSearch]),
    ("parseRecovered", &[AstSearch]),
    ("partialTreeFailures", &[GhStructure]),
    ("patchUnavailable", &[GhGetHistoryItem]),
    ("providerBatchLimit", &[GhGetHistoryItem]),
    ("providerCommitListLimit", &[GhGetHistoryItem]),
    ("providerContentsLimit", &[GhStructure]),
    ("providerFileLimit", &[GhGetHistoryItem]),
    (
        "providerIncompleteResults",
        &[
            GhSearchCode,
            GhSearchRepo,
            GhSearchHistory,
            GhGetHistoryItem,
            ArtifactSearch,
            SecurityCheck,
        ],
    ),
    (
        "providerResultCap",
        &[GhSearchCode, GhSearchRepo, GhSearchHistory, ArtifactSearch],
    ),
    ("providerResultsShifted", &[GhSearchCode]),
    ("regexMatchLimit", &[LocalSearch]),
    (
        "scanBudget",
        &[
            LocalSearch,
            LocalFetch,
            GhGetFileContent,
            AstSearch,
            GraphBuild,
        ],
    ),
    ("scanOnlyImplementations", &[LspSearch]),
    (
        "securitySelectedViewSizeLimit",
        &[LocalSearch, LocalFetch, GhGetFileContent, LspSearch],
    ),
    ("serverUnanswered", &[LspSearch]),
    ("serverUnhealthy", &[LspSearch]),
    ("sharedModuleFile", &[LspSearch]),
    ("sourceFailed", &[SecurityCheck]),
    ("typeHierarchyExpansionFailed", &[LspSearch]),
    ("walkDeadline", &[StructureSearch, GhStructure]),
    ("walkWindow", &[StructureSearch]),
    ("workspaceSymbolCoverageUnknown", &[LspSearch]),
    ("workspaceSymbolOneProject", &[LspSearch]),
];

/// `dictionary` less every reason `tool` never emits. A reason the table
/// does not know is kept, so a new core reason is never hidden by a stale
/// table; the registry test names it.
pub(crate) fn emitted_by(tool: ToolId, dictionary: &Map<String, Value>) -> Map<String, Value> {
    dictionary
        .iter()
        .filter(|(reason, _)| {
            EMITTERS
                .iter()
                .find(|(name, _)| name == reason)
                .is_none_or(|(_, tools)| tools.contains(&tool))
        })
        .map(|(reason, meaning)| (reason.clone(), meaning.clone()))
        .collect()
}

#[cfg(test)]
mod tests;
