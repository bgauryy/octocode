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
    'octocode search — typed read-only queries over code, files, symbols, repos, packages, history, diffs, and graph evidence. Think source + answer type + filters + read/output options; use --explain for routing; follow next.* for pages, exact reads, materialization, or proof.',
  activeTargets: ACTIVE_TARGETS,
  reservedTargets: RESERVED_TARGETS,
  sourceGuide: {
    local:
      'from:{kind:"local",path:"./src"}; shorthand: search "term" ./src, search ./src --tree, search file.ts --op documentSymbols. Best for local code/content/files/tree/LSP/diff.',
    github:
      'from:{kind:"github",repo:"owner/repo",ref?}; shorthand: search "term" owner/repo, search owner/repo --tree, search owner/repo#123 --target pullRequests. GitHub code is indexed/default-branch scoped; materialize for AST/LSP/local proof.',
    npm: 'from:{kind:"npm"} with target:"packages"; shorthand: search zod --target packages. Use to resolve package metadata and source repo, then continue with GitHub or local/materialized proof.',
    materialized:
      'from:{kind:"materialized",localPath:"/abs/path"} after target:"materialize", clone, or cache fetch. Use when a remote repo/subtree must behave like local code.',
  },
  plainLanguage: {
    source:
      '`from` = where to look: local path, GitHub repo, npm, or a materialized checkout.',
    answerType:
      '`target` = what kind of answer you want: code matches, file content, tree, files, symbols, repos, packages, PRs, commits, diffs, research packets, graph proof, or materialization.',
    filters:
      '`where` = match/filter conditions for code and file discovery only: text, regex, AST shape, file fields, and boolean combinations.',
    readOptions:
      '`fetch` = what to read once you know the file/tree: exact content, compact content, symbol outline, ranges, match slices, or tree depth.',
    targetOptions:
      '`params` = options that belong to one answer type, such as LSP operation, PR number, package name, research intent, or graph proof.',
    output:
      '`view`, `select`, and `controls` = response shape, projected fields, search tuning, and cost limits.',
    paging:
      "`page` + `itemsPerPage` page the target's primary result domain. For code search that may be matched files; per-file match paging uses `controls.search.matchPage` / `--match-page`.",
  },
  // ── Task → target router: pick the target first, then add filters/options ──
  targetDecisionTree: {
    'find a string/pattern in code':
      'target:code (shorthand default for text/regex/--pattern/--rule; needs a where filter)',
    'read or slice a known file':
      'target:content (file positional + --content-view, --start-line/--end-line, or --match-string)',
    'see the file/dir layout': 'target:structure (--tree)',
    'find files by name/glob/ext/size/mtime':
      'target:files (--search path, --name/--ext/--size-greater/--modified-within)',
    'symbol defs/refs/callers/hover':
      'target:semantics (--op …; local or materialized only)',
    'search GitHub repos': 'target:repositories (--stars/--lang/--topic)',
    'resolve an npm package': 'target:packages',
    'PR list or deep-read': 'target:pullRequests (owner/repo#N)',
    'commit history': 'target:commits (--since/--until)',
    'a diff':
      'target:diff ({prNumber} for a PR patch, {baseRef,headRef,path} for two refs)',
    'dead-code / reachability sweep':
      'target:research, then upgrade with target:graph proof:"lsp"',
    'make a remote repo behave like local (AST/LSP/negation)':
      'target:materialize (or clone / cache fetch)',
  },
  agentBestPractices: [
    'Start with cheap orientation: --tree, --search path, --view discovery, or --content-view symbols.',
    'Then narrow and read exact evidence: --match-string, --start-line/--end-line, --char-offset/--char-length, or --content-view none.',
    'Use snippets as discovery only; make decisions from exact content, PR/commit metadata, or LSP/graph proof.',
    'For semantics, run documentSymbols first to get line anchors, then references/callers/hover with symbolName + lineHint.',
    'For GitHub zero rows / providerUnindexed, do NOT claim absence — follow evidenceSemantics.providerUnindexed (verify path with structure, then materialize a bounded path).',
    'Read evidence.answerReady, evidence.complete, diagnostics, pagination, and next.* before concluding.',
  ],
  // ── Quick-start recipes — copy-paste these, swap the path/text ──────────
  quickStart: {
    'text search (local)': 'search "functionName" ./src',
    'text search (GitHub)': 'search "functionName" vercel/next.js',
    'package lookup (npm)': 'search zod --target packages',
    'PR deep read (GitHub)':
      'search vercel/next.js#1 --target pullRequests --comments --patches',
    'commit history (GitHub)':
      'search vercel/next.js/packages/next/src --target commits --since 2024-01-01T00:00:00Z',
    'browse a tree (local dir or owner/repo)':
      'search ./src --tree   |   search vercel/next.js --tree',
    'read a file (local or owner/repo/path)':
      'search ./src/index.ts   |   search vercel/next.js/packages/next/src/server/config.ts',
    'read a remote file (exact)':
      'search vercel/next.js/README.md --content-view none',
    'semantics (local/materialized)':
      'search ./src/index.ts --op documentSymbols   |   search ./src/index.ts --op references --symbol runCLI --line 42',
    'PR diff (GitHub)':
      'search vercel/next.js#123 --target diff   |   search vercel/next.js --target diff --pr 123',
    'two-ref / two-file diff':
      'search src/a.ts src/b.ts --target diff   |   search owner/repo --target diff --base-ref <sha> --head-ref <sha> --path <file>',
    'structural AST (local — needs full node shape)':
      'search --pattern "function $NAME($$$ARGS) { $$$BODY }" ./src --lang ts',
    'structural AST (GitHub — clones bounded subtree)':
      'search --pattern "function $NAME($$$ARGS) { $$$BODY }" vercel/next.js/packages/next/src --lang ts --materialize auto',
    'GitHub index miss recovery':
      'search useState packages/next/src --repo vercel/next.js --materialize required   |   clone vercel/next.js/packages/next/src   |   cache fetch vercel/next.js packages/next/src --depth tree',
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
      'Normal, NOT a failure — the results above are valid; only answerReady:true means no follow-up is needed. Follow next.* for more pages, LSP proof, or content.',
    'complete:false':
      'Pages/proof/slices may remain. Read diagnostics: non-blocking warnings can still leave usable rows, but deletion/absence claims need the requested scope plus the listed continuations.',
    'kind:proof': 'Backend evaluated the request exactly.',
    'kind:partial': 'Truncation, pagination, or residual checks remain.',
    'kind:candidate':
      'Useful evidence, not proof. research/graph are always candidate — upgrade via next.semantic/search/fetch.',
    'kind:unsupported': 'OQL could not safely execute the requested semantics.',
    'proofStatus:confirmed-by-lsp':
      'LSP refs=0 inside the bounded workspace. Inspect for deletion only after checking entrypoints, framework conventions, dynamic imports, package exports, and scripts.',
    'proofStatus:conflicting-evidence':
      'LSP refs>0 — symbol IS retained; check retainedBy before acting.',
    'proofStatus:needs-framework-graph':
      "Maybe an entrypoint (framework/export/dynamic import) — LSP alone can't prove reachability.",
    'proofStatus:candidate':
      'Pre-proof state (no LSP run yet) — run the row\'s next.graph (proof:"lsp") to resolve it to confirmed-by-lsp / conflicting-evidence / needs-framework-graph.',
    partialParse:
      'Non-fatal structural-search warning. Some files were not parsed, often because a literal prefilter had no anchor; add a literal/rule or broaden proof before claiming absence.',
    providerUnindexed:
      'GitHub provider returned zero rows. This is NOT absence. Verify the path with structure, then use bounded local proof: search "term" path --repo owner/repo --materialize required, clone owner/repo[/path], or cache fetch owner/repo [path] --depth file|tree|clone.',
  },
  // LSP op distinctions agents most often confuse. documentSymbols is the
  // anchor step; references ≠ callers.
  semanticsGuide: {
    documentSymbols:
      "list a file's symbols to get line anchors first (run before references/callers/hover)",
    references:
      'all usages of a symbol across the workspace (often includes its declaration)',
    callers:
      'incoming calls to a function/method (call sites only) — narrower than references',
    hover: 'type, signature, and docs at a symbol',
  },
  query: {
    schema: '"oql" (inserted by normalization)',
    target: ACTIVE_TARGETS.join(' | '),
    from: '{ kind:"local", path } | { kind:"github", repo?, owner?, ref? } | { kind:"materialized", localPath, source? } | { kind:"npm" } — local row.path is relative to from.path; the pre-filled next.fetch carries the resolved ABSOLUTE path, so follow it directly rather than re-joining paths yourself',
    scope:
      '{ path?, language?, include?, exclude?, excludeDir?, hidden?, noIgnore?, minDepth?, maxDepth? } — minDepth/maxDepth bound directory recursion depth (0-64)',
    where:
      'filters for code/files only: text | regex | structural | field | all | any | not. To read a matched file slice, use fetch.content.match. For PR/commit text narrowing, use that target params hint.',
    materialize:
      '{ mode:"never"|"auto"|"required", strategy?:"file"|"tree"|"subtree"|"repo", allowFullRepo?, forceRefresh? }',
    fetch:
      '{ content?: { contentView:"none"|"standard"|"symbols", fullContent?, match?:{text|regex,case?}, range?:{startLine?,endLine?,contextLines?}, charOffset?, charLength? }, tree?:{ maxDepth?, pattern?, includeSizes?, extensions?, filesOnly?, directoriesOnly?, sortBy?:"name"|"size"|"time"|"extension", reverse? } } — read options for known files/trees; fetch.content.fullContent:true returns the WHOLE file in one shot (lossless, no char-window paging); to read the region around a string, anchor with fetch.content.match (NOT a top-level where, which is code/files only)',
    params:
      'target options (validated by OQL for common fields and by the backing tool exhaustively) — see params hints below',
    select: 'string[] projection of result/continuation fields',
    view: 'discovery | paginated | detailed',
    controls:
      '{ search?: { countLinesPerFile?, countMatchesPerFile?, onlyMatching?, unique?, countUnique?, contextLines?, invertMatch?, matchWindow?, matchContentLength?, maxMatchesPerFile?, matchPage?, sort?:"relevance"|"matchCount"|"path"|"modified"|"accessed"|"created"|"size"|"name", sortReverse?, rankingProfile?, debugRanking? }, budget?: { maxFiles?, maxCandidates?, maxBytes?, maxMaterializedBytes?, maxPlanNodes?, maxBooleanExpansion?, timeoutMs? } } — output/cost controls; sort values "size"/"name" apply to target:"files" only (lowered to localFindFiles sortBy), the rest are code-search sorts',
    limit:
      'number — total result cap where supported. Prefer itemsPerPage for paged research/graph/file-history continuations.',
    page: 'number — top-level page number for OQL windowing/continuations',
    itemsPerPage:
      'number — page size for the target primary result domain. For code search this may be matched files, not individual matches; per-file match paging uses controls.search.matchPage. Per-target params expose backing-tool sub-pages only (filePage/commentPage/commitPage, etc.).',
    explain: 'boolean',
  },
  // Per-target `params` hints (full schema: `tools <name> --scheme`).
  params: {
    semantics:
      '{ type:"definition"|"references"|"callers"|"callees"|"callHierarchy"|"hover"|"documentSymbols"|"typeDefinition"|"implementation"|"workspaceSymbol"|"supertypes"|"subtypes"|"diagnostic", uri?, symbolName?, symbolKind?, lineHint?, orderHint?, depth?, contextLines?, includeDeclaration?, groupByFile?, workspaceRoot?, format? } — backing tool lspGetSemantics; contextLines adds call-flow snippets; symbolKind filters returned symbol rows after documentSymbols/workspaceSymbol',
    repositories:
      '{ keywords?: string[], topicsToSearch?: string[], language?, owner?, stars?, license?, sort?, archived?, limit?, page? } — backing tool ghSearchRepos; keywords/topicsToSearch are arrays even for one term',
    packages:
      '{ packageName?: string | keywords?: string[], mode?:"lean"|"full", page? } — backing tool npmSearch',
    pullRequests:
      '{ state?:"open"|"closed"|"merged", author?, label?, keywordsToSearch?, prNumber?, reviewMode?, filePage?, commentPage?, commitPage?, limit?, page?, matchString?, matchScope?:"body"|"title"|"comments"|"reviews"|"all", content? } — backing tool ghHistoryResearch; matchString filters fetched PR title/body/comments/reviews per matchScope (default body), not a search-index query — no match → zeroMatches',
    commits:
      '{ path?, branch?, since?, until?, includeDiff?, limit?, page?, filePage?, itemsPerPage?, matchString? } — backing tool ghHistoryResearch type:"commits"; matchString filters commit messages; repo/directory diffs page changed files per commit with filePage/itemsPerPage',
    diff: '{ prNumber, files? } (PR patch via ghHistoryResearch) | { baseRef, headRef, path } (direct two-ref file diff via ghGetFileContent + local line diff); neither shape -> invalidQuery repair',
    research:
      '{ goal?, intent?:"general"|"reachability"|"dependencies"|"symbols", facets?:("symbols"|"files"|"dependencies"|"relations")[], mode?:"plan"|"analyze"|"prove", maxFiles? } — TWO-PHASE: page:1+itemsPerPage:1 → data.summary (full-scope counts) and may include a bounded first packet page; page:2+ → data.packets[] continuation pages (candidates w/ retainedBy edges + per-packet next.*). Always evidence:"candidate"/answerReady:false (normal). Follow the row\'s pre-filled next.graph (proof:"lsp", proofLimit-bounded) to upgrade a page to LSP-proven proofStatus.',
    graph:
      '{ goal?, intent?:"general"|"reachability"|"dependencies"|"symbols", facets?:(…)[], mode?:"plan"|"analyze"|"prove", maxFiles?, subject?, subjectKind?, relation?, verdict?, direction?:"incoming"|"outgoing"|"both", proof?:"none"|"lsp", proofLimit?, includePackets?, includeFacts?, includeEdges? } — UPGRADE PATH: run a research row\'s pre-filled next.graph directly. proof:"lsp" sets per-row proofStatus: "confirmed-by-lsp" (refs=0 in bounded workspace; still inspect entrypoints/exports before deletion), "conflicting-evidence" (refs>0 → retained, check retainedBy), "needs-framework-graph" (maybe an entrypoint). answerReady:false is normal — follow next.* for more pages/proof.',
    materialize:
      '(no params; no `where`) clone/cache a bounded corpus (from:{kind:"github",repo} + scope.path) and return a stable materialized checkpoint row (localPath/repoRoot/ref/cache/complete) with next.structure/next.files. Use after GitHub providerUnindexed; for CLI alternatives use clone owner/repo[/path] or cache fetch owner/repo [path] --depth file|tree|clone.',
  },
  predicates: {
    text: '{ kind:"text", value, case?, wholeWord? }',
    regex:
      '{ kind:"regex", value, dialect?:"rust"|"pcre2"|"provider", case?, wholeWord?, multiline?, dotAll? }',
    structural:
      '{ kind:"structural", lang, pattern? | rule? } (exactly one; rule is a JSON object or grep-compatible YAML rule string) — pattern must match the COMPLETE node, so include the parts the real node has: a fn WITH a return type only matches if the pattern has one too (`function $N($$$A): $R { $$$B }`); omitting it returns 0. Shapes: `function $N($$$A) { $$$B }` (no-return-type fn), `($$$A) => $$$B` (arrow, block+expression), `$F($$$A)` (call), `$O.$M($$$A)` (method). For "find symbol X" the ROBUST form is a rule, not a pattern: `{ kind:"function_declaration", has:{ pattern:"X" } }`. 0 matches + no parse error = pattern shape ≠ real node (add `: $R`, or switch to a rule). Note: $$$-only patterns skip files with no literal anchor → low counts; add a literal name or use a regex where.',
    field:
      '{ kind:"field", field:"path"|"basename"|"extension"|"size"|"modified"|"accessed"|"empty"|"permissions"|"executable"|"readable"|"writable"|"entryType", op:"="|"!="|"in"|"exists"|"glob"|"regex"|">"|">="|"<"|"<="|"within"|"before", value? } (use symbolic ops like "="; aliases such as "eq" are invalid; there is no "contains" op — use op:"glob", value:"*term*" or op:"regex"; "within"/"before" compare modified/accessed times; empty/executable/readable/writable are boolean file attributes paired with op:"exists" or op:"=") SCOPE: field predicates evaluate file attributes, so they run on target:"files" (and mixed into files-lane booleans); target:"code" rejects a bare field predicate with unsupportedPredicate — use text/regex/structural for code content, or target:"files" for file discovery.',
    boolean:
      '{ kind:"all"|"any", of: Predicate[] } | { kind:"not", predicate }',
    booleanSugar:
      'Top-level sugar keys lower to canonical booleans at normalize time: and:[...]→all, or:[...]→any, noneOf:[...]→not(any), xor:[a,b]→any(all(a,not b),all(not a,b)), oneOf:[...]→exactly-one expansion (bounded by controls.budget.maxBooleanExpansion), invert:true→not(where). Prefer canonical all/any/not in programmatic queries; sugar is for terse hand-written ones.',
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

/**
 * Extract the first copy-paste shorthand from a sourceGuide entry. Shorthands
 * are comma-separated; the list ends at a sentence boundary (". "). Path dots
 * like `./src` are followed by a non-space, so they don't end the shorthand.
 */
function firstShorthand(guide: string): string | null {
  const marker = 'shorthand: ';
  const i = guide.indexOf(marker);
  if (i === -1) return null;
  const rest = guide.slice(i + marker.length).trim();
  let end = rest.length;
  const comma = rest.indexOf(',');
  if (comma !== -1) end = Math.min(end, comma);
  const sentenceEnd = rest.search(/\.\s/);
  if (sentenceEnd !== -1) end = Math.min(end, sentenceEnd);
  const shorthand = rest.slice(0, end).trim().replace(/\.$/, '');
  return shorthand || null;
}

type CompactTargetEntry = {
  target: string;
  task: string;
};

function compactTargetEntries(): CompactTargetEntry[] {
  const entries: CompactTargetEntry[] = [];
  const seenTargets = new Set<string>();
  for (const [task, route] of Object.entries(
    OQL_SCHEMA_DOC.targetDecisionTree
  )) {
    for (const match of route.matchAll(/target:"?(\w+)"?/g)) {
      const target = match[1];
      if (!target || seenTargets.has(target)) continue;
      seenTargets.add(target);
      entries.push({ target, task });
    }
  }
  for (const target of OQL_SCHEMA_DOC.activeTargets) {
    if (seenTargets.has(target)) continue;
    seenTargets.add(target);
    entries.push({ target, task: 'advanced target; see full schema' });
  }
  return entries;
}

function compactSchemeDoc(): Record<string, unknown> {
  const d = OQL_SCHEMA_DOC;
  const sourceOrder = ['local', 'github', 'npm', 'materialized'] as const;
  const recipeKeys = [
    'text search (local)',
    'text search (GitHub)',
    'read a remote file (exact)',
    'package lookup (npm)',
    'semantics (local/materialized)',
  ];
  const quickStart = d.quickStart as Record<string, string>;
  const snippetRule = d.agentBestPractices.find(b => /snippet/i.test(b));
  return {
    schema: 'oql',
    kind: 'octocode.search.compactScheme',
    description: d.description,
    sources: Object.fromEntries(
      sourceOrder.map(key => [
        key,
        firstShorthand(d.sourceGuide[key]) ??
          'a prior clone / cache fetch / materialize localPath',
      ])
    ),
    targets: compactTargetEntries(),
    recipes: Object.fromEntries(
      recipeKeys
        .map(key => [key, quickStart[key]])
        .filter(
          (entry): entry is [string, string] => typeof entry[1] === 'string'
        )
    ),
    semantics: d.semanticsGuide,
    proof: {
      snippets: snippetRule,
      providerUnindexed: d.evidenceSemantics.providerUnindexed,
    },
    commands: {
      explain: "search --explain --query '{...}'",
      fullSchema: 'search --scheme',
    },
  };
}

export function oqlCompactSchemeJson(): string {
  return JSON.stringify(compactSchemeDoc(), null, 2);
}

/**
 * Lean, agent-facing summary of the OQL contract, served by
 * `octocode search --scheme --compact`. Every section is DERIVED from
 * `OQL_SCHEMA_DOC` (sourceGuide, targetDecisionTree, quickStart, semanticsGuide,
 * agentBestPractices, evidenceSemantics) so it can never drift from the full
 * `--scheme`. Cold agents read this; advanced flows stay in the full schema.
 */
export function oqlCompactSchemeText(): string {
  const d = OQL_SCHEMA_DOC;
  const lines: string[] = [
    'octocode search — compact agent guide (full reference: search --scheme)',
    '',
    'SOURCE — where to look (choose one):',
  ];

  const sourceOrder = ['local', 'github', 'npm', 'materialized'] as const;
  for (const key of sourceOrder) {
    const sh = firstShorthand(d.sourceGuide[key]);
    lines.push(
      `  ${key.padEnd(13)}${sh ?? 'a prior clone / cache fetch / materialize localPath'}`
    );
  }

  lines.push('', 'TARGET — answer type (--target, or inferred from the args):');
  for (const { target, task } of compactTargetEntries()) {
    lines.push(`  ${target.padEnd(13)}${task}`);
  }

  lines.push('', 'COMMON RECIPES:');
  const recipeKeys = [
    'text search (local)',
    'text search (GitHub)',
    'read a remote file (exact)',
    'package lookup (npm)',
    'semantics (local/materialized)',
  ];
  const quickStart = d.quickStart as Record<string, string>;
  for (const key of recipeKeys) {
    const recipe = quickStart[key];
    if (recipe) lines.push(`  ${recipe}`);
  }

  lines.push('', 'LSP SEMANTICS (run documentSymbols first, then narrow):');
  for (const [op, desc] of Object.entries(d.semanticsGuide)) {
    lines.push(`  ${op.padEnd(16)}${desc}`);
  }

  lines.push('', 'PROOF — snippets are discovery, not proof:');
  const snippetRule = d.agentBestPractices.find(b => /snippet/i.test(b));
  if (snippetRule) lines.push(`  ${snippetRule}`);
  lines.push(`  ${d.evidenceSemantics.providerUnindexed}`);

  lines.push(
    '',
    "Routing debug: search --explain --query '{...}'",
    'Full schema:   search --scheme'
  );
  return lines.join('\n');
}
