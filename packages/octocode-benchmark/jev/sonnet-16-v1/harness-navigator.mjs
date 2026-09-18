export const meta = {
  name: 'jev-sonnet-navigator-v3',
  description: 'Jev-as-navigator: use Jev as a high-frequency System-1 classifier to rank candidate reads and gate sufficiency during octocode research. Measures octocode-call reduction vs plain research (hunch KPI), correctness held.',
  phases: [
    { title: 'Research', detail: '16 cases x 2 arms (plain vs jev-navigator)' },
    { title: 'Judge', detail: 'blinded judge verifies at ref, scores correctness + efficiency' },
  ],
}

const SKILL = '/Users/bgaryy/code/octocode/skills/octocode-jev-reasoning-loop'

const CASES = [
  { id: 'Q1', prompt: 'In vercel/next.js on canary, locate the exported getRouteRegex() function. Name its file, the internal helper it calls to parameterize the route, and the top-level fields returned by getRouteRegex().' },
  { id: 'Q2', prompt: "Discover the GitHub repository owned by sindresorhus for the type-checking utility package named 'is'. Confirm its primary language and default branch, then determine with bounded evidence whether its public export surface defines or exports isQuantumSuperposition. Explain the search and evidence used for the YES/NO answer." },
  { id: 'Q3', prompt: 'In pallets/flask, identify the current file and owning base class for the route decorator. Then explain, from the changed code in commit 705e5268 rather than its title alone, what route-registration behavior it introduced.' },
  { id: 'Q4', prompt: "Across axios/axios and follow-redirects/follow-redirects, trace how Axios's Node adapter delegates redirect-following HTTP(S) requests. Cite the Axios dependency field, import and transport-selection branch, then name the upstream request type and methods that issue a request and process a redirect response, with their files." },
  { id: 'Q5', prompt: 'Review the code changes in vuejs/core PR #15035. Name at least two concrete hydration/interoperability scenarios fixed by the patch and explain why changes were required in both runtime-core and runtime-vapor.' },
  { id: 'Q6', prompt: 'On the current default branch of expressjs/express, determine whether the layer-matching loop lives in that repository. If not, cite the dependency that leads to the implementation repository, then name the function that advances layers and the helper that tests one layer against the path, with their files.' },
  { id: 'Q7', prompt: 'Across vercel/next.js and pmndrs/zustand, determine whether examples/with-zustand/src/lib/store.ts creates a module singleton or a React Context-backed per-request store factory. Name the APIs used, then cite the field in Zustand root package.json that establishes whether React is a required dependency or an optional peer.' },
  { id: 'Q8', prompt: 'In microsoft/vscode, identify the concrete workbench keybinding service class and file. Then identify the base class, file, and public method that receives a keypress for dispatch.' },
  { id: 'Q9', prompt: 'In fastify/fastify, report the documented order from Incoming Request through User Handler, including onRequest, preParsing, Parsing, preValidation, Validation, and preHandler. Then identify the per-route context property and runner function used by lib/route.js to invoke onRequest hooks.' },
  { id: 'Q10', prompt: 'Discover the GitHub repository for Axios, report the dominant implementation language from the repository language breakdown, and trace Node CommonJS resolution from main through the relevant exports target to the underlying source entry under lib/.' },
  { id: 'ISS1', prompt: 'Investigate facebook/react issue #37637 (nested <ViewTransition> inside a portal-mounted parent never receives its own view-transition-name/class when both mount in the same commit). Verify against current source; identify mechanism, trigger and divergence; propose the smallest safe fix only if warranted. Cite source at ref.' },
  { id: 'ISS2', prompt: 'Investigate facebook/react issue #37619 (cyclic references inside Map/Set values silently corrupted to null over Flight/RSC). Verify serializeMap/serializeSet + outlineModel in packages/react-server/src/ReactFlightServer.js against current source; trace mechanism/trigger. Cite source at ref.' },
  { id: 'ISS3', prompt: "Investigate langchain-ai/langchain issue #40592 (InMemoryRecordManager.list_keys(limit=0) returns all keys). Verify the 'if limit:' falsy-zero claim against current langchain-core source; confirm whether alist_keys shares the bug; name the fix. Cite source at ref." },
  { id: 'ISS4', prompt: 'Investigate langchain-ai/langchain issue #40590 (ChatGroq accepts n>1 for non-streaming although Groq only supports n=1). Verify against current langchain-groq source where n is validated and why n>1 is rejected only when streaming is enabled. Cite source at ref.' },
  { id: 'ISS5', prompt: "Investigate vercel/next.js issue #49169 (ERR_PACKAGE_PATH_NOT_EXPORTED: subpath './server.edge'). Establish the root cause and how it was resolved; trace the exports map / server.edge entry and the fixing PR or commit. Cite source at ref." },
  { id: 'ISS6', prompt: 'Investigate vercel/next.js issue #45508 (jest-worker processChild.js processes still alive after next build). Establish the root cause (worker pool not torn down) and the resolution; trace the teardown in next build source and the fixing change. Cite source at ref.' },
]

const CITE = 'Cite every claim as a source at a specific ref (owner/repo path @ ref, line range where possible). Establish behavior from source/diff at ref, not PR/issue prose. Count and report your octocode tool calls.'

const SRC = { type: 'array', items: { type: 'object', additionalProperties: false, required: ['uri'], properties: { uri: { type: 'string' }, ref: { type: 'string' }, lines: { type: 'string' } } } }
const BASE_SCHEMA = { type: 'object', additionalProperties: false, required: ['answer', 'sources', 'octocodeCalls', 'confidence'], properties: { answer: { type: 'string' }, sources: SRC, octocodeCalls: { type: 'integer' }, confidence: { type: 'string', enum: ['low', 'medium', 'high'] } } }
const NAV_SCHEMA = {
  type: 'object', additionalProperties: false,
  required: ['answer', 'sources', 'octocodeCalls', 'confidence', 'jevCalls', 'jevInputTokens', 'jevOutputTokens', 'navNote'],
  properties: {
    answer: { type: 'string' }, sources: SRC, octocodeCalls: { type: 'integer' }, confidence: { type: 'string', enum: ['low', 'medium', 'high'] },
    jevCalls: { type: 'integer', description: 'How many live Jev calls you made for navigation (ranking + sufficiency)' },
    jevInputTokens: { type: 'integer' }, jevOutputTokens: { type: 'integer' },
    jevRefused: { type: 'integer', description: 'How many Jev calls the runner refused/routed to missing_fact instead of ranking' },
    navNote: { type: 'string', description: 'Did Jev-ranking change which file you opened first, or cut reads? Did the sufficiency gate stop you early or push you further?' },
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

const basePrompt = (t) => `You are a code-research worker. Load octocode-local MCP tool schemas first (ToolSearch query "octocode ghSearch ghGetFileContent ghSearchHistory ghGetHistoryItem"). Research from real source and answer, minimizing wasted reads.

TASK (${t.id}): ${t.prompt}

Use as few octocode calls as you need; there is no fixed cap, but efficiency is scored. ${CITE}
Return the structured object.`

const navPrompt = (t) => `You are a code-research worker using Jev as a NAVIGATION classifier (a fast System-1 primitive) to reach the answer in the FEWEST octocode reads. Load octocode-local MCP tool schemas first (ToolSearch query "octocode ghSearch ghGetFileContent ghSearchHistory ghGetHistoryItem").

TASK (${t.id}): ${t.prompt}

NAVIGATOR PROTOCOL — use Jev to decide WHICH file to open next and WHEN to stop, so you open only the files that matter:
1. DISCOVER (cheap): one ghSearch (code or tree) to list candidate files/paths with snippets. This is your evidence for ranking.
2. RANK with Jev (live): build a hypothesis_triage packet where each hypothesis = "the answer lives in candidate <path>", evidence = the search snippets (CRITICAL: every state.evidence[].scope MUST equal state.scope exactly), next_checks = "read candidate <path>". Run:
   node ${SKILL}/scripts/run-loop.mjs --input <file> --dry-run --output <dir>   (until status:ready)
   node ${SKILL}/scripts/run-loop.mjs --input <file> --output <dir>              (live; key in ~/.octocode/.env)
   Read apply.json: open the file Jev ranks highest FIRST. If the runner refuses (routes to missing_fact/needs_evidence), do one targeted read, then retry the rank with that evidence; count it in jevRefused.
3. READ only the top-ranked candidate(s) with ghGetFileContent (targeted line ranges / matchString, not whole files).
4. SUFFICIENCY gate with Jev (live, optional): after a read, if unsure whether you have enough, build a hunch_check ("is the gathered context sufficient to answer confidently?") and run it. If Jev says yes/sharp, answer; if ambiguous, open one more ranked file.
5. Answer. Report octocodeCalls (aim to minimize), jevCalls (ranking + sufficiency), jevRefused, jevInputTokens/jevOutputTokens (sum from run metrics), and navNote.

${CITE} Return the structured object.`

const judgePrompt = (t, r1, r2) => `Impartial judge. Independently verify two responses using octocode-local MCP tools (ToolSearch first). Do NOT trust either response — reopen the cited source at ref yourself (at most 4 octocode calls). Score each 0-5: correctness (right per real source at ref?), researchQuality (claims backed by exact source@ref?), efficiency (correct answer in FEWER octocode calls scores higher; counts below).

TASK (${t.id}): ${t.prompt}

--- RESPONSE 1 (octocodeCalls=${r1.octocodeCalls}) ---
${r1.answer}
SOURCES: ${JSON.stringify(r1.sources)}

--- RESPONSE 2 (octocodeCalls=${r2.octocodeCalls}) ---
${r2.answer}
SOURCES: ${JSON.stringify(r2.sources)}

Pick a winner (or tie) and explain. Return the structured object.`

phase('Research')
const empty = { answer: '(no answer produced)', sources: [], octocodeCalls: 0, confidence: 'low' }
const emptyNav = { ...empty, jevCalls: 0, jevInputTokens: 0, jevOutputTokens: 0, jevRefused: 0, navNote: '' }

const rows = await pipeline(
  CASES,
  async (t, _o, i) => {
    const [base, nav] = await Promise.all([
      agent(basePrompt(t), { label: `base:${t.id}`, phase: 'Research', model: 'sonnet', schema: BASE_SCHEMA }),
      agent(navPrompt(t), { label: `nav:${t.id}`, phase: 'Research', model: 'sonnet', schema: NAV_SCHEMA }),
    ])
    return { t, i, base: base || empty, nav: nav || emptyNav }
  },
  async (r) => {
    const baseIsR1 = r.i % 2 === 0
    const r1 = baseIsR1 ? r.base : r.nav
    const r2 = baseIsR1 ? r.nav : r.base
    const v = await agent(judgePrompt(r.t, r1, r2), { label: `judge:${r.t.id}`, phase: 'Judge', model: 'sonnet', schema: JUDGE_SCHEMA })
    if (!v) return null
    const baseScore = baseIsR1 ? v.response1 : v.response2
    const navScore = baseIsR1 ? v.response2 : v.response1
    let winner = 'tie'
    if (v.winner === 'response1') winner = baseIsR1 ? 'base' : 'nav'
    else if (v.winner === 'response2') winner = baseIsR1 ? 'nav' : 'base'
    return {
      id: r.t.id,
      base: { calls: r.base.octocodeCalls, score: baseScore },
      nav: { calls: r.nav.octocodeCalls, jevCalls: r.nav.jevCalls, jevRefused: r.nav.jevRefused || 0, jevInTok: r.nav.jevInputTokens || 0, jevOutTok: r.nav.jevOutputTokens || 0, note: r.nav.navNote, score: navScore },
      winner, reasoning: v.reasoning,
    }
  },
)

const ok = rows.filter(Boolean)
const sum = (a, f) => a.reduce((s, x) => s + (f(x) || 0), 0)
const avg = (a, f) => a.length ? +(sum(a, f) / a.length).toFixed(2) : 0
const tot = (s) => (s ? (s.correctness || 0) + (s.researchQuality || 0) + (s.efficiency || 0) : 0)
const agg = {
  cases: ok.length,
  jevCallsTotal: sum(ok, r => r.nav.jevCalls), jevRefusedTotal: sum(ok, r => r.nav.jevRefused),
  jevUsedCases: sum(ok, r => r.nav.jevCalls > 0 ? 1 : 0),
  wins: { nav: sum(ok, r => r.winner === 'nav' ? 1 : 0), base: sum(ok, r => r.winner === 'base' ? 1 : 0), tie: sum(ok, r => r.winner === 'tie' ? 1 : 0) },
  baseAvg: { correctness: avg(ok, r => r.base.score?.correctness), total15: avg(ok, r => tot(r.base.score)), octocodeCalls: avg(ok, r => r.base.calls) },
  navAvg: { correctness: avg(ok, r => r.nav.score?.correctness), total15: avg(ok, r => tot(r.nav.score)), octocodeCalls: avg(ok, r => r.nav.calls) },
  octocodeCallDelta: +(avg(ok, r => r.nav.calls) - avg(ok, r => r.base.calls)).toFixed(2),
  correctnessDelta: +(avg(ok, r => r.nav.score?.correctness) - avg(ok, r => r.base.score?.correctness)).toFixed(2),
  jevOverhead: { totalInputTokens: sum(ok, r => r.nav.jevInTok), totalOutputTokens: sum(ok, r => r.nav.jevOutTok) },
}
log(`NAVIGATOR DONE: jev used ${agg.jevUsedCases}/${ok.length} (${agg.jevCallsTotal} calls, ${agg.jevRefusedTotal} refused); octocode calls base ${agg.baseAvg.octocodeCalls} vs nav ${agg.navAvg.octocodeCalls} (delta ${agg.octocodeCallDelta}); correctness delta ${agg.correctnessDelta}; wins nav ${agg.wins.nav}/base ${agg.wins.base}/tie ${agg.wins.tie}`)
return { agg, rows: ok }
