// One matrix row on both surfaces: walk every required continuation, compare the
// union with the oracle, then compare the surfaces. Failures carry the exact call.
import { canon } from '../tool-health/calls.mjs';
import { statusClass } from '../tool-health/driver.mjs';

const byText = (a, b) => (a < b ? -1 : a > b ? 1 : 0);
const ordered = rows => rows.map(canon).toSorted(byText);
const short = value => JSON.stringify(value)?.slice(0, 600);
const credentialHeader = /^(?:authorization|proxy-authorization|cookie|set-cookie|(?:x-)?api-key)$/i;
const providerReceipt = headers => headers.map(entry => Object.fromEntries(Object.entries(entry).filter(([name]) => !credentialHeader.test(name))));

export function firstDifference(expected, actual) {
  const a = ordered(expected), b = ordered(actual);
  const counts = new Map();
  for (const item of a) counts.set(item, (counts.get(item) ?? 0) + 1);
  const extra = [];
  for (const item of b) {
    const left = counts.get(item) ?? 0;
    if (left) counts.set(item, left - 1); else extra.push(item);
  }
  const missing = [...counts].flatMap(([item, count]) => Array.from({ length: count }, () => item));
  return missing.length || extra.length ? { missing: missing.slice(0, 5), extra: extra.slice(0, 5), missingCount: missing.length, extraCount: extra.length } : null;
}

export async function walkOn(worker, { surface, tool, input, ledger }) {
  const pages = [];
  const report = await worker.driver.explore(tool, input, {
    surface, maxCalls: 1,
    call: async (name, request) => {
      const result = await worker.call(surface, name, request);
      if (name === tool) pages.push(result);
      ledger?.observe(worker, { surface, tool: name, result });
      return result;
    },
  });
  return { pages, report };
}

function refusalProblem(root, refusal, cls) {
  const code = root?.results?.[0]?.data?.errorCode ?? root?.errorCode;
  return code === refusal ? [] : [['refusal', `expected ${refusal}, got ${code ?? cls}: ${short(root)}`]];
}

/** A refusal can prove only the input fields its own policy actually judged. */
const valuesProved = (spec, testCase, refusal) => (refusal && spec.refusalValues ? spec.refusalValues(testCase) : testCase.values);

/** One surface's judgment of a row: refusal, admission, evidence, order. */
async function judge(spec, worker, { testCase, surface, walked, expected, refusal }) {
  const { pages, report } = walked;
  const problems = report.failures.map(failure => [`walk.${failure.check}`, `${failure.detail} [at ${failure.at}]`]);
  const root = report.root?.json;
  const cls = statusClass(root);
  if (refusal) return { problems: [...problems, ...refusalProblem(root, refusal, cls)] };
  if (!['ok', 'empty', 'outOfRange'].includes(cls)) {
    problems.push(['admitted', `the schema admits this row, the tool answered ${cls}: ${short(root)}`]);
    return { problems };
  }
  const judged = await spec.judge?.(worker, { testCase, surface, pages });
  if (judged) problems.push(['judge', judged]);
  if (!expected) return { problems };
  const evidence = spec.observed(worker.context, testCase, pages);
  const difference = firstDifference(expected, evidence);
  if (difference) problems.push(['evidence', JSON.stringify(difference)]);
  const order = spec.check?.(worker.context, testCase, pages);
  if (order) problems.push(['order', order]);
  return { problems, evidence, surface };
}

export async function runRow(worker, spec, testCase, ledger) {
  const failures = [];
  const fail = failure => failures.push({ id: testCase.id, tool: testCase.tool, input: testCase.input, ...failure });
  const input = spec.resolve ? spec.resolve(worker.context, testCase.input) : testCase.input;
  const refusal = spec.refusal?.(worker.context, testCase);
  const expected = refusal ? null : spec.truth?.(worker.context, testCase);
  const evidence = {};
  const observations = {};
  const passed = new Set();
  for (const surface of worker.surfaces) {
    worker.context.providerHeaders.length = 0;
    const walked = await walkOn(worker, { surface, tool: testCase.tool, input, ledger });
    // A check may consume the provider receipt. Keep full failed-row evidence
    // before judgment, without retaining credential-bearing request headers.
    const observation = { pages: walked.pages, providerHeaders: providerReceipt(worker.context.providerHeaders) };
    observations[surface] = observation;
    const verdict = await judge(spec, worker, { testCase, surface, walked, expected, refusal });
    for (const [check, detail] of verdict.problems) fail({ check, surface, detail, observation });
    if (verdict.evidence) evidence[surface] = verdict.evidence;
    if (!verdict.problems.length) passed.add(surface);
  }
  const parity = evidence.cli && evidence.mcp ? firstDifference(evidence.cli, evidence.mcp) : null;
  if (parity) fail({ check: 'parity', surface: 'both', detail: JSON.stringify(parity), observations });
  return { failures, passed, hasOracle: !!expected || !!refusal || !!spec.judge, provedValues: valuesProved(spec, testCase, refusal) };
}
