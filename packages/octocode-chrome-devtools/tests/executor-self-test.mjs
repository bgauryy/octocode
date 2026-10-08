#!/usr/bin/env node
import assert from 'node:assert/strict';
import {
  mkdtempSync,
  readFileSync,
  rmSync,
  writeFileSync,
  openSync,
  writeSync,
  closeSync,
} from 'node:fs';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { spawnSync } from 'node:child_process';
if (process.argv.includes('--help')) {
  console.log(
    'Usage: executor-self-test.mjs\nChecks filtered evidence reconstruction, reusable indexes, precision, oversized records and changed-source rejection.'
  );
  process.exit(0);
}
const scripts = join(import.meta.dirname, '../dist/engine'),
  root = mkdtempSync(join(tmpdir(), 'octo-executor-query-'));
const cli = args =>
  spawnSync(process.execPath, [join(scripts, 'evidence-query.mjs'), ...args], {
    cwd: root,
    encoding: 'utf8',
    timeout: 10000,
  });
function page(args) {
  const r = cli(args);
  assert.equal(r.status, 0, r.stderr);
  return JSON.parse(r.stdout);
}
function resume(next) {
  const r = spawnSync(next.command, next.args, {
    cwd: root,
    encoding: 'utf8',
    timeout: 10000,
  });
  assert.equal(r.status, 0, r.stderr);
  return JSON.parse(r.stdout);
}
try {
  const file = join(root, 'network.json'),
    rows = Array.from({ length: 73 }, (_, i) => ({
      id: i,
      status: i % 2 ? 200 : 503,
      url: 'https://app.test/api/' + i,
      body: i === 4 ? 'x'.repeat(40000) : 'ok',
    }));
  writeFileSync(file, JSON.stringify({ rows }));
  const args = [
    '--file',
    file,
    '--pointer',
    '/rows',
    '--where',
    JSON.stringify([
      { path: '/status', op: 'gte', value: 400 },
      { path: '/url', op: 'contains', value: '/api/' },
    ]),
    '--limit',
    '7',
  ];
  let result = page(args),
    firstPage = result,
    found = [],
    firstNext = result.next.continue;
  assert.equal(result.scanned, 73);
  assert.equal(result.matched, 37);
  assert.equal(result.indexReused, false);
  for (;;) {
    assert(Buffer.byteLength(JSON.stringify(result.rows)) < 20000);
    for (const record of result.rows) {
      if (!record.oversized) found.push(record.value);
      else {
        let chunk = resume(record.next.continue),
          text = chunk.content;
        while (chunk.next) {
          chunk = resume(chunk.next.continue);
          text += chunk.content;
        }
        found.push(JSON.parse(text).value);
      }
    }
    if (!result.next) break;
    result = resume(result.next.continue);
    assert.equal(result.indexReused, true);
  }
  assert.deepEqual(
    found,
    rows.filter(r => r.status >= 400)
  );
  const projected = page([...args, '--select', '["/id","/url"]']);
  assert.deepEqual(Object.keys(projected.rows[0].value), ['/id', '/url']);
  assert.equal(projected.rows[0].sourcePointer, '/rows/0');
  const empty = page([
    '--file',
    file,
    '--pointer',
    '/rows',
    '--where',
    '[{"path":"/id","op":"eq","value":100}]',
  ]);
  assert.equal(empty.returned, 0);
  assert.equal(empty.matched, 0);
  assert.equal(empty.next, undefined);
  const jsonl = join(root, 'events.jsonl');
  writeFileSync(
    jsonl,
    '{"type":"input","trusted":true}\n\n{"type":"click","trusted":false}\n'
  );
  const events = page([
    '--file',
    jsonl,
    '--format',
    'jsonl',
    '--where',
    '[{"path":"/trusted","op":"eq","value":true}]',
  ]);
  assert.equal(events.matched, 1);
  assert.equal(events.rows[0].sourcePointer, 'line:1');
  const precise = join(root, 'precise.json');
  writeFileSync(precise, '[{"id":900719925474099312345}]');
  const raw = cli(['--file', precise]);
  assert.equal(raw.status, 0, raw.stderr);
  assert(raw.stdout.includes('900719925474099312345'));
  const numeric = page([
    '--file',
    precise,
    '--where',
    '[{"path":"/id","op":"gte","value":"900719925474099312345"}]',
  ]);
  assert.equal(numeric.matched, 1);
  const oversized = firstPage.rows.find(r => r.oversized),
    oversizedFile =
      oversized.next.continue.args[
        oversized.next.continue.args.indexOf('--file') + 1
      ];
  writeFileSync(oversizedFile, '{}');
  const corrupted = cli(args);
  assert.notEqual(corrupted.status, 0);
  assert.match(corrupted.stderr, /Oversized evidence row changed/);
  writeFileSync(file, JSON.stringify({ rows: rows.slice(1) }));
  const stale = spawnSync(firstNext.command, firstNext.args, {
    cwd: root,
    encoding: 'utf8',
  });
  assert.notEqual(stale.status, 0);
  assert.match(stale.stderr, /Capture changed/);
  assert.notEqual(cli(['--file', file, '--pointer', '/absent']).status, 0);
  assert.notEqual(
    cli([
      '--file',
      file,
      '--where',
      '[{"path":"/id","op":"unknown","value":3}]',
    ]).status,
    0
  );
  assert(readFileSync(jsonl).length > 0);
  const large = join(root, 'large.json'),
    fd = openSync(large, 'w');
  writeSync(fd, '{"discard":"');
  const block = 'z'.repeat(1024 * 1024);
  for (let i = 0; i < 64; i++) writeSync(fd, block);
  writeSync(
    fd,
    '","data":{"/rows":' +
      JSON.stringify(
        Array.from({ length: 2000 }, (_, id) => ({
          id,
          text: 'emoji 🧠 quote ' + String.fromCharCode(34, 92),
        }))
      ) +
      '}}'
  );
  closeSync(fd);
  const bounded = spawnSync(
    process.execPath,
    [
      '--max-old-space-size=24',
      join(scripts, 'evidence-query.mjs'),
      '--file',
      large,
      '--pointer',
      '/data/~1rows',
      '--limit',
      '3',
    ],
    { cwd: root, encoding: 'utf8', timeout: 30000 }
  );
  assert.equal(bounded.status, 0, bounded.stderr);
  assert.equal(JSON.parse(bounded.stdout).matched, 2000);
  for (const invalid of [
    '{"rows":[1,]}',
    '{"rows":[1]} trailing',
    '{"rows":[1],"rows":[2]}',
    '{"rows":["bad\\q"]}',
  ]) {
    writeFileSync(join(root, 'invalid.json'), invalid);
    assert.notEqual(
      cli(['--file', join(root, 'invalid.json'), '--pointer', '/rows']).status,
      0
    );
  }
  console.log(
    JSON.stringify({
      ok: true,
      suite: 'executor-evidence',
      cases: 13,
      rows: 73,
      matched: 37,
    })
  );
} finally {
  rmSync(root, { recursive: true, force: true });
}
