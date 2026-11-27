// A chunk's evidence files hold each page of this build once: a refresh that rebuilds a chunk over a reused engine
// snapshot asks the same queries, so its walk files keep their names, and must not grow by the previous build's pages.
import assert from 'node:assert/strict';
import test from 'node:test';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { gunzipSync } from 'node:zlib';
import { evidenceWriter } from '../src/runtime/section.ts';
import { tempDir } from './test-tmp.mjs';

const pages = (file) => gunzipSync(readFileSync(file)).toString().split('\n').filter(Boolean);

test('a rebuilt chunk replaces the evidence a previous build wrote under the same name', async () => {
  const dir = tempDir('section-evidence');
  const file = join(dir, 'walk.jsonl.gz');
  const first = evidenceWriter();
  first.append(file, 'page 1\n');
  first.append(file, 'page 2\n');
  await first.flush();
  assert.deepEqual(pages(file), ['page 1', 'page 2']);
  // The refresh: a new writer over the same directory, the same query, one page this time.
  const again = evidenceWriter();
  again.append(file, 'page 1\n');
  await again.flush();
  assert.deepEqual(pages(file), ['page 1']);
});
