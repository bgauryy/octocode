import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { node, withDir } from './helpers.mjs';

const TOKEN = 'ghp_abcdefghijklmnopqrstuvwxyz0123456789';
const report = `# Report\n\nCall with Authorization: Bearer abc.def-123456 and {"api_key":"k-998877"}.\nhttps://ex.test/a?session_id=s3cr3t&page=2\n${TOKEN}\nContact: sales@ex.test\n`;

test('redact-secrets masks credentials in place and keeps the surrounding text', () => withDir((dir) => {
  const file = join(dir, 'report.md'), clean = join(dir, 'clean.md');
  writeFileSync(file, report);
  writeFileSync(clean, 'No secret here.\n');
  const res = node('redact-secrets.mjs', ['--file', file, '--file', clean, '--in-place']);
  assert.equal(res.status, 0, res.stdout + res.stderr);
  const out = JSON.parse(res.stdout);
  const text = readFileSync(file, 'utf8');
  for (const secret of ['abc.def-123456', 'k-998877', 's3cr3t', TOKEN]) assert.ok(!text.includes(secret), secret);
  assert.ok(!res.stdout.includes('k-998877'), 'output never carries file text');
  assert.ok(text.includes('page=2') && text.includes('sales@ex.test') && text.startsWith('# Report'));
  assert.equal(out.files[0].redactions, text.split('[REDACTED]').length - 1);
  assert.equal(out.files[0].changed, true);
  assert.deepEqual(out.files[1], { file: clean, out: clean, redactions: 0, changed: false });
  assert.equal(readFileSync(clean, 'utf8'), 'No secret here.\n');
}));

test('redact-secrets --out leaves the source and rejects ambiguous input', () => withDir((dir) => {
  const file = join(dir, 'snippet.txt'), copy = join(dir, 'snippet.redacted.txt');
  writeFileSync(file, report);
  const res = node('redact-secrets.mjs', ['--file', file, '--out', copy]);
  assert.equal(res.status, 0, res.stdout);
  assert.equal(readFileSync(file, 'utf8'), report);
  assert.ok(!readFileSync(copy, 'utf8').includes(TOKEN));
  for (const args of [[], ['--file', file], ['--file', file, '--in-place', '--out', copy], ['--file', file, '--file', file, '--out', copy], ['--file', join(dir, 'missing'), '--in-place'], ['--file', file, '--in-place', '--all']]) {
    const bad = node('redact-secrets.mjs', args);
    assert.equal(bad.status, 2, args.join(' '));
    assert.equal(JSON.parse(bad.stdout).error.code, 'invalidArguments');
  }
}));
