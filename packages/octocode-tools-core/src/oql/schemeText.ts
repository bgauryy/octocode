/**
 * Human/agent-readable OQL schema description, served by
 * `octocode search --scheme`. This is the current contract surface; the canonical
 * language reference lives in docs/octocode-language/OCTOCODE_QUERY_LANGUAGE.md.
 */
import { DEFAULTS } from './defaults.js';
import { ACTIVE_TARGETS, RESERVED_TARGETS } from './types.js';

export const OQL_SCHEMA_DOC = {
  schema: 'oql',
  description:
    'Use octocode search for bounded research over local paths and GitHub scopes: search code matches, file lists, directory trees, or exact/minified content; set from, scope, and where.kind; keep output small with view/select/controls; materialize only for bounded local proof; use --explain and follow next.* continuations when routing or paging is uncertain. Run `octocode search --scheme` to print this schema before writing JSON queries.',
  activeTargets: ACTIVE_TARGETS,
  reservedTargets: RESERVED_TARGETS,
  // ── Quick-start recipes — copy-paste these, swap the path/text ──────────
  quickStart: {
    'text search (local)': 'search "functionName" ./src',
    'text search (GitHub)': 'search "functionName" facebook/react',
    'structural AST (local — needs full node shape)':
      'search --pattern "function $NAME($$$ARGS) { $$$BODY }" ./src --lang ts',
    'structural AST (GitHub — clones bounded subtree)':
      'search --pattern "function $NAME($$$ARGS) { $$$BODY }" facebook/react/packages --lang js --materialize auto',
    'dead-code triage (research)':
      'search --query \'{"schema":"oql","target":"research","from":{"kind":"local","path":"./src"},"params":{"intent":"reachability","facets":["symbols","files"]},"itemsPerPage":1,"page":1}\'',
    'LSP-proven dead symbols (graph)':
      'search --query \'{"schema":"oql","target":"graph","from":{"kind":"local","path":"./src"},"params":{"intent":"reachability","facets":["symbols"],"proof":"lsp","proofLimit":5,"includePackets":true},"page":1,"itemsPerPage":10}\'',
    'OQL full-schema reference': 'search --scheme',
    'routing explanation before running':
      'search --explain --query \'{"target":"code","from":{"kind":"local","path":"./src"},"where":{"kind":"text","value":"term"}}\'',
  },
  evidenceSemantics: {
    'answerReady:true':
      'The envelope answers the query as asked. No required follow-up.',
    'answerReady:false':
      'More proof work remains. Follow next.* continuations for additional pages, LSP proof, or content reads. This is NOT a failure signal — it means the result is partial or candidate-grade.',
    'complete:false':
      'Required pages or proof steps are still outstanding. Page with next.page or follow next.semantic before making deletion or absence claims.',
    'kind:proof': 'Backend evaluated the request exactly.',
    'kind:partial': 'Truncation, pagination, or residual checks remain.',
    'kind:candidate':
      'Useful evidence — not final proof. research/graph targets always return candidate; upgrade with next.semantic/next.search/next.fetch.',
    'kind:unsupported': 'OQL could not safely execute the requested semantics.',
    'proofStatus:confirmed-by-lsp':
      'LSP found zero references to this symbol — safe to inspect further for deletion.',
    'proofStatus:conflicting-evidence':
      'LSP found references — the symbol IS retained by other code. Check retainedBy edges before acting.',
    'proofStatus:needs-framework-graph':
      'Symbol may be an entrypoint (framework, export, dynamic import). LSP alone cannot prove reachability.',
  },
  query: {
    schema: '"oql" (inserted by normalization)',
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
      'target-specific options (validated by OQL for common fields and by the backing tool exhaustively) — see params hints below',
    select: 'string[] projection of result/continuation fields',
    view: 'discovery | paginated | detailed',
    controls: '{ search?: {...}, budget?: {...} }',
    limit: 'number',
    page: 'number',
    itemsPerPage: 'number',
    explain: 'boolean',
  },
  // Per-target `params` hints (full schema: `tools <name> --scheme`).
  params: {
    semantics:
      '{ type:"definition"|"references"|"callers"|"callees"|"callHierarchy"|"hover"|"documentSymbols"|"typeDefinition"|"implementation"|"workspaceSymbol"|"supertypes"|"subtypes"|"diagnostic", uri?, symbolName?, lineHint?, orderHint?, depth?, includeDeclaration?, groupByFile?, workspaceRoot?, format? } — backing tool lspGetSemantics',
    repositories:
      '{ keywords?: string[], topicsToSearch?: string[], language?, owner?, stars?, license?, sort?, archived?, limit?, page? } — backing tool ghSearchRepos; keywords/topicsToSearch are arrays even for one term',
    packages:
      '{ packageName?: string | keywords?: string[], mode?:"lean"|"full", page? } — backing tool npmSearch',
    pullRequests:
      '{ state?:"open"|"closed"|"merged", author?, label?, keywordsToSearch?, prNumber?, reviewMode?, filePage?, commentPage?, commitPage?, limit?, page?, matchString?, matchScope?:"body"|"title"|"comments"|"reviews"|"all" } — backing tool ghHistoryResearch; matchString is an OQL content filter over fetched PR text (default scope body), not a search-index query — no match → zeroMatches',
    commits:
      '{ path?, branch?, since?, until?, includeDiff?, limit?, page?, filePage?, itemsPerPage? } — backing tool ghHistoryResearch type:"commits"; repo/directory diffs page changed files per commit with filePage/itemsPerPage',
    artifacts:
      '{ mode:"inspect"|"list"|"extract"|"decompress"|"strings"|"unpack", minLength?, entryPageNumber?, scanOffset? } — backing tool localBinaryInspect',
    diff: '{ prNumber, files? } (PR patch via ghHistoryResearch) | { baseRef, headRef, path } (direct two-ref file diff via ghGetFileContent + local line diff); neither shape -> invalidQuery repair',
    research:
      '{ goal?, intent?:"general"|"reachability"|"dependencies"|"symbols", facets?:("symbols"|"files"|"dependencies"|"relations")[], mode?:"plan"|"analyze"|"prove", maxFiles? } — TWO-PHASE WORKFLOW: (1) page:1 + itemsPerPage:1 gets data.summary (full-scope counts: sourceFiles, unusedFiles, exportedSymbols, candidateUnusedExports…) WITHOUT a bulk payload — read the summary first. (2) page:2..N with larger itemsPerPage pages through data.packets[] (individual candidates). Packets carry retainedBy edges (what keeps the symbol alive) plus packet-level next.fetch/next.semantic/next.search continuations. The research result row carries a pre-filled next.graph continuation for the current packet page. Results are always evidence.kind:"candidate" — answerReady:false is expected and normal, not a failure. Follow the result row\'s next.graph (pre-filled proof:"lsp", bounded by proofLimit) to upgrade a page of candidates to LSP-proven proofStatus verdicts.',
    graph:
      '{ goal?, intent?:"general"|"reachability"|"dependencies"|"symbols", facets?:("symbols"|"files"|"dependencies"|"relations")[], mode?:"plan"|"analyze"|"prove", maxFiles?, subject?, subjectKind?, relation?, verdict?, direction?:"incoming"|"outgoing"|"both", proof?:"none"|"lsp", proofLimit?, includePackets?, includeFacts?, includeEdges? } — UPGRADE PATH FROM research: take the next.graph from a research result (it is pre-filled and page-aligned) and run it directly — no manual JSON construction needed. proof:"lsp" runs bounded LSP reference counts for current-page symbol packets and sets proofStatus per row: "confirmed-by-lsp" (refs=0 → safe to inspect for deletion), "conflicting-evidence" (refs>0 → symbol IS retained, check retainedBy before acting), "needs-framework-graph" (may be an entrypoint — LSP alone cannot prove reachability). Rows without LSP proof emit their own next.graph to upgrade the current page. answerReady:false is expected on both research and graph — always follow next.* to get more pages or upgrade evidence.',
    materialize:
      '(no params; no `where`) clone/cache a bounded corpus (from:{kind:"github",repo} + scope.path) and return a stable materialized checkpoint row (localPath/repoRoot/ref/cache/complete) with next.structure/next.files',
  },
  predicates: {
    text: '{ kind:"text", value, case?, wholeWord? }',
    regex:
      '{ kind:"regex", value, dialect?:"rust"|"pcre2"|"provider", case?, wholeWord?, multiline?, dotAll? }',
    structural:
      '{ kind:"structural", lang, pattern? | rule? } (exactly one of pattern/rule) — IMPORTANT: pattern must match a complete AST node; a function pattern usually needs a body ({ $$$BODY }), return type, and any required syntax. Partial patterns produce partialParse/zeroMatches — use a relational rule instead for partial or relational matches.',
    field:
      '{ kind:"field", field:"path"|"basename"|"extension"|"size"|"modified"|"entryType", op:"="|"!="|"in"|"exists"|"glob"|"regex"|">"|">="|"<"|"<="|"within", value? } (use symbolic ops like "="; aliases such as "eq" are invalid)',
    boolean:
      '{ kind:"all"|"any", of: Predicate[] } | { kind:"not", predicate }',
  },
  batch: {
    queries: 'OqlQuery[] (1-5)',
    combine: 'independent | merge',
  },
  explainRoutes: {
    PUSHDOWN:
      'Backend evaluates this predicate exactly — good. No residual work.',
    RESIDUAL:
      'Backend narrows candidates but OQL must finish evaluation locally.',
    ROUTE: 'OQL must use a different lane, often materialization.',
    UNSUPPORTED:
      'OQL cannot execute this predicate safely on the chosen source.',
  },
  defaults: DEFAULTS,
} as const;

export function oqlSchemaText(): string {
  return JSON.stringify(OQL_SCHEMA_DOC, null, 2);
}
