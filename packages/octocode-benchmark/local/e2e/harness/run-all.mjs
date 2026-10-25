// Run suites sequentially (each owns one MCP server) and summarize.
//
//   node harness/run-all.mjs                 # the hermetic quality suites
//   OCTOCODE_LIVE=1 node harness/run-all.mjs # + the live suites
//   node harness/run-all.mjs grammar,transport    # only the named suites
//
// HERMETIC suites need only the local clones (repos/, fixtures/): each runs
// with a fresh empty OCTOCODE_HOME and GH_CONFIG_DIR, no GitHub or classifier
// credentials, and HTTP(S) routed to a dead proxy (loopback stays reachable),
// so a hidden network or home dependency fails instead of passing by luck.
// LIVE suites call GitHub and package registries with the caller's credentials;
// they run only with OCTOCODE_LIVE=1.
// Performance and historical benchmark programs run separately.
//
// Each suite prints one `PASS <suite>` or `FAIL <suite>` line; the exit code
// is non-zero when any selected suite fails, errors, times out or reports no
// checks.
import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { LIVE } from './mcp-client.mjs';
import { RESULTS, TESTING } from './paths.mjs';
import { hermeticEnv } from '../../../../../test-utils/hermetic-env.ts';

const script = name => [path.join(TESTING, 'harness', `${name}.mjs`)];
const HERMETIC = {
  acceptance: ['--test', '--test-reporter=spec', path.join(TESTING, 'harness', '*.test.mjs'), path.join(TESTING, 'harness', 'tool-health', '*.test.mjs')],
  transport: script('transport'),
  grammar: script('grammar'),
  'lsp-importers': script('lsp-importers'),
  'classification-retry': script('classification-retry'),
  'tool-health': script('tool-health'),
};
const LIVE_SUITES = { remote: script('remote') };
const ALL = { ...HERMETIC, ...LIVE_SUITES };

const named = process.argv[2]?.split(',');
const selected = named ?? [...Object.keys(HERMETIC), ...(LIVE ? Object.keys(LIVE_SUITES) : [])];
const unknown = selected.filter(s => !ALL[s]);
if (unknown.length) throw new Error(`unknown suite: ${unknown.join(', ')} (known: ${Object.keys(ALL).join(', ')})`);
const gated = selected.filter(s => LIVE_SUITES[s] && !LIVE);
if (gated.length) throw new Error(`live suite needs OCTOCODE_LIVE=1: ${gated.join(', ')}`);

const suiteTimeout = Number(process.env.OCTOCODE_TEST_SUITE_TIMEOUT_MS ?? 1800000);
/** Check counts from the harness `PASS`/`FAIL` lines or a node:test summary. */
function counts(stdout) {
  const failed = stdout.match(/^FAIL .*/gm) ?? [];
  const passed = (stdout.match(/^PASS /gm) ?? []).length;
  const nodeTest = name => Number(stdout.match(new RegExp(`^ℹ ${name} (\\d+)`, 'm'))?.[1] ?? 0);
  const nodeFailed = (stdout.match(/^✖ .*/gm) ?? []).filter(l => !/failing tests:/.test(l));
  return { passed: passed + nodeTest('pass'), failed: [...failed, ...nodeFailed.slice(0, nodeTest('fail'))] };
}

// Record contention when a language-server check times out during a busy build.
const OVERSUBSCRIBED = 2 * os.cpus().length;
const loadAtStart = os.loadavg()[0];

fs.mkdirSync(RESULTS, { recursive: true });
const rows = [];
const wallStart = Date.now();
for (const suite of selected) {
  const kind = HERMETIC[suite] ? 'hermetic' : 'live';
  const started = Date.now();
  const run = spawnSync(process.execPath, ALL[suite], { cwd: TESTING, env: kind === 'hermetic' ? hermeticEnv(process.env, suite) : process.env, encoding: 'utf8', maxBuffer: 256 * 1024 * 1024, timeout: suiteTimeout, killSignal: 'SIGTERM' });
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
  console.log(`NOTE: load average ${loadPeak.toFixed(0)} on ${os.cpus().length} cores: LSP-backed checks (cold lspSearch timeouts) may be affected by CPU contention; retain the failure and rerun on an idle machine.`);
}
process.exitCode = failedSuites.length ? 1 : 0;
