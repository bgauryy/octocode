export const meta = {
  name: 'jev-sonnet-gated-v2',
  description: 'Gated + CoT variant: baseline vs (think-first, route-gate, call Jev ONLY on disputed_inference). 19 cases incl. 3 attractive-wrong-lead forks. Tests whether gating removes the forced-call tax while keeping fork wins.',
  phases: [
    { title: 'Research', detail: '19 cases x 2 arms (baseline vs gated-CoT-jev)' },
    { title: 'Judge', detail: 'blinded judge verifies at ref, scores both arms' },
  ],
}

const SKILL = '/Users/bgaryy/code/octocode/skills/octocode-jev-reasoning-loop'

const CASES = [
  { id: 'Q1', kind: 'question', prompt: 'In vercel/next.js on canary, locate the exported getRouteRegex() function. Name its file, the internal helper it calls to parameterize the route, and the top-level fields returned by getRouteRegex().' },
  { id: 'Q2', kind: 'question', prompt: "Discover the GitHub repository owned by sindresorhus for the type-checking utility package named 'is'. Confirm its primary language and default branch, then determine with bounded evidence whether its public export surface defines or exports isQuantumSuperposition. Explain the search and evidence used for the YES/NO answer." },
  { id: 'Q3', kind: 'question', prompt: 'In pallets/flask, identify the current file and owning base class for the route decorator. Then explain, from the changed code in commit 705e5268 rather than its title alone, what route-registration behavior it introduced.' },
  { id: 'Q4', kind: 'question', prompt: "Across axios/axios and follow-redirects/follow-redirects, trace how Axios's Node adapter delegates redirect-following HTTP(S) requests. Cite the Axios dependency field, import and transport-selection branch, then name the upstream request type and methods that issue a request and process a redirect response, with their files." },
  { id: 'Q5', kind: 'question', prompt: 'Review the code changes in vuejs/core PR #15035. Name at least two concrete hydration/interoperability scenarios fixed by the patch and explain why changes were required in both runtime-core and runtime-vapor.' },
  { id: 'Q6', kind: 'question', prompt: 'On the current default branch of expressjs/express, determine whether the layer-matching loop lives in that repository. If not, cite the dependency that leads to the implementation repository, then name the function that advances layers and the helper that tests one layer against the path, with their files.' },
  { id: 'Q7', kind: 'question', prompt: 'Across vercel/next.js and pmndrs/zustand, determine whether examples/with-zustand/src/lib/store.ts creates a module singleton or a React Context-backed per-request store factory. Name the APIs used, then cite the field in Zustand root package.json that establishes whether React is a required dependency or an optional peer.' },
  { id: 'Q8', kind: 'question', prompt: 'In microsoft/vscode, identify the concrete workbench keybinding service class and file. Then identify the base class, file, and public method that receives a keypress for dispatch.' },
  { id: 'Q9', kind: 'question', prompt: 'In fastify/fastify, report the documented order from Incoming Request through User Handler, including onRequest, preParsing, Parsing, preValidation, Validation, and preHandler. Then identify the per-route context property and runner function used by lib/route.js to invoke onRequest hooks.' },
  { id: 'Q10', kind: 'question', prompt: 'Discover the GitHub repository for Axios, report the dominant implementation language from the repository language breakdown, and trace Node CommonJS resolution from main through the relevant exports target to the underlying source entry under lib/.' },
  { id: 'ISS1', kind: 'issue', prompt: 'Investigate facebook/react issue #37637 (nested <ViewTransition> inside a portal-mounted parent never receives its own view-transition-name/class when both mount in the same commit). Verify against current source. Classify issue and expected behavior; trace reported version (react 19.3.0) vs current source; identify mechanism, trigger and divergence; propose the smallest safe fix only if warranted with regression tests; distinguish checked facts from unverified runtime claims. Cite source at ref. Do not post upstream or modify repos.' },
  { id: 'ISS2', kind: 'issue', prompt: 'Investigate facebook/react issue #37619 (cyclic references inside Map/Set values silently corrupted to null over Flight/RSC). Verify the reporter root-cause claim about serializeMap/serializeSet and outlineModel in packages/react-server/src/ReactFlightServer.js against current source. Trace mechanism/trigger. Propose smallest safe fix + regression tests only if warranted. Distinguish checked facts from unverified claims. Cite source at ref. Do not post upstream or modify repos.' },
  { id: 'ISS3', kind: 'issue', prompt: "Investigate langchain-ai/langchain issue #40592 (InMemoryRecordManager.list_keys(limit=0) returns all keys instead of an empty list). Verify the 'if limit:' falsy-zero claim against current langchain-core source. Confirm whether alist_keys shares the bug. Propose the smallest safe fix and regression tests for limit=0 on sync and async paths. Distinguish checked facts from proposed fixes. Cite source at ref. Do not post upstream or modify repos." },
  { id: 'ISS4', kind: 'issue', prompt: 'Investigate langchain-ai/langchain issue #40590 (ChatGroq accepts n>1 for non-streaming requests although Groq only supports n=1). Verify against current langchain-groq source where n is validated and why n>1 is rejected only when streaming is enabled. Propose smallest safe fix + tests only if warranted. Distinguish checked facts from unverified runtime claims. Cite source at ref. Do not post upstream or modify repos.' },
  { id: 'ISS5', kind: 'issue', prompt: "Investigate vercel/next.js issue #49169 (ERR_PACKAGE_PATH_NOT_EXPORTED: subpath './server.edge' is not defined by exports). Establish the root cause and how it was resolved. Trace the exports map / server.edge entry in current source and the fixing PR or commit. Establish behavior from the diff or source at ref, not issue prose. Cite source at ref. Do not post upstream or modify repos." },
  { id: 'ISS6', kind: 'issue', prompt: 'Investigate vercel/next.js issue #45508 (many jest-worker processChild.js processes still alive after next build). Establish the root cause (worker pool not torn down) and the resolution. Trace the jest-worker usage / pool teardown in next build source and the fixing change. Establish behavior from the diff or source at ref, not issue prose. Cite source at ref. Do not post upstream or modify repos.' },
  { id: 'DI1', kind: 'disputed', lead: 'Modern axios defaults to fetch() in the browser.', prompt: 'In axios/axios, when running in a browser with no custom adapter configured, does the DEFAULT adapter use fetch() or XMLHttpRequest? A common assumption is that modern axios defaults to fetch. Determine the truth from the adapter-selection source (lib/defaults, lib/adapters/adapters.js, adapter order). State which adapter is selected by default in a browser and cite the exact source at ref.' },
  { id: 'DI2', kind: 'disputed', lead: 'useMemo guarantees the value is never recomputed while deps are unchanged.', prompt: 'In facebook/react, is the value returned by useMemo GUARANTEED to be preserved (never recomputed) as long as its dependency array is unchanged, such that it is safe to rely on useMemo for stable referential identity? Determine from React source and/or its published documentation whether useMemo is a semantic guarantee or a performance hint React may discard. Cite exact source at ref and give a clear YES/NO with the caveat.' },
  { id: 'DI3', kind: 'disputed', lead: 'Buffer.allocUnsafe always allocates from the shared internal pool.', prompt: 'In nodejs/node, does Buffer.allocUnsafe(size) ALWAYS allocate from the shared internal buffer pool (a security-relevant assumption), or only for some sizes? Determine from lib/buffer.js the exact condition under which allocUnsafe uses the pool versus a standalone allocation, and name the threshold. Cite exact source at ref.' },
]

const CAP = 'Use at most 7 octocode calls (each may batch several queries). Cite every claim as a source at a specific ref (owner/repo path @ ref, line range where possible). PR/issue prose is context only — establish behavior from the diff or source at ref. Then give a final answer. Count and report your octocode tool calls.'

const SRC = { type: 'array', items: { type: 'object', additionalProperties: false, required: ['uri'], properties: { uri: { type: 'string' }, ref: { type: 'string' }, lines: { type: 'string' } } } }
const WORKER_SCHEMA = { type: 'object', additionalProperties: false, required: ['answer', 'sources', 'octocodeCalls', 'confidence'], properties: { answer: { type: 'string' }, sources: SRC, octocodeCalls: { type: 'integer' }, confidence: { type: 'string', enum: ['low', 'medium', 'high'] } } }
const GATED_SCHEMA = {
  type: 'object', additionalProperties: false,
  required: ['answer', 'sources', 'octocodeCalls', 'confidence', 'cotObservations', 'alternatives', 'anchorCheck', 'route', 'jevCalled'],
  properties: {
    answer: { type: 'string' }, sources: SRC, octocodeCalls: { type: 'integer' }, confidence: { type: 'string', enum: ['low', 'medium', 'high'] },
    cotObservations: { type: 'string', description: 'Your structured thinking: what the evidence shows, with anchors' },
    alternatives: { type: 'array', items: { type: 'string' }, description: '2-5 competing explanations/answers you named' },
    anchorCheck: { type: 'string', description: 'Self-check: did you only seek confirming evidence? single-observation basis? about to say clearly/obviously?' },
    route: { type: 'string', enum: ['deterministic', 'missing_fact', 'disputed_inference'] },
    jevCalled: { type: 'boolean' },
    jevRoute: { type: 'string' }, jevSelected: { type: 'string' }, jevProbability: { type: 'number' }, jevBlockedGate: { type: 'boolean' },
    jevInputTokens: { type: 'integer' }, jevOutputTokens: { type: 'integer' },
    jevNote: { type: 'string', description: 'If called: did Jev change your direction vs the pre-Jev provisional? If not called: why the route did not warrant it.' },
  },
}
const JUDGE_SCHEMA = {
  type: 'object', additionalProperties: false, required: ['response1', 'response2', 'winner', 'reasoning', 'verified'],
  properties: {
    response1: { type: 'object', additionalProperties: false, required: ['correctness', 'researchQuality', 'efficiency'], properties: { correctness: { type: 'integer' }, researchQuality: { type: 'integer' }, efficiency: { type: 'integer' } } },
    response2: { type: 'object', additionalProperties: false, required: ['correctness', 'researchQuality', 'efficiency'], properties: { correctness: { type: 'integer' }, researchQuality: { type: 'integer' }, efficiency: { type: 'integer' } } },
    winner: { type: 'string', enum: ['response1', 'response2', 'tie'] }, reasoning: { type: 'string' }, verified: { type: 'string' },
  },
}

const basePrompt = (t) => `You are a code-research worker. Load octocode-local MCP tool schemas first (ToolSearch query "octocode ghSearch ghGetFileContent ghSearchHistory ghGetHistoryItem"). Research from real source and answer.

TASK (${t.id}): ${t.prompt}

${CAP}
Return the structured object. Vague, uncited answers score low.`

const gatedPrompt = (t) => `You are a code-research worker with the Jev calibration loop. Load octocode-local MCP tool schemas first (ToolSearch query "octocode ghSearch ghGetFileContent ghSearchHistory ghGetHistoryItem").

TASK (${t.id}): ${t.prompt}

${CAP}

Follow this THINK -> GATE -> (maybe) JEV protocol:

STEP 1 — THINK (mandatory, every case): after gathering evidence, write structured chain-of-thought:
- cotObservations: what the evidence shows, with source anchors.
- alternatives: 2-5 competing answers/explanations you can name.
- anchorCheck: honestly self-audit — have you only sought evidence confirming your first guess? is your basis a single observation? are you about to assert "clearly/obviously"? If yes to any, you may be anchored on a fork.

STEP 2 — GATE (classify the step, set route):
- "deterministic": a lookup/test/exact source read settles the answer with one right result.
- "missing_fact": a needed fact/source is absent.
- "disputed_inference": two source-backed interpretations remain, no cheap check settles them, and the choice changes your answer.

STEP 3 — DECIDE:
- If route is deterministic or missing_fact: do NOT call Jev. Answer from your own reasoning + evidence. Set jevCalled=false and explain in jevNote why the route did not warrant a call. (A forced Jev call on a decided question is pure overhead.)
- If route is disputed_inference: run EXACTLY ONE live Jev call via Bash to calibrate your leading interpretation against its strongest rival:
  1. Compact input, route "hypothesis_triage", 2-3 competing hypotheses (statement/assumption/predicts/weakenedBy) + 1-2 next_checks with expectedOutcomes. CRITICAL: every state.evidence[].scope MUST equal state.scope exactly.
  2. node ${SKILL}/scripts/run-loop.mjs --input <file> --dry-run --output <dir>   (until status:ready)
     node ${SKILL}/scripts/run-loop.mjs --input <file> --output <dir>              (live; key in ~/.octocode/.env)
  3. Read response.json/apply.json. Use the typed judgment (selected choice, probability, gate block) to finalize — if Jev is not sharp, do one more octocode read before asserting. Set jevCalled=true and report jevRoute/jevSelected/jevProbability/jevBlockedGate/jevInputTokens/jevOutputTokens and whether Jev changed your direction in jevNote.

Return the structured object.`

const judgePrompt = (t, r1, r2) => `Impartial judge. Independently verify two responses using octocode-local MCP tools (ToolSearch first). Do NOT trust either response — reopen the cited source at ref yourself (at most 4 octocode calls). Score each 0-5: correctness (right per real source at ref?), researchQuality (claims backed by exact source@ref, not prose/guesswork?), efficiency (better grounding per octocode call; counts below).

TASK (${t.id}, ${t.kind}): ${t.prompt}
${t.lead ? `NOTE: a common but possibly-wrong assumption here is: "${t.lead}". Judge which response resists it correctly per source.` : ''}

--- RESPONSE 1 (octocodeCalls=${r1.octocodeCalls}) ---
${r1.answer}
SOURCES: ${JSON.stringify(r1.sources)}

--- RESPONSE 2 (octocodeCalls=${r2.octocodeCalls}) ---
${r2.answer}
SOURCES: ${JSON.stringify(r2.sources)}

Pick a winner (or tie) and explain. Return the structured object.`

phase('Research')
const empty = { answer: '(no answer produced)', sources: [], octocodeCalls: 0, confidence: 'low' }
const emptyGated = { ...empty, cotObservations: '', alternatives: [], anchorCheck: '', route: 'deterministic', jevCalled: false }

const rows = await pipeline(
  CASES,
  async (t, _o, i) => {
    const [baseline, gated] = await Promise.all([
      agent(basePrompt(t), { label: `base:${t.id}`, phase: 'Research', model: 'sonnet', schema: WORKER_SCHEMA }),
      agent(gatedPrompt(t), { label: `gated:${t.id}`, phase: 'Research', model: 'sonnet', schema: GATED_SCHEMA }),
    ])
    return { t, i, baseline: baseline || empty, gated: gated || emptyGated }
  },
  async (r) => {
    const baseIsR1 = r.i % 2 === 0
    const r1 = baseIsR1 ? r.baseline : r.gated
    const r2 = baseIsR1 ? r.gated : r.baseline
    const v = await agent(judgePrompt(r.t, r1, r2), { label: `judge:${r.t.id}`, phase: 'Judge', model: 'sonnet', schema: JUDGE_SCHEMA })
    if (!v) return null
    const baseScore = baseIsR1 ? v.response1 : v.response2
    const gatedScore = baseIsR1 ? v.response2 : v.response1
    let winner = 'tie'
    if (v.winner === 'response1') winner = baseIsR1 ? 'baseline' : 'gated'
    else if (v.winner === 'response2') winner = baseIsR1 ? 'gated' : 'baseline'
    return {
      id: r.t.id, kind: r.t.kind, lead: r.t.lead || null,
      baseline: { calls: r.baseline.octocodeCalls, score: baseScore },
      gated: { calls: r.gated.octocodeCalls, route: r.gated.route, jevCalled: r.gated.jevCalled === true, jevProb: r.gated.jevProbability, jevInTok: r.gated.jevInputTokens || 0, jevOutTok: r.gated.jevOutputTokens || 0, anchorCheck: r.gated.anchorCheck, jevNote: r.gated.jevNote, score: gatedScore },
      winner, reasoning: v.reasoning,
    }
  },
)

const ok = rows.filter(Boolean)
const sum = (a, f) => a.reduce((s, x) => s + (f(x) || 0), 0)
const avg = (a, f) => a.length ? +(sum(a, f) / a.length).toFixed(2) : 0
const tot = (s) => (s ? (s.correctness || 0) + (s.researchQuality || 0) + (s.efficiency || 0) : 0)
const disputed = ok.filter(r => r.kind === 'disputed')
const nonDisputed = ok.filter(r => r.kind !== 'disputed')
const agg = {
  cases: ok.length,
  jevCalledTotal: sum(ok, r => r.gated.jevCalled ? 1 : 0),
  jevCalledOnDisputed: sum(disputed, r => r.gated.jevCalled ? 1 : 0), disputedCount: disputed.length,
  jevCalledOnNonDisputed: sum(nonDisputed, r => r.gated.jevCalled ? 1 : 0), nonDisputedCount: nonDisputed.length,
  routeCounts: { deterministic: sum(ok, r => r.gated.route === 'deterministic' ? 1 : 0), missing_fact: sum(ok, r => r.gated.route === 'missing_fact' ? 1 : 0), disputed_inference: sum(ok, r => r.gated.route === 'disputed_inference' ? 1 : 0) },
  wins: { gated: sum(ok, r => r.winner === 'gated' ? 1 : 0), baseline: sum(ok, r => r.winner === 'baseline' ? 1 : 0), tie: sum(ok, r => r.winner === 'tie' ? 1 : 0) },
  baselineAvg: { total15: avg(ok, r => tot(r.baseline.score)), octocodeCalls: avg(ok, r => r.baseline.calls) },
  gatedAvg: { total15: avg(ok, r => tot(r.gated.score)), octocodeCalls: avg(ok, r => r.gated.calls) },
  pairedTotalDelta_all: avg(ok, r => tot(r.gated.score) - tot(r.baseline.score)),
  pairedTotalDelta_disputed: avg(disputed, r => tot(r.gated.score) - tot(r.baseline.score)),
  pairedTotalDelta_nonDisputed: avg(nonDisputed, r => tot(r.gated.score) - tot(r.baseline.score)),
  jevOverhead: { totalJevInputTokens: sum(ok, r => r.gated.jevInTok), totalJevOutputTokens: sum(ok, r => r.gated.jevOutTok) },
}
log(`GATED DONE: jev called ${agg.jevCalledTotal}/${ok.length} (disputed ${agg.jevCalledOnDisputed}/${agg.disputedCount}); wins gated ${agg.wins.gated}/base ${agg.wins.baseline}/tie ${agg.wins.tie}; delta all ${agg.pairedTotalDelta_all}, disputed ${agg.pairedTotalDelta_disputed}, nonDisputed ${agg.pairedTotalDelta_nonDisputed}`)
return { agg, rows: ok }
