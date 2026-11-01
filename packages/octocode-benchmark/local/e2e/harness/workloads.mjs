#!/usr/bin/env node
// BENCHMARK, not a correctness test: run-all never runs it.
// Latency and peak memory of representative workloads on the resolved native
// binary (OCTOCODE_NATIVE_BIN overrides; measure a release build). Every case
// runs in a fresh `octocode mcp` under /usr/bin/time: the first call is cold,
// then --repeat warm calls in the same process. The server's own peak is the
// exact macOS peak footprint plus its sampled RSS; language servers and other
// children are the sampled tree. rusage max RSS also counts reaped children.
//
//   node harness/workloads.mjs [--groups=cli,idle,grammar,huge,minify+matchString,github,workflow]
//        [--only=<regex>] [--repeat=3] [--out=<dir>] [--list]
//
// Output: <out>/workloads.json and <out>/workloads.md. GitHub cases use the
// caller's credentials; a missing credential shows as a failed row, not a skip.
import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { nativeBinaryPath, nativeBinaryStamp } from './native-binary.mjs';
import { REPOS, ROOT } from './paths.mjs';
import { allCases } from './workloads/cases.mjs';
import { workflowCases } from './workloads/workflows.mjs';
import { parseResourceReport, startMeasuredServer } from './workloads/server.mjs';
import { renderMarkdown } from './workloads/report.mjs';

const arg = (name, fallback) => process.argv.find(a => a.startsWith(`--${name}=`))?.slice(name.length + 3) ?? fallback;
const GROUPS = arg('groups', 'cli,idle,grammar,huge,minify+matchString,github,workflow').split(',');
const ONLY = arg('only') ? new RegExp(arg('only')) : null;
const REPEAT = Number(arg('repeat', 3));
const OUT = path.resolve(arg('out', path.join(ROOT, '.octocode/octocode-benchmark', `workloads-${new Date().toISOString().slice(0, 10)}`)));
const BIN = nativeBinaryPath();

const median = xs => { const s = xs.toSorted((a, b) => a - b); return s.length ? s[Math.floor((s.length - 1) / 2)] : null; };
const round = x => (x == null ? null : Math.round(x));
const say = line => process.stdout.write(`${line}\n`);

/** Row-level failures: a protocol error, an error result, or any result row with an errorCode. */
function failure(response) {
  if (response.error) return response.error.message ?? 'rpc error';
  const sc = response.result?.structuredContent;
  const row = sc?.results?.find(r => r?.status === 'error' || r?.errorCode || r?.data?.errorCode);
  if (row) return row.errorCode ?? row.data?.errorCode ?? 'error';
  return response.result?.isError ? (sc?.errorCode ?? 'isError') : null;
}

async function measure(c) {
  const server = await startMeasuredServer(BIN, { cwd: c.cwd ?? ROOT, env: process.env });
  const call = async (tool, args) => {
    const response = await server.call(tool, args);
    return { data: response.result?.structuredContent, stat: { ms: round(response.ms), bytes: response.bytes, error: failure(response) } };
  };
  const runs = [];
  let error = null;
  try {
    for (let i = 0; i <= REPEAT; i++) {
      const t0 = performance.now();
      const steps = c.chain ? await c.chain(call) : [{ tool: c.tool, ...(await call(c.tool, c.args)).stat }];
      runs.push({ ms: round(performance.now() - t0), steps, ...server.mark() });
    }
  } catch (e) { error = e.message; }
  const report = await server.close();
  const [cold] = runs;
  // A chain that runs out of leads ends where an agent would stop; only an errored step fails it.
  const stepError = cold?.steps.find(s => s.error);
  return {
    group: c.group, name: c.name, tool: c.tool ?? 'chain', meta: c.meta,
    ok: !error && !!cold && !stepError,
    // A missing language server is a capability gap of this machine, not a failed workload.
    unavailable: stepError?.error === 'serverUnavailable',
    error: error ?? stepError?.error ?? null,
    initMs: server.initMs,
    ...timings(runs, report),
  };
}

/** Cold and warm latency, response size and memory of one case's runs. */
function timings(runs, report) {
  const [cold, ...warm] = runs;
  return {
    coldMs: cold?.ms ?? null, warmP50Ms: median(warm.map(r => r.ms)), warmMaxMs: warm.length ? Math.max(...warm.map(r => r.ms)) : null,
    bytes: cold ? cold.steps.reduce((n, s) => n + (s.bytes ?? 0), 0) : null,
    steps: cold?.steps,
    peakRssKb: report.peakRssKb, peakFootprintKb: report.peakFootprintKb,
    sampledRssKb: Math.max(0, ...runs.map(r => r.sampledRssKb)),
    sampledTreeKb: Math.max(0, ...runs.map(r => r.sampledTreeKb)),
    exit: report.exit, stderrTail: report.stderrTail,
  };
}

/** Fresh CLI processes: start-up, schema, an in-process call, and the warm-server path. */
function cliCases() {
  const time = process.platform === 'darwin' ? ['-l'] : ['-v'];
  const run = (name, argv, env = {}, cwd = ROOT) => {
    const t0 = performance.now();
    const p = spawnSync('/usr/bin/time', [...time, BIN, ...argv], { cwd, env: { ...process.env, ...env }, encoding: 'utf8', maxBuffer: 1 << 28 });
    const row = { group: 'cli', name, tool: argv[0], ok: p.status === 0, error: p.status === 0 ? null : p.stderr.slice(-400), coldMs: round(performance.now() - t0), bytes: Buffer.byteLength(p.stdout ?? ''), ...parseResourceReport(p.stderr ?? '') };
    // Kept off the report: a dependent step reads it.
    return Object.defineProperty(row, 'stdout', { value: p.stdout ?? '' });
  };
  const search = ['localSearch', JSON.stringify({ queries: [{ path: 'packages/octocode-native/crates', matchString: ['ToolRuntime'], view: 'countMatches' }] })];
  const rows = [run('--version', ['--version']), run('schema (all tools)', ['schema']), run('localSearch in process (storage memory)', search, { OCTOCODE_STORAGE_MODE: 'memory' })];
  // The first served call starts `octocode serve`; later ones reuse it. The client's own peak is reported.
  const served = Array.from({ length: REPEAT + 1 }, () => run('localSearch served', search));
  rows.push({ ...served[0], name: 'localSearch served (first: starts server)' }, { ...served.at(-1), name: 'localSearch served (warm)', warmP50Ms: median(served.slice(1).map(r => r.coldMs)) });
  return [...rows, ...graphCases(run)];
}

/** The graph workflow is CLI-only: a forced cold build, an unchanged rebuild, then queries over the snapshot. */
function graphCases(run) {
  const root = path.join(REPOS, 'rust');
  if (!fs.existsSync(root)) return [];
  const build = force => ['graphBuild', JSON.stringify({ queries: [{ path: root, exclude: ['target'], ...(force ? { force: true } : {}) }] }), '--json'];
  const cold = run('graphBuild rust (forced cold)', build(true), {}, root);
  const id = (() => { try { return JSON.parse(cold.stdout).results[0].data.id; } catch { return null; } })();
  const rows = [{ ...cold, group: 'graph' }, { ...run('graphBuild rust (unchanged rebuild)', build(false), {}, root), group: 'graph' }];
  for (const operation of ['stats', 'hubs', 'cycles']) {
    rows.push({ ...run(`graphQuery ${operation}`, ['graphQuery', JSON.stringify({ queries: [{ operation, path: root, ...(id ? { graph: id } : {}) }] }), '--json'], {}, root), group: 'graph' });
  }
  return rows;
}

const cases = [...allCases(), ...workflowCases()].filter(c => GROUPS.includes(c.group) && (!ONLY || ONLY.test(c.name)));
if (process.argv.includes('--list')) {
  for (const c of cases) say(`${c.group}\t${c.name}${c.missing ? `\tMISSING ${c.missing}` : ''}`);
  process.exit(0);
}

fs.mkdirSync(OUT, { recursive: true });
const rows = [];
const write = () => {
  const run = { at: new Date().toISOString(), binary: nativeBinaryStamp(), host: { platform: process.platform, arch: process.arch, cpus: os.cpus().length, cpuModel: os.cpus()[0]?.model, memGb: Math.round(os.totalmem() / 2 ** 30), loadAtEnd: os.loadavg() }, repeat: REPEAT, rows };
  fs.writeFileSync(path.join(OUT, 'workloads.json'), `${JSON.stringify(run, null, 2)}\n`);
  fs.writeFileSync(path.join(OUT, 'workloads.md'), renderMarkdown(run));
};
say(`binary ${BIN}\nload ${os.loadavg().map(x => x.toFixed(1)).join(' ')}; ${cases.length} cases${GROUPS.includes('cli') ? ' + cli' : ''}; out ${OUT}`);
if (GROUPS.includes('cli') && !ONLY) for (const r of cliCases()) { rows.push(r); say(`${r.ok ? 'ok  ' : 'FAIL'} cli ${r.name}: ${r.coldMs} ms, peak ${Math.round((r.peakRssKb ?? 0) / 1024)} MB`); }
for (const c of cases) {
  if (c.missing) { rows.push({ group: c.group, name: c.name, ok: false, error: `missing: ${c.missing}` }); continue; }
  const r = await measure(c);
  rows.push(r);
  say(`${r.ok ? 'ok  ' : 'FAIL'} ${r.group} ${r.name}: cold ${r.coldMs} ms, warm ${r.warmP50Ms} ms, footprint ${Math.round((r.peakFootprintKb ?? 0) / 1024)} MB, server ${Math.round((r.sampledRssKb ?? 0) / 1024)} MB, tree ${Math.round((r.sampledTreeKb ?? 0) / 1024)} MB${r.error ? ` — ${String(r.error).slice(0, 160)}` : ''}`);
  write();
}
write();
const failed = rows.filter(r => !r.ok);
say(`${rows.length - failed.length}/${rows.length} ok; report ${path.join(OUT, 'workloads.md')}`);
