import { describe, expect, it, beforeEach, afterEach } from 'vitest';
import { mkdtempSync, mkdirSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { getCheckedOutSizeBytes } from '../../../src/tools/github_clone_repo/contentSize.js';

// A sparse blob:none checkout of a few KB otherwise reports the ~1MB packed-git
// floor because `.git` is counted. getCheckedOutSizeBytes measures only the
// working tree.
describe('getCheckedOutSizeBytes — excludes .git from the size metric', () => {
  let dir: string;

  beforeEach(() => {
    dir = mkdtempSync(join(tmpdir(), 'clone-size-'));
  });

  afterEach(() => {
    rmSync(dir, { recursive: true, force: true });
  });

  it('counts checked-out content but not the .git directory', () => {
    // 10 bytes of real content.
    writeFileSync(join(dir, 'README.md'), '0123456789');
    mkdirSync(join(dir, 'src'));
    writeFileSync(join(dir, 'src', 'a.ts'), 'abcde'); // 5 bytes

    // A big fake .git that would dominate if counted.
    mkdirSync(join(dir, '.git', 'objects', 'pack'), { recursive: true });
    writeFileSync(
      join(dir, '.git', 'objects', 'pack', 'pack-x.pack'),
      'x'.repeat(50_000)
    );

    const size = getCheckedOutSizeBytes(dir);
    expect(size).toBe(15);
  });

  it('returns 0 for a missing path', () => {
    expect(getCheckedOutSizeBytes(join(dir, 'nope'))).toBe(0);
  });
});
