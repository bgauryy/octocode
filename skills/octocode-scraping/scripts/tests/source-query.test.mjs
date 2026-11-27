import { test } from 'node:test';
import assert from 'node:assert/strict';
import { writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { node, readSource, scripts, withDir } from './helpers.mjs';

const first = (file, extra = []) => ({ command: process.execPath, args: [join(scripts, 'source-query.mjs'), '--file', file, ...extra] });

test('utf8 pages end on character boundaries and join to the exact file', () => withDir((dir) => {
  const file = join(dir, 'mixed.md');
  // A byte-order mark, 2-, 3- and 4-byte characters, so most page lengths cut inside one.
  const text = `\uFEFF${'aé€🧠\n'.repeat(3)}`;
  writeFileSync(file, text);
  for (const length of [1, 2, 3, 5, 7]) {
    const pages = [];
    for (let next = first(file, ['--length', String(length), '--encoding', 'utf8']); next;) {
      const res = node('source-query.mjs', next.args.slice(1));
      assert.equal(res.status, 0, res.stdout);
      const page = JSON.parse(res.stdout);
      assert.equal(page.encoding, 'utf8');
      // A page runs past --length only to finish one character (at most 4 bytes).
      assert.ok(page.bytes <= Math.max(length, 4), `length ${length}: page of ${page.bytes} bytes`);
      assert.equal(Buffer.byteLength(page.content), page.bytes, 'content is the whole page, no replacement characters');
      pages.push(page.content);
      next = page.next?.continue;
    }
    assert.equal(pages.join(''), text, `length ${length}`);
  }
}));

test('auto pages text as utf8 and other bytes as base64; utf8 refuses invalid UTF-8', () => withDir((dir) => {
  const text = join(dir, 'page.json'), binary = join(dir, 'image.bin');
  writeFileSync(text, JSON.stringify({ title: 'Prix 12 €' }));
  const bytes = Buffer.from([0x89, 0x50, 0x4e, 0x47, 0xff, 0xfe, 0x00, 0xc3]);
  writeFileSync(binary, bytes);
  const read = readSource(first(text, ['--length', '4']));
  assert.ok(read.encodings.every((e) => e === 'utf8'));
  assert.deepEqual(JSON.parse(read.bytes), { title: 'Prix 12 €' });
  const raw = readSource(first(binary, ['--length', '3']));
  assert.ok(raw.encodings.every((e) => e === 'base64'));
  assert.deepEqual(raw.bytes, bytes);
  const refused = node('source-query.mjs', ['--file', binary, '--encoding', 'utf8']);
  assert.equal(refused.status, 2);
  assert.equal(JSON.parse(refused.stdout).error.code, 'notUtf8');
}));

test('a utf8 offset inside a character and an unknown encoding are invalid input', () => withDir((dir) => {
  const file = join(dir, 'emoji.txt');
  writeFileSync(file, '🧠🧠');
  for (const args of [['--offset', '1', '--encoding', 'utf8'], ['--encoding', 'latin1']]) {
    const res = node('source-query.mjs', ['--file', file, ...args]);
    assert.equal(res.status, 2, res.stdout);
    assert.equal(JSON.parse(res.stdout).error.code, 'invalidArguments');
  }
}));
