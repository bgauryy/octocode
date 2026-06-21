/**
 * Human/agent-readable OQL schema description, served by
 * `octocode search --scheme`. This is the V1 contract surface; the canonical
 * language reference lives in docs/octocode-language/OCTOCODE_QUERY_LANGUAGE.md.
 */
import { DEFAULTS } from './defaults.js';
import { ACTIVE_TARGETS, RESERVED_TARGETS } from './types.js';

export const OQL_SCHEMA_DOC = {
  schema: 'oql/v1',
  description:
    'Use octocode search for bounded research over local paths and GitHub scopes: search code matches, file lists, directory trees, or exact/minified content; set from, scope, and where.kind; keep output small with view/select/controls; materialize only for bounded local proof; use --explain and follow next.* continuations when routing or paging is uncertain.',
  activeTargets: ACTIVE_TARGETS,
  reservedTargets: RESERVED_TARGETS,
  query: {
    schema: '"oql/v1" (inserted by normalization)',
    target: ACTIVE_TARGETS.join(' | '),
    from: '{ kind:"local", path } | { kind:"github", repo?, owner?, ref? } | { kind:"materialized", localPath, source? } | { kind:"npm" }',
    scope:
      '{ path?, language?, include?, exclude?, excludeDir?, hidden?, noIgnore?, maxDepth? }',
    where:
      'discriminated predicate: text | regex | structural | field | all | any | not (code/files only)',
    materialize:
      '{ mode:"never"|"auto"|"required", strategy?, allowFullRepo?, forceRefresh? }',
    fetch:
      '{ content?: { contentView:"exact"|"compact"|"symbols", range?:{startLine?,endLine?,contextLines?}, charOffset?, charLength? }, tree?: {...} }',
    params:
      'target-specific options for V2 targets (validated by the backing tool) — see params hints below',
    select: 'string[] projection of result/continuation fields',
    view: 'discovery | paginated | detailed',
    controls: '{ search?: {...}, budget?: {...} }',
    limit: 'number',
    page: 'number',
    itemsPerPage: 'number',
    explain: 'boolean',
  },
  // Per-target `params` for V2 targets (full schema: `tools <name> --scheme`).
  params: {
    semantics:
      '{ type:"definition"|"references"|"callers"|"callees"|"callHierarchy"|"hover"|"documentSymbols"|"typeDefinition"|"implementation", symbolName?, lineHint?, orderHint?, depth?, includeDeclaration?, groupByFile?, format? } — backing tool lspGetSemantics',
    repositories:
      '{ keywords?, topicsToSearch?, language?, owner?, stars?, license?, sort?, archived?, limit?, page? } — backing tool ghSearchRepos',
    packages:
      '{ packageName | keywords, mode?:"lean"|"full", page? } — backing tool npmSearch',
    pullRequests:
      '{ state?:"open"|"closed"|"merged", author?, label?, keywordsToSearch?, prNumber?, reviewMode?, filePage?, commentPage?, commitPage?, limit?, page? } — backing tool ghHistoryResearch',
    commits:
      '{ path?, branch?, since?, until?, includeDiff?, limit?, page? } — backing tool ghHistoryResearch type:"commits"',
    artifacts:
      '{ mode:"inspect"|"list"|"extract"|"decompress"|"strings"|"unpack", minLength?, entryPageNumber?, scanOffset? } — backing tool localBinaryInspect',
    diff: '{ prNumber, files? } (PR patch via ghHistoryResearch) | { baseRef, headRef, path } (direct two-ref file diff via ghGetFileContent + local line diff); neither shape -> invalidQuery repair',
    research:
      '{ goal?, intent?:"general"|"reachability"|"dependencies"|"symbols", facets?, mode?:"plan"|"analyze", maxFiles? } — smart internal research flow over a complete local/materialized corpus; uses files/manifests/import graph now and is designed to refine with AST + LSP evidence',
    materialize:
      '(no params; no `where`) clone/cache a bounded corpus (from:{kind:"github",repo} + scope.path) and return a stable materialized checkpoint row (localPath/repoRoot/ref/cache/complete) with next.structure/next.files',
  },
  predicates: {
    text: '{ kind:"text", value, case?, wholeWord? }',
    regex:
      '{ kind:"regex", value, dialect?:"rust"|"pcre2"|"provider", case?, wholeWord?, multiline?, dotAll? }',
    structural:
      '{ kind:"structural", lang, pattern? | rule? } (exactly one of pattern/rule)',
    field:
      '{ kind:"field", field:"path"|"basename"|"extension"|"size"|"modified"|"entryType", op, value? }',
    boolean:
      '{ kind:"all"|"any", of: Predicate[] } | { kind:"not", predicate }',
  },
  batch: {
    queries: 'OqlQuery[] (1-5)',
    combine: 'independent | merge',
  },
  defaults: DEFAULTS,
} as const;

export function oqlSchemaText(): string {
  return JSON.stringify(OQL_SCHEMA_DOC, null, 2);
}
