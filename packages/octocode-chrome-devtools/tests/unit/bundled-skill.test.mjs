import test from 'node:test';
import assert from 'node:assert/strict';
import { readdirSync, readFileSync } from 'node:fs';
import { join, resolve } from 'node:path';

const root = resolve(import.meta.dirname, '../..');
const guide = resolve(root, '../../skills/octocode-chrome-devtools');
const bundled = join(root, 'dist/skills/octocode-chrome-devtools');

/** Every file under directory, relative and sorted. */
const entries = directory =>
  readdirSync(directory, { recursive: true, withFileTypes: true })
    .filter(entry => entry.isFile())
    .map(entry =>
      join(entry.parentPath, entry.name).slice(directory.length + 1)
    )
    .sort();

// The build copies the canonical skill into dist; a stale copy ships old guidance.
test('the built skill is a byte-identical copy of the canonical skill', () => {
  assert.deepEqual(entries(bundled), entries(guide));
  for (const file of entries(guide))
    assert.deepEqual(
      readFileSync(join(bundled, file)),
      readFileSync(join(guide, file)),
      `bundled skill differs: ${file}`
    );
});

test('the package exports the bundled skill guide', () => {
  const pkg = JSON.parse(readFileSync(join(root, 'package.json'), 'utf8'));
  assert.equal(
    pkg.exports['./skill'],
    './dist/skills/octocode-chrome-devtools/SKILL.md'
  );
});
