#!/usr/bin/env node

import { createHash } from 'node:crypto';
import { readFileSync, existsSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const subjectPaths = ['SKILL.md', 'references/orchestration-contract.md', 'references/spawn-gate.md', 'references/decompose.md', 'references/packets.md', 'references/evaluation.md', 'references/awareness.md', 'references/completion.md'];
const required = [...subjectPaths, 'README.md', 'evals/cases.json', 'references/references.md'];
const usage = `eval-contract [--results <fresh-receipt.json>] [--print-digest]\n\nValidates the orchestration suite and optionally grades provenance-bearing fresh-agent outputs.\n--results <path>  grade a fresh receipt for the current subject digest\n--print-digest    print current subject and case digests\n--help            show this help`;

function sha(text) { return createHash('sha256').update(text).digest('hex'); }
function digestSubject() { return sha(subjectPaths.map((path) => `${path}\0${readFileSync(join(root, path), 'utf8')}\0`).join('')); }
function fail(message, code = 1) { console.error(message); process.exit(code); }

const args = process.argv.slice(2);
if (args.includes('--help')) { console.log(usage); process.exit(0); }
const allowed = new Set(['--help', '--print-digest', '--results']);
for (const arg of args) if (arg.startsWith('--') && !allowed.has(arg)) fail(`unknown option: ${arg}\n${usage}`, 2);
const resultIndex = args.indexOf('--results');
if (resultIndex >= 0 && (!args[resultIndex + 1] || args[resultIndex + 1].startsWith('--'))) fail(`--results requires a file path\n${usage}`, 2);
const suiteText = readFileSync(join(root, 'evals/cases.json'), 'utf8');
const caseDigest = sha(suiteText);
if (args.includes('--print-digest')) { console.log(JSON.stringify({ subjectDigest: digestSubject(), caseDigest })); process.exit(0); }
const resultsPath = resultIndex >= 0 ? resolve(args[resultIndex + 1]) : null;
const failures = required.filter((path) => !existsSync(join(root, path))).map((path) => `missing ${path}`);
const suite = JSON.parse(suiteText);
const ids = new Set();
const decisions = new Set(['SOLO', 'BATCH', 'SPAWN', 'HANDOFF']);
const safetyFields = {
  owner: new Set(['PARENT', 'SPECIALIST']),
  authority: new Set(['PROCEED', 'ASK', 'STOP']),
  proof: new Set(['PARENT_ANCHOR', 'MIXED_INDEPENDENT']),
  cleanup: new Set(['NONE', 'PROVE_THEN_REMOVE', 'ASK_BEFORE_REMOVE']),
  budget: new Set(['BOUNDED', 'NOT_APPLICABLE'])
};

for (const testCase of suite.cases ?? []) {
  if (!testCase.id || ids.has(testCase.id)) failures.push(`invalid or duplicate case id: ${testCase.id ?? '<missing>'}`);
  ids.add(testCase.id);
  if (!['train', 'regression'].includes(testCase.split)) failures.push(`${testCase.id}: invalid split`);
  if (!['trigger', 'near-miss'].includes(testCase.activation)) failures.push(`${testCase.id}: invalid activation`);
  if (!decisions.has(testCase.expected.decision)) failures.push(`${testCase.id}: invalid decision`);
  for (const [field, values] of Object.entries(safetyFields)) if (testCase.expected[field] !== undefined && !values.has(testCase.expected[field])) failures.push(`${testCase.id}: invalid ${field}`);
  for (const flag of ['eval', 'awareness']) if (typeof testCase.expected[flag] !== 'boolean') failures.push(`${testCase.id}: ${flag} must be boolean`);
  if (testCase.split === 'regression' && typeof testCase.expected.tdd !== 'boolean') failures.push(`${testCase.id}: tdd must be boolean`);
}
for (const activation of ['trigger', 'near-miss']) if (suite.cases.filter((item) => item.split === 'regression' && item.activation === activation).length < 4) failures.push(`need at least four regression ${activation} cases`);
for (const field of Object.keys(safetyFields)) if (suite.cases.filter((item) => item.split === 'regression' && item.expected[field] !== undefined).length < 2) failures.push(`need at least two regression cases grading ${field}`);

if (existsSync(join(root, 'SKILL.md'))) {
  const skill = readFileSync(join(root, 'SKILL.md'), 'utf8');
  if (!skill.startsWith('---\nname: octocode-subagent\n')) failures.push('frontmatter name mismatch');
  if (!skill.includes('description: "Use when ')) failures.push('description is not trigger-led');
  if (skill.split('\n').length > 50) failures.push('SKILL.md exceeds 50 lines');
}
if (resultsPath && !existsSync(resultsPath)) failures.push(`missing results file: ${resultsPath}`);

if (!failures.length && resultsPath) {
  const receipt = JSON.parse(readFileSync(resultsPath, 'utf8'));
  if (receipt.subjectDigest !== digestSubject()) failures.push('forward results are stale for the current skill digest');
  if (receipt.caseDigest !== caseDigest) failures.push('forward results are stale for the current case digest');
  const observed = new Map();
  for (const run of receipt.runs ?? []) {
    for (const field of ['task', 'model', 'reasoningEffort', 'forkTurns', 'packet', 'rawOutput']) if (typeof run[field] !== 'string' || !run[field]) failures.push(`run missing provenance field: ${field}`);
    if (run.forkTurns !== 'none') failures.push(`${run.task ?? '<run>'}: evaluator did not use a fresh fork`);
    let parsed;
    try { parsed = JSON.parse(run.rawOutput); } catch { failures.push(`${run.task ?? '<run>'}: rawOutput is not JSON`); continue; }
    for (const item of Array.isArray(parsed) ? parsed : [parsed]) {
      if (!item.id || observed.has(item.id)) failures.push(`duplicate or missing observed id: ${item.id ?? '<missing>'}`);
      observed.set(item.id, item);
    }
  }
  const regression = suite.cases.filter((item) => item.split === 'regression');
  for (const testCase of regression) {
    const result = observed.get(testCase.id);
    if (!result) { failures.push(`${testCase.id}: missing observed result`); continue; }
    if (result.prompt !== testCase.prompt) failures.push(`${testCase.id}: result prompt does not match suite`);
    for (const field of ['decision', 'tdd', 'eval', 'awareness', ...Object.keys(safetyFields)]) if (testCase.expected[field] !== undefined && result[field] !== testCase.expected[field]) failures.push(`${testCase.id}: ${field} expected ${testCase.expected[field]} observed ${result[field]}`);
    if (result.activate !== (testCase.activation === 'trigger')) failures.push(`${testCase.id}: activation mismatch`);
    if (typeof result.rationale !== 'string' || result.rationale.length < 20) failures.push(`${testCase.id}: missing rationale`);
  }
  if (observed.size !== regression.length) failures.push('observed result count must equal regression case count');
}

if (failures.length) fail(failures.join('\n'));
const count = suite.cases.filter((item) => item.split === 'regression').length;
if (resultsPath) console.log(`RECEIPT PASS ${count}/${count} fresh-agent behavior cases; provenance, subject digest, and case digest current`);
else console.log(`SUITE PASS ${count} regression cases; no fresh behavior receipt graded`);
