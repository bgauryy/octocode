#!/usr/bin/env node
import assert from 'node:assert/strict';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { spawnSync } from 'node:child_process';
if (process.argv.includes('--help')) {
  console.log(
    'Usage: source-pagination-self-test.mjs\nChecks arbitrary source reconstruction, large rows, Unicode, precision and immutable continuations.'
  );
  process.exit(0);
}
const scripts = join(import.meta.dirname, '../dist/engine'),
  root = mkdtempSync(join(tmpdir(), 'octo-source-pages-'));
let pages = 0;
function call(args) {
  return spawnSync(
    process.execPath,
    [join(scripts, 'artifact-query.mjs'), ...args],
    { encoding: 'utf8', timeout: 10000 }
  );
}
function collect(file, format, extra = [], pageLength = '511') {
  let result = call([
      '--file',
      file,
      '--format',
      format,
      '--length',
      pageLength,
      ...extra,
    ]),
    content = '',
    consumed = 0,
    binary = [];
  for (;;) {
    assert.equal(result.status, 0, result.stderr);
    const page = JSON.parse(result.stdout);
    pages++;
    assert(
      Buffer.byteLength(JSON.stringify(page.content)) <= 20000,
      'encoded page must fit its response budget'
    );
    assert.equal(page.offset, consumed);
    consumed += page.length;
    content += page.content;
    if (format === 'binary') binary.push(Buffer.from(page.content, 'base64'));
    if (!page.next) {
      assert.equal(consumed, page.totalUnits);
      return format === 'binary' ? Buffer.concat(binary) : content;
    }
    result = spawnSync(page.next.continue.command, page.next.continue.args, {
      encoding: 'utf8',
      timeout: 10000,
    });
  }
}
try {
  const text = '\uFEFF' + 'delayed content 😀\n'.repeat(200) + 'x'.repeat(9000),
    textFile = join(root, 'events.jsonl');
  writeFileSync(textFile, text);
  assert.equal(collect(textFile, 'text'), text);
  const tinyFile = join(root, 'tiny.txt');
  writeFileSync(tinyFile, '😀א中');
  assert.equal(collect(tinyFile, 'text', [], '1'), '😀א中');
  const escapedFile = join(root, 'escaped.txt');
  writeFileSync(escapedFile, '\u0000'.repeat(25000));
  assert.equal(
    collect(escapedFile, 'text', [], '20000'),
    '\u0000'.repeat(25000)
  );
  const json =
      '{"console":[{"message":"' +
      'x'.repeat(12000) +
      '"}],"a/b":{"~key":900719925474099312345}}',
    jsonFile = join(root, 'capture.json');
  writeFileSync(jsonFile, json);
  assert.equal(collect(jsonFile, 'json'), json);
  assert.equal(
    collect(jsonFile, 'json', ['--pointer', '/a~1b/~0key']),
    '900719925474099312345'
  );
  assert.equal(
    JSON.parse(collect(jsonFile, 'json', ['--pointer', '/console']))[0].message
      .length,
    12000
  );
  const binary = Buffer.from(Array.from({ length: 4099 }, (_, i) => i % 256)),
    binFile = join(root, 'trace.bin');
  writeFileSync(binFile, binary);
  assert.deepEqual(collect(binFile, 'binary'), binary);
  assert.notEqual(call(['--file', binFile, '--format', 'text']).status, 0);
  const first = JSON.parse(call(['--file', textFile, '--length', '10']).stdout);
  writeFileSync(textFile, text + 'changed');
  const changed = spawnSync(
    first.next.continue.command,
    first.next.continue.args,
    { encoding: 'utf8' }
  );
  assert.notEqual(changed.status, 0);
  assert.match(changed.stderr, /Capture changed/);
  assert.notEqual(
    call(['--file', jsonFile, '--format', 'json', '--pointer', '/missing'])
      .status,
    0
  );
  assert.notEqual(call(['--file', jsonFile, '--length', '0']).status, 0);
  assert(readFileSync(jsonFile).length > 10000);
  console.log(
    JSON.stringify({ ok: true, suite: 'source-pagination', cases: 11, pages })
  );
} finally {
  rmSync(root, { recursive: true, force: true });
}
