#!/usr/bin/env node
/**
 * bug-triage-v1 harness
 *
 * Usage:
 *   node harness.mjs --freeze           hash CONTRACT.md + cases.json + judge.mjs + inspect.mjs → frozen.json
 *   node harness.mjs --prompts          print agent prompts for all cases (both arms)
 *   node harness.mjs --prompt BUG-01 baseline   print one arm prompt
 *   node harness.mjs --prompt BUG-01 treatment
 *   node harness.mjs --preflight        check Octocode CLI is reachable (no remote calls)
 *   node harness.mjs --validate         validate frozen.json hashes still match
 *
 * Agents are spawned externally (via Pi workflow, Codex, or equivalent).
 * Each agent receives the prompt from --prompt and writes its output to:
 *   runs/baseline/BUG-{id}/answer.md
 *   runs/baseline/BUG-{id}/result.json
 *   runs/treatment/BUG-{id}/answer.md
 *   runs/treatment/BUG-{id}/result.json       (+ jev-calls.json, decision-before.json, decision-after.json)
 *
 * After all agents complete, run: node inspect.mjs --save
 * Then run:  node judge.mjs --all  to grade, then inspect.mjs --aggregate for RESULTS.md
 */

import { createHash } from 'crypto';
import { readFileSync, writeFileSync, existsSync, mkdirSync } from 'fs';
import { resolve, dirname } from 'path';
import { fileURLToPath } from 'url';

const __dir = dirname(fileURLToPath(import.meta.url));
const rel = (...p) => resolve(__dir, ...p);

const FREEZE_FILES = ['CONTRACT.md', 'cases.json', 'judge.mjs', 'inspect.mjs'];
const CASES = JSON.parse(readFileSync(rel('cases.json'), 'utf8')).cases;

// ── helpers ─────────────────────────────────────────────────────────────────

function sha256file(path) {
  return createHash('sha256').update(readFileSync(path)).digest('hex');
}

function ensureDir(p) {
  if (!existsSync(p)) mkdirSync(p, { recursive: true });
}

// ── freeze ───────────────────────────────────────────────────────────────────

function freeze() {
  const hashes = {};
  for (const f of FREEZE_FILES) {
    const p = rel(f);
    if (!existsSync(p)) { console.error(`MISSING: ${f}`); process.exit(1); }
    hashes[f] = sha256file(p);
  }
  const frozen = {
    suiteId: 'bug-triage-v1',
    frozenAt: new Date().toISOString(),
    caseCount: CASES.length,
    files: hashes,
  };
  writeFileSync(rel('frozen.json'), JSON.stringify(frozen, null, 2));
  console.log('frozen.json written');
  for (const [f, h] of Object.entries(hashes)) console.log(`  ${h.slice(0,16)}  ${f}`);
}

// ── validate ─────────────────────────────────────────────────────────────────

function validate() {
  if (!existsSync(rel('frozen.json'))) { console.error('frozen.json missing — run --freeze first'); process.exit(1); }
  const frozen = JSON.parse(readFileSync(rel('frozen.json'), 'utf8'));
  let ok = true;
  for (const [f, expected] of Object.entries(frozen.files)) {
    const actual = sha256file(rel(f));
    const match = actual === expected;
    console.log(`${match ? '✓' : '✗'} ${f}  ${match ? '' : `\n  expected ${expected}\n  got      ${actual}`}`);
    if (!match) ok = false;
  }
  if (!ok) { console.error('\nINTEGRITY FAILURE — run was frozen against different files'); process.exit(1); }
  console.log('\nAll hashes valid.');
}

// ── preflight ────────────────────────────────────────────────────────────────

async function preflight() {
  const { execSync } = await import('child_process');
  try {
    execSync('node packages/octocode/out/octocode.js tools --json', {
      cwd: resolve(__dir, '../../../..'),
      stdio: 'pipe',
    });
    console.log('✓ Octocode CLI reachable');
  } catch {
    console.error('✗ Octocode CLI not reachable — build first: yarn workspace octocode build:dev');
    process.exit(1);
  }
}

// ── prompts ──────────────────────────────────────────────────────────────────

function baselinePrompt(c) {
  return `\
# Bug-Triage Benchmark — BASELINE arm
Suite: bug-triage-v1  Case: ${c.id}  Repo: ${c.repo}  Issue: #${c.issueNumber}
Anchor ref: \`${c.anchorRef}\`  Octocode call cap: 8

## Your task
${c.task}

## Rules
- Use Octocode tools (ghSearch, ghGetFileContent, ghSearchHistory, ghGetHistoryItem) for all evidence.
- Cite every claim with exact source file + line at the anchor ref.
- Do NOT use Jev or any external decision tool.
- Stay within 8 Octocode calls total (schema discovery calls are free).
- Do not access or read \`cases.json\` hypotheses — form your own diagnosis from source.

## Output format
Write your answer to: runs/baseline/${c.id}/answer.md

Include:
1. **Root cause** — exact mechanism, source file(s), line(s), function name(s)
2. **Fix proposal** — minimal code change with rationale
3. **Evidence** — bulleted source citations at anchorRef
4. **Confidence** — low/medium/high + one sentence explaining why

Write your call log to: runs/baseline/${c.id}/result.json
\`\`\`json
{
  "arm": "baseline",
  "caseId": "${c.id}",
  "repo": "${c.repo}",
  "issueNumber": ${c.issueNumber},
  "octocodeCalls": <number>,
  "octocodeQueryRows": <number>,
  "toolErrorRows": <number>,
  "startedAt": "<ISO8601>",
  "endedAt": "<ISO8601>",
  "confidence": "low|medium|high"
}
\`\`\`
`;
}

function treatmentPrompt(c) {
  return `\
# Bug-Triage Benchmark — TREATMENT arm (Jev-enabled)
Suite: bug-triage-v1  Case: ${c.id}  Repo: ${c.repo}  Issue: #${c.issueNumber}
Anchor ref: \`${c.anchorRef}\`  Octocode call cap: 8

## Your task
${c.task}

## Rules
- Use Octocode tools (ghSearch, ghGetFileContent, ghSearchHistory, ghGetHistoryItem) for all evidence.
- Cite every claim with exact source file + line at the anchor ref.
- Use the \`octocode-jev-reasoning-loop\` skill — but ONLY at a genuine \`disputed_inference\` gate.
- THINK → GATE first: if the step is deterministic or missing_fact, do NOT call Jev.
- At most 1 Jev call total. Record before/after decision snapshots.
- Stay within 8 Octocode calls total (schema discovery + Jev calls are free).
- Do not access or read \`cases.json\` hypotheses — form your own diagnosis from source.

## Jev gate decision process
Before calling Jev, write a THINK block:
  - Observations (with source anchors)
  - 2–3 competing hypotheses you have formed
  - What observation would falsify your leading hypothesis
  - Gate classification: deterministic / missing_fact / disputed_inference

Only proceed to call Jev if classification = \`disputed_inference\`.

Save your pre-Jev state to: runs/treatment/${c.id}/decision-before.json
\`\`\`json
{
  "caseId": "${c.id}",
  "observations": ["..."],
  "hypotheses": ["H1: ...", "H2: ..."],
  "leadingHypothesis": "H1",
  "falsifier": "...",
  "gateClassification": "disputed_inference",
  "jevWillBeCalled": true
}
\`\`\`

Save your post-Jev state to: runs/treatment/${c.id}/decision-after.json
\`\`\`json
{
  "caseId": "${c.id}",
  "jevRoute": "hypothesis_triage|hunch_check|reflection_delta|hallucination_gate",
  "jevChoice": "...",
  "jevProbability": 0.0,
  "decisionChangedByJev": true,
  "reasoning": "..."
}
\`\`\`

Save Jev call log to: runs/treatment/${c.id}/jev-calls.json
\`\`\`json
{
  "calls": [
    {
      "inputTokens": <number>,
      "outputTokens": <number>,
      "latencyMs": <number>,
      "route": "...",
      "model": "jev-1.13.0"
    }
  ]
}
\`\`\`

## Output format
Write your answer to: runs/treatment/${c.id}/answer.md

Include:
1. **Root cause** — exact mechanism, source file(s), line(s), function name(s)
2. **Fix proposal** — minimal code change with rationale
3. **Evidence** — bulleted source citations at anchorRef
4. **Jev usage** — gate classification used, whether Jev was called, what it changed (or "not called — deterministic")
5. **Confidence** — low/medium/high + one sentence

Write your call log to: runs/treatment/${c.id}/result.json
\`\`\`json
{
  "arm": "treatment",
  "caseId": "${c.id}",
  "repo": "${c.repo}",
  "issueNumber": ${c.issueNumber},
  "octocodeCalls": <number>,
  "octocodeQueryRows": <number>,
  "toolErrorRows": <number>,
  "jevCalled": true,
  "jevCalls": <number>,
  "jevInputTokens": <number>,
  "jevOutputTokens": <number>,
  "jevLatencyMs": <number>,
  "gateClassification": "disputed_inference",
  "decisionChangedByJev": true,
  "startedAt": "<ISO8601>",
  "endedAt": "<ISO8601>",
  "confidence": "low|medium|high"
}
\`\`\`
`;
}

// ── CLI ──────────────────────────────────────────────────────────────────────

const args = process.argv.slice(2);

if (args[0] === '--freeze') {
  freeze();
} else if (args[0] === '--validate') {
  validate();
} else if (args[0] === '--preflight') {
  await preflight();
} else if (args[0] === '--prompts') {
  for (const c of CASES) {
    console.log('\n' + '═'.repeat(80));
    console.log(`=== ${c.id} BASELINE ===`);
    console.log(baselinePrompt(c));
    console.log(`\n=== ${c.id} TREATMENT ===`);
    console.log(treatmentPrompt(c));
  }
} else if (args[0] === '--prompt') {
  const id = args[1];
  const arm = args[2];
  const c = CASES.find(x => x.id === id);
  if (!c) { console.error(`Unknown case: ${id}`); process.exit(1); }
  if (arm === 'baseline') console.log(baselinePrompt(c));
  else if (arm === 'treatment') console.log(treatmentPrompt(c));
  else { console.error('arm must be baseline or treatment'); process.exit(1); }
} else if (args[0] === '--cases') {
  console.log(CASES.map(c => `${c.id}  ${c.repo}#${c.issueNumber}  ${c.title}`).join('\n'));
} else {
  console.log(`bug-triage-v1 harness

Commands:
  --freeze               Hash key files → frozen.json (run before spawning agents)
  --validate             Verify frozen.json hashes still match
  --preflight            Check Octocode CLI is reachable
  --prompts              Print all 20 agent prompts (10 cases × 2 arms)
  --prompt <ID> <arm>    Print one agent prompt  (arm: baseline | treatment)
  --cases                List all case IDs and titles

Agent output dirs:
  runs/baseline/BUG-{id}/   answer.md  result.json
  runs/treatment/BUG-{id}/  answer.md  result.json  jev-calls.json  decision-before.json  decision-after.json

After agents complete:
  node inspect.mjs --save       verify completeness + integrity
  node judge.mjs --all          spawn 10 judge agents
  node inspect.mjs --aggregate  write RESULTS.md
`);
}
