import { describe, it, expect } from 'vitest';
import { execSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, realpathSync, rmSync, symlinkSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { canonicalizePath, fillScope, normalizeWorkspacePath } from '../src/git.js';

describe('canonicalizePath', () => {
  it('resolves an existing directory to its real (symlink-free) path', () => {
    const base = mkdtempSync(join(tmpdir(), 'oc-canon-'));
    const real = join(base, 'real');
    const link = join(base, 'link');
    mkdirSync(real);
    try {
      symlinkSync(real, link);
      expect(canonicalizePath(link)).toBe(realpathSync(real));
    } finally {
      rmSync(base, { recursive: true, force: true });
    }
  });

  it('resolves the longest existing ancestor and rejoins a not-yet-created tail', () => {
    const base = mkdtempSync(join(tmpdir(), 'oc-canon-'));
    const real = join(base, 'real');
    const link = join(base, 'link');
    mkdirSync(real);
    try {
      symlinkSync(real, link);
      // "nested/subdir" under the symlink does not exist on disk.
      expect(canonicalizePath(join(link, 'nested', 'subdir'))).toBe(
        join(realpathSync(real), 'nested', 'subdir'),
      );
    } finally {
      rmSync(base, { recursive: true, force: true });
    }
  });

  it('falls back to a plain resolve when no ancestor exists (defensive, should not throw)', () => {
    expect(() => canonicalizePath('/definitely/does/not/exist/anywhere')).not.toThrow();
  });
});

describe('fillScope / normalizeWorkspacePath — symlink and git-init timing stability (regression)', () => {
  // Root cause reproduced live during the 2026-07-08 audit: fillScope only
  // canonicalized workspace_path via git's already-symlink-resolved root
  // (git rev-parse --show-toplevel) once a directory was a git repo, but used
  // a raw path.resolve() before that or for symlinked non-git paths. Two
  // requests for the "same" workspace (pre/post `git init`, or symlink vs.
  // real path) could therefore key different scope rows with no error.

  function tempDirWithLink(): { real: string; link: string; base: string } {
    const base = mkdtempSync(join(tmpdir(), 'oc-scope-'));
    const real = join(base, 'real');
    const link = join(base, 'link');
    mkdirSync(real, { recursive: true });
    symlinkSync(real, link);
    return { real, link, base };
  }

  it('a symlinked path and its real path resolve to the same workspace_path', () => {
    const { real, link, base } = tempDirWithLink();
    try {
      const viaLink = normalizeWorkspacePath(link, link);
      const viaReal = normalizeWorkspacePath(real, real);
      expect(viaLink).toBe(viaReal);
    } finally {
      rmSync(base, { recursive: true, force: true });
    }
  });

  it('a non-git workspace resolves to the same scope key before and after `git init`', () => {
    const base = mkdtempSync(join(tmpdir(), 'oc-scope-gitinit-'));
    try {
      // Before git init: no repo detected, fillScope used to fall through to a
      // raw resolve() with no realpath canonicalization.
      const before = fillScope({ workspace_path: base }, base).workspace_path;

      execSync('git init -q', { cwd: base });
      execSync('git config user.email t@t.test', { cwd: base });
      execSync('git config user.name t', { cwd: base });
      writeFileSync(join(base, 'README.md'), 'seed');
      execSync('git add -A && git commit -q -m seed', { cwd: base });

      // After git init: git rev-parse --show-toplevel resolves symlinks, which
      // used to produce a DIFFERENT key than the pre-init raw resolve() above
      // whenever `base` crossed a symlink (e.g. macOS /tmp -> /private/tmp).
      const after = fillScope({ workspace_path: base }, base).workspace_path;

      expect(after).toBe(before);
    } finally {
      rmSync(base, { recursive: true, force: true });
    }
  });

  it('normalizeWorkspacePath is idempotent — normalizing twice yields the same key', () => {
    const { link, base } = tempDirWithLink();
    try {
      const once = normalizeWorkspacePath(link, link);
      const twice = once ? normalizeWorkspacePath(once, once) : null;
      expect(twice).toBe(once);
    } finally {
      rmSync(base, { recursive: true, force: true });
    }
  });
});
