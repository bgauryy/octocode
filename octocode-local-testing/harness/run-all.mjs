// Run every suite sequentially (each owns one MCP server) and summarize.
// Usage: node harness/run-all.mjs [suite,...]
import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { RESULTS, TESTING } from './mcp-client.mjs';

const SUITES = ['workflows', 'grammar', 'navigate', 'remote', 'github', 'artifacts', 'perf', 'deps-flows', 'repo-sweep', 'large-files', 'clasify', 'rewrite', 'usage-regressions', 'competitors'];
const selected = process.argv[2]?.split(',') ?? SUITES;
if (selected.some(s => !SUITES.includes(s))) throw new Error('unknown suite');
fs.mkdirSync(RESULTS, { recursive: true });
const rows = [];
for (const suite of selected) {
  const started = Date.now();
  const run = spawnSync(process.execPath, [path.join(TESTING, 'harness', `${suite}.mjs`)], { cwd: TESTING, encoding: 'utf8', maxBuffer: 256 * 1024 * 1024, timeout: Number(process.env.OCTOCODE_TEST_SUITE_TIMEOUT_MS ?? 1800000), killSignal: 'SIGTERM' });
  fs.writeFileSync(path.join(RESULTS, `${suite}.log`), `${run.stdout ?? ''}\n${run.stderr ?? ''}`);
  const failed = ((run.stdout ?? '').match(/^FAIL .*/gm) ?? []);
  const passed = ((run.stdout ?? '').match(/^PASS /gm) ?? []).length;
  rows.push({ suite, passed, failed: failed.length, seconds: ((Date.now() - started) / 1000).toFixed(0), exit: run.status, signal: run.signal, error: run.error?.message, complete: run.status === 0 && !run.signal && !run.error && passed + failed.length > 0 });
  console.log(`\n== ${suite}: ${passed} passed, ${failed.length} failed (${rows.at(-1).seconds}s)`);
  for (const line of failed) console.log(`   ${line.slice(0, 260)}`);
}
console.table(rows);
fs.writeFileSync(path.join(RESULTS, 'summary.json'), JSON.stringify({ at: new Date().toISOString(), rows }, null, 2));
process.exitCode = rows.some(r => r.failed || !r.complete || r.exit !== 0) ? 1 : 0;
