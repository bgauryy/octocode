export const meta = {
  name: 'jev-sonnet-16-v1',
  description: 'Jev A/B pilot: octocode research WITH vs WITHOUT the Jev reasoning loop, 16 grounded cases, Sonnet host, blinded judge',
  phases: [
    { title: 'Research', detail: '16 cases x 2 arms (baseline vs jev)' },
    { title: 'Judge', detail: 'blinded judge verifies at ref, scores both arms' },
  ],
}

const SKILL = '/Users/bgaryy/code/octocode/skills/octocode-jev-reasoning-loop'

// 16 cases — mirror of cases.json (Q1-Q10 canonical + ISS1-ISS6 real issues)
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
  { id: 'ISS1', kind: 'issue', prompt: 'Investigate facebook/react issue #37637 (nested <ViewTransition> inside a portal-mounted parent never receives its own view-transition-name/class when both mount in the same commit). Verify against current source. Classify issue and expected behavior; trace reported version (react 19.3.0) vs current source; identify mechanism, trigger and divergence; test a plausible alternative explanation; propose the smallest safe fix (files/symbols + concrete edit) only if warranted, with regression tests; distinguish checked facts from proposed fixes and unverified runtime claims. Cite source at ref. Do not post upstream or modify repos.' },
  { id: 'ISS2', kind: 'issue', prompt: 'Investigate facebook/react issue #37619 (cyclic references inside Map/Set values silently corrupted to null over Flight/RSC). Verify the reporter root-cause claim about serializeMap/serializeSet and outlineModel in packages/react-server/src/ReactFlightServer.js against current source. Trace mechanism/trigger and why plain objects/arrays round-trip but Map/Set self-cycles do not. Test an alternative explanation. Propose smallest safe fix + regression tests only if warranted. Distinguish checked facts from unverified claims. Cite source at ref. Do not post upstream or modify repos.' },
  { id: 'ISS3', kind: 'issue', prompt: "Investigate langchain-ai/langchain issue #40592 (InMemoryRecordManager.list_keys(limit=0) returns all keys instead of an empty list). Verify the reporter claim that an 'if limit:' falsy-zero check causes it, against current langchain-core source. Confirm whether alist_keys shares the bug. Propose the smallest safe fix (file/symbol + concrete edit) and regression tests for limit=0 on both sync and async paths. Distinguish checked facts from proposed fixes. Cite source at ref. Do not post upstream or modify repos." },
  { id: 'ISS4', kind: 'issue', prompt: 'Investigate langchain-ai/langchain issue #40590 (ChatGroq accepts n>1 for non-streaming requests although Groq only supports n=1). Verify against current langchain-groq source: where n is validated, why n>1 is rejected only when streaming is enabled, whether non-streaming construction with n=2 is accepted. Trace the validator. Propose smallest safe fix + regression/negative-control tests only if warranted. Distinguish checked facts from proposed fixes and unverified runtime claims. Cite source at ref. Do not post upstream or modify repos.' },
  { id: 'ISS5', kind: 'issue', prompt: "Investigate vercel/next.js issue #49169 (ERR_PACKAGE_PATH_NOT_EXPORTED: subpath './server.edge' is not defined by exports). Establish the root cause and how it was resolved. Trace the exports map / server.edge entry in current source and the fixing PR or commit. Establish behavior from the diff or source at ref, not issue prose. Distinguish checked facts from unverified claims. Cite source at ref. Do not post upstream or modify repos." },
  { id: 'ISS6', kind: 'issue', prompt: 'Investigate vercel/next.js issue #45508 (many jest-worker processChild.js processes still alive after next build). Establish the root cause (worker pool not torn down) and the resolution. Trace the jest-worker usage / pool teardown in next build source and the fixing change. Establish behavior from the diff or source at ref, not issue prose. Distinguish checked facts from unverified claims. Cite source at ref. Do not post upstream or modify repos.' },
]

const CAP = 'Use at most 7 octocode calls (each may batch several queries). Cite every claim as a source at a specific ref (owner/repo path @ ref, with line range where possible). PR/issue prose is context only — establish behavior from the diff or source at ref. Then give a final answer. Count and report your octocode tool calls.'

const SRC = { type: 'array', items: { type: 'object', additionalProperties: false, required: ['uri'], properties: { uri: { type: 'string' }, ref: { type: 'string' }, lines: { type: 'string' } } } }

const WORKER_SCHEMA = {
  type: 'object', additionalProperties: false,
  required: ['answer', 'sources', 'octocodeCalls', 'confidence'],
  properties: { answer: { type: 'string' }, sources: SRC, octocodeCalls: { type: 'integer' }, confidence: { type: 'string', enum: ['low', 'medium', 'high'] } },
}
const JEV_SCHEMA = {
  type: 'object', additionalProperties: false,
  required: ['answer', 'sources', 'octocodeCalls', 'confidence', 'jevUsed'],
  properties: {
    answer: { type: 'string' }, sources: SRC, octocodeCalls: { type: 'integer' }, confidence: { type: 'string', enum: ['low', 'medium', 'high'] },
    jevUsed: { type: 'boolean' }, jevRoute: { type: 'string' }, jevSelected: { type: 'string' }, jevProbability: { type: 'number' },
    jevBlockedGate: { type: 'boolean' }, jevInputTokens: { type: 'integer' }, jevOutputTokens: { type: 'integer' },
    jevNote: { type: 'string', description: 'How Jev changed (or did not change) the direction vs the pre-Jev provisional answer' },
  },
}
const JUDGE_SCHEMA = {
  type: 'object', additionalProperties: false,
  required: ['response1', 'response2', 'winner', 'reasoning', 'verified'],
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

const jevPrompt = (t) => `You are a code-research worker equipped with the Jev reasoning loop. Load octocode-local MCP tool schemas first (ToolSearch query "octocode ghSearch ghGetFileContent ghSearchHistory ghGetHistoryItem"). Gather evidence AND use Jev to discipline your key decision.

TASK (${t.id}): ${t.prompt}

${CAP}

JEV STEP (required — exactly one live call): after initial evidence, write your provisional answer, then at your single most important fork (which competing explanation/answer is correct, OR whether the answer is safe to assert) build a bounded deck and run Jev LIVE via Bash:
1. Compact input JSON, route "hypothesis_triage": 2-3 competing hypotheses (statement/assumption/predicts/weakenedBy) + 1-2 next_checks with expectedOutcomes. CRITICAL: every state.evidence[].scope MUST equal state.scope exactly, or validation fails.
2. node ${SKILL}/scripts/run-loop.mjs --input <file> --dry-run --output <dir>   (repeat until it prints status:ready)
   node ${SKILL}/scripts/run-loop.mjs --input <file> --output <dir>              (live; key is in ~/.octocode/.env)
3. Read response.json/apply.json. Use Jev's typed judgment (selected choice, probability, whether the claim gate blocked) to decide the final answer — e.g. if Jev is not sharp, keep provisional and do one more octocode read.

Report jevUsed, jevRoute, jevSelected, jevProbability, jevBlockedGate, jevInputTokens/jevOutputTokens (sum from run metrics), and jevNote (did Jev change your direction vs the provisional?). If Jev genuinely could not run, jevUsed=false + why in jevNote. Return the structured object.`

const judgePrompt = (t, r1, r2) => `Impartial judge. Independently verify two responses using octocode-local MCP tools (ToolSearch first). Do NOT trust either response — reopen the cited source at ref yourself (at most 4 octocode calls). Score each 0-5:
- correctness: factually right per real source at ref?
- researchQuality: claims backed by exact source@ref (not prose/guesswork)? penalize uncited/web-rendered citations.
- efficiency: better grounding per octocode call scores higher (counts below).

TASK (${t.id}, ${t.kind}): ${t.prompt}

--- RESPONSE 1 (octocodeCalls=${r1.octocodeCalls}) ---
${r1.answer}
SOURCES: ${JSON.stringify(r1.sources)}

--- RESPONSE 2 (octocodeCalls=${r2.octocodeCalls}) ---
${r2.answer}
SOURCES: ${JSON.stringify(r2.sources)}

Pick a winner (or tie) and explain. Return the structured object.`

phase('Research')
const empty = { answer: '(no answer produced)', sources: [], octocodeCalls: 0, confidence: 'low' }

const rows = await pipeline(
  CASES,
  async (t, _o, i) => {
    const [baseline, jev] = await Promise.all([
      agent(basePrompt(t), { label: `base:${t.id}`, phase: 'Research', model: 'sonnet', schema: WORKER_SCHEMA }),
      agent(jevPrompt(t), { label: `jev:${t.id}`, phase: 'Research', model: 'sonnet', schema: JEV_SCHEMA }),
    ])
    return { t, i, baseline: baseline || empty, jev: jev || { ...empty, jevUsed: false } }
  },
  async (r) => {
    const baselineIsR1 = r.i % 2 === 0
    const r1 = baselineIsR1 ? r.baseline : r.jev
    const r2 = baselineIsR1 ? r.jev : r.baseline
    const v = await agent(judgePrompt(r.t, r1, r2), { label: `judge:${r.t.id}`, phase: 'Judge', model: 'sonnet', schema: JUDGE_SCHEMA })
    if (!v) return null
    const baselineScore = baselineIsR1 ? v.response1 : v.response2
    const jevScore = baselineIsR1 ? v.response2 : v.response1
    let winner = 'tie'
    if (v.winner === 'response1') winner = baselineIsR1 ? 'baseline' : 'jev'
    else if (v.winner === 'response2') winner = baselineIsR1 ? 'jev' : 'baseline'
    return {
      id: r.t.id, kind: r.t.kind,
      baseline: { calls: r.baseline.octocodeCalls, conf: r.baseline.confidence, score: baselineScore },
      jev: { calls: r.jev.octocodeCalls, conf: r.jev.confidence, used: r.jev.jevUsed === true, route: r.jev.jevRoute, prob: r.jev.jevProbability, blocked: r.jev.jevBlockedGate, jevInTok: r.jev.jevInputTokens || 0, jevOutTok: r.jev.jevOutputTokens || 0, score: jevScore, note: r.jev.jevNote },
      winner, reasoning: v.reasoning, verified: v.verified,
    }
  },
)

const ok = rows.filter(Boolean)
const sum = (a, f) => a.reduce((s, x) => s + (f(x) || 0), 0)
const avg = (a, f) => a.length ? +(sum(a, f) / a.length).toFixed(2) : 0
const tot = (s) => (s ? (s.correctness || 0) + (s.researchQuality || 0) + (s.efficiency || 0) : 0)
const agg = {
  cases: ok.length,
  jevActuallyUsed: sum(ok, r => r.jev.used ? 1 : 0),
  wins: { jev: sum(ok, r => r.winner === 'jev' ? 1 : 0), baseline: sum(ok, r => r.winner === 'baseline' ? 1 : 0), tie: sum(ok, r => r.winner === 'tie' ? 1 : 0) },
  baselineAvg: { correctness: avg(ok, r => r.baseline.score?.correctness), researchQuality: avg(ok, r => r.baseline.score?.researchQuality), efficiency: avg(ok, r => r.baseline.score?.efficiency), total15: avg(ok, r => tot(r.baseline.score)), octocodeCalls: avg(ok, r => r.baseline.calls) },
  jevAvg: { correctness: avg(ok, r => r.jev.score?.correctness), researchQuality: avg(ok, r => r.jev.score?.researchQuality), efficiency: avg(ok, r => r.jev.score?.efficiency), total15: avg(ok, r => tot(r.jev.score)), octocodeCalls: avg(ok, r => r.jev.calls) },
  pairedTotalDelta: avg(ok, r => tot(r.jev.score) - tot(r.baseline.score)),
  jevOverhead: { totalJevInputTokens: sum(ok, r => r.jev.jevInTok), totalJevOutputTokens: sum(ok, r => r.jev.jevOutTok) },
}
log(`DONE: jev ${agg.wins.jev} / baseline ${agg.wins.baseline} / tie ${agg.wins.tie}; jev used ${agg.jevActuallyUsed}/${ok.length}; paired total delta ${agg.pairedTotalDelta}`)
return { agg, rows: ok }
