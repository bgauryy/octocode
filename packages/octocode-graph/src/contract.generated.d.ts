// Generated from @octocodeai/config/tool-types.schema.json. The native engine validates the wire format.

export type GraphBuildInput = (GraphBuildInputQueries | GraphBuildInputCursor)
export type DefaultExcludes = boolean
export type GraphBuildOutput = (GraphBuildOutputResults | GraphBuildOutputError)
export type GraphBuildData = (GraphBuildDataGraph | GraphBuildDataError | GraphBuildDataProgress)
export type ErrorCode = ("invalidInput" | "invalidPattern" | "invalidGlob" | "notAFile" | "notADirectory" | "pathValidationFailed" | "languageRequired" | "languageMismatch" | "languageDirectoryRequired" | "invalidClassificationContext" | "invalidClassificationRequest" | "invalidClassificationView" | "invalidGraphQuery" | "invalidPagination" | "unsupportedResponsePagination" | "notFound" | "scopeTooBroad" | "responseTooLarge" | "classificationContextTooLarge" | "classificationStateTooLarge" | "classificationOverBudget" | "commitMismatch" | "redirectDenied" | "workspaceRootInvalid" | "authentication" | "permissionDenied" | "outsideAllowedRoots" | "pathPolicyDenied" | "symlinkEscape" | "policy" | "persistentStorageDisabled" | "missingConfiguration" | "toolUnavailable" | "config" | "invalidClassificationConfiguration" | "credentialStoreUnavailable" | "contentSecurityLimit" | "securityValidationFailed" | "checkoutDirty" | "classificationQuotaExhausted" | "binaryFileUnsupported" | "fileTooLarge" | "largeSourceWindowOnly" | "symbolsUnsupported" | "languageUnsupported" | "capabilityUnavailable" | "serverUnavailable" | "unavailable" | "fileAccessFailed" | "fileReadFailed" | "scanFailed" | "gitFailed" | "gitUnavailable" | "cacheUnavailable" | "candidateIneligible" | "classificationContextEmpty" | "classificationContextUnavailable" | "executionFailed" | "outputContractViolation" | "contract" | "response" | "runtime" | "unroutedTool" | "timeout" | "transport" | "server" | "serverCrashed" | "providerError" | "httpStatus" | "rateLimited" | "classificationRateLimited" | "classificationProviderError" | "classificationProviderUnavailable" | "classificationContextFailed" | "invalidClassificationResponse" | "decode" | "busy" | "duplicateRequest" | "cancelled" | "documentSyncFailed" | "requestFailed" | "staleSnapshot" | "responseSnapshotExpired" | "snapshotIncompatible" | "sourceMismatch" | "closed")
export type ErrorClass = ("correctInput" | "refused" | "unservable" | "transient" | "restart")
/**
 * @minItems 1
 */
export type RowHints = string[]
export type RelatedCall = (GraphBuildRelatedCall | GraphQueryRelatedCall | ClasifyRelatedCall | GhSearchRepoRelatedCall | GhSearchCodeRelatedCall | GhStructureRelatedCall | GhGetFileContentRelatedCall | GhSearchHistoryRelatedCall | GhGetHistoryItemRelatedCall | ArtifactSearchRelatedCall | GhCloneRepoRelatedCall | LocalSearchRelatedCall | StructureSearchRelatedCall | AstSearchRelatedCall | LocalFetchRelatedCall | LspSearchRelatedCall | SecurityCheckRelatedCall)
export type DefaultExcludesContinuationInput = boolean
export type GraphScopepathContinuationInput = string
export type GraphScopegraphContinuationInput = string
export type GraphScopeincludeProvenanceContinuationInput = boolean
export type GraphQueryStatsViewContinuationInput = ("summary" | "details")
export type GraphPagelimitContinuationInput = number
export type GraphReferencetarget = string
export type GraphReferencekindContinuationInput = ("file" | "symbol" | "package")
/**
 * @minItems 1
 * @maxItems 6
 */
export type GraphEdgeedgesContinuationInput = ("contains" | "imports" | "uses" | "calls" | "inherits" | "passes")[]
export type GraphEdgeconfidenceContinuationInput = ("high" | "medium" | "low")
export type ClassificationContext = string
export type ClasifyViewContinuationInput = ("summary" | "details")
export type GitHubArchive = ("exclude" | "include" | "only")
export type SortOrder = ("asc" | "desc")
/**
 * @minItems 1
 * @maxItems 100
 */
export type TextMatchTerms = string[]
export type RegexEngine = ("literal" | "rust" | "pcre2")
export type CaseMode = ("smart" | "sensitive" | "insensitive")
export type ArtifactSearchViewContinuationInput = ("summary" | "details")
export type TestCodeContinuationInput = ("include" | "exclude" | "only")
export type StructureSearchView = ("paths" | "details")
export type AstParser = ("typescript" | "tsx" | "javascript" | "python" | "go" | "rust" | "java" | "c" | "asm" | "scala" | "cpp" | "cuda" | "csharp")
export type ARSyntax = string
export type TestCode = ("include" | "exclude" | "only")
export type SecurityCheckViewContinuationInput = ("summary" | "details")
export type PartialReason = ("filesSkipped" | "filesUnevaluated" | "parseRecovered" | "fileTooLarge" | "deadline" | "scanBudget" | "walkWindow" | "walkDeadline" | "cancelled" | "binaryQuit" | "securitySelectedViewSizeLimit" | "regexMatchLimit" | "languageServerIndexing" | "diagnosticsNotPublished" | "serverUnanswered" | "serverUnhealthy" | "hierarchyCandidateLimit" | "hierarchyNodeLimit" | "hierarchyFanOutLimit" | "hierarchyParentUnanchored" | "callHierarchyExpansionFailed" | "typeHierarchyExpansionFailed" | "heritageUnread" | "scanOnlyImplementations" | "workspaceSymbolCoverageUnknown" | "workspaceSymbolOneProject" | "locationsWithheld" | "definitionHopFailed" | "documentAnalysisErrors" | "includeOutsidePackage" | "buildScriptsOff" | "cfgGatedFiles" | "cfgNegatedFeatures" | "sharedModuleFile" | "dynamicDispatch" | "importUnresolved" | "inferredProject" | "noCompileDatabase" | "noProject" | "importerScanCapped" | "importerScanFailed" | "importerRequestFailed" | "aliasRecoveryFailed" | "anchorLineError" | "declarationClipped" | "sourceFailed" | "providerResultCap" | "providerIncompleteResults" | "providerResultsShifted" | "providerContentsLimit" | "providerFileLimit" | "providerCommitListLimit" | "providerBatchLimit" | "partialTreeFailures" | "materializeDepth" | "materializeIncomplete" | "patchUnavailable" | "contextClipped" | "closingReferenceLimit" | "callSitesUnresolved" | "hitLinesNotListed")
export type RowStatus = ("empty" | "error" | "outOfRange")
export type GraphQueryInput = (GraphQueryInputQueries | GraphQueryInputCursor)
export type GraphQueryQuery = (GraphQueryStats | GraphQueryFind | GraphQueryCandidates | GraphQueryNode | GraphQuerySymbols | GraphQueryTraversal | GraphQueryGroupedTraversal | GraphQueryPath | GraphQueryCycles | GraphQueryHubs | GraphQueryCochange | GraphQueryDiagnostics | GraphQueryWitnesses | GraphQueryUnresolved | GraphQueryStale | GraphQueryIssues | GraphQueryImpact | GraphQueryDrift)
export type GraphScopepath = string
export type GraphScopegraph = string
export type GraphScopeincludeProvenance = boolean
export type GraphQueryStatsView = ("summary" | "details")
export type GraphPagelimit = number
export type GraphPageoffset = number
export type GraphPagepage = number
export type GraphReferencekind = ("file" | "symbol" | "package")
/**
 * @minItems 1
 * @maxItems 6
 */
export type GraphEdgeedges = ("contains" | "imports" | "uses" | "calls" | "inherits" | "passes")[]
export type GraphEdgeconfidence = ("high" | "medium" | "low")
export type GraphQueryImpact = (GraphQueryImpactTarget | GraphQueryImpactChanged | GraphQueryImpactSince)
export type GraphQueryOutput = (GraphQueryOutputResults | GraphQueryOutputError)
export type GraphQueryData = ({
error?: string
commitSha?: string
errorCode?: ErrorCode
errorClass?: ErrorClass
httpStatus?: number
retryable?: boolean
ssoUrl?: string
/**
 * @minItems 1
 */
clientWaits?: ClientWait[]
observedAt?: string
redacted?: boolean
rateLimit?: {
resource?: string
remaining?: number
resetEpochSeconds?: number
retryAfterSeconds?: number
}
hints?: RowHints
related?: RelatedCall[]
warnings?: string[]
next?: GraphQueryNext
pagination?: OutputPagination
isPartial?: boolean
terminalLimit?: boolean
partialReasons?: PartialReason[]
coverage?: {
language?: string
linkage?: {
linked: number
eligible: number
excluded: number
sites: number
ratio: (number | null)
population: "syntax-call-sites"
}
basis: ("syntax" | "syntax+scip")
callExtraction: ("supported" | "unsupported" | "notAssessed")
warning?: string
}
sourceEvidence?: OutputSourceEvidence
graph?: {
id: string
root?: string
createdAt?: string
isPartial?: boolean
partialReasons?: PartialReason[]
}
operation?: string
rows?: (OutputGraphImpactRow | OutputGraphResultRow | string)[]
freshness?: {
fresh: boolean
analysisCurrent: boolean
}
argumentCoverage?: {
scope: "snapshot"
population: "syntax-argument-slots"
byLanguage: {
[k: string]: {
extraction: "supported"
/**
 * @minItems 1
 */
referenceForms: ("identifier" | "qualified" | "member" | "addressOf" | "borrow" | "methodReference")[]
counts?: {
sites: number
groups: number
slots: number
directReferences: number
nonReferenceValues: number
unsupportedSlots: number
unsupportedGroups: number
}
} | {
extraction: ("unsupported" | "notAssessed")
}
}
}
linkage?: {
linked: number
eligible: number
excluded: number
sites: number
ratio: (number | null)
population: "syntax-call-sites"
}
linkageByLanguage?: {
[k: string]: {
linked: number
eligible: number
excluded: number
sites: number
ratio: (number | null)
population: "syntax-call-sites"
}
}
extraction?: {
parsedFiles: number
reusedFiles: number
resumedFiles?: number
checkpointedFiles?: number
spilledBytes?: number
memoryOnlyFiles?: number
maxSpillBytes?: number
maxMemoryOnlyBytes?: number
}
counts?: {
[k: string]: unknown
}
scan?: {
[k: string]: unknown
}
imports?: {
[k: string]: unknown
}
calls?: {
[k: string]: unknown
}
diagnostics?: {
total: number
byCode?: {
[k: string]: number
}
}
octocodeVersion?: string
languages?: unknown[]
callConfidence?: ("high" | "medium" | "low")
confidence?: ("high" | "medium" | "low")
testEdges?: string
evidence?: string
from?: string
to?: string
node?: OutputGraphNodeEvidence
target?: OutputGraphNodeEvidence
edges?: string[]
summary?: {
targetCommits?: number
filesAdded?: number
filesRemoved?: number
filesChanged?: number
cyclesAdded?: number
cyclesResolved?: number
staticToDynamic?: number
dynamicToStatic?: number
relationsAdded?: number
relationsRemoved?: number
nodes?: number
edges?: number
changed?: unknown[]
configChanges?: unknown[]
affectedEntrypoints?: number
testsToRun?: number
testFunctions?: number
components?: unknown[]
unmapped?: number
notes?: unknown[]
unlinkedPathImports?: unknown[]
precision?: ("symbol" | "file")
allAffected?: boolean
byDepth?: {
[k: string]: number
}
depthLimited?: boolean
maxDepth?: number
since?: string
reason?: string
byDetector?: {
[k: string]: number
}
entrypoints?: {
[k: string]: unknown
}
fileRoles?: {
[k: string]: number
}
health?: {
[k: string]: unknown
}
method?: string
minTier?: number
hiddenBelowTier?: number
[k: string]: unknown
}
baseline?: ({
id: string
root?: string
createdAt?: string
isPartial?: boolean
partialReasons?: PartialReason[]
} | {
graph?: string
new?: number
existing?: number
resolved?: number
resolvedFindings?: unknown[]
})
rankBy?: string
found?: boolean
length?: number
path?: OutputGraphNodeEvidence[]
interpretation?: {
kind: "candidate"
conditionCompatibility: "notAssessed"
execution: "notObserved"
}
checked?: number
fresh?: boolean
analysisCurrent?: boolean
reingest?: RelatedCall
list?: string
cycle?: string
candidates?: (OutputGraphNodeEvidence | string)[]
[k: string]: unknown
} & ({
graph: {
id: string
root?: string
createdAt?: string
isPartial?: boolean
partialReasons?: PartialReason[]
}
[k: string]: unknown
} | {
error: string
[k: string]: unknown
}) & ({
operation?: ("stats" | "find" | "candidates" | "node" | "symbols" | "deps" | "dependents" | "callers" | "callees" | "walk" | "path" | "cycles" | "hubs" | "diagnostics" | "stale" | "issues" | "impact" | "drift" | "cochange")
[k: string]: unknown
} | {
operation: "witnesses"
rows?: (({
id?: string
from?: string
path?: string
line: (number | null)
sourceDigest: string
sourceRange: OutputSourceByteRange
sourceOrigin: OutputGraphSourceOrigin
derivation?: {
rule: string
version: string
}
evidence?: "syntax"
to: string
detail: string
confidence: ("high" | "medium" | "low")
test?: boolean
edge: ("contains" | "imports" | "uses" | "calls" | "inherits")
} | {
id?: string
from?: string
path?: string
line: (number | null)
sourceDigest: string
sourceRange: null
sourceOrigin: null
derivation?: {
rule: string
version: string
}
evidence?: "syntax"
to: string
detail: string
confidence: ("high" | "medium" | "low")
test?: boolean
edge: ("contains" | "imports" | "uses" | "calls" | "inherits")
} | {
id?: string
from?: string
path?: string
line: (number | null)
derivation?: {
rule: string
version: string
}
evidence?: "syntax"
to: string
detail: string
confidence: ("high" | "medium" | "low")
test?: boolean
edge: ("contains" | "imports" | "uses" | "calls" | "inherits")
sourceRange?: OutputSourceByteRange
parseAssessment?: OutputParseAssessment
}) | {
id?: string
from?: string
path?: string
line: number
sourceDigest: string
sourceRange: OutputSourceByteRange
sourceOrigin: OutputGraphSourceOrigin
derivation?: {
rule: string
version: string
}
evidence?: "syntax"
to: string
detail: string
confidence: ("high" | "medium" | "low")
test?: boolean
recipient: {
occurrenceId: string
sourceRange: OutputSourceByteRange
sourceOrigin: OutputGraphSourceOrigin
}
parseAssessment: OutputParseAssessment
argument: {
groupIndex: number
index: number
label?: string
form: ("identifier" | "qualified" | "member" | "addressOf" | "borrow" | "methodReference")
}
targetName: string
selectionByteRange: OutputSourceByteRange
selectionOrigin: OutputGraphSourceOrigin
edge: "passes"
} | {
id?: string
from?: string
path?: string
line: number
derivation?: {
rule: string
version: string
}
evidence?: "syntax"
to: string
detail: string
confidence: ("high" | "medium" | "low")
test?: boolean
argument: {
groupIndex: number
index: number
label?: string
form: ("identifier" | "qualified" | "member" | "addressOf" | "borrow" | "methodReference")
}
targetName: string
edge: "passes"
sourceRange?: OutputSourceByteRange
parseAssessment?: OutputParseAssessment
})[]
filteredByConfidence?: number
[k: string]: unknown
} | {
operation: "unresolved"
rows?: (({
id?: string
from?: string
path?: string
line: number
sourceDigest: string
sourceRange: OutputSourceByteRange
sourceOrigin: OutputGraphSourceOrigin
derivation?: {
rule: string
version: string
}
evidence?: "syntax"
targetName: string
receiverType?: string
edge: "calls"
reason: ("external-package" | "not-declared-in-repo" | "type-qualified-unmatched" | "member-unknown-receiver-in-repo" | "ambiguous" | "other" | "local-binding" | "recovered-declaration")
candidates?: string[]
} | {
id?: string
from?: string
path?: string
line: number
sourceDigest: string
sourceRange: null
sourceOrigin: null
derivation?: {
rule: string
version: string
}
evidence?: "syntax"
targetName: string
receiverType?: string
edge: "calls"
reason: ("external-package" | "not-declared-in-repo" | "type-qualified-unmatched" | "member-unknown-receiver-in-repo" | "ambiguous" | "other" | "local-binding" | "recovered-declaration")
candidates?: string[]
} | {
id?: string
from?: string
path?: string
line: number
derivation?: {
rule: string
version: string
}
evidence?: "syntax"
targetName: string
receiverType?: string
edge: "calls"
reason: ("external-package" | "not-declared-in-repo" | "type-qualified-unmatched" | "member-unknown-receiver-in-repo" | "ambiguous" | "other" | "local-binding" | "recovered-declaration")
candidates?: string[]
sourceRange?: OutputSourceByteRange
parseAssessment?: OutputParseAssessment
}) | ({
id?: string
from?: string
path?: string
line: number
sourceDigest: string
sourceRange: OutputSourceByteRange
sourceOrigin: OutputGraphSourceOrigin
derivation?: {
rule: string
version: string
}
evidence?: "syntax"
targetName: string
edge: "calls"
reason: ("constructor-target-unavailable" | "constructor-binding-not-assessed" | "unsupported-constructor-form")
} | {
id?: string
from?: string
path?: string
line: number
sourceDigest: string
sourceRange: null
sourceOrigin: null
derivation?: {
rule: string
version: string
}
evidence?: "syntax"
targetName: string
edge: "calls"
reason: ("constructor-target-unavailable" | "constructor-binding-not-assessed" | "unsupported-constructor-form")
} | {
id?: string
from?: string
path?: string
line: number
derivation?: {
rule: string
version: string
}
evidence?: "syntax"
targetName: string
edge: "calls"
reason: ("constructor-target-unavailable" | "constructor-binding-not-assessed" | "unsupported-constructor-form")
sourceRange?: OutputSourceByteRange
parseAssessment?: OutputParseAssessment
}) | {
id?: string
from?: string
path?: string
line: number
sourceDigest: string
sourceRange: OutputSourceByteRange
sourceOrigin: OutputGraphSourceOrigin
derivation?: {
rule: string
version: string
}
evidence?: "syntax"
recipient: {
occurrenceId: string
sourceRange: OutputSourceByteRange
sourceOrigin: OutputGraphSourceOrigin
}
parseAssessment: OutputParseAssessment
argument: {
groupIndex: number
index: number
label?: string
form: ("identifier" | "qualified" | "member" | "addressOf" | "borrow" | "methodReference")
}
targetName: string
selectionByteRange: OutputSourceByteRange
selectionOrigin: OutputGraphSourceOrigin
edge: "passes"
reason: ("external-package" | "not-declared-in-repo" | "type-qualified-unmatched" | "member-unknown-receiver-in-repo" | "ambiguous" | "other")
candidates?: string[]
} | {
id?: string
from?: string
path?: string
line: number
derivation?: {
rule: string
version: string
}
evidence?: "syntax"
argument: {
groupIndex: number
index: number
label?: string
form: ("identifier" | "qualified" | "member" | "addressOf" | "borrow" | "methodReference")
}
targetName: string
edge: "passes"
reason: ("external-package" | "not-declared-in-repo" | "type-qualified-unmatched" | "member-unknown-receiver-in-repo" | "ambiguous" | "other")
candidates?: string[]
sourceRange?: OutputSourceByteRange
parseAssessment?: OutputParseAssessment
} | {
id?: string
from?: string
path?: string
line: number
sourceDigest: string
sourceRange: OutputSourceByteRange
sourceOrigin: OutputGraphSourceOrigin
derivation?: {
rule: string
version: string
}
evidence?: "syntax"
recipient: {
occurrenceId: string
sourceRange: OutputSourceByteRange
sourceOrigin: OutputGraphSourceOrigin
}
parseAssessment: OutputParseAssessment
argument: {
groupIndex: number
index: number
label?: string
form: ("identifier" | "qualified" | "member" | "addressOf" | "borrow" | "methodReference")
}
targetName: string
selectionByteRange: OutputSourceByteRange
selectionOrigin: OutputGraphSourceOrigin
edge: "passes"
reason: "recovered-occurrence"
} | {
id?: string
from?: string
path?: string
line: number
derivation?: {
rule: string
version: string
}
evidence?: "syntax"
argument: {
groupIndex: number
index: number
label?: string
form: ("identifier" | "qualified" | "member" | "addressOf" | "borrow" | "methodReference")
}
targetName: string
edge: "passes"
reason: "recovered-occurrence"
sourceRange?: OutputSourceByteRange
parseAssessment?: OutputParseAssessment
} | {
id?: string
from?: string
path?: string
line: number
sourceDigest: string
sourceRange: OutputSourceByteRange
sourceOrigin: OutputGraphSourceOrigin
derivation?: {
rule: string
version: string
}
evidence?: "syntax"
recipient: {
occurrenceId: string
sourceRange: OutputSourceByteRange
sourceOrigin: OutputGraphSourceOrigin
}
parseAssessment: OutputParseAssessment
argument: {
groupIndex: number
index: number
label?: string
form: ("identifier" | "qualified" | "member" | "addressOf" | "borrow" | "methodReference")
}
targetName: string
selectionByteRange: OutputSourceByteRange
selectionOrigin: OutputGraphSourceOrigin
edge: "passes"
reason: "recovered-occurrence"
bindingSourceRange: OutputSourceByteRange
bindingOrigin: OutputGraphSourceOrigin
} | {
id?: string
from?: string
path?: string
line: number
sourceDigest: string
sourceRange: OutputSourceByteRange
sourceOrigin: OutputGraphSourceOrigin
derivation?: {
rule: string
version: string
}
evidence?: "syntax"
recipient: {
occurrenceId: string
sourceRange: OutputSourceByteRange
sourceOrigin: OutputGraphSourceOrigin
}
parseAssessment: OutputParseAssessment
argument: {
groupIndex: number
index: number
label?: string
form: ("identifier" | "qualified" | "member" | "addressOf" | "borrow" | "methodReference")
}
targetName: string
selectionByteRange: OutputSourceByteRange
selectionOrigin: OutputGraphSourceOrigin
edge: "passes"
reason: "scope-not-assessed"
} | {
id?: string
from?: string
path?: string
line: number
derivation?: {
rule: string
version: string
}
evidence?: "syntax"
argument: {
groupIndex: number
index: number
label?: string
form: ("identifier" | "qualified" | "member" | "addressOf" | "borrow" | "methodReference")
}
targetName: string
edge: "passes"
reason: "scope-not-assessed"
sourceRange?: OutputSourceByteRange
parseAssessment?: OutputParseAssessment
} | {
id?: string
from?: string
path?: string
line: number
sourceDigest: string
sourceRange: OutputSourceByteRange
sourceOrigin: OutputGraphSourceOrigin
derivation?: {
rule: string
version: string
}
evidence?: "syntax"
recipient: {
occurrenceId: string
sourceRange: OutputSourceByteRange
sourceOrigin: OutputGraphSourceOrigin
}
parseAssessment: OutputParseAssessment
argument: {
groupIndex: number
index: number
label?: string
form: ("identifier" | "qualified" | "member" | "addressOf" | "borrow" | "methodReference")
}
targetName: string
selectionByteRange: OutputSourceByteRange
selectionOrigin: OutputGraphSourceOrigin
edge: "passes"
reason: "binding-unrepresented"
bindingSourceRange: OutputSourceByteRange
bindingOrigin: OutputGraphSourceOrigin
} | {
id?: string
from?: string
path?: string
line: number
derivation?: {
rule: string
version: string
}
evidence?: "syntax"
argument: {
groupIndex: number
index: number
label?: string
form: ("identifier" | "qualified" | "member" | "addressOf" | "borrow" | "methodReference")
}
targetName: string
edge: "passes"
reason: "binding-unrepresented"
sourceRange?: OutputSourceByteRange
parseAssessment?: OutputParseAssessment
} | {
id?: string
from?: string
path?: string
line: number
sourceDigest: string
sourceRange: OutputSourceByteRange
sourceOrigin: OutputGraphSourceOrigin
derivation?: {
rule: string
version: string
}
evidence?: "syntax"
recipient: {
occurrenceId: string
sourceRange: OutputSourceByteRange
sourceOrigin: OutputGraphSourceOrigin
}
parseAssessment: OutputParseAssessment
construct: string
argument: {
groupIndex: number
index: number
label?: string
form: "unsupported"
}
reason: "unsupported-argument-form"
edge: "passes"
} | {
id?: string
from?: string
path?: string
line: number
derivation?: {
rule: string
version: string
}
evidence?: "syntax"
construct: string
argument: {
groupIndex: number
index: number
label?: string
form: "unsupported"
}
reason: "unsupported-argument-form"
edge: "passes"
sourceRange?: OutputSourceByteRange
parseAssessment?: OutputParseAssessment
} | {
id?: string
from?: string
path?: string
line: number
sourceDigest: string
sourceRange: OutputSourceByteRange
sourceOrigin: OutputGraphSourceOrigin
derivation?: {
rule: string
version: string
}
evidence?: "syntax"
recipient: {
occurrenceId: string
sourceRange: OutputSourceByteRange
sourceOrigin: OutputGraphSourceOrigin
}
parseAssessment: OutputParseAssessment
construct: string
argument: {
groupIndex: number
form: "unsupportedGroup"
}
reason: "unsupported-argument-group"
edge: "passes"
} | {
id?: string
from?: string
path?: string
line: number
derivation?: {
rule: string
version: string
}
evidence?: "syntax"
construct: string
argument: {
groupIndex: number
form: "unsupportedGroup"
}
reason: "unsupported-argument-group"
edge: "passes"
sourceRange?: OutputSourceByteRange
parseAssessment?: OutputParseAssessment
})[]
[k: string]: unknown
}))
export type OutputSourceSnapshot = ({
id: string
path: string
digest: string
byteLength: number
encoding: "utf8"
bomBytes: (0 | 3)
} | {
id: string
path: string
digest: string
byteLength: number
encoding: "latin1"
bomBytes: (0 | 3)
} | {
id: string
path: string
digest: string
byteLength: number
encoding: "lossy"
bomBytes: (0 | 3)
} | {
id: string
path: string
digest: string
byteLength: number
encoding: "utf16le"
bomBytes: 2
} | {
id: string
path: string
digest: string
byteLength: number
encoding: "utf16be"
bomBytes: 2
})
export type AnalysisOperation = ("structuralSearch" | "syntaxTree" | "signatureOutline" | "declarations" | "graphFacts")
export type OutputGraphSourceOrigin = (OutputSourceOrigin | "identity")
export type OutputOriginBranch = ({
kind: "copied"
origin: OutputOriginalSourceRange
} | {
kind: "transcoded"
origin: OutputOriginalSourceRange
} | {
kind: "derived"
/**
 * @minItems 1
 */
contributors: OutputSourceContribution[]
} | {
kind: "unmapped"
reason: OutputUnmappedReason
})
export type OutputSourceContribution = ({
kind: "source"
origin: OutputOriginalSourceRange
} | {
kind: "unmapped"
reason: OutputUnmappedReason
})
export type OutputUnmappedReason = ("synthetic" | "rawSourceUnavailable" | "mappingUnavailable")
export type OutputParseAssessment = ({
status: "parsed"
scope: ("occurrence" | "declaration" | "enclosingDeclaration" | "document")
range?: {
start: {
line: number
character: number
}
end: {
line: number
character: number
}
}
} | {
status: "recovered"
scope: ("occurrence" | "declaration" | "enclosingDeclaration" | "document")
reason: "syntaxRecovery"
range?: {
start: {
line: number
character: number
}
end: {
line: number
character: number
}
}
} | {
status: "notAssessed"
scope?: ("occurrence" | "declaration" | "enclosingDeclaration" | "document")
reason: "assessmentUnavailable"
})
export type OutputTestIdentity = ({
framework: "googletest"
suite: string
name?: string
parameterized?: boolean
kind?: "declaration"
} | {
framework: "rust"
name?: string
parameterized?: false
kind?: "declaration"
})

export interface GraphContract {
GraphBuildInput: GraphBuildInput
GraphBuildOutput: GraphBuildOutput
GraphQueryInput: GraphQueryInput
GraphQueryOutput: GraphQueryOutput
}
export interface GraphBuildInputQueries {
/**
 * @minItems 1
 * @maxItems 1
 */
queries: GraphBuildQuery[]
responseChars?: number
}
export interface GraphBuildQuery {
path?: string
/**
 * @maxItems 100
 */
exclude?: string[]
defaultExcludes?: DefaultExcludes
includeIgnored?: boolean
includeHidden?: boolean
keep?: number
refresh?: boolean
parserGlobs?: AstParserGlobs
mainGoal?: string
reasoning?: string
}
export interface AstParserGlobs {
/**
 * @minItems 1
 * @maxItems 100
 */
[k: string]: string[]
}
export interface GraphBuildInputCursor {
cursor: string
}
export interface GraphBuildOutputResults {
warnings?: string[]
oversized?: true
next?: GraphBuildContinueCall
expiresAt?: string
replayed?: true
results: {
index: number
meta?: {
evidence: {
kind: ("exact" | "lexical" | "structural" | "syntactic" | "semantic" | "provider")
confidence: ("low" | "medium" | "high")
[k: string]: unknown
}
diagnostics?: {
codes?: string[]
hints?: string[]
partial?: boolean
[k: string]: unknown
}
[k: string]: unknown
}
data: GraphBuildData
status?: RowStatus
rowPart?: {
part: number
of: number
continues?: {
array: string
index: number
}
text?: string
}
[k: string]: unknown
}[]
shared?: {
data: {
owner?: string
repo?: string
ref?: string
commitSha?: string
root?: string
workspaceRoot?: string
lsp?: {
[k: string]: unknown
}
partialReasons?: PartialReason[]
coverage?: {
[k: string]: unknown
}
}
}
hints?: RowHints
related?: RelatedCall[]
error?: {
[k: string]: unknown
}
errorCode?: {
[k: string]: unknown
}
errorClass?: {
[k: string]: unknown
}
details?: {
[k: string]: unknown
}
}
export interface GraphBuildContinueCall {
kind?: "continue"
tool: "graphBuild"
input: {
cursor: string
}
}
export interface GraphBuildDataGraph {
error?: string
commitSha?: string
errorCode?: ErrorCode
errorClass?: ErrorClass
httpStatus?: number
retryable?: boolean
ssoUrl?: string
/**
 * @minItems 1
 */
clientWaits?: ClientWait[]
observedAt?: string
redacted?: boolean
rateLimit?: {
resource?: string
remaining?: number
resetEpochSeconds?: number
retryAfterSeconds?: number
}
hints?: RowHints
related?: RelatedCall[]
warnings?: string[]
next?: GraphBuildNext
pagination?: OutputPagination
isPartial?: boolean
terminalLimit?: boolean
partialReasons?: PartialReason[]
coverage?: Coverage
graph: string
root?: string
counts?: {
[k: string]: unknown
}
linkage?: {
linked: number
eligible: number
excluded: number
sites: number
ratio: (number | null)
population: "syntax-call-sites"
}
scan?: {
[k: string]: unknown
}
calls?: {
[k: string]: unknown
}
diagnostics?: {
[k: string]: unknown
}
skippedPaths?: SkippedPaths
pruned?: string[]
unfinishedJobBytes?: number
progress?: {
committedFiles: number
remainingFiles: number
}
scip?: ("indexed" | "failed")
reused?: true
[k: string]: unknown
}
export interface ClientWait {
kind: ("quotaPacing" | "spacing" | "retryAfter" | "secondaryLimit")
ms: number
}
export interface GraphBuildRelatedCall {
kind?: "related"
tool: "graphBuild"
input: {
/**
 * @minItems 1
 * @maxItems 1
 */
queries: {
path?: string
/**
 * @maxItems 100
 */
exclude?: string[]
defaultExcludes?: DefaultExcludesContinuationInput
includeIgnored?: boolean
includeHidden?: boolean
keep?: number
refresh?: boolean
parserGlobs?: AstParserGlobsContinuationInput
mainGoal?: string
reasoning?: string
}[]
responseChars?: number
}
}
export interface AstParserGlobsContinuationInput {
/**
 * @minItems 1
 * @maxItems 100
 */
[k: string]: string[]
}
export interface GraphQueryRelatedCall {
kind?: "related"
tool: "graphQuery"
input: {
/**
 * @minItems 1
 * @maxItems 5
 */
queries: ({
operation: "stats"
path?: GraphScopepathContinuationInput
graph?: GraphScopegraphContinuationInput
includeProvenance?: GraphScopeincludeProvenanceContinuationInput
view?: GraphQueryStatsViewContinuationInput
mainGoal?: string
reasoning?: string
} | {
operation: "find"
path?: GraphScopepathContinuationInput
graph?: GraphScopegraphContinuationInput
includeProvenance?: GraphScopeincludeProvenanceContinuationInput
limit?: GraphPagelimitContinuationInput
kind?: ("file" | "symbol" | "package")
target?: string
mainGoal?: string
reasoning?: string
} | {
operation: "candidates"
path?: GraphScopepathContinuationInput
graph?: GraphScopegraphContinuationInput
includeProvenance?: GraphScopeincludeProvenanceContinuationInput
target: GraphReferencetarget
kind?: GraphReferencekindContinuationInput
limit?: GraphPagelimitContinuationInput
mainGoal?: string
reasoning?: string
} | {
operation: "node"
path?: GraphScopepathContinuationInput
graph?: GraphScopegraphContinuationInput
includeProvenance?: GraphScopeincludeProvenanceContinuationInput
target: GraphReferencetarget
kind?: GraphReferencekindContinuationInput
mainGoal?: string
reasoning?: string
} | {
operation: "symbols"
path?: GraphScopepathContinuationInput
graph?: GraphScopegraphContinuationInput
includeProvenance?: GraphScopeincludeProvenanceContinuationInput
target?: (string | string[])
kind?: ("file" | "symbol" | "package")
limit?: GraphPagelimitContinuationInput
mainGoal?: string
reasoning?: string
} | {
operation: ("deps" | "dependents" | "callers" | "callees" | "walk")
path?: GraphScopepathContinuationInput
graph?: GraphScopegraphContinuationInput
includeProvenance?: GraphScopeincludeProvenanceContinuationInput
target: GraphReferencetarget
kind?: GraphReferencekindContinuationInput
edges?: GraphEdgeedgesContinuationInput
confidence?: GraphEdgeconfidenceContinuationInput
limit?: GraphPagelimitContinuationInput
direction?: ("out" | "in" | "both")
depth?: number
mainGoal?: string
reasoning?: string
} | {
operation: ("deps" | "dependents" | "callers")
path?: GraphScopepathContinuationInput
graph?: GraphScopegraphContinuationInput
includeProvenance?: GraphScopeincludeProvenanceContinuationInput
target: GraphReferencetarget
kind?: GraphReferencekindContinuationInput
edges?: GraphEdgeedgesContinuationInput
confidence?: GraphEdgeconfidenceContinuationInput
limit?: GraphPagelimitContinuationInput
direction?: ("out" | "in" | "both")
depth?: number
view: ("files" | "modules")
mainGoal?: string
reasoning?: string
} | {
operation: "path"
path?: GraphScopepathContinuationInput
graph?: GraphScopegraphContinuationInput
includeProvenance?: GraphScopeincludeProvenanceContinuationInput
target: GraphReferencetarget
kind?: GraphReferencekindContinuationInput
edges?: GraphEdgeedgesContinuationInput
confidence?: GraphEdgeconfidenceContinuationInput
to: string
direction?: ("out" | "in" | "both")
mainGoal?: string
reasoning?: string
} | {
operation: "cycles"
path?: GraphScopepathContinuationInput
graph?: GraphScopegraphContinuationInput
includeProvenance?: GraphScopeincludeProvenanceContinuationInput
edges?: GraphEdgeedgesContinuationInput
confidence?: GraphEdgeconfidenceContinuationInput
limit?: GraphPagelimitContinuationInput
mainGoal?: string
reasoning?: string
} | {
operation: "hubs"
path?: GraphScopepathContinuationInput
graph?: GraphScopegraphContinuationInput
includeProvenance?: GraphScopeincludeProvenanceContinuationInput
edges?: GraphEdgeedgesContinuationInput
confidence?: GraphEdgeconfidenceContinuationInput
limit?: GraphPagelimitContinuationInput
kind?: ("file" | "symbol" | "package")
direction?: ("in" | "out")
rankBy?: ("degree" | "pageRank")
mainGoal?: string
reasoning?: string
} | {
operation: "cochange"
path?: GraphScopepathContinuationInput
graph?: GraphScopegraphContinuationInput
includeProvenance?: GraphScopeincludeProvenanceContinuationInput
limit?: GraphPagelimitContinuationInput
target: string
mainGoal?: string
reasoning?: string
} | {
operation: "diagnostics"
path?: GraphScopepathContinuationInput
graph?: GraphScopegraphContinuationInput
includeProvenance?: GraphScopeincludeProvenanceContinuationInput
limit?: GraphPagelimitContinuationInput
target?: string
mainGoal?: string
reasoning?: string
} | {
operation: "witnesses"
path?: GraphScopepathContinuationInput
graph?: GraphScopegraphContinuationInput
includeProvenance?: GraphScopeincludeProvenanceContinuationInput
limit?: GraphPagelimitContinuationInput
edges?: GraphEdgeedgesContinuationInput
confidence?: GraphEdgeconfidenceContinuationInput
target?: (string | string[])
to?: string
symbolName?: string
mainGoal?: string
reasoning?: string
} | {
operation: "unresolved"
path?: GraphScopepathContinuationInput
graph?: GraphScopegraphContinuationInput
includeProvenance?: GraphScopeincludeProvenanceContinuationInput
limit?: GraphPagelimitContinuationInput
target?: (string | string[])
edges?: GraphEdgeedgesContinuationInput
symbolName?: string
/**
 * @minItems 1
 * @maxItems 16
 */
reasons?: ("external-package" | "not-declared-in-repo" | "type-qualified-unmatched" | "member-unknown-receiver-in-repo" | "ambiguous" | "other" | "local-binding" | "recovered-declaration" | "constructor-target-unavailable" | "constructor-binding-not-assessed" | "unsupported-constructor-form" | "recovered-occurrence" | "scope-not-assessed" | "binding-unrepresented" | "unsupported-argument-form" | "unsupported-argument-group")[]
mainGoal?: string
reasoning?: string
} | {
operation: "stale"
path?: GraphScopepathContinuationInput
graph?: GraphScopegraphContinuationInput
includeProvenance?: GraphScopeincludeProvenanceContinuationInput
limit?: GraphPagelimitContinuationInput
mainGoal?: string
reasoning?: string
} | {
operation: "issues"
path?: GraphScopepathContinuationInput
graph?: GraphScopegraphContinuationInput
includeProvenance?: GraphScopeincludeProvenanceContinuationInput
limit?: GraphPagelimitContinuationInput
/**
 * @minItems 1
 * @maxItems 11
 */
detectors?: ("cycle" | "dir-cycle" | "unreachable-file" | "test-only" | "unused-export" | "export-only-local" | "undeclared-dependency" | "boundary-violation" | "unresolved-import" | "critical-file" | "single-point-of-failure")[]
/**
 * @minItems 1
 * @maxItems 100
 */
entries?: string[]
baseline?: string
minScore?: number
minTier?: (100 | 90 | 60)
mainGoal?: string
reasoning?: string
} | ({
operation: "impact"
path?: GraphScopepathContinuationInput
graph?: GraphScopegraphContinuationInput
includeProvenance?: GraphScopeincludeProvenanceContinuationInput
limit?: GraphPagelimitContinuationInput
kind?: ("file" | "symbol" | "package")
target: string
/**
 * @minItems 1
 * @maxItems 100
 */
changed?: string[]
since?: string
depth?: number
mainGoal?: string
reasoning?: string
} | {
operation: "impact"
path?: GraphScopepathContinuationInput
graph?: GraphScopegraphContinuationInput
includeProvenance?: GraphScopeincludeProvenanceContinuationInput
limit?: GraphPagelimitContinuationInput
kind?: ("file" | "symbol" | "package")
/**
 * @minItems 1
 * @maxItems 100
 */
changed: string[]
since?: string
depth?: number
mainGoal?: string
reasoning?: string
} | {
operation: "impact"
path?: GraphScopepathContinuationInput
graph?: GraphScopegraphContinuationInput
includeProvenance?: GraphScopeincludeProvenanceContinuationInput
limit?: GraphPagelimitContinuationInput
kind?: ("file" | "symbol" | "package")
since: string
depth?: number
mainGoal?: string
reasoning?: string
}) | {
operation: "drift"
path?: GraphScopepathContinuationInput
graph?: GraphScopegraphContinuationInput
includeProvenance?: GraphScopeincludeProvenanceContinuationInput
limit?: GraphPagelimitContinuationInput
baseline: string
mainGoal?: string
reasoning?: string
})[]
responseChars?: number
}
}
export interface ClasifyRelatedCall {
kind?: "related"
tool: "clasify"
input: {
/**
 * @minItems 1
 * @maxItems 5
 */
queries: (({
background?: ClassificationContext
model?: string
longContext?: boolean
view?: ClasifyViewContinuationInput
questions?: ClasifyQuestions
assets?: {
[k: string]: {
source: ({
tool: "localFetch"
query: {
path: string
}
} | {
tool: "ghGetFileContent"
query: {
owner: string
repo: string
path: string
ref?: string
}
} | {
url: string
})
}
}
operation: "evaluate"
preview?: boolean
/**
 * @minItems 1
 * @maxItems 100
 */
predicates?: string[]
maxRequests?: number
content: (string | {
[k: string]: unknown
} | unknown[])
perItem?: boolean
mainGoal?: string
reasoning?: string
} | {
background?: ClassificationContext
model?: string
longContext?: boolean
view?: ClasifyViewContinuationInput
questions?: ClasifyQuestions
assets?: {
[k: string]: {
source: ({
tool: "localFetch"
query: {
path: string
}
} | {
tool: "ghGetFileContent"
query: {
owner: string
repo: string
path: string
ref?: string
}
} | {
url: string
})
}
}
operation: "evaluate"
preview?: boolean
/**
 * @minItems 1
 * @maxItems 100
 */
predicates?: string[]
maxRequests?: number
source: ({
tool: "localFetch"
query: {
path: string
/**
 * @minItems 1
 * @maxItems 100
 */
ranges?: SourceLineRangeContinuationInput[]
}
} | {
tool: "ghGetFileContent"
query: {
owner: string
repo: string
path: string
ref?: string
/**
 * @minItems 1
 * @maxItems 100
 */
ranges?: SourceLineRangeContinuationInput[]
}
} | {
url: string
} | {
tool: ("localSearch" | "structureSearch" | "astSearch" | "lspSearch" | "ghSearchCode" | "ghSearchRepo" | "ghSearchHistory" | "ghGetHistoryItem" | "artifactSearch")
query: {
[k: string]: unknown
}
})
perHit?: boolean
collapse?: boolean
mainGoal?: string
reasoning?: string
} | {
background?: ClassificationContext
model?: string
longContext?: boolean
view?: ClasifyViewContinuationInput
questions?: ClasifyQuestions
assets?: {
[k: string]: {
source: ({
tool: "localFetch"
query: {
path: string
}
} | {
tool: "ghGetFileContent"
query: {
owner: string
repo: string
path: string
ref?: string
}
} | {
url: string
})
}
}
operation: "evaluate"
preview?: boolean
/**
 * @minItems 1
 * @maxItems 100
 */
predicates?: string[]
maxRequests?: number
resources: {
[k: string]: {
tool: "localFetch"
query: {
path: string
/**
 * @minItems 1
 * @maxItems 100
 */
ranges?: SourceLineRange[]
}
} | {
tool: "ghGetFileContent"
query: {
owner: string
repo: string
path: string
ref?: string
/**
 * @minItems 1
 * @maxItems 100
 */
ranges?: SourceLineRange[]
}
} | {
url: string
}
}
mainGoal?: string
reasoning?: string
}) | {
operation: "models"
preview?: boolean
mainGoal?: string
reasoning?: string
} | {
operation: "sections"
preview?: boolean
resources: {
[k: string]: {
tool: "localFetch"
query: {
path: string
/**
 * @minItems 1
 * @maxItems 100
 */
ranges?: SourceLineRange[]
}
} | {
tool: "ghGetFileContent"
query: {
owner: string
repo: string
path: string
ref?: string
/**
 * @minItems 1
 * @maxItems 100
 */
ranges?: SourceLineRange[]
}
} | {
url: string
}
}
maxRequests?: number
background?: ClassificationContext
questions: ClasifyQuestions
model?: string
longContext?: boolean
view?: ClasifyViewContinuationInput
mainGoal?: string
reasoning?: string
})[]
responseChars?: number
}
}
export interface ClasifyQuestions {
[k: string]: {
type: "noul"
instructions?: ((string | {
[k: string]: unknown
} | unknown[]) | null)
criteria?: ({
true?: ((string | {
[k: string]: unknown
} | unknown[]) | null)
false?: ((string | {
[k: string]: unknown
} | unknown[]) | null)
} | null)
} | {
type: "choice"
instructions?: ((string | {
[k: string]: unknown
} | unknown[]) | null)
criteria: {
[k: string]: string | {
[k: string]: unknown
} | unknown[] | null
}
} | {
type: "score"
instructions?: ((string | {
[k: string]: unknown
} | unknown[]) | null)
/**
 * @minItems 2
 * @maxItems 10
 */
criteria: (string | {
[k: string]: unknown
} | unknown[])[]
} | {
type: "box"
instructions?: ((string | {
[k: string]: unknown
} | unknown[]) | null)
}
}
export interface SourceLineRangeContinuationInput {
startLine: number
endLine: number
}
export interface SourceLineRange {
startLine: number
endLine: number
}
export interface GhSearchRepoRelatedCall {
kind?: "related"
tool: "ghSearchRepo"
input: {
/**
 * @minItems 1
 * @maxItems 5
 */
queries: ({
operation: "search"
owner?: string
repo?: string
/**
 * @minItems 1
 * @maxItems 100
 */
keywords?: string[]
/**
 * @minItems 1
 * @maxItems 4
 */
matchIn?: ("name" | "description" | "readme" | "topics")[]
filters?: {
/**
 * @minItems 1
 * @maxItems 100
 */
topics?: string[]
language?: string
stars?: NumericRangeContinuationInput
createdAt?: DateRangeContinuationInput
pushedAt?: DateRangeContinuationInput
license?: string
archive?: GitHubArchive
forks?: NumericRangeContinuationInput
sizeKiB?: NumericRangeContinuationInput
goodFirstIssueCount?: NumericRangeContinuationInput
helpWantedIssueCount?: NumericRangeContinuationInput
topicCount?: NumericRangeContinuationInput
followerCount?: NumericRangeContinuationInput
visibility?: ("public" | "private" | "internal")
isSponsorable?: true
}
sort?: ({
by: "relevance"
order?: "desc"
} | {
by: ("stars" | "forks" | "helpWantedIssueCount" | "updatedAt")
order?: SortOrder
})
/**
 * @minItems 1
 * @maxItems 18
 */
fields?: ("owner" | "repo" | "stars" | "forks" | "pushedAt" | "createdAt" | "updatedAt" | "homepage" | "openIssuesCount" | "sizeKiB" | "archived" | "fork" | "visibility" | "description" | "language" | "defaultBranch" | "license" | "topics")[]
limit?: number
mainGoal?: string
reasoning?: string
} | {
operation: "list"
owner: string
filters?: {
archive?: GitHubArchive
}
sort?: {
by: "pushedAt"
order?: SortOrder
}
/**
 * @minItems 1
 * @maxItems 18
 */
fields?: ("owner" | "repo" | "stars" | "forks" | "pushedAt" | "createdAt" | "updatedAt" | "homepage" | "openIssuesCount" | "sizeKiB" | "archived" | "fork" | "visibility" | "description" | "language" | "defaultBranch" | "license" | "topics")[]
limit?: number
mainGoal?: string
reasoning?: string
} | {
operation: "get"
owner: string
repo: string
/**
 * @minItems 1
 * @maxItems 18
 */
fields?: ("owner" | "repo" | "stars" | "forks" | "pushedAt" | "createdAt" | "updatedAt" | "homepage" | "openIssuesCount" | "sizeKiB" | "archived" | "fork" | "visibility" | "description" | "language" | "defaultBranch" | "license" | "topics")[]
mainGoal?: string
reasoning?: string
})[]
responseChars?: number
}
}
export interface NumericRangeContinuationInput {
min?: number
max?: number
}
export interface DateRangeContinuationInput {
from?: string
to?: string
}
export interface GhSearchCodeRelatedCall {
kind?: "related"
tool: "ghSearchCode"
input: {
/**
 * @minItems 1
 * @maxItems 5
 */
queries: {
owner: string
repo?: string
ref?: string
/**
 * @minItems 1
 * @maxItems 100
 */
keywords?: string[]
/**
 * @minItems 1
 * @maxItems 100
 */
extensions?: string[]
nameContains?: string
path?: string
/**
 * @minItems 1
 * @maxItems 2
 */
matchIn?: ("content" | "path")[]
contextLines?: {
before?: number
after?: number
}
/**
 * @minItems 1
 * @maxItems 10
 */
fields?: ("owner" | "repo" | "path" | "alsoAt" | "commitSha" | "matches" | "matchedLines" | "matchCount" | "atRef" | "lineResolved")[]
limit?: number
mainGoal?: string
reasoning?: string
}[]
responseChars?: number
}
}
export interface GhStructureRelatedCall {
kind?: "related"
tool: "ghStructure"
input: {
/**
 * @minItems 1
 * @maxItems 5
 */
queries: ({
operation: "tree"
owner: string
repo: string
ref?: string
path?: string
maxDepth?: number
/**
 * @minItems 1
 * @maxItems 100
 */
include?: string[]
materialize?: boolean
sort?: ({
by: "path"
order?: SortOrder
} | {
by: "bytes"
order?: SortOrder
})
limit?: number
mainGoal?: string
reasoning?: string
} | {
operation: "refs"
owner: string
repo: string
ref?: string
/**
 * @minItems 1
 * @maxItems 2
 */
refKinds?: ("tags" | "branches")[]
limit?: number
mainGoal?: string
reasoning?: string
} | {
operation: "languages"
owner: string
repo: string
mainGoal?: string
reasoning?: string
})[]
responseChars?: number
}
}
export interface GhGetFileContentRelatedCall {
kind?: "related"
tool: "ghGetFileContent"
input: {
/**
 * @minItems 1
 * @maxItems 5
 */
queries: ({
owner: string
repo: string
ref?: string
path: string
view?: "content"
mainGoal?: string
reasoning?: string
} | {
owner: string
repo: string
ref?: string
path: string
view?: "content"
/**
 * @minItems 1
 * @maxItems 100
 */
ranges: SourceLineRangeContinuationInput[]
block?: boolean
mainGoal?: string
reasoning?: string
} | {
owner: string
repo: string
ref?: string
path: string
view?: "content"
matchString: TextMatchTerms
regexEngine?: RegexEngine
caseMode?: CaseMode
wholeWord?: boolean
/**
 * @minItems 1
 * @maxItems 100
 */
ranges?: SourceLineRangeContinuationInput[]
block?: boolean
contextLines?: {
before?: number
after?: number
}
mainGoal?: string
reasoning?: string
} | {
owner: string
repo: string
ref?: string
path: string
view: "compact"
mainGoal?: string
reasoning?: string
} | {
owner: string
repo: string
ref?: string
path: string
view: "compact"
/**
 * @minItems 1
 * @maxItems 100
 */
ranges: SourceLineRangeContinuationInput[]
block?: boolean
mainGoal?: string
reasoning?: string
} | {
owner: string
repo: string
ref?: string
path: string
view: "symbols"
mainGoal?: string
reasoning?: string
})[]
responseChars?: number
}
}
export interface GhSearchHistoryRelatedCall {
kind?: "related"
tool: "ghSearchHistory"
input: {
/**
 * @minItems 1
 * @maxItems 5
 */
queries: ({
operation: "pullRequest"
/**
 * @minItems 1
 * @maxItems 100
 */
keywords?: string[]
limit?: number
/**
 * @minItems 1
 * @maxItems 15
 */
fields?: ("number" | "owner" | "repo" | "title" | "state" | "author" | "baseRef" | "createdAt" | "updatedAt" | "mergedAt" | "labels" | "commentsCount" | "totalCommentsCount" | "reactionCount" | "body")[]
owner?: string
repo?: string
filters?: {
state?: ("open" | "closed")
createdAt?: DateRangeContinuationInput
updatedAt?: DateRangeContinuationInput
closedAt?: DateRangeContinuationInput
author?: string
assignee?: string
commenter?: string
mentions?: string
/**
 * @minItems 1
 * @maxItems 100
 */
labels?: string[]
commentsCount?: NumericRangeContinuationInput
reactionCount?: NumericRangeContinuationInput
archive?: GitHubArchive
/**
 * @minItems 1
 * @maxItems 3
 */
matchIn?: ("title" | "body" | "comments")[]
linked?: "issue"
reviewRequested?: string
reviewedBy?: string
review?: ("none" | "required" | "approved" | "changesRequested")
checkStatus?: ("pending" | "success" | "failure")
mergedAt?: DateRangeContinuationInput
isMerged?: boolean
isDraft?: boolean
headRef?: string
baseRef?: string
}
sort?: ({
by: ("relevance" | "commentsCount" | "reactionCount")
order?: "desc"
} | {
by: ("createdAt" | "updatedAt")
order?: SortOrder
})
mainGoal?: string
reasoning?: string
} | {
operation: "issue"
/**
 * @minItems 1
 * @maxItems 100
 */
keywords?: string[]
limit?: number
/**
 * @minItems 1
 * @maxItems 12
 */
fields?: ("number" | "owner" | "repo" | "title" | "state" | "author" | "createdAt" | "updatedAt" | "labels" | "commentsCount" | "reactionCount" | "body")[]
owner: string
repo: string
filters?: {
state?: ("open" | "closed")
createdAt?: DateRangeContinuationInput
updatedAt?: DateRangeContinuationInput
closedAt?: DateRangeContinuationInput
author?: string
assignee?: string
commenter?: string
mentions?: string
/**
 * @minItems 1
 * @maxItems 100
 */
labels?: string[]
commentsCount?: NumericRangeContinuationInput
reactionCount?: NumericRangeContinuationInput
archive?: GitHubArchive
/**
 * @minItems 1
 * @maxItems 3
 */
matchIn?: ("title" | "body" | "comments")[]
linked?: "pullRequest"
}
sort?: ({
by: ("relevance" | "commentsCount" | "reactionCount")
order?: "desc"
} | {
by: ("createdAt" | "updatedAt")
order?: SortOrder
})
mainGoal?: string
reasoning?: string
} | {
operation: "commit"
owner: string
repo: string
/**
 * @minItems 1
 * @maxItems 8
 */
fields?: ("commitSha" | "owner" | "repo" | "messageHeadline" | "committedAt" | "author" | "committer" | "messageBody")[]
limit?: number
/**
 * @minItems 1
 * @maxItems 100
 */
keywords: string[]
filters?: {
author?: string
committer?: string
committedAt?: DateRangeContinuationInput
}
mainGoal?: string
reasoning?: string
} | {
operation: "commit"
owner: string
repo: string
/**
 * @minItems 1
 * @maxItems 8
 */
fields?: ("commitSha" | "owner" | "repo" | "messageHeadline" | "committedAt" | "author" | "committer" | "messageBody")[]
limit?: number
path?: string
ref?: string
filters?: {
author?: string
committer?: string
committedAt?: DateRangeContinuationInput
}
mainGoal?: string
reasoning?: string
})[]
responseChars?: number
}
}
export interface GhGetHistoryItemRelatedCall {
kind?: "related"
tool: "ghGetHistoryItem"
input: {
/**
 * @minItems 1
 * @maxItems 5
 */
queries: ({
owner: string
repo: string
operation: "pullRequest"
number: number
/**
 * @minItems 1
 * @maxItems 8
 */
sections?: ("body" | "comments" | "reviewComments" | "reviews" | "commits" | "files" | "patches" | "commitFiles")[]
bots?: boolean
/**
 * @minItems 1
 * @maxItems 100
 */
include?: string[]
/**
 * @minItems 1
 * @maxItems 7
 */
fileStatus?: ("added" | "removed" | "modified" | "renamed" | "copied" | "changed" | "unchanged")[]
minChanges?: number
mainGoal?: string
reasoning?: string
} | {
owner: string
repo: string
operation: "pullRequest"
number: number
/**
 * @minItems 1
 * @maxItems 8
 */
sections?: ("body" | "comments" | "reviewComments" | "reviews" | "commits" | "files" | "patches" | "commitFiles")[]
bots?: boolean
/**
 * @minItems 1
 * @maxItems 100
 */
include?: string[]
/**
 * @minItems 1
 * @maxItems 7
 */
fileStatus?: ("added" | "removed" | "modified" | "renamed" | "copied" | "changed" | "unchanged")[]
minChanges?: number
matchString: TextMatchTerms
regexEngine?: RegexEngine
caseMode?: CaseMode
wholeWord?: boolean
contextLines?: {
before?: number
after?: number
}
mainGoal?: string
reasoning?: string
} | {
owner: string
repo: string
operation: "issue"
number: number
/**
 * @minItems 1
 * @maxItems 2
 */
sections?: ("body" | "comments")[]
bots?: boolean
mainGoal?: string
reasoning?: string
} | {
owner: string
repo: string
operation: "issue"
number: number
/**
 * @minItems 1
 * @maxItems 2
 */
sections?: ("body" | "comments")[]
bots?: boolean
matchString: TextMatchTerms
regexEngine?: RegexEngine
caseMode?: CaseMode
wholeWord?: boolean
contextLines?: {
before?: number
after?: number
}
mainGoal?: string
reasoning?: string
} | {
owner: string
repo: string
operation: "commit"
ref: string
/**
 * @minItems 1
 * @maxItems 2
 */
sections?: ("files" | "patches")[]
/**
 * @minItems 1
 * @maxItems 100
 */
include?: string[]
/**
 * @minItems 1
 * @maxItems 7
 */
fileStatus?: ("added" | "removed" | "modified" | "renamed" | "copied" | "changed" | "unchanged")[]
minChanges?: number
mainGoal?: string
reasoning?: string
} | {
owner: string
repo: string
operation: "commit"
ref: string
/**
 * @minItems 1
 * @maxItems 2
 */
sections?: ("files" | "patches")[]
/**
 * @minItems 1
 * @maxItems 100
 */
include?: string[]
/**
 * @minItems 1
 * @maxItems 7
 */
fileStatus?: ("added" | "removed" | "modified" | "renamed" | "copied" | "changed" | "unchanged")[]
minChanges?: number
matchString: TextMatchTerms
regexEngine?: RegexEngine
caseMode?: CaseMode
wholeWord?: boolean
contextLines?: {
before?: number
after?: number
}
mainGoal?: string
reasoning?: string
} | {
owner: string
repo: string
operation: "compare"
baseRef: string
headRef: string
/**
 * @minItems 1
 * @maxItems 3
 */
sections?: ("commits" | "files" | "patches")[]
/**
 * @minItems 1
 * @maxItems 100
 */
include?: string[]
/**
 * @minItems 1
 * @maxItems 7
 */
fileStatus?: ("added" | "removed" | "modified" | "renamed" | "copied" | "changed" | "unchanged")[]
minChanges?: number
mainGoal?: string
reasoning?: string
} | {
owner: string
repo: string
operation: "compare"
baseRef: string
headRef: string
/**
 * @minItems 1
 * @maxItems 3
 */
sections?: ("commits" | "files" | "patches")[]
/**
 * @minItems 1
 * @maxItems 100
 */
include?: string[]
/**
 * @minItems 1
 * @maxItems 7
 */
fileStatus?: ("added" | "removed" | "modified" | "renamed" | "copied" | "changed" | "unchanged")[]
minChanges?: number
matchString: TextMatchTerms
regexEngine?: RegexEngine
caseMode?: CaseMode
wholeWord?: boolean
contextLines?: {
before?: number
after?: number
}
mainGoal?: string
reasoning?: string
})[]
responseChars?: number
}
}
export interface ArtifactSearchRelatedCall {
kind?: "related"
tool: "artifactSearch"
input: {
/**
 * @minItems 1
 * @maxItems 5
 */
queries: ({
operation: "resolve"
preview?: boolean
ecosystem: "npm"
packageName: string
version?: string
registryUrl?: string
view?: ArtifactSearchViewContinuationInput
mainGoal?: string
reasoning?: string
} | {
operation: "search"
preview?: boolean
/**
 * @minItems 1
 * @maxItems 100
 */
keywords: string[]
limit?: number
view?: ArtifactSearchViewContinuationInput
ecosystem: "npm"
registryUrl?: string
mainGoal?: string
reasoning?: string
} | {
operation: "resolve"
preview?: boolean
ecosystem: ("pypi" | "crates")
packageName: string
version?: string
view?: ArtifactSearchViewContinuationInput
mainGoal?: string
reasoning?: string
} | {
operation: "resolve"
preview?: boolean
ecosystem: "maven"
packageName: string
version?: string
view?: ArtifactSearchViewContinuationInput
mainGoal?: string
reasoning?: string
} | {
operation: "resolve"
preview?: boolean
ecosystem: ("nuget" | "go" | "packagist" | "rubygems")
packageName: string
version?: string
view?: ArtifactSearchViewContinuationInput
mainGoal?: string
reasoning?: string
} | {
operation: "search"
preview?: boolean
/**
 * @minItems 1
 * @maxItems 100
 */
keywords: string[]
limit?: number
view?: ArtifactSearchViewContinuationInput
ecosystem: ("crates" | "maven" | "nuget" | "go" | "packagist" | "rubygems")
mainGoal?: string
reasoning?: string
})[]
responseChars?: number
}
}
export interface GhCloneRepoRelatedCall {
kind?: "related"
tool: "ghCloneRepo"
input: {
/**
 * @minItems 1
 * @maxItems 5
 */
queries: {
owner: string
repo: string
ref?: string
/**
 * @minItems 1
 * @maxItems 100
 */
paths?: string[]
historyDepth?: number
mainGoal?: string
reasoning?: string
}[]
responseChars?: number
}
}
export interface LocalSearchRelatedCall {
kind?: "related"
tool: "localSearch"
input: {
/**
 * @minItems 1
 * @maxItems 5
 */
queries: ({
path?: string
invertMatch?: boolean
/**
 * @maxItems 100
 */
include?: string[]
/**
 * @maxItems 100
 */
exclude?: string[]
defaultExcludes?: DefaultExcludesContinuationInput
includeIgnored?: boolean
includeHidden?: boolean
maxDepth?: number
multiline?: ("off" | "on" | "dotall")
sort?: ({
by: "traversal"
order?: "asc"
} | {
by: ("matchCount" | "modifiedAt" | "accessedAt" | "createdAt")
order?: SortOrder
} | {
by: "path"
order?: SortOrder
})
testCode?: TestCodeContinuationInput
/**
 * @minItems 1
 * @maxItems 100
 */
matchString: string[]
regexEngine?: RegexEngine
caseMode?: CaseMode
wholeWord?: boolean
fileType?: string
/**
 * @minItems 1
 * @maxItems 100
 */
fileHas?: string[]
/**
 * @minItems 1
 * @maxItems 100
 */
fileLacks?: string[]
keyPath?: string
limit?: number
view?: "content"
contextLines?: {
before?: number
after?: number
}
mainGoal?: string
reasoning?: string
} | {
path?: string
invertMatch?: boolean
/**
 * @maxItems 100
 */
include?: string[]
/**
 * @maxItems 100
 */
exclude?: string[]
defaultExcludes?: DefaultExcludesContinuationInput
includeIgnored?: boolean
includeHidden?: boolean
maxDepth?: number
multiline?: ("off" | "on" | "dotall")
sort?: ({
by: "traversal"
order?: "asc"
} | {
by: ("matchCount" | "modifiedAt" | "accessedAt" | "createdAt")
order?: SortOrder
} | {
by: "path"
order?: SortOrder
})
testCode?: TestCodeContinuationInput
/**
 * @minItems 1
 * @maxItems 100
 */
matchString: string[]
regexEngine?: RegexEngine
caseMode?: CaseMode
wholeWord?: boolean
fileType?: string
/**
 * @minItems 1
 * @maxItems 100
 */
fileHas?: string[]
/**
 * @minItems 1
 * @maxItems 100
 */
fileLacks?: string[]
keyPath?: string
limit?: number
view: ("files" | "filesWithout" | "countLines" | "countMatches")
mainGoal?: string
reasoning?: string
} | {
path?: string
invertMatch?: boolean
/**
 * @maxItems 100
 */
include?: string[]
/**
 * @maxItems 100
 */
exclude?: string[]
defaultExcludes?: DefaultExcludesContinuationInput
includeIgnored?: boolean
includeHidden?: boolean
maxDepth?: number
multiline?: ("off" | "on" | "dotall")
sort?: ({
by: "traversal"
order?: "asc"
} | {
by: ("matchCount" | "modifiedAt" | "accessedAt" | "createdAt")
order?: SortOrder
} | {
by: "path"
order?: SortOrder
})
testCode?: TestCodeContinuationInput
/**
 * @minItems 1
 * @maxItems 100
 */
matchString: string[]
regexEngine?: RegexEngine
caseMode?: CaseMode
wholeWord?: boolean
fileType?: string
/**
 * @minItems 1
 * @maxItems 100
 */
fileHas?: string[]
/**
 * @minItems 1
 * @maxItems 100
 */
fileLacks?: string[]
keyPath?: string
limit?: number
view: "matchOnly"
unique?: ("off" | "list" | "count")
mainGoal?: string
reasoning?: string
})[]
responseChars?: number
}
}
export interface StructureSearchRelatedCall {
kind?: "related"
tool: "structureSearch"
input: {
/**
 * @minItems 1
 * @maxItems 5
 */
queries: ({
path?: string
view?: StructureSearchView
includeHidden?: boolean
includeIgnored?: boolean
entryType?: ("file" | "directory")
/**
 * @maxItems 100
 */
include?: string[]
/**
 * @minItems 1
 * @maxItems 100
 */
extensions?: string[]
/**
 * @maxItems 100
 */
exclude?: string[]
defaultExcludes?: boolean
limit?: number
operation: "tree"
maxDepth?: number
mainGoal?: string
reasoning?: string
} | {
path?: string
view?: StructureSearchView
includeHidden?: boolean
includeIgnored?: boolean
entryType?: ("file" | "directory")
/**
 * @maxItems 100
 */
include?: string[]
/**
 * @minItems 1
 * @maxItems 100
 */
extensions?: string[]
/**
 * @maxItems 100
 */
exclude?: string[]
defaultExcludes?: boolean
limit?: number
operation: "files"
maxDepth?: number
testCode?: ("include" | "exclude" | "only")
minDepth?: number
nameRegex?: string
empty?: boolean
modifiedAt?: LocalTimeRangeContinuationInput
accessedAt?: LocalTimeRangeContinuationInput
bytes?: NumericRangeContinuationInput
permissions?: string
access?: ("executable" | "readable" | "writable")
sort?: ({
by: ("name" | "path")
order?: SortOrder
} | {
by: ("modifiedAt" | "bytes" | "totalLines")
order?: SortOrder
} | {
by: "traversal"
order?: "asc"
})
mainGoal?: string
reasoning?: string
})[]
responseChars?: number
}
}
export interface LocalTimeRangeContinuationInput {
from?: string
to?: string
}
export interface AstSearchRelatedCall {
kind?: "related"
tool: "astSearch"
input: {
/**
 * @minItems 1
 * @maxItems 5
 */
queries: ({
content?: string
path?: string
/**
 * @maxItems 100
 */
exclude?: string[]
defaultExcludes?: DefaultExcludesContinuationInput
includeIgnored?: boolean
includeHidden?: boolean
testCode?: TestCodeContinuationInput
/**
 * @maxItems 100
 */
include?: string[]
parser?: AstParser
parserGlobs?: AstParserGlobsContinuationInput
limit?: number
operation: "match"
scope?: AstMatchScopeContinuationInput
maxDepth?: number
sort?: ({
by: "path"
order?: SortOrder
} | {
by: "matchCount"
order?: SortOrder
})
rule: (AstSearchRule & {
field?: {
[k: string]: unknown
}
stopBy?: {
[k: string]: unknown
}
[k: string]: unknown
})
view?: "content"
includeCaptures?: boolean
includeProvenance?: boolean
mainGoal?: string
reasoning?: string
} | {
content?: string
path?: string
/**
 * @maxItems 100
 */
exclude?: string[]
defaultExcludes?: DefaultExcludesContinuationInput
includeIgnored?: boolean
includeHidden?: boolean
testCode?: TestCodeContinuationInput
/**
 * @maxItems 100
 */
include?: string[]
parser?: AstParser
parserGlobs?: AstParserGlobsContinuationInput
limit?: number
operation: "match"
scope?: AstMatchScopeContinuationInput
maxDepth?: number
sort?: ({
by: "path"
order?: SortOrder
} | {
by: "matchCount"
order?: SortOrder
})
rule: (AstSearchRule & {
field?: {
[k: string]: unknown
}
stopBy?: {
[k: string]: unknown
}
[k: string]: unknown
})
view: ("files" | "countMatches")
mainGoal?: string
reasoning?: string
} | {
operation: "syntaxTree"
includeProvenance?: boolean
line?: number
namedOnly?: boolean
path: string
parser?: AstParser
mainGoal?: string
reasoning?: string
} | {
operation: "syntaxTree"
includeProvenance?: boolean
line?: number
namedOnly?: boolean
content: string
parser: AstParser
mainGoal?: string
reasoning?: string
} | {
path?: string
/**
 * @maxItems 100
 */
exclude?: string[]
defaultExcludes?: DefaultExcludesContinuationInput
includeIgnored?: boolean
includeHidden?: boolean
testCode?: TestCodeContinuationInput
/**
 * @maxItems 100
 */
include?: string[]
parser?: AstParser
parserGlobs?: AstParserGlobsContinuationInput
limit?: number
operation: "symbols"
symbolFilter?: AstSymbolFilterContinuationInput
/**
 * @minItems 1
 * @maxItems 100
 */
symbolKinds?: string[]
includeProvenance?: boolean
mainGoal?: string
reasoning?: string
})[]
responseChars?: number
}
}
export interface AstMatchScopeContinuationInput {
start: AstScopePointContinuationInput
end: AstScopePointContinuationInput
mode?: ("contained" | "overlap")
}
export interface AstScopePointContinuationInput {
line: number
column: number
}
export interface AstSearchRule {
pattern?: (ARSyntax | {
context: ARSyntax
selector: ARSyntax
})
kind?: ARSyntax
regex?: ARSyntax
nthChild?: (number | string | StructuralOrdinal)
range?: StructuralRange
inside?: AstSearchRule
has?: AstSearchRule
precedes?: AstSearchRule2
follows?: AstSearchRule3
/**
 * @minItems 1
 * @maxItems 50
 */
all?: AstSearchRule4[]
/**
 * @minItems 1
 * @maxItems 50
 */
any?: AstSearchRule5[]
not?: AstSearchRule6
field?: ARSyntax
stopBy?: (("neighbor" | "end") | AstSearchRule7)
}
export interface StructuralOrdinal {
position: (number | string)
reverse?: boolean
ofRule?: AstSearchRule1
}
export interface AstSearchRule1 {
pattern?: (ARSyntax | {
context: ARSyntax
selector: ARSyntax
})
kind?: ARSyntax
regex?: ARSyntax
nthChild?: (number | string | StructuralOrdinal)
range?: StructuralRange
inside?: AstSearchRule
has?: AstSearchRule
precedes?: AstSearchRule2
follows?: AstSearchRule3
/**
 * @minItems 1
 * @maxItems 50
 */
all?: AstSearchRule4[]
/**
 * @minItems 1
 * @maxItems 50
 */
any?: AstSearchRule5[]
not?: AstSearchRule6
field?: ARSyntax
stopBy?: (("neighbor" | "end") | AstSearchRule7)
}
export interface StructuralRange {
start: StructuralPoint
end: StructuralPoint
}
export interface StructuralPoint {
line: number
column?: number
}
export interface AstSearchRule2 {
pattern?: (ARSyntax | {
context: ARSyntax
selector: ARSyntax
})
kind?: ARSyntax
regex?: ARSyntax
nthChild?: (number | string | StructuralOrdinal)
range?: StructuralRange
inside?: AstSearchRule
has?: AstSearchRule
precedes?: AstSearchRule2
follows?: AstSearchRule3
/**
 * @minItems 1
 * @maxItems 50
 */
all?: AstSearchRule4[]
/**
 * @minItems 1
 * @maxItems 50
 */
any?: AstSearchRule5[]
not?: AstSearchRule6
field?: ARSyntax
stopBy?: (("neighbor" | "end") | AstSearchRule7)
}
export interface AstSearchRule3 {
pattern?: (ARSyntax | {
context: ARSyntax
selector: ARSyntax
})
kind?: ARSyntax
regex?: ARSyntax
nthChild?: (number | string | StructuralOrdinal)
range?: StructuralRange
inside?: AstSearchRule
has?: AstSearchRule
precedes?: AstSearchRule2
follows?: AstSearchRule3
/**
 * @minItems 1
 * @maxItems 50
 */
all?: AstSearchRule4[]
/**
 * @minItems 1
 * @maxItems 50
 */
any?: AstSearchRule5[]
not?: AstSearchRule6
field?: ARSyntax
stopBy?: (("neighbor" | "end") | AstSearchRule7)
}
export interface AstSearchRule4 {
pattern?: (ARSyntax | {
context: ARSyntax
selector: ARSyntax
})
kind?: ARSyntax
regex?: ARSyntax
nthChild?: (number | string | StructuralOrdinal)
range?: StructuralRange
inside?: AstSearchRule
has?: AstSearchRule
precedes?: AstSearchRule2
follows?: AstSearchRule3
/**
 * @minItems 1
 * @maxItems 50
 */
all?: AstSearchRule4[]
/**
 * @minItems 1
 * @maxItems 50
 */
any?: AstSearchRule5[]
not?: AstSearchRule6
field?: ARSyntax
stopBy?: (("neighbor" | "end") | AstSearchRule7)
}
export interface AstSearchRule5 {
pattern?: (ARSyntax | {
context: ARSyntax
selector: ARSyntax
})
kind?: ARSyntax
regex?: ARSyntax
nthChild?: (number | string | StructuralOrdinal)
range?: StructuralRange
inside?: AstSearchRule
has?: AstSearchRule
precedes?: AstSearchRule2
follows?: AstSearchRule3
/**
 * @minItems 1
 * @maxItems 50
 */
all?: AstSearchRule4[]
/**
 * @minItems 1
 * @maxItems 50
 */
any?: AstSearchRule5[]
not?: AstSearchRule6
field?: ARSyntax
stopBy?: (("neighbor" | "end") | AstSearchRule7)
}
export interface AstSearchRule6 {
pattern?: (ARSyntax | {
context: ARSyntax
selector: ARSyntax
})
kind?: ARSyntax
regex?: ARSyntax
nthChild?: (number | string | StructuralOrdinal)
range?: StructuralRange
inside?: AstSearchRule
has?: AstSearchRule
precedes?: AstSearchRule2
follows?: AstSearchRule3
/**
 * @minItems 1
 * @maxItems 50
 */
all?: AstSearchRule4[]
/**
 * @minItems 1
 * @maxItems 50
 */
any?: AstSearchRule5[]
not?: AstSearchRule6
field?: ARSyntax
stopBy?: (("neighbor" | "end") | AstSearchRule7)
}
export interface AstSearchRule7 {
pattern?: (ARSyntax | {
context: ARSyntax
selector: ARSyntax
})
kind?: ARSyntax
regex?: ARSyntax
nthChild?: (number | string | StructuralOrdinal)
range?: StructuralRange
inside?: AstSearchRule
has?: AstSearchRule
precedes?: AstSearchRule2
follows?: AstSearchRule3
/**
 * @minItems 1
 * @maxItems 50
 */
all?: AstSearchRule4[]
/**
 * @minItems 1
 * @maxItems 50
 */
any?: AstSearchRule5[]
not?: AstSearchRule6
field?: ARSyntax
stopBy?: (("neighbor" | "end") | AstSearchRule7)
}
export interface AstSymbolFilterContinuationInput {
/**
 * @minItems 1
 * @maxItems 100
 */
terms: string[]
mode?: ("exact" | "substring" | "exactFirst")
caseMode?: CaseMode
}
export interface LocalFetchRelatedCall {
kind?: "related"
tool: "localFetch"
input: {
/**
 * @minItems 1
 * @maxItems 5
 */
queries: ({
path: string
limit?: number
view?: "content"
mainGoal?: string
reasoning?: string
} | {
path: string
limit?: number
view?: "content"
/**
 * @minItems 1
 * @maxItems 100
 */
ranges: SourceLineRangeContinuationInput[]
block?: boolean
mainGoal?: string
reasoning?: string
} | {
path: string
limit?: number
view?: "content"
matchString: TextMatchTerms
regexEngine?: RegexEngine
caseMode?: CaseMode
wholeWord?: boolean
/**
 * @minItems 1
 * @maxItems 100
 */
ranges?: SourceLineRangeContinuationInput[]
block?: boolean
contextLines?: {
before?: number
after?: number
}
mainGoal?: string
reasoning?: string
} | {
path: string
limit?: number
view: "compact"
mainGoal?: string
reasoning?: string
} | {
path: string
limit?: number
view: "compact"
/**
 * @minItems 1
 * @maxItems 100
 */
ranges: SourceLineRangeContinuationInput[]
block?: boolean
mainGoal?: string
reasoning?: string
} | {
path: string
limit?: number
view: "symbols"
mainGoal?: string
reasoning?: string
})[]
responseChars?: number
}
}
export interface LspSearchRelatedCall {
kind?: "related"
tool: "lspSearch"
input: {
/**
 * @minItems 1
 * @maxItems 5
 */
queries: ({
workspaceRoot?: string
limit?: number
path: string
symbolName: string
line: number
occurrence?: number
operation?: ("definition" | "typeDefinition" | "implementation")
view?: "content"
contextLines?: {
before?: number
after?: number
}
mainGoal?: string
reasoning?: string
} | {
workspaceRoot?: string
limit?: number
path: string
symbolName: string
line: number
occurrence?: number
operation?: ("definition" | "typeDefinition" | "implementation")
view: "files"
mainGoal?: string
reasoning?: string
} | {
workspaceRoot?: string
limit?: number
path: string
symbolName: string
line: number
occurrence?: number
operation: "references"
includeDeclaration?: boolean
view?: "content"
contextLines?: {
before?: number
after?: number
}
testCode?: TestCode
mainGoal?: string
reasoning?: string
} | {
workspaceRoot?: string
limit?: number
path: string
symbolName: string
line: number
occurrence?: number
operation: "references"
includeDeclaration?: boolean
view: "files"
testCode?: TestCode
mainGoal?: string
reasoning?: string
} | {
workspaceRoot?: string
limit?: number
path: string
symbolName: string
line: number
occurrence?: number
operation: "callers"
depth?: number
testCode?: TestCode
mainGoal?: string
reasoning?: string
} | {
workspaceRoot?: string
limit?: number
path: string
symbolName: string
line: number
occurrence?: number
operation: ("callees" | "supertypes")
depth?: number
mainGoal?: string
reasoning?: string
} | {
workspaceRoot?: string
limit?: number
path: string
symbolName: string
line: number
occurrence?: number
operation: "subtypes"
depth?: number
mainGoal?: string
reasoning?: string
} | {
workspaceRoot?: string
limit?: number
path: string
symbolName: string
line: number
occurrence?: number
operation: "hover"
mainGoal?: string
reasoning?: string
} | {
workspaceRoot?: string
limit?: number
path: string
operation: ("documentSymbols" | "diagnostic")
mainGoal?: string
reasoning?: string
} | {
workspaceRoot?: string
limit?: number
operation: "workspaceSymbol"
symbolFuzzy: string
path: string
mainGoal?: string
reasoning?: string
} | {
workspaceRoot: string
limit?: number
operation: "workspaceSymbol"
symbolFuzzy: string
mainGoal?: string
reasoning?: string
})[]
responseChars?: number
}
}
export interface SecurityCheckRelatedCall {
kind?: "related"
tool: "securityCheck"
input: {
/**
 * @minItems 1
 * @maxItems 5
 */
queries: ({
operation: "package"
preview?: boolean
ecosystem: ("npm" | "pypi" | "crates" | "maven" | "nuget" | "go" | "packagist" | "rubygems" | "Pub" | "Hex" | "SwiftURL" | "GitHub Actions" | "CRAN" | "Bioconductor" | "Debian" | "Ubuntu" | "Alpine" | "Rocky Linux" | "AlmaLinux" | "Red Hat" | "SUSE" | "openSUSE" | "Wolfi" | "Chainguard" | "Alpaquita" | "Mageia" | "openEuler" | "Azure Linux")
packageName: string
version?: string
release?: string
view?: SecurityCheckViewContinuationInput
mainGoal?: string
reasoning?: string
} | {
operation: "url"
preview?: boolean
url: string
view?: SecurityCheckViewContinuationInput
mainGoal?: string
reasoning?: string
} | {
operation: "advisory"
preview?: boolean
id: string
owner?: string
repo?: string
view?: SecurityCheckViewContinuationInput
mainGoal?: string
reasoning?: string
} | {
operation: "commit"
preview?: boolean
ref: string
owner?: string
repo?: string
view?: SecurityCheckViewContinuationInput
mainGoal?: string
reasoning?: string
} | {
operation: "repository"
preview?: boolean
owner: string
repo: string
view?: SecurityCheckViewContinuationInput
mainGoal?: string
reasoning?: string
})[]
responseChars?: number
}
}
export interface GraphBuildNext {
[k: string]: GraphBuildContinueCall
}
export interface OutputPagination {
unit?: ("items" | "files" | "matches" | "lines" | "bytes" | "chars")
offset: number
length?: number
total?: number
oversized?: true
}
export interface Coverage {
state?: ("complete" | "partial" | "error")
reason?: string
[k: string]: unknown
}
export interface SkippedPaths {
entries: SkippedPath[]
sampled?: boolean
}
export interface SkippedPath {
path: string
cause: ("ignored" | "hidden" | "default-excluded" | "ignored-default-excluded" | "octocode-artifact" | "withheld" | "unreadable" | "unparsed")
count?: number
reason?: string
}
export interface GraphBuildDataError {
error: string
commitSha?: string
errorCode?: ErrorCode
errorClass?: ErrorClass
httpStatus?: number
retryable?: boolean
ssoUrl?: string
/**
 * @minItems 1
 */
clientWaits?: ClientWait[]
observedAt?: string
redacted?: boolean
rateLimit?: {
resource?: string
remaining?: number
resetEpochSeconds?: number
retryAfterSeconds?: number
}
hints?: RowHints
related?: RelatedCall[]
warnings?: string[]
next?: GraphBuildNext1
pagination?: OutputPagination
isPartial?: boolean
terminalLimit?: boolean
partialReasons?: PartialReason[]
coverage?: Coverage
graph?: string
root?: string
counts?: {
[k: string]: unknown
}
linkage?: {
linked: number
eligible: number
excluded: number
sites: number
ratio: (number | null)
population: "syntax-call-sites"
}
scan?: {
[k: string]: unknown
}
calls?: {
[k: string]: unknown
}
diagnostics?: {
[k: string]: unknown
}
skippedPaths?: SkippedPaths
pruned?: string[]
unfinishedJobBytes?: number
progress?: {
committedFiles: number
remainingFiles: number
}
scip?: ("indexed" | "failed")
reused?: true
[k: string]: unknown
}
export interface GraphBuildNext1 {
[k: string]: GraphBuildContinueCall
}
export interface GraphBuildDataProgress {
error?: string
commitSha?: string
errorCode?: ErrorCode
errorClass?: ErrorClass
httpStatus?: number
retryable?: boolean
ssoUrl?: string
/**
 * @minItems 1
 */
clientWaits?: ClientWait[]
observedAt?: string
redacted?: boolean
rateLimit?: {
resource?: string
remaining?: number
resetEpochSeconds?: number
retryAfterSeconds?: number
}
hints?: RowHints
related?: RelatedCall[]
warnings?: string[]
next?: GraphBuildNext2
pagination?: OutputPagination
isPartial?: boolean
terminalLimit?: boolean
partialReasons?: PartialReason[]
coverage?: Coverage
graph?: string
root?: string
counts?: {
[k: string]: unknown
}
linkage?: {
linked: number
eligible: number
excluded: number
sites: number
ratio: (number | null)
population: "syntax-call-sites"
}
scan?: {
[k: string]: unknown
}
calls?: {
[k: string]: unknown
}
diagnostics?: {
[k: string]: unknown
}
skippedPaths?: SkippedPaths
pruned?: string[]
unfinishedJobBytes?: number
progress: {
committedFiles: number
remainingFiles: number
}
scip?: ("indexed" | "failed")
reused?: true
[k: string]: unknown
}
export interface GraphBuildNext2 {
[k: string]: GraphBuildContinueCall
}
export interface GraphBuildOutputError {
error: string
errorCode: ErrorCode
errorClass?: ErrorClass
details?: string[]
hints?: RowHints
related?: RelatedCall[]
}
export interface GraphQueryInputQueries {
/**
 * @minItems 1
 * @maxItems 5
 */
queries: GraphQueryQuery[]
responseChars?: number
}
export interface GraphQueryStats {
operation: "stats"
path?: GraphScopepath
graph?: GraphScopegraph
includeProvenance?: GraphScopeincludeProvenance
view?: GraphQueryStatsView
mainGoal?: string
reasoning?: string
}
export interface GraphQueryFind {
operation: "find"
path?: GraphScopepath
graph?: GraphScopegraph
includeProvenance?: GraphScopeincludeProvenance
limit?: GraphPagelimit
offset?: GraphPageoffset
page?: GraphPagepage
kind?: ("file" | "symbol" | "package")
target?: string
mainGoal?: string
reasoning?: string
}
export interface GraphQueryCandidates {
operation: "candidates"
path?: GraphScopepath
graph?: GraphScopegraph
includeProvenance?: GraphScopeincludeProvenance
target: GraphReferencetarget
kind?: GraphReferencekind
limit?: GraphPagelimit
offset?: GraphPageoffset
page?: GraphPagepage
mainGoal?: string
reasoning?: string
}
export interface GraphQueryNode {
operation: "node"
path?: GraphScopepath
graph?: GraphScopegraph
includeProvenance?: GraphScopeincludeProvenance
target: GraphReferencetarget
kind?: GraphReferencekind
mainGoal?: string
reasoning?: string
}
export interface GraphQuerySymbols {
operation: "symbols"
path?: GraphScopepath
graph?: GraphScopegraph
includeProvenance?: GraphScopeincludeProvenance
target?: (string | string[])
kind?: ("file" | "symbol" | "package")
limit?: GraphPagelimit
offset?: GraphPageoffset
page?: GraphPagepage
mainGoal?: string
reasoning?: string
}
export interface GraphQueryTraversal {
operation: ("deps" | "dependents" | "callers" | "callees" | "walk")
path?: GraphScopepath
graph?: GraphScopegraph
includeProvenance?: GraphScopeincludeProvenance
target: GraphReferencetarget
kind?: GraphReferencekind
edges?: GraphEdgeedges
confidence?: GraphEdgeconfidence
limit?: GraphPagelimit
offset?: GraphPageoffset
page?: GraphPagepage
direction?: ("out" | "in" | "both")
depth?: number
list?: string
mainGoal?: string
reasoning?: string
}
export interface GraphQueryGroupedTraversal {
operation: ("deps" | "dependents" | "callers")
path?: GraphScopepath
graph?: GraphScopegraph
includeProvenance?: GraphScopeincludeProvenance
target: GraphReferencetarget
kind?: GraphReferencekind
edges?: GraphEdgeedges
confidence?: GraphEdgeconfidence
limit?: GraphPagelimit
offset?: GraphPageoffset
page?: GraphPagepage
direction?: ("out" | "in" | "both")
depth?: number
list?: string
view: ("files" | "modules")
mainGoal?: string
reasoning?: string
}
export interface GraphQueryPath {
operation: "path"
path?: GraphScopepath
graph?: GraphScopegraph
includeProvenance?: GraphScopeincludeProvenance
target: GraphReferencetarget
kind?: GraphReferencekind
edges?: GraphEdgeedges
confidence?: GraphEdgeconfidence
to: string
direction?: ("out" | "in" | "both")
mainGoal?: string
reasoning?: string
}
export interface GraphQueryCycles {
operation: "cycles"
path?: GraphScopepath
graph?: GraphScopegraph
includeProvenance?: GraphScopeincludeProvenance
edges?: GraphEdgeedges
confidence?: GraphEdgeconfidence
limit?: GraphPagelimit
offset?: GraphPageoffset
page?: GraphPagepage
cycle?: string
list?: string
mainGoal?: string
reasoning?: string
}
export interface GraphQueryHubs {
operation: "hubs"
path?: GraphScopepath
graph?: GraphScopegraph
includeProvenance?: GraphScopeincludeProvenance
edges?: GraphEdgeedges
confidence?: GraphEdgeconfidence
limit?: GraphPagelimit
offset?: GraphPageoffset
page?: GraphPagepage
kind?: ("file" | "symbol" | "package")
direction?: ("in" | "out")
rankBy?: ("degree" | "pageRank")
mainGoal?: string
reasoning?: string
}
export interface GraphQueryCochange {
operation: "cochange"
path?: GraphScopepath
graph?: GraphScopegraph
includeProvenance?: GraphScopeincludeProvenance
limit?: GraphPagelimit
offset?: GraphPageoffset
page?: GraphPagepage
target: string
mainGoal?: string
reasoning?: string
}
export interface GraphQueryDiagnostics {
operation: "diagnostics"
path?: GraphScopepath
graph?: GraphScopegraph
includeProvenance?: GraphScopeincludeProvenance
limit?: GraphPagelimit
offset?: GraphPageoffset
page?: GraphPagepage
target?: string
mainGoal?: string
reasoning?: string
}
export interface GraphQueryWitnesses {
operation: "witnesses"
path?: GraphScopepath
graph?: GraphScopegraph
includeProvenance?: GraphScopeincludeProvenance
limit?: GraphPagelimit
offset?: GraphPageoffset
page?: GraphPagepage
edges?: GraphEdgeedges
confidence?: GraphEdgeconfidence
target?: (string | string[])
to?: string
symbolName?: string
mainGoal?: string
reasoning?: string
}
export interface GraphQueryUnresolved {
operation: "unresolved"
path?: GraphScopepath
graph?: GraphScopegraph
includeProvenance?: GraphScopeincludeProvenance
limit?: GraphPagelimit
offset?: GraphPageoffset
page?: GraphPagepage
target?: (string | string[])
edges?: GraphEdgeedges
symbolName?: string
/**
 * @minItems 1
 * @maxItems 16
 */
reasons?: ("external-package" | "not-declared-in-repo" | "type-qualified-unmatched" | "member-unknown-receiver-in-repo" | "ambiguous" | "other" | "local-binding" | "recovered-declaration" | "constructor-target-unavailable" | "constructor-binding-not-assessed" | "unsupported-constructor-form" | "recovered-occurrence" | "scope-not-assessed" | "binding-unrepresented" | "unsupported-argument-form" | "unsupported-argument-group")[]
mainGoal?: string
reasoning?: string
}
export interface GraphQueryStale {
operation: "stale"
path?: GraphScopepath
graph?: GraphScopegraph
includeProvenance?: GraphScopeincludeProvenance
limit?: GraphPagelimit
offset?: GraphPageoffset
page?: GraphPagepage
mainGoal?: string
reasoning?: string
}
export interface GraphQueryIssues {
operation: "issues"
path?: GraphScopepath
graph?: GraphScopegraph
includeProvenance?: GraphScopeincludeProvenance
limit?: GraphPagelimit
offset?: GraphPageoffset
page?: GraphPagepage
list?: string
/**
 * @minItems 1
 * @maxItems 11
 */
detectors?: ("cycle" | "dir-cycle" | "unreachable-file" | "test-only" | "unused-export" | "export-only-local" | "undeclared-dependency" | "boundary-violation" | "unresolved-import" | "critical-file" | "single-point-of-failure")[]
/**
 * @minItems 1
 * @maxItems 100
 */
entries?: string[]
baseline?: string
minScore?: number
minTier?: (100 | 90 | 60)
mainGoal?: string
reasoning?: string
}
export interface GraphQueryImpactTarget {
operation: "impact"
path?: GraphScopepath
graph?: GraphScopegraph
includeProvenance?: GraphScopeincludeProvenance
limit?: GraphPagelimit
offset?: GraphPageoffset
page?: GraphPagepage
kind?: ("file" | "symbol" | "package")
list?: string
target: string
/**
 * @minItems 1
 * @maxItems 100
 */
changed?: string[]
since?: string
depth?: number
mainGoal?: string
reasoning?: string
}
export interface GraphQueryImpactChanged {
operation: "impact"
path?: GraphScopepath
graph?: GraphScopegraph
includeProvenance?: GraphScopeincludeProvenance
limit?: GraphPagelimit
offset?: GraphPageoffset
page?: GraphPagepage
kind?: ("file" | "symbol" | "package")
list?: string
target?: {
[k: string]: unknown
}
/**
 * @minItems 1
 * @maxItems 100
 */
changed: string[]
since?: string
depth?: number
mainGoal?: string
reasoning?: string
}
export interface GraphQueryImpactSince {
operation: "impact"
path?: GraphScopepath
graph?: GraphScopegraph
includeProvenance?: GraphScopeincludeProvenance
limit?: GraphPagelimit
offset?: GraphPageoffset
page?: GraphPagepage
kind?: ("file" | "symbol" | "package")
list?: string
target?: {
[k: string]: unknown
}
changed?: {
[k: string]: unknown
}
since: string
depth?: number
mainGoal?: string
reasoning?: string
}
export interface GraphQueryDrift {
operation: "drift"
path?: GraphScopepath
graph?: GraphScopegraph
includeProvenance?: GraphScopeincludeProvenance
limit?: GraphPagelimit
offset?: GraphPageoffset
page?: GraphPagepage
list?: string
baseline: string
mainGoal?: string
reasoning?: string
}
export interface GraphQueryInputCursor {
cursor: string
}
export interface GraphQueryOutputResults {
warnings?: string[]
oversized?: true
next?: GraphQueryContinueCall
expiresAt?: string
replayed?: true
graph?: {
id: string
root?: string
createdAt?: string
isPartial?: boolean
partialReasons?: PartialReason[]
}
freshness?: {
fresh: boolean
analysisCurrent: boolean
}
results: {
index: number
meta?: {
evidence: {
kind: ("exact" | "lexical" | "structural" | "syntactic" | "semantic" | "provider")
confidence: ("low" | "medium" | "high")
[k: string]: unknown
}
diagnostics?: {
codes?: string[]
hints?: string[]
partial?: boolean
[k: string]: unknown
}
[k: string]: unknown
}
data: GraphQueryData
status?: RowStatus
rowPart?: {
part: number
of: number
continues?: {
array: string
index: number
}
text?: string
}
[k: string]: unknown
}[]
shared?: {
data: {
owner?: string
repo?: string
ref?: string
commitSha?: string
root?: string
workspaceRoot?: string
lsp?: {
[k: string]: unknown
}
partialReasons?: PartialReason[]
coverage?: {
[k: string]: unknown
}
}
}
hints?: RowHints
related?: RelatedCall[]
error?: {
[k: string]: unknown
}
errorCode?: {
[k: string]: unknown
}
errorClass?: {
[k: string]: unknown
}
details?: {
[k: string]: unknown
}
}
export interface GraphQueryContinueCall {
kind?: "continue"
tool: "graphQuery"
input: {
cursor: string
}
}
export interface GraphQueryNext {
[k: string]: GraphQueryContinueCall
}
export interface OutputSourceEvidence {
snapshots: OutputSourceSnapshot[]
/**
 * @minItems 1
 */
representations: OutputTextRepresentation[]
/**
 * @minItems 1
 */
units: OutputAnalysisUnit[]
}
export interface OutputTextRepresentation {
id: string
sourceDigest: string
byteLength: number
producer: OutputSourceProducer
sourceIds: string[]
}
export interface OutputSourceProducer {
id: string
version: string
}
export interface OutputAnalysisUnit {
id: string
adapter: OutputSourceProducer
operation: AnalysisOperation
representationId: string
/**
 * @minItems 1
 */
regions: OutputSourceByteRange[]
}
export interface OutputSourceByteRange {
startByte: number
endByte: number
}
export interface OutputGraphImpactRow {
id: string
depth: number
risk: ("changed" | "direct" | "likely" | "transitive" | "candidate")
confidence: ("high" | "medium" | "low")
packageDistance?: number
from?: string
edgeLine?: number
via: string
typesOnly?: true
roles?: string[]
config?: string
}
export interface OutputGraphResultRow {
sourceDigest?: string
sourceRange?: (OutputSourceByteRange | null)
sourceOrigin?: (OutputGraphSourceOrigin | null)
occurrenceId?: string
selectionByteRange?: OutputSourceByteRange
selectionOrigin?: OutputGraphSourceOrigin
parseAssessment?: OutputParseAssessment
testIdentity?: OutputTestIdentity
path?: string
category?: ("file" | "cycle" | "transition" | "relation")
change?: ("added" | "removed" | "changed" | "resolved" | "staticToDynamic" | "dynamicToStatic")
from?: string
direction?: ("in" | "out")
to?: string
edge?: string
sites?: number
pageRank?: number
commits?: number
share?: number
linked?: boolean
nodes?: unknown[]
files?: unknown[]
candidates?: unknown[]
evidence?: (string | {
members?: unknown[]
witness?: unknown[]
suggestedCuts?: unknown[]
dirs?: unknown[]
exports?: unknown[]
sites?: unknown[]
mentionedIn?: (unknown[] | {
[k: string]: unknown[]
})
textMentions?: {
[k: string]: unknown[]
}
[k: string]: unknown
})
[k: string]: unknown
}
export interface OutputSourceOrigin {
analysisUnitId: string
/**
 * @minItems 1
 */
segments: OutputOriginSegment[]
}
export interface OutputOriginSegment {
sourceRange: OutputSourceByteRange
analysis: ("included" | "excluded")
/**
 * @minItems 1
 */
branches: OutputOriginBranch[]
}
export interface OutputOriginalSourceRange {
sourceId: string
rawRange: OutputSourceByteRange
}
export interface OutputGraphNodeEvidence {
sourceDigest?: string
sourceRange?: OutputSourceByteRange
sourceOrigin?: OutputGraphSourceOrigin
occurrenceId?: string
selectionByteRange?: OutputSourceByteRange
selectionOrigin?: OutputGraphSourceOrigin
parseAssessment?: OutputParseAssessment
testIdentity?: OutputTestIdentity
id?: string
kind?: string
symbolName?: string
language?: string
roles?: string[]
path?: string
line?: number
endLine?: number
exported?: boolean
test?: boolean
ecosystem?: string
parent?: string
in?: {
[k: string]: number
}
out?: {
[k: string]: number
}
next?: {
[k: string]: unknown
}[]
[k: string]: unknown
}
export interface GraphQueryOutputError {
error: string
errorCode: ErrorCode
errorClass?: ErrorClass
details?: string[]
hints?: RowHints
related?: RelatedCall[]
}
