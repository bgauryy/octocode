// Runs the typed-question regression cases in regressions.json through the local CLI clasify
// and scores each answer against its expected label. Every call is billed by the configured
// provider; without a key the tool refuses unbilled (missingConfiguration) and the case is skipped.
//
//   node packages/octocode-benchmark/local/e2e/fixtures/clasify/run-regressions.mjs [--case ID,...]
//
// Prints one line per case and call and writes results/clasify-regressions/<stamp>.json; copy the
// observed answers into regressions.json by hand. Exit 0 pass or skipped, 1 a wrong answer, 2 an error.
import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(HERE, '../../../../../..');
export const FIXTURE_FILE = path.join(HERE, 'regressions.json');
const FIXTURES_TOKEN = '{{FIXTURES}}';

/** Replace the {{FIXTURES}} prefix with this folder's path relative to the repository root. */
export function resolvePath(value, root = ROOT) {
  return value.replaceAll(FIXTURES_TOKEN, path.relative(root, HERE));
}

/** The clasify envelope for one call: one evaluate row on the case's source, one question. */
export function buildQuery(testCase, call, root = ROOT) {
  const { tool, path: sourcePath } = testCase.source;
  const question = { type: call.type, instructions: call.instructions };
  if (call.criteria !== undefined) question.criteria = call.criteria;
  return {
    queries: [{
      operation: 'evaluate',
      mainGoal: `Regression ${testCase.id}`,
      source: { tool, query: { path: resolvePath(sourcePath, root) } },
      questions: { answer: question },
    }],
  };
}

/** Every object under value matching predicate, depth first. */
function collect(value, predicate, out = []) {
  if (value && typeof value === 'object') {
    if (predicate(value)) out.push(value);
    for (const child of Object.values(value)) collect(child, predicate, out);
  }
  return out;
}

/** The first error code in a response, top level or per row. */
export function errorCode(response) {
  return collect(response, node => typeof node.errorCode === 'string')[0]?.errorCode ?? null;
}

/** The answer object for question key `answer` and the response's fresh usage. */
export function extractAnswer(response) {
  const answer = collect(response, node => node.answers && typeof node.answers === 'object' && 'answer' in node.answers)[0]?.answers.answer ?? null;
  const usage = collect(response, node => node.usage && typeof node.usage === 'object' && 'calls' in node.usage)[0]?.usage ?? null;
  return { answer, usage };
}

/** Index of the largest probability, or null without a distribution. */
function argmax(probabilities) {
  if (!probabilities || typeof probabilities !== 'object') return null;
  const entries = Array.isArray(probabilities) ? probabilities.map((p, i) => [i, p]) : Object.entries(probabilities);
  if (!entries.length) return null;
  return entries.reduce((best, entry) => (entry[1] > best[1] ? entry : best))[0];
}

/** A comparison like ">0.5", "<0.3", ">=0.6" applied to value. */
export function meetsBound(value, bound) {
  const match = /^(>=|<=|>|<)\s*(-?\d+(?:\.\d+)?)$/.exec(bound ?? '');
  if (!match || typeof value !== 'number') return false;
  const limit = Number(match[2]);
  return { '>': value > limit, '<': value < limit, '>=': value >= limit, '<=': value <= limit }[match[1]];
}

/**
 * The observed label of an answer: noul → yes when P(yes) > 0.5; choice → the provider's label;
 * score → the criterion with the largest probability (the expected level `score` is recorded too).
 */
export function observedLabel(call, answer) {
  if (!answer) return null;
  if (call.type === 'noul') return typeof answer.noul === 'number' ? (answer.noul > 0.5 ? 'yes' : 'no') : null;
  if (call.type === 'choice') return answer.choice ?? null;
  if (call.type === 'score') {
    const top = argmax(answer.probabilities);
    if (top !== null) {
      const index = Number(top);
      return Number.isInteger(index) ? (call.criteria[index] ?? String(top)) : String(top);
    }
    return typeof answer.score === 'number' ? (call.criteria[Math.round(answer.score)] ?? null) : null;
  }
  return null;
}

/** Pass when the label matches and, for noul, P(yes) meets the expected bound. */
export function scoreCall(call, answer) {
  const label = observedLabel(call, answer);
  const checks = { label: label === call.expect.label };
  if (call.expect.noul !== undefined) checks.noul = meetsBound(answer?.noul, call.expect.noul);
  return {
    pass: Object.values(checks).every(Boolean),
    checks,
    observed: {
      label,
      ...(answer?.noul !== undefined && { noul: answer.noul }),
      ...(answer?.choice !== undefined && { choice: answer.choice }),
      ...(answer?.score !== undefined && { score: answer.score }),
      ...(answer?.probabilities !== undefined && { probabilities: answer.probabilities }),
      ...(answer?.confidence !== undefined && { confidence: answer.confidence }),
    },
  };
}

/** Run the local CLI clasify on one envelope; returns its parsed JSON. */
export function cliInvoke(envelope, { root = ROOT, env = process.env } = {}) {
  const cli = path.join(root, 'packages/octocode/out/octocode.js');
  const run = spawnSync(process.execPath, [cli, 'clasify', '--json', '--input', '-'], {
    cwd: root, env, input: JSON.stringify(envelope), encoding: 'utf8', maxBuffer: 256 * 1024 * 1024,
  });
  if (run.error) throw run.error;
  try {
    return JSON.parse(run.stdout);
  } catch {
    throw new Error(`clasify exit ${run.status}: unparsable stdout ${JSON.stringify(run.stdout)} stderr ${JSON.stringify(run.stderr)}`);
  }
}

/**
 * Run every call of the selected cases. A missingConfiguration refusal (no key; unbilled) marks
 * every remaining call skipped without invoking again; any other error fails that call.
 */
export async function runRegressions({ fixture, invoke, only = null, root = ROOT }) {
  const results = [];
  let skipped = false;
  let providerCalls = 0;
  for (const testCase of fixture.cases) {
    if (only && !only.includes(testCase.id)) continue;
    const calls = [];
    for (const call of testCase.calls) {
      const base = { type: call.type, instructions: call.instructions, expect: call.expect };
      if (skipped) { calls.push({ ...base, status: 'skipped', reason: 'missingConfiguration' }); continue; }
      const response = await invoke(buildQuery(testCase, call, root));
      const code = errorCode(response);
      const { answer, usage } = extractAnswer(response);
      providerCalls += usage?.calls ?? 0;
      if (code === 'missingConfiguration' && !answer) {
        skipped = true;
        calls.push({ ...base, status: 'skipped', reason: code });
        continue;
      }
      if (!answer) { calls.push({ ...base, status: 'error', errorCode: code, response }); continue; }
      const scored = scoreCall(call, answer);
      calls.push({ ...base, status: scored.pass ? 'pass' : 'fail', ...scored, usage });
    }
    const statuses = new Set(calls.map(call => call.status));
    const status = statuses.has('error') ? 'error' : statuses.has('fail') ? 'fail' : statuses.has('skipped') ? 'skipped' : 'pass';
    results.push({ id: testCase.id, status, calls });
  }
  return { skipped, providerCalls, results };
}

async function main(argv) {
  const caseArg = argv.indexOf('--case');
  const only = caseArg >= 0 ? argv[caseArg + 1].split(',') : null;
  const fixture = JSON.parse(fs.readFileSync(FIXTURE_FILE, 'utf8'));
  const run = await runRegressions({ fixture, invoke: envelope => cliInvoke(envelope), only });
  const stamp = new Date().toISOString();
  const outDir = path.join(ROOT, 'packages/octocode-benchmark/local/e2e/results/clasify-regressions');
  fs.mkdirSync(outDir, { recursive: true });
  const outFile = path.join(outDir, `${stamp.replaceAll(':', '')}.json`);
  fs.writeFileSync(outFile, `${JSON.stringify({ at: stamp, ...run }, null, 1)}\n`);
  for (const result of run.results) {
    console.log(`${result.status.padEnd(7)} ${result.id}`);
    for (const call of result.calls) console.log(`  ${call.status.padEnd(7)} ${call.type} expect=${call.expect.label} observed=${JSON.stringify(call.observed ?? call.errorCode ?? call.reason)}`);
  }
  console.log(`provider calls: ${run.providerCalls}; results: ${path.relative(ROOT, outFile)}`);
  return run.skipped ? 0 : run.results.some(result => result.status === 'error') ? 2 : run.results.some(result => result.status === 'fail') ? 1 : 0;
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  process.exitCode = await main(process.argv.slice(2));
}
