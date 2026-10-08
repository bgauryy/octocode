import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve, join } from 'node:path';

const root = resolve(import.meta.dirname, '..');
test('production sources are TypeScript and real typechecking gates verification', () => {
  const manifest = JSON.parse(readFileSync(join(root, 'package.json'), 'utf8'));
  assert.match(manifest.scripts.typecheck, /tsc/);
  assert.match(manifest.scripts.verify, /typecheck/);
  const sources = directory =>
    readdirSync(directory, { withFileTypes: true }).flatMap(row =>
      row.isDirectory()
        ? sources(join(directory, row.name))
        : [join(directory, row.name)]
    );
  const files = sources(join(root, 'src'));
  assert(files.some(file => file.endsWith('.ts') || file.endsWith('.mts')));
  assert(
    !files.some(file => /\.(?:m?js)$/.test(file)),
    'authored JavaScript remains under src'
  );
  assert(
    !manifest.files.includes('scripts'),
    'source/test scripts must not ship alongside generated runtime'
  );
});
