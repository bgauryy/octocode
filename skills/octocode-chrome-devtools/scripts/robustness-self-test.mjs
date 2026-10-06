#!/usr/bin/env node
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { createServer } from 'node:http';
import { mkdtempSync, realpathSync, mkdirSync, readFileSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
const ROOT = dirname(fileURLToPath(import.meta.url));
const WORK = realpathSync(mkdtempSync(join(tmpdir(), 'octo-robustness-')));
const cli = (script, args = []) => new Promise((resolve, reject) => {
  const child = spawn(process.execPath, [join(ROOT, script), ...args], { cwd: WORK, stdio: ['ignore', 'pipe', 'pipe'] });
  let stdout = '', stderr = '';
  child.stdout.on('data', b => stdout += b); child.stderr.on('data', b => stderr += b);
  const timer = setTimeout(() => { child.kill('SIGKILL'); reject(new Error('CLI timeout')); }, 10000);
  child.on('error', reject);
  child.on('exit', code => { clearTimeout(timer); resolve({ code, stdout, stderr }); });
});
let requests = 0;
const text = 'λ response '.repeat(900);
const server = createServer((req, res) => { requests++; if (req.url === '/slow') setTimeout(() => res.end(text), 300); else res.end(text); });
await new Promise(r => server.listen(0, '127.0.0.1', r));
let cases = 0;
try {
  const url = 'http://127.0.0.1:' + server.address().port;
  let result = await cli('cdp-checks/api-replay.mjs', ['--url', url, '--method', 'POST', '--body', '{}', '--max-chars', '1000']);
  assert.equal(result.code, 0, result.stderr);
  let page = JSON.parse(result.stdout), collected = page.contentPreview;
  while (page.next) {
    const next = page.next.continue;
    result = await cli('cdp-checks/api-replay.mjs', next.args.slice(1));
    assert.equal(result.code, 0, result.stderr); page = JSON.parse(result.stdout); collected += page.contentPreview;
  }
  assert.equal(collected, text); assert.equal(requests, 1); cases++;
  result = await cli('cdp-checks/api-replay.mjs', ['--url', url, '--method', 'POST', '--page', '2']);
  assert.notEqual(result.code, 0); assert.equal(requests, 1); cases++;
  result = await cli('cdp-checks/api-replay.mjs', ['--url', url + '/slow', '--timeout-ms', '30']);
  assert.notEqual(result.code, 0); cases++;
  const har = join(WORK, 'source.har');
  writeFileSync(har, JSON.stringify({ log: { entries: [{ request: { url: 'https://a.test/?token=SECRET#SECRET', headers: [{ name: 'x-custom-token', value: 'SECRET' }], postData: { text: 'SECRET', params: [{ name: 'data', value: 'SECRET' }] } }, response: { redirectURL: 'https://a.test/?key=SECRET#SECRET', headers: [{ name: 'Location', value: 'https://a.test/?auth=SECRET' }], content: { text: 'SECRET' }, cookies: [{ name: 'session', value: 'SECRET' }] } }] } }));
  result = await cli('cdp-checks/har-redact.mjs', [har]);
  assert.equal(result.code, 0, result.stderr);
  const redacted = readFileSync(result.stdout.match(/REDACTED_HAR (.+)/)[1], 'utf8');
  assert.ok(!redacted.includes('SECRET')); assert.ok(readFileSync(har, 'utf8').includes('SECRET')); cases++;
  result = await cli('prune-artifacts.mjs', ['--max-count', '-1']); assert.equal(result.code, 2); cases++;
  const base = join(WORK, '.octocode', 'prune-test');
  for (let n = 1; n <= 3; n++) mkdirSync(join(base, '2026-10-06-00-00-0' + n), { recursive: true });
  result = await cli('prune-artifacts.mjs', ['--base', base, '--max-count', '0', '--dry-run']);
  assert.equal(JSON.parse(result.stdout).runDirs.paths.length, 3);
  result = await cli('prune-artifacts.mjs', ['--base', base, '--max-count', '0']);
  assert.equal(JSON.parse(result.stdout).runDirs.removed, 3); cases++;
  const out = join(WORK, 'measure'); mkdirSync(out);
  const storage = await import('./cdp-checks/storage-measure-check.mjs');
  const old = process.env.MEASURE_EXISTING; process.env.MEASURE_EXISTING = '1';
  const storageValue = { url: 'https://a.test', localStorageKeys: [], sessionStorageKeys: [], suspiciousLocalKeys: [], suspiciousSessionKeys: [], indexedDBDatabases: [], cacheNames: [], serviceWorkers: [], note: null };
  const cdp = { outputDir: out, targetInfo: { url: 'https://a.test' }, on() {}, off() {}, async send(method) { if (method === 'Network.getAllCookies') throw new Error('inventory denied'); if (method === 'Runtime.evaluate') return { result: { value: storageValue } }; return {}; } };
  const log = console.log; console.log = () => {};
  try { await storage.run(cdp); } finally { console.log = log; }
  const measured = JSON.parse(readFileSync(join(out, 'storage-measure.json')));
  assert.equal(measured.score.health, null); assert.equal(measured.complete, false); assert.equal(process.exitCode, 1); process.exitCode = 0; cases++;
  const perf = await import('./cdp-checks/performance-measure-check.mjs');
  cdp.send = async method => method === 'Runtime.evaluate' ? { exceptionDetails: { text: 'evaluation failed' } } : {};
  await assert.rejects(() => perf.run(cdp), /Performance evaluation unavailable/); cases++;
  if (old === undefined) delete process.env.MEASURE_EXISTING; else process.env.MEASURE_EXISTING = old;
  console.log(JSON.stringify({ ok: true, suite: 'chrome-devtools-robustness', cases }));
} finally { server.closeAllConnections(); await new Promise(r => server.close(r)); rmSync(WORK, { recursive: true, force: true }); }
