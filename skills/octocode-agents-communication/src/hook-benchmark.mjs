// Real CLI benchmark; isolated databases only, no vendor/model process.
import { spawnSync, spawn } from 'node:child_process';
import { mkdtempSync, rmSync, readFileSync, mkdirSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, dirname, resolve } from 'node:path';
import { createHash } from 'node:crypto';
import { DatabaseSync } from 'node:sqlite';

const [baseline, candidate, output] = process.argv.slice(2);
if (!baseline || !candidate) throw Error('Usage: node src/hook-benchmark.mjs BASELINE CANDIDATE [REPORT.json]');
const samples = 30;
const percentile = (values, p) => [...values].sort((a, b) => a - b)[Math.ceil(values.length * p) - 1];
async function measure(binary, vendor) {
  const workspace = mkdtempSync(join(tmpdir(), 'communication-hook-bench-'));
  const database = join(workspace, 'coord.sqlite');
  const args = ['host-hook', '--vendor', vendor, '--workspace', workspace, '--database', database];
  const input = event => JSON.stringify(vendor === 'cursor'
    ? {hook_event_name: event, conversation_id: 'bench', workspace_roots: [workspace]}
    : {hookEventName: event, sessionId: 'bench', workspaceRoot: workspace});
  const call = event => {
    const start = performance.now();
    const result = spawnSync(binary, args, {input: input(event), encoding: 'utf8', timeout: 10000});
    if (result.status !== 0 || result.stderr) throw Error(result.stderr || String(result.error));
    return {ms: performance.now() - start, bytes: Buffer.byteLength(result.stdout), value: JSON.parse(result.stdout)};
  };
  let db;
  try {
    call('sessionStart');
    call('postToolUse');
    db = new DatabaseSync(database);
    const version = () => db.prepare('PRAGMA data_version').get().data_version;
    const rows = [];
    let changedCommits = 0;
    for (let i = 0; i < samples; i++) {
      const before = version();
      const result = call(i % 2 ? 'postToolUseFailure' : 'postToolUse');
      if (JSON.stringify(result.value) !== '{}') throw Error('Routine hook repeated context');
      if (version() !== before) changedCommits++;
      rows.push(result);
    }
    // An unrelated WAL writer must not delay an otherwise read-only hook.
    db.exec('BEGIN IMMEDIATE');
    const start = performance.now();
    const child = spawn(binary, args, {stdio: ['pipe', 'pipe', 'pipe']});
    let stdout = '', stderr = '';
    child.stdout.on('data', value => stdout += value);
    child.stderr.on('data', value => stderr += value);
    const release = setTimeout(() => db.exec('ROLLBACK'), 300);
    const completed = new Promise((res, rej) => {
      child.once('error', rej);
      child.once('close', code => code ? rej(Error(stderr)) : res());
    });
    child.stdin.end(input('postToolUse'));
    await completed;
    const contentionMs = performance.now() - start;
    clearTimeout(release);
    if (db.isTransaction) db.exec('ROLLBACK');
    if (stderr || JSON.stringify(JSON.parse(stdout)) !== '{}') throw Error('Contention changed hook output');
    return {vendor, samples, calls: samples + 3, idleMedianMs: percentile(rows.map(x => x.ms), .5), idleP95Ms: percentile(rows.map(x => x.ms), .95), idleOutputBytes: rows.reduce((n, x) => n + x.bytes, 0), repeatedContextBytes: 0, committedChangePolls: changedCommits, contentionWriterHoldMs: 300, contentionMs};
  } finally { db?.close(); rmSync(workspace, {recursive: true, force: true}); }
}
const report = {schemaVersion: 1, date: new Date().toISOString(), platform: `${process.platform}-${process.arch}`, criterion: 'Preserve empty envelopes and once-only context; routine hooks complete without waiting for an unrelated writer. Wall-time improvement is informational.', runs: []};
for (const [label, path] of [['baseline', baseline], ['candidate', candidate]]) {
  const binary = resolve(path);
  const measurements = [];
  for (const vendor of ['cursor', 'grok']) measurements.push(await measure(binary, vendor));
  report.runs.push({label, binarySha256: createHash('sha256').update(readFileSync(binary)).digest('hex'), measurements});
}
const text = JSON.stringify(report, null, 2) + '\n';
if (output) { mkdirSync(dirname(resolve(output)), {recursive: true}); writeFileSync(output, text); }
process.stdout.write(text);
