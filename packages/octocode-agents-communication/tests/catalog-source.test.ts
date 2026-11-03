// assets/catalog.json is generated from the Zod sources in src/catalog; a hand edit or a source change not yet emitted fails here.
import assert from 'node:assert/strict';
import { readFileSync, readdirSync } from 'node:fs';
import { join } from 'node:path';
import { test } from 'node:test';
import { renderCatalog } from '../src/catalog/index.ts';
import { catalogFile } from '../src/emit-catalog.ts';
import { root } from '../src/entry-points.ts';
import { isReply } from '../src/script-support.ts';

test('assets/catalog.json is exactly what src/catalog generates', () => {
  assert.equal(readFileSync(catalogFile, 'utf8'), renderCatalog(), 'edit the Zod sources in src/catalog and run `yarn catalog`, never the JSON');
});

test('Zod stays a build-time tool: no runtime source imports it and it is not a runtime dependency', () => {
  const manifest: unknown = JSON.parse(readFileSync(join(root, 'package.json'), 'utf8'));
  assert.ok(isReply(manifest));
  assert.ok(isReply(manifest.devDependencies) && 'zod' in manifest.devDependencies);
  assert.equal(isReply(manifest.dependencies) && 'zod' in manifest.dependencies, false);
  for (const directory of ['runtime', 'entries', 'hosts']) {
    for (const file of readdirSync(join(root, 'src', directory), { recursive: true, encoding: 'utf8' })) {
      if (file.endsWith('.ts')) assert.doesNotMatch(readFileSync(join(root, 'src', directory, file), 'utf8'), /from 'zod'/, `${directory}/${file}`);
    }
  }
});

test('every catalog source module stays small enough to read', () => {
  for (const file of readdirSync(join(root, 'src/catalog'))) {
    const lines = readFileSync(join(root, 'src/catalog', file), 'utf8').split('\n').length;
    assert.ok(lines <= 400, `${file} has ${lines} lines`);
  }
});
