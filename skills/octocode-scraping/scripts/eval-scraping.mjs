#!/usr/bin/env node
import { mkdir, readFile, rm, writeFile } from 'node:fs/promises';
import { existsSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { spawnSync } from 'node:child_process';

const here = new URL('.', import.meta.url).pathname;
const script = resolve(here, 'fetch.mjs');
const root = resolve(process.cwd());
const outBase = join(root, '.octocode/tmp/scrape-eval');
await mkdir(outBase, { recursive: true });

const checks = [];
function run(name, args, expectOk = true) {
  const res = spawnSync(process.execPath, [script, ...args, '--out', outBase], { cwd: root, encoding: 'utf8' });
  const ok = expectOk ? res.status === 0 : res.status !== 0;
  checks.push({ name, ok, status: res.status, stdout: res.stdout.slice(0, 1000), stderr: res.stderr.slice(0, 1000) });
  return res;
}
function assert(name, condition, detail = '') { checks.push({ name, ok: Boolean(condition), detail }); }

await rm(outBase, { recursive: true, force: true });
await mkdir(outBase, { recursive: true });

run('example html success', ['--url', 'https://example.com', '--mode', 'html', '--provider', 'direct', '--session', 'eval-html', '--max-text-bytes', '20000', '--chunk-bytes', '5000', '--extract-links']);
assert('html map exists', existsSync(join(outBase, 'eval-html/MAP.md')));
assert('html agent index exists', existsSync(join(outBase, 'eval-html/AGENT_INDEX.json')));
assert('html graph exists', existsSync(join(outBase, 'eval-html/graph/site-graph.json')));
assert('html page index exists', existsSync(join(outBase, 'eval-html/indexes/pages-001.json')));
assert('html workflows exists', existsSync(join(outBase, 'eval-html/graph/workflows.json')));
assert('html workflow candidates exists', existsSync(join(outBase, 'eval-html/indexes/workflow-candidates.jsonl')));
assert('html clean chunk exists', existsSync(join(outBase, 'eval-html/text/page-001.clean.part-001.md')));
const htmlStdout = checks.find((c) => c.name === 'example html success')?.stdout || '';
assert('no raw html stdout', !htmlStdout.includes('<!DOCTYPE html>'));

run('example markdown success', ['--url', 'https://example.com', '--mode', 'markdown', '--session', 'eval-md', '--max-text-bytes', '20000', '--chunk-bytes', '5000']);
assert('markdown metadata exists', existsSync(join(outBase, 'eval-md/extracts/metadata.json')));

const mockFile = join(outBase, 'mock-423.json');
await writeFile(mockFile, JSON.stringify({ detail: 'mock anti-bot detection' }));
run('simulated provider error', ['--url', 'https://example.com', '--mode', 'markdown', '--session', 'eval-423', '--mock-status', '423', '--mock-body-file', mockFile], false);
const failure = await readFile(join(outBase, 'eval-423/reports/failures.md'), 'utf8');
assert('provider error recorded', failure.includes('mock anti-bot detection'));
const agentIndex = JSON.parse(await readFile(join(outBase, 'eval-423/AGENT_INDEX.json'), 'utf8'));
assert('agent index warning recorded', agentIndex.warnings.some((w) => /provider HTTP 423/.test(w.warning)));
assert('agent index schema version recorded', agentIndex.schemaVersion === 1);

const target404Mock = join(outBase, 'mock-target-404.json');
await writeFile(target404Mock, JSON.stringify({ markdown: '# 404 Not Found\n\nnginx' }));
run('target error despite provider 200', ['--url', 'https://example.com/missing', '--mode', 'markdown', '--session', 'eval-target-404', '--mock-status', '200', '--mock-body-file', target404Mock], false);
const target404Index = JSON.parse(await readFile(join(outBase, 'eval-target-404/AGENT_INDEX.json'), 'utf8'));
assert('target error warning recorded', target404Index.ok === false && target404Index.warnings.some((w) => /target likely returned/.test(w.warning)));

const secretRun = run('secret param rejected', ['--url', 'https://example.com', '--mode', 'html', '--provider', 'direct', '--session', 'eval-secret', '--param', 'token=abc'], false);
assert('secret rejection is sanitized json', secretRun.stderr.includes('"ok": false') && secretRun.stderr.includes('Refusing secret-like') && !secretRun.stderr.includes('at file://'));

run('mock cost captured', ['--url', 'https://example.com', '--mode', 'markdown', '--session', 'eval-cost', '--mock-status', '200', '--mock-body-file', mockFile, '--mock-credit-cost', '7']);
const costs = await readFile(join(outBase, 'eval-cost/extracts/costs.jsonl'), 'utf8');
assert('cost captured', costs.includes('"antCreditsCost":7'));

const failed = checks.filter((c) => !c.ok);
console.log(JSON.stringify({ ok: failed.length === 0, checks }, null, 2));
process.exit(failed.length === 0 ? 0 : 1);
