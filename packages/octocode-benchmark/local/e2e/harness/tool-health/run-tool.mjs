// One tool's tool-health run, phase by phase: happy (+ parity), continuation
// walks, empty, extra seeds, invalid input, the case's additional checks,
// then latency. `suite` = { driver, surfaces (surfaces.mjs), deep, validateOutput, validateInput }.
import { contract } from '../contract.mjs';
import { CASES } from './cases.mjs';
import { dataOf, errorCodes, rowsOf, statusClass } from './driver.mjs';
import { firstDiff, firstError, parityView } from './compare.mjs';
import { tokenFor } from './surfaces.mjs';
import { inputBoundaries } from './input-boundaries.mjs';
import { advancedContext } from './advanced-context.mjs';

const ERROR_CODES = new Set(Object.keys(contract.errorCodes));
const EXIT = { ok: 0, empty: 1, invalid: 2, tool: 5 };
/** A calm machine's CLI invalid-input round trip; above it the latency budget scales, up to MAX_LOAD_FACTOR. */
const CALM_CLI_MS = 500;
const MAX_LOAD_FACTOR = 4;

/** Non-page leads executed per explored seed. */
const callCap = deep => (deep ? 40 : 6);
/** Pages per walk: a runaway guard, not a size bound (the 3,000-line patch fixture takes about 12 pages at the default 20,000-char page). */
const pageCap = deep => (deep ? 40 : 24);

/** Default Array#sort order (UTF-16 code units). */
const byCodeUnit = (a, b) => (a < b ? -1 : a > b ? 1 : 0);
const resolveQuery = (ctx, query) => (typeof query === 'function' ? query(ctx) : query);
const caseInput = (ctx, spec) => spec.input ? resolveQuery(ctx, spec.input) : { queries: [resolveQuery(ctx, spec.query)] };

/**
 * A parked tool (contract `disabled`): the CLI refuses its happy and invalid seeds
 * alike with the one disabled error (exit 5) and does nothing; MCP never lists it
 * (the surfaces check). Its case stays for the day it returns.
 */
export async function runParked(suite, name, spec) {
  const t0 = performance.now();
  const { cliCall, ctx } = suite.surfaces;
  const disabled = { error: `Tool ${name} is not available in this native runtime: it is disabled in this build.`, errorCode: 'toolUnavailable' };
  const checks = [];
  const timings = [];
  for (const label of ['happy', 'invalid']) {
    const res = await cliCall(name, caseInput(ctx, spec[label]));
    timings.push({ phase: label, surface: 'cli', ms: res.ms, responseBytes: res.responseBytes });
    const ok = res.status === EXIT.tool && JSON.stringify(res.raw) === JSON.stringify(disabled);
    checks.push({ check: `parked.${label}`, ok, detail: `exit ${res.status} ${res.stdout.slice(0, 200)}` });
  }
  const failed = checks.filter(c => !c.ok);
  return {
    tool: name, status: failed.length ? 'FAIL' : 'PASS', reason: 'parked (disabled on every surface)', ms: Math.round(performance.now() - t0),
    slowestMs: Math.max(...timings.map(t => t.ms)), calls: timings.length,
    firstFailure: failed[0] ? `${failed[0].check}: ${failed[0].detail}` : undefined, checks, timings,
  };
}

/** The state of one tool run: its checks, timings, skips, explore reports and call helpers. */
function createRun(suite, { name, spec, cwd, server }) {
  const { surfaces: s } = suite;
  const run = { suite, name, spec, cwd, server, ctx: s.ctx, phase: 'happy', checks: [], timings: [], skips: [], advancedCalls: [], reports: { cli: [], mcp: [] } };
  run.surfaces = spec.cliOnly ? ['cli'] : ['cli', 'mcp'];
  run.record = (check, ok, detail) => { run.checks.push({ check, ok: !!ok, detail: detail === undefined ? '' : String(detail).slice(0, 300) }); return !!ok; };
  run.na = (check, reason) => run.checks.push({ check, ok: null, na: reason });
  const timed = res => { run.timings.push({ phase: run.phase, surface: res.surface, ms: res.ms, responseBytes: res.responseBytes }); return res; };
  /** Any tool on one surface, in this tool's workspace and with its credential. */
  run.callOn = surface => async (tool, input, { env: extraEnv, cwd: directory = cwd } = {}) => timed(surface === 'mcp' ? await s.mcpCall(server, tool, input) : await s.cliCall(tool, input, { cwd: directory, credentialTool: name, extraEnv }));
  run.call = (input, surface) => run.callOn(surface)(name, input);
  run.q = query => ({ queries: [resolveQuery(s.ctx, query)] });
  run.input = entry => caseInput(s.ctx, entry);
  return run;
}

/** Rows of a lead's page: this tool's channel rows (its page lead defaults to the happy rows), another tool's case channels. */
const defaultRows = ({ name, spec }) => (tool, key) => (tool === name
  ? spec.channels?.[key] ?? (key === spec.page.lead ? spec.page.rows ?? spec.happy.rows : undefined)
  : CASES[tool]?.channels?.[key]);

/** Explore `seed` on every surface with the driver; records each distinct failure, then graph parity. */
async function exploreOn(run, label, seed, opts = {}) {
  const { driver, deep } = run.suite;
  const out = {};
  await Promise.all(run.surfaces.map(async s => {
    const report = await driver.explore(run.name, seed, { surface: s, call: run.callOn(s), maxCalls: callCap(deep), maxPages: pageCap(deep), rows: defaultRows(run), root: run.cwd, ...opts });
    report.label = label;
    run.reports[s].push(report);
    out[s] = report;
    const seenFailures = new Set();
    for (const f of report.failures) {
      const k = `${f.check}|${f.detail}`;
      if (seenFailures.has(k)) continue;
      seenFailures.add(k);
      run.record(`explore.${label}.${s}.${f.check}`, false, `${f.detail} [at ${f.at}]`);
    }
    run.record(`explore.${label}.${s}`, true, `${report.calls.length} calls, ${report.chains.length} chains, ${report.skipped.length} skipped, ${report.budgetCut} over budget`);
  }));
  if (out.cli && out.mcp) { const d = run.suite.driver.graphParity(out.cli, out.mcp); run.record(`parity.graph.${label}`, !d, d ?? ''); }
  return out;
}

function recordHappyRows(run, s, res) {
  const { happy } = run.spec;
  const data = dataOf(rowsOf(res.json)[0]);
  const missing = happy.fields.filter(f => data?.[f] === undefined);
  run.record(`happy.fields.${s}`, !missing.length, missing.length ? `missing ${missing.join(', ')} in ${Object.keys(data ?? {}).join(',')}` : '');
  const ids = happy.rows(data, res.json);
  run.record(`happy.rows.${s}`, ids.length >= happy.min && new Set(ids).size === ids.length, `${ids.length} rows (min ${happy.min}), ${ids.length - new Set(ids).size} duplicate`);
  if (happy.check) { const why = happy.check(data); run.record(`happy.check.${s}`, !why, why ?? ''); }
}

/** The happy call answered: CLI exit 0, MCP no error, class ok. */
function recordHappyClass(run, s, res) {
  const cls = statusClass(res.json);
  if (s === 'cli') run.record('happy.exit', res.status === 0, `exit ${res.status}, class ${cls}${cls !== 'ok' ? ` ${firstError(res.json) || res.stdout.slice(0, 160) || res.stderr.slice(0, 160)}` : ''}`);
  else run.record('happy.mcp', !res.isError && cls === 'ok', `isError ${res.isError}, class ${cls} ${firstError(res.json) || (cls === 'broken' ? res.text.slice(0, 160) : '')}`);
}

/** (a) happy path and (e) its parity; the happy seed is explored unchanged. */
async function happyPhase(run) {
  const { spec } = run;
  const happyRuns = await exploreOn(run, 'happy', run.input(spec.happy), spec.happy.explore);
  const happy = Object.fromEntries(run.surfaces.map(s => [s, happyRuns[s].root]));
  for (const s of run.surfaces) {
    const res = happy[s];
    const cls = statusClass(res.json);
    recordHappyClass(run, s, res);
    if (cls === 'ok') recordHappyRows(run, s, res);
  }
  const reason = happy.cli.json && statusClass(happy.cli.json) === 'error' && spec.skipOnError?.(happy.cli.json);
  if (reason) { run.checks.length = 0; run.skips.push(`happy: ${reason}`); }
  if (!spec.cliOnly) {
    const d = firstDiff(parityView(happy.cli.json, run.name), parityView(happy.mcp.json, run.name));
    run.record('parity.happy', !d && statusClass(happy.cli.json) === statusClass(happy.mcp.json), d ?? '');
  } else run.na('parity', 'CLI-only per contract (not exposed over MCP)');
  return { happy, happyRuns };
}

/** Why one surface's walk of `chain` fails ('' when it passes): too short, unended, empty, a union unlike the unpaged rows, or the case's check. */
function walkProblem(run, walk, chain, expected) {
  const minPages = walk.minPages ?? 2;
  if (chain.pages < minPages) return `${chain.pages} pages (min ${minPages})`;
  if (!chain.ended) return `walk did not end within ${pageCap(run.suite.deep)} pages`;
  if (chain.rows.length === 0) return 'no rows on any page';
  if (expected) {
    const missing = expected.filter(x => !chain.seenRows.has(x));
    const extra = chain.rows.filter(x => !expected.includes(x));
    if (missing.length || extra.length) return `paged union differs from unpaged: missing ${missing.length} (${missing.slice(0, 3).join(', ') || '-'}), extra ${extra.length} (${extra.slice(0, 3).join(', ') || '-'})`;
  }
  return walk.check?.(chain.rows, run.ctx) ?? '';
}

/** Record one surface's walk of `walk.lead` from the seed. */
function recordWalk(run, { tag, walk, surface, report, expected }) {
  const chain = report.chains.find(c => c.key === walk.lead && c.from === 'seed');
  const rootCls = statusClass(report.root.json);
  let problem;
  if (rootCls !== 'ok') problem = `page 1 class ${rootCls} ${firstError(report.root.json)}`;
  else if (!chain) problem = `no next.${walk.lead} on page 1`;
  else problem = walkProblem(run, walk, chain, expected);
  run.record(`${tag}.${surface}`, !problem, problem || `${chain.pages} pages, ${chain.rows.length} rows`);
}

/** The rows a walk's union must equal: the case's `expect`, else the unpaged (or happy) call's rows. */
async function expectedRows(run, walk, rowsFn, happy) {
  if (walk.expect) return walk.expect(run.ctx);
  const unpaged = walk.unpaged ? await run.call(run.input(walk.unpaged), 'cli') : walk.label === 'page' ? happy.cli : null;
  return unpaged && statusClass(unpaged.json) === 'ok' ? rowsFn(dataOf(rowsOf(unpaged.json)[0]), unpaged.json) : null;
}

/** (b) continuation: the primary page channel, then the tool's other walks. */
async function walkPhase(run, happy) {
  const { spec, name } = run;
  run.phase = 'continuation';
  const walks = [...(spec.page.na ? [] : [{ label: 'page', ...spec.page }]), ...(spec.walks ?? [])];
  if (spec.page.na) run.na('continuation', spec.page.na);
  if (run.skips.length) return;
  for (const walk of walks) {
    const tag = walk.label === 'page' ? 'continuation' : `walk.${walk.label}`;
    const rowsFn = walk.rows ?? spec.channels?.[walk.lead] ?? spec.happy.rows;
    const expected = await expectedRows(run, walk, rowsFn, happy);
    const runs = await exploreOn(run, walk.label, run.input(walk), { rows: (tool, key) => (tool === name && key === walk.lead ? rowsFn : spec.channels?.[key]) });
    for (const s of run.surfaces) recordWalk(run, { tag, walk, surface: s, report: runs[s], expected });
  }
}

/** (c) empty: exit 1, every row empty, and parity. */
async function emptyPhase(run) {
  const { spec } = run;
  run.phase = 'empty';
  if (spec.empty.na) { run.na('empty', spec.empty.na); return; }
  const runs = await exploreOn(run, 'empty', run.input(spec.empty));
  const cli = runs.cli.root;
  const cls = statusClass(cli.json);
  run.record('empty.exit', cli.status === 1 && cls === 'empty', `exit ${cli.status}, class ${cls} ${firstError(cli.json)}`);
  if (spec.cliOnly) return;
  const mcp = runs.mcp.root;
  const d = firstDiff(parityView(cli.json, run.name), parityView(mcp.json, run.name));
  run.record('parity.empty', !d && statusClass(mcp.json) === cls, d ?? `mcp class ${statusClass(mcp.json)}`);
}

/** More seeds the tool's surface offers (operations, views), explored the same way. */
async function seedsPhase(run) {
  run.phase = 'explore';
  for (const [i, seed] of (run.spec.explore ?? []).filter(x => run.suite.deep || !x.deep).entries()) {
    const label = seed.label ?? `seed${i + 1}`;
    const runs = await exploreOn(run, label, run.input(seed));
    for (const s of run.surfaces) {
      const root = runs[s].root.json;
      const cls = statusClass(root);
      run.record(`${label}.class.${s}`, cls === (seed.class ?? 'ok'), `class ${cls} (expected ${seed.class ?? 'ok'}) ${firstError(root)}`);
      if (seed.check) { const why = seed.check(dataOf(rowsOf(root)[0]), runs[s]); run.record(`${label}.check.${s}`, !why, why ?? ''); }
    }
  }
}

/** (d) invalid input: exit 2 with a typed contract error, the same code over MCP. */
async function invalidPhase(run) {
  run.phase = 'invalid';
  const res = {};
  for (const s of run.surfaces) res[s] = await run.call(run.input(run.spec.invalid), s);
  const code = errorCodes(res.cli.json)[0];
  const error = res.cli.json?.error ?? rowsOf(res.cli.json).find(row => row.data?.errorCode)?.data?.error;
  run.record('invalid.exit', res.cli.status === EXIT.invalid && statusClass(res.cli.json) === 'invalid', `exit ${res.cli.status}, errorCode ${code}`);
  run.record('invalid.typed', ERROR_CODES.has(code) && typeof error === 'string' && error.length > 0, `errorCode ${code}`);
  if (!run.spec.cliOnly) run.record('parity.invalid', res.mcp.isError && errorCodes(res.mcp.json)[0] === code, `mcp isError ${res.mcp.isError}, errorCode ${errorCodes(res.mcp.json)[0] ?? res.mcp.text.slice(0, 100)}`);
  await boundaryPhase(run);
  await rowIsolationPhase(run);
}

async function boundaryPhase(run) {
  const tokens = new Set(run.surfaces.map(surface => tokenFor(run.name, surface)));
  const providerCalls = () => run.ctx.mock.requestsFrom?.filter(request => tokens.has(request.token)).length;
  const before = providerCalls();
  const inputSchema = contract.tools.find(tool => tool.name === run.name).inputSchema;
  for (const [label, input] of inputBoundaries(run.input(run.spec.happy), inputSchema)) {
    run.record(`input.${label}.schema`, run.suite.validateInput(run.name, input) !== null, 'boundary must be rejected by the published schema');
    for (const surface of run.surfaces) {
      const actual = await run.call(input, surface);
      const rejected = errorCodes(actual.json).includes('invalidInput');
      run.record(`input.${label}.${surface}`, rejected && (surface === 'cli' ? actual.status === EXIT.invalid : actual.isError), JSON.stringify(actual.json));
    }
  }
  if (before !== undefined) run.record('input.noProviderWork', providerCalls() === before, `${providerCalls() - before} provider requests during invalid admission`);
}

async function rowIsolationPhase(run) {
  const row = run.input(run.spec.happy).queries[0];
  for (const surface of run.surfaces) {
    const token = tokenFor(run.name, surface);
    const providerCalls = () => run.ctx.mock.requestsFrom.filter(request => request.token === token).length;
    const before = providerCalls();
    const actual = await run.call({ queries: [row, { ...row, unexpectedField: true }] }, surface);
    if (run.spec.mixedBatch === 'reject') {
      run.record(`input.batchAtomic.${surface}`, rejectsWholeBatch(actual, surface) && providerCalls() === before, JSON.stringify(actual.json));
      continue;
    }
    const rows = rowsOf(actual.json);
    const valid = rows.find(entry => entry.index === 0);
    const invalid = rows.find(entry => entry.index === 1);
    run.record(`input.rowIsolation.${surface}`, rows.length === 2 && valid && !valid.data?.errorCode
      && invalid?.data?.errorCode === 'invalidInput' && (surface === 'cli' ? actual.status === EXIT.invalid : !actual.isError)
      && !run.suite.validateOutput(run.name, actual.json), JSON.stringify(actual.json));
  }
}
function rejectsWholeBatch(actual, surface) {
  return actual.json?.errorCode === 'invalidInput' && (surface === 'cli' ? actual.status === EXIT.invalid : actual.isError);
}

/** Additional behavior checks supplied by a tool case. */
async function extraPhase(run, { happy, happyRuns }) {
  run.phase = 'extra';
  const callbacks = [run.spec.extra, ...(run.suite.advanced?.[run.name] ?? [])].filter(Boolean);
  if (!callbacks.length) return;
  const { suite, cwd } = run;
  const context = {
    ...run.ctx, cwd, q: run.q, surfaces: run.surfaces, happy, happyRuns, reports: run.reports, skips: run.skips,
    validateOutput: suite.validateOutput, validateInput: suite.validateInput,
    ...advancedContext(run),
  };
  for (const callback of callbacks) {
    const results = await callback(context);
    for (const [check, ok, detail] of results) run.record(check, ok, detail);
  }
}

/**
 * (f) latency, then the tool's result. A loaded machine slows every process start:
 * the budget scales by the tool's own invalid-input CLI round trip (pure startup +
 * validation) over a calm one, capped.
 */
function result(run, t0) {
  const { name, spec, timings, checks, reports } = run;
  const slowest = timings.reduce((m, t) => Math.max(m, t.ms), 0);
  const overhead = timings.find(t => t.phase === 'invalid' && t.surface === 'cli')?.ms ?? CALM_CLI_MS;
  const load = Math.min(MAX_LOAD_FACTOR, Math.max(1, overhead / CALM_CLI_MS));
  const budget = Math.round(spec.budgetMs * load);
  run.record('latency', slowest <= budget, `slowest call ${slowest}ms (budget ${budget}ms = ${spec.budgetMs}ms x load ${load.toFixed(2)}; CLI startup ${overhead}ms)`);
  run.record('measurement.responseBytes', timings.every(call => Number.isSafeInteger(call.responseBytes) && call.responseBytes > 0), `${timings.length} calls include non-empty response payload byte counts`);
  const mine = new Set(['cli', 'mcp'].map(s => tokenFor(name, s)));
  const newUnhandled = run.ctx.mock.unhandledFrom.filter(u => mine.has(u.token)).map(u => u.request);
  run.record('mock.requests', newUnhandled.length === 0, newUnhandled.join('; '));
  const failed = checks.filter(c => c.ok === false);
  const all = [...reports.cli, ...reports.mcp];
  return {
    tool: name, status: failed.length ? 'FAIL' : run.skips.length ? 'SKIP' : 'PASS', reason: run.skips.join('; ') || undefined,
    ms: Math.round(performance.now() - t0), slowestMs: slowest, budgetMs: budget, calls: timings.length,
    coverage: {
      seeds: reports.cli.map(r => r.label),
      leadKeys: [...new Set(all.flatMap(r => r.calls.filter(c => c.label !== 'seed').map(c => c.label.split(' > ').pop())))].toSorted(byCodeUnit),
      chains: [...new Set(all.flatMap(r => r.chains.map(c => `${c.tool}.${c.key}`)))].toSorted(byCodeUnit),
      skippedLeads: [...new Set(all.flatMap(r => r.skipped.map(x => `${x.key}: ${x.reason}`)))],
      metadata: all.flatMap(report => report.metadata),
      advanced: run.advancedCalls,
      overBudget: all.reduce((n, r) => n + r.budgetCut, 0),
    },
    firstFailure: failed[0] ? `${failed[0].check}: ${failed[0].detail}` : undefined,
    ...(newUnhandled.length && { mockUnhandled: newUnhandled }), checks, timings,
  };
}

/** Run one tool's case on its surfaces (CLI only for a CLI-only tool); SKIP when a needed capability is missing. */
export async function runTool(suite, name, spec) {
  const t0 = performance.now();
  const { surfaces } = suite;
  const skipReason = spec.needs ? surfaces.needs[spec.needs]() : null;
  if (skipReason) return { tool: name, status: 'SKIP', reason: skipReason, checks: [], timings: [], ms: 0 };
  const cwd = spec.workspace ? spec.workspace({ ...surfaces.ctx, name }) : surfaces.workspace(name);
  const server = spec.cliOnly ? null : await surfaces.mcpServer(name, cwd);
  const run = createRun(suite, { name, spec, cwd, server });
  try {
    const happy = await happyPhase(run);
    await walkPhase(run, happy.happy);
    await emptyPhase(run);
    await seedsPhase(run);
    await invalidPhase(run);
    await extraPhase(run, happy);
  } finally {
    server?.close();
  }
  return result(run, t0);
}
