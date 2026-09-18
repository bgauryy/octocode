#!/usr/bin/env node
import { mkdirSync, readFileSync, writeFileSync, readdirSync, existsSync } from 'node:fs';
import { resolve, dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';

const root = dirname(fileURLToPath(import.meta.url));
const repo = resolve(root, '../../..');
const [arm, caseId, kind, ...args] = process.argv.slice(2);
if (!['baseline', 'treatment', 'curator'].includes(arm) || !/^(Q([1-9]|10)|BUG)$/.test(caseId || '') || !['octocode', 'jev', 'schema'].includes(kind)) {
  console.error('Usage: node run-case.mjs baseline|treatment|curator Q1..Q10|BUG octocode|jev|schema <args>');
  process.exit(2);
}
const dir = join(root, 'runs/terra-20260918-v1', arm, caseId);
mkdirSync(dir, { recursive: true });
const events = readdirSync(dir).filter(name => /^event-\d+\.json$/.test(name)).map(name => JSON.parse(readFileSync(join(dir, name), 'utf8')));
const count = type => events.filter(event => event.kind === type).length;
if (kind === 'octocode' && count(kind) >= 12) throw new Error('12 Octocode invocations per case exhausted. Return bounded findings.');
if (kind === 'jev' && (arm !== 'treatment' || count(kind) >= 1)) throw new Error('Only treatment may make exactly one Jev invocation per case.');
const cli = join(repo, 'packages/octocode/out/octocode.js');
let command;
let request = null;
if (kind === 'jev') {
  if (args.length !== 1) throw new Error('Jev mode takes one request-file path.');
  request = JSON.parse(readFileSync(resolve(args[0]), 'utf8'));
  if (request.model !== 'jev-1.13.0') throw new Error('Pin jev-1.13.0.');
  command = [join(repo, 'skills/octocode-jev-reasoning-loop/scripts/jev.mjs'), 'evaluate', '--input', resolve(args[0]), '--retries', '0', '--timeout-ms', '10000'];
} else if (kind === 'schema') {
  command = [cli, 'tools', ...args, '--scheme', '--json', '--compact'];
} else {
  if (!['ghSearch', 'ghGetFileContent', 'ghGetHistoryItem', 'ghSearchHistory', 'artifactSearch', 'ghCloneRepo', 'localFetch', 'localSearch', 'astSearch', 'lspSearch'].includes(args[0])) throw new Error('Unknown research tool.');
  command = [cli, 'tools', ...args];
}
const startedAt = new Date().toISOString();
const start = performance.now();
const result = spawnSync(process.execPath, command, { cwd: repo, encoding: 'utf8', timeout: 90000, maxBuffer: 8 * 1024 * 1024 });
const elapsedMs = Math.round(performance.now() - start);
const id = String(events.length + 1).padStart(3, '0');
const stdout = result.stdout || '';
const stderr = result.stderr || '';
const sha = value => createHash('sha256').update(value).digest('hex');
const event = { arm, caseId, kind, args, startedAt, endedAt: new Date().toISOString(), elapsedMs, exitCode: result.status, signal: result.signal, stdoutSha256: sha(stdout), stdoutBytes: Buffer.byteLength(stdout), requestSha256: request ? sha(JSON.stringify(request)) : null };
writeFileSync(join(dir, `event-${id}.json`), JSON.stringify(event, null, 2) + '\n');
writeFileSync(join(dir, `event-${id}.stdout.txt`), stdout);
writeFileSync(join(dir, `event-${id}.stderr.txt`), stderr);
if (request) writeFileSync(join(dir, `event-${id}.request.json`), JSON.stringify(request, null, 2) + '\n');
process.stdout.write(stdout);
process.stderr.write(stderr);
process.exitCode = result.status ?? 3;
