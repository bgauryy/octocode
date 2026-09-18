#!/usr/bin/env node
/**
 * bug-triage-v1 judge
 *
 * Usage:
 *   node judge.mjs --prompt BUG-01     print the judge prompt for one case (for external agent)
 *   node judge.mjs --all               print all 10 judge prompts
 *   node judge.mjs --validate BUG-01   validate grades/BUG-01/grade.json schema
 *   node judge.mjs --validate-all      validate all saved grade files
 *
 * Each judge agent:
 *   - Receives both arms as Response A / Response B (randomised per case)
 *   - Re-opens cited sources via Octocode (≤ 3 calls)
 *   - Scores each response 0–10 on the rubric
 *   - Writes grades/BUG-{id}/grade.json
 *
 * Randomisation: case id parity determines A/B assignment:
 *   odd-numbered BUG-id (BUG-01,03,05,07,09)  → A=baseline, B=treatment
 *   even-numbered BUG-id (BUG-02,04,06,08,10) → A=treatment, B=baseline
 * De-blinding happens in inspect.mjs --aggregate only.
 */

import { readFileSync, existsSync } from 'fs';
import { resolve, dirname } from 'path';
import { fileURLToPath } from 'url';

const __dir = dirname(fileURLToPath(import.meta.url));
const rel = (...p) => resolve(__dir, ...p);

const CASES = JSON.parse(readFileSync(rel('cases.json'), 'utf8')).cases;

const RUBRIC = `\
## Scoring rubric (0–10 total per response)

| Dimension | Points | What earns full marks |
|---|---|---|
| Root-cause correctness | 0–4 | Exact mechanism identified; correct source file, function, line; matches the actual defect |
| Fix specificity & groundedness | 0–3 | Minimal targeted fix; every claim traced to exact source at anchorRef; no speculative edits |
| Research flow quality | 0–2 | Evidence precedes claims; no unsupported assertions; citations are real and at the pinned ref |
| Calibration | 0–1 | Confidence proportional to evidence; no "clearly/obviously" without proof; hedges where uncertain |

### Major false claim (flag separately)
A major false claim is a factual statement that the source directly contradicts — e.g. naming the wrong function as root cause, claiming a line does something it does not. One major false claim caps the total at 5.`;

function assignment(c) {
  const n = parseInt(c.id.replace('BUG-', ''), 10);
  return n % 2 === 1
    ? { A: 'baseline', B: 'treatment' }
    : { A: 'treatment', B: 'baseline' };
}

function readAnswer(arm, id) {
  const p = rel('runs', arm, id, 'answer.md');
  return existsSync(p) ? readFileSync(p, 'utf8') : '[MISSING]';
}

function judgePrompt(c) {
  const { A, B } = assignment(c);
  const ansA = readAnswer(A, c.id);
  const ansB = readAnswer(B, c.id);

  return `\
# Bug-Triage Benchmark — JUDGE
Suite: bug-triage-v1  Case: ${c.id}  Repo: ${c.repo}#${c.issueNumber}
Anchor ref: \`${c.anchorRef}\`  Octocode call cap: 3

## Case task
${c.task}

${RUBRIC}

## Your job
1. Read Response A and Response B below.
2. Open cited sources via Octocode at \`${c.anchorRef}\` to verify key claims (≤ 3 calls).
3. Score each response independently — do not consider which arm wrote it.
4. Write your grade to: grades/${c.id}/grade.json

## grade.json schema
\`\`\`json
{
  "caseId": "${c.id}",
  "scoreA": <0–10>,
  "scoreB": <0–10>,
  "winner": "A|B|tie",
  "majorFalseClaimA": false,
  "majorFalseClaimB": false,
  "jevChangeEvident": false,
  "rationaleA": "<2–4 sentences>",
  "rationaleB": "<2–4 sentences>",
  "octocodeCallsUsed": <number>,
  "gradedAt": "<ISO8601>"
}
\`\`\`

Field notes:
- \`jevChangeEvident\`: set true if Response B's reasoning shows a visible mid-investigation direction shift with explicit Jev-attributed rationale. Ignore if no such shift is visible.
- If a response is MISSING, score it 0 and note "MISSING" in rationale.
- Do NOT try to identify which arm is which — score only on content quality.

---

## Response A

${ansA}

---

## Response B

${ansB}

---

Write grades/${c.id}/grade.json now.
`;
}

// ── grade validation ──────────────────────────────────────────────────────────

const REQUIRED_GRADE_FIELDS = [
  'caseId', 'scoreA', 'scoreB', 'winner',
  'majorFalseClaimA', 'majorFalseClaimB',
  'jevChangeEvident', 'rationaleA', 'rationaleB',
  'octocodeCallsUsed', 'gradedAt',
];

function validateGrade(id) {
  const p = rel('grades', id, 'grade.json');
  if (!existsSync(p)) { console.error(`MISSING: grades/${id}/grade.json`); return false; }
  const g = JSON.parse(readFileSync(p, 'utf8'));
  let ok = true;
  for (const f of REQUIRED_GRADE_FIELDS) {
    if (!(f in g)) { console.error(`  grades/${id}/grade.json missing field: ${f}`); ok = false; }
  }
  if (typeof g.scoreA !== 'number' || g.scoreA < 0 || g.scoreA > 10) {
    console.error(`  grades/${id}/grade.json scoreA out of range`); ok = false;
  }
  if (typeof g.scoreB !== 'number' || g.scoreB < 0 || g.scoreB > 10) {
    console.error(`  grades/${id}/grade.json scoreB out of range`); ok = false;
  }
  if (!['A', 'B', 'tie'].includes(g.winner)) {
    console.error(`  grades/${id}/grade.json winner must be A|B|tie`); ok = false;
  }
  if (ok) console.log(`✓ grades/${id}/grade.json valid  A=${g.scoreA} B=${g.scoreB} winner=${g.winner}`);
  return ok;
}

// ── CLI ──────────────────────────────────────────────────────────────────────

const args = process.argv.slice(2);

if (args[0] === '--prompt') {
  const id = args[1];
  const c = CASES.find(x => x.id === id);
  if (!c) { console.error(`Unknown case: ${id}`); process.exit(1); }
  console.log(judgePrompt(c));
} else if (args[0] === '--all') {
  for (const c of CASES) {
    console.log('\n' + '═'.repeat(80));
    console.log(judgePrompt(c));
  }
} else if (args[0] === '--validate') {
  const ok = validateGrade(args[1]);
  process.exit(ok ? 0 : 1);
} else if (args[0] === '--validate-all') {
  let allOk = true;
  for (const c of CASES) allOk = validateGrade(c.id) && allOk;
  process.exit(allOk ? 0 : 1);
} else {
  console.log(`bug-triage-v1 judge

Commands:
  --prompt BUG-{id}     Print judge prompt for one case (paste to judge agent)
  --all                 Print all 10 judge prompts
  --validate BUG-{id}   Validate one grade file
  --validate-all        Validate all grade files

A/B assignment (so judge stays blinded):
  BUG-01,03,05,07,09   A=baseline  B=treatment
  BUG-02,04,06,08,10   A=treatment B=baseline
De-blinding happens in: node inspect.mjs --aggregate
`);
}
