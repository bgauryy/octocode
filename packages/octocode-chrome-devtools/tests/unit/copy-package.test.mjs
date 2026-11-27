import test from 'node:test';
import assert from 'node:assert/strict';
import {
  mkdtempSync,
  mkdirSync,
  readFileSync,
  readdirSync,
  rmSync,
  writeFileSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { copyPackage } from '../copy-package.mjs';

test('standalone copies retain declared artifacts without workspace caches or dependencies', () => {
  const work = mkdtempSync(join(tmpdir(), 'octo-package-copy-'));
  try {
    const source = join(work, 'source');
    for (const dir of ['dist', '.octocode', 'node_modules', 'src']) {
      mkdirSync(join(source, dir), { recursive: true });
      writeFileSync(join(source, dir, 'entry.js'), dir);
    }
    writeFileSync(join(source, 'README.md'), 'standalone guide');
    writeFileSync(
      join(source, 'package.json'),
      JSON.stringify({ files: ['dist', 'README.md'] })
    );
    const destination = join(work, 'copy');
    copyPackage(source, destination);
    assert.deepEqual(readdirSync(destination).toSorted(), [
      'README.md',
      'dist',
      'package.json',
    ]);
    assert.equal(
      readFileSync(join(destination, 'dist/entry.js'), 'utf8'),
      'dist'
    );
    assert.equal(
      readFileSync(join(destination, 'README.md'), 'utf8'),
      'standalone guide'
    );
    rmSync(join(source, 'dist'), { recursive: true });
    assert.throws(() => copyPackage(source, join(work, 'missing-artifact')), {
      code: 'ENOENT',
    });
  } finally {
    rmSync(work, { recursive: true, force: true });
  }
});
