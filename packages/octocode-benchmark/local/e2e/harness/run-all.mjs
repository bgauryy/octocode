// Run suites sequentially (each owns one MCP server) and summarize.
//
//   node harness/run-all.mjs                 # the hermetic quality suites
//   OCTOCODE_LIVE=1 node harness/run-all.mjs # + the live suites
//   node harness/run-all.mjs grammar,perf    # only the named suites
//
// HERMETIC suites need only the local clones (repos/, fixtures/): each runs
// with a fresh empty OCTOCODE_HOME and GH_CONFIG_DIR, no GitHub or classifier
// credentials, and HTTP(S) routed to a dead proxy (loopback stays reachable),
// so a hidden network or home dependency fails instead of passing by luck.
// LIVE suites call GitHub, package registries or the paid classifier with the
// caller's own credentials; they run only with OCTOCODE_LIVE=1, which also
// enables the live sections of mixed suites (chain-check, lead-replay).
// BENCH suites are benchmarks and soaks, not correctness tests: they run only
// when named (soak: OCTOCODE_SOAK_MINUTES, default 30).
//
// Each suite prints one `PASS <suite>` or `FAIL <suite>` line; the exit code
// is non-zero when any selected suite fails, errors, times out or reports no
// checks.
import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { LIVE, RESULTS, TESTING, hermeticEnv } from './mcp-client.mjs';

const script = name => [path.join(TESTING, 'harness', `${name}.mjs`)];
const HERMETIC = {
  acceptance: ['--test', '--test-reporter=spec', path.join(TESTING, 'harness', 'acceptance.test.mjs')],
  'usage-regressions': script('usage-regressions'),
  'lsp-importers': script('lsp-importers'),
  grammar: script('grammar'),
  workflows: script('workflows'),
  'chain-check': script('chain-check'),
  'lead-replay': script('lead-replay'),
  navigate: script('navigate'),
  'repo-sweep': script('repo-sweep'),
  'large-files': script('large-files'),
  'local-ranking': script('local-ranking'),
  'read-tool-selection': script('read-tool-selection'),
  'classification-retry': script('classification-retry'),
  'clasify-evidence': script('clasify-evidence'),
  'tool-health': script('tool-health'),
};
const LIVE_SUITES = { remote: script('remote'), github: script('github'), 'review-large': script('review-large'), artifacts: script('artifacts'), clasify: script('clasify') };
const BENCH = { perf: script('perf'), competitors: script('competitors'), kpi: script('kpi'), soak: script('soak') };
const ALL = { ...HERMETIC, ...LIVE_SUITES, ...BENCH };

const named = process.argv[2]?.split(',');
const selected = named ?? [...Object.keys(HERMETIC), ...(LIVE ? Object.keys(LIVE_SUITES) : [])];
const unknown = selected.filter(s => !ALL[s]);
if (unknown.length) throw new Error(`unknown suite: ${unknown.join(', ')} (known: ${Object.keys(ALL).join(', ')})`);
const gated = selected.filter(s => LIVE_SUITES[s] && !LIVE);
if (gated.length) throw new Error(`live suite needs OCTOCODE_LIVE=1: ${gated.join(', ')}`);

const suiteTimeout = suite => Math.max(Number(process.env.OCTOCODE_TEST_SUITE_TIMEOUT_MS ?? 1800000), suite === 'soak' ? (Number(process.env.OCTOCODE_SOAK_MINUTES ?? 30) + 15) * 60000 : 0);
/** Check counts from the harness `PASS`/`FAIL` lines or a node:test summary. */
function counts(stdout) {
  const failed = stdout.match(/^FAIL .*/gm) ?? [];
  const passed = (stdout.match(/^PASS /gm) ?? []).length;
  const nodeTest = name => Number(stdout.match(new RegExp(`^ℹ ${name} (\\d+)`, 'm'))?.[1] ?? 0);
  const nodeFailed = (stdout.match(/^✖ .*/gm) ?? []).filter(l => !/failing tests:/.test(l));
  return { passed: passed + nodeTest('pass'), failed: [...failed, ...nodeFailed.slice(0, nodeTest('fail'))] };
}

// Language servers (rust-analyzer, tsgo) spend several CPU-seconds on a cold
// first request; with the run queue far above the core count that wall time
// grows past the suites' own time bounds (U17 allows a cold call 90 s; a
// progressing server is waited for up to the 300 s tool-call deadline).
const OVERSUBSCRIBED = 2 * os.cpus().length;
const loadAtStart = os.loadavg()[0];

fs.mkdirSync(RESULTS, { recursive: true });
const rows = [];
const wallStart = Date.now();
for (const suite of selected) {
  const kind = HERMETIC[suite] ? 'hermetic' : LIVE_SUITES[suite] ? 'live' : 'bench';
  const started = Date.now();
  const run = spawnSync(process.execPath, ALL[suite], { cwd: TESTING, env: kind === 'hermetic' ? hermeticEnv(process.env, suite) : process.env, encoding: 'utf8', maxBuffer: 256 * 1024 * 1024, timeout: suiteTimeout(suite), killSignal: 'SIGTERM' });
  fs.writeFileSync(path.join(RESULTS, `${suite}.log`), `${run.stdout ?? ''}\n${run.stderr ?? ''}`);
  const { passed, failed } = counts(run.stdout ?? '');
  const seconds = Math.round((Date.now() - started) / 1000);
  const complete = run.status === 0 && !run.signal && !run.error && passed + failed.length > 0;
  const ok = complete && !failed.length;
  rows.push({ suite, kind, result: ok ? 'PASS' : 'FAIL', passed, failed: failed.length, seconds, exit: run.status, signal: run.signal, error: run.error?.message });
  const why = ok ? '' : complete ? '' : ` — ${run.error?.message ?? (run.signal ? `killed by ${run.signal}` : passed + failed.length ? `exit ${run.status}` : `no checks reported, exit ${run.status}`)}`;
  console.log(`${ok ? 'PASS' : 'FAIL'} ${suite} [${kind}]: ${passed} passed, ${failed.length} failed (${seconds}s)${why}`);
  for (const line of failed) console.log(`   ${line.slice(0, 260)}`);
  if (!ok && !failed.length) console.log(`   see ${path.relative(TESTING, path.join(RESULTS, `${suite}.log`))}`);
}
console.table(rows);
const failedSuites = rows.filter(r => r.result === 'FAIL').map(r => r.suite);
const wallSeconds = Math.round((Date.now() - wallStart) / 1000);
fs.writeFileSync(path.join(RESULTS, 'summary.json'), JSON.stringify({ at: new Date().toISOString(), live: LIVE, wallSeconds, rows }, null, 2));
console.log(failedSuites.length ? `FAIL run-all: ${failedSuites.length}/${rows.length} suites failed (${failedSuites.join(', ')}) in ${wallSeconds}s` : `PASS run-all: ${rows.length} suites in ${wallSeconds}s`);
const loadPeak = Math.max(loadAtStart, os.loadavg()[0]);
if (failedSuites.length && loadPeak > OVERSUBSCRIBED) {
  console.log(`NOTE: load average ${loadPeak.toFixed(0)} on ${os.cpus().length} cores: LSP-backed checks (cold lspSearch timeouts) fail from CPU starvation; rerun on an idle machine before treating them as product failures.`);
}
process.exitCode = failedSuites.length ? 1 : 0;
