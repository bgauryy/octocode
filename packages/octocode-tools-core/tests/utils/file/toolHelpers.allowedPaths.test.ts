import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import {
  mkdtempSync,
  mkdirSync,
  realpathSync,
  rmSync,
  symlinkSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { resetPathValidator } from '@octocodeai/octocode-engine/pathValidator';

// Control the resolved config the tool layer sees, keeping every other field
// as the real resolver produces it. This lets us prove that `.octocoderc`
// `local.allowedPaths` is honored by the path allow-list — i.e. parity with
// the `ALLOWED_PATHS` env var, which the PathValidator reads directly.
const state: { allowedPaths: string[]; workspaceRoot: string | undefined } = {
  allowedPaths: [],
  workspaceRoot: undefined,
};

vi.mock('@octocodeai/config', async importOriginal => {
  const actual = (await importOriginal()) as Record<string, unknown> & {
    getConfigSync: () => { local: Record<string, unknown> };
  };
  return {
    ...actual,
    getConfigSync: () => {
      const base = actual.getConfigSync();
      return {
        ...base,
        local: {
          ...base.local,
          allowedPaths: state.allowedPaths,
          workspaceRoot: state.workspaceRoot,
        },
      };
    },
  };
});

import { validateToolPath } from '../../../src/utils/file/toolHelpers.js';

describe('validateToolPath — .octocoderc local.allowedPaths wiring', () => {
  let outside: string;
  let previousOctocodeHome: string | undefined;

  beforeEach(() => {
    outside = realpathSync(mkdtempSync(join(tmpdir(), 'octocoderc-allowed-')));
    mkdirSync(join(outside, 'proj'));
    previousOctocodeHome = process.env.OCTOCODE_HOME;
    state.allowedPaths = [];
    state.workspaceRoot = undefined;
    // Restore the home-inclusive default so no roots leak between tests.
    resetPathValidator();
  });

  afterEach(() => {
    if (previousOctocodeHome === undefined) delete process.env.OCTOCODE_HOME;
    else process.env.OCTOCODE_HOME = previousOctocodeHome;
    rmSync(outside, { recursive: true, force: true });
    resetPathValidator();
  });

  it('denies a path outside home when local.allowedPaths is empty', () => {
    const r = validateToolPath(
      { path: join(outside, 'proj') },
      'LOCAL_FIND_FILES'
    );
    expect(r.isValid).toBe(false);
  });

  it('allows a path once .octocoderc local.allowedPaths includes its root (parity with ALLOWED_PATHS env)', () => {
    state.allowedPaths = [outside];
    const r = validateToolPath(
      { path: join(outside, 'proj') },
      'LOCAL_FIND_FILES'
    );
    expect(r.isValid).toBe(true);
    expect(r.sanitizedPath).toBe(join(outside, 'proj'));
  });

  it('supports a leading ~ in local.allowedPaths (expanded like the env var)', () => {
    // Home is already allowed, so this just proves ~ expansion does not throw
    // and the path resolves; use a subdir that exists under the temp root by
    // registering the temp root via ~-free absolute entry alongside a ~ entry.
    state.allowedPaths = ['~', outside];
    const r = validateToolPath(
      { path: join(outside, 'proj') },
      'LOCAL_FIND_FILES'
    );
    expect(r.isValid).toBe(true);
  });

  it('allows clone and tree continuations under custom OCTOCODE_HOME managed roots', () => {
    const customHome = join(outside, 'custom-home');
    const clonedRepo = join(
      customHome,
      'tmp',
      'clone',
      'octocode',
      'repo',
      'main'
    );
    const materializedTree = join(
      customHome,
      'tmp',
      'tree',
      'octocode',
      'repo',
      '0123456789abcdef0123456789abcdef01234567'
    );
    mkdirSync(clonedRepo, { recursive: true });
    mkdirSync(materializedTree, { recursive: true });
    process.env.OCTOCODE_HOME = customHome;

    const cloneResult = validateToolPath(
      { path: clonedRepo },
      'LOCAL_VIEW_STRUCTURE'
    );
    const treeResult = validateToolPath(
      { path: materializedTree },
      'LOCAL_VIEW_STRUCTURE'
    );

    expect(cloneResult.isValid).toBe(true);
    expect(cloneResult.sanitizedPath).toBe(clonedRepo);
    expect(treeResult.isValid).toBe(true);
    expect(treeResult.sanitizedPath).toBe(materializedTree);
  });

  it('does not authorize arbitrary paths elsewhere under custom OCTOCODE_HOME', () => {
    const customHome = join(outside, 'custom-home');
    const arbitraryPath = join(customHome, 'private');
    mkdirSync(arbitraryPath, { recursive: true });
    process.env.OCTOCODE_HOME = customHome;

    const r = validateToolPath({ path: arbitraryPath }, 'LOCAL_VIEW_STRUCTURE');

    expect(r.isValid).toBe(false);
  });

  it('rejects a symlink escape from the managed clone root', () => {
    const customHome = join(outside, 'custom-home');
    const cloneRoot = join(customHome, 'tmp', 'clone');
    const escapedTarget = join(outside, 'escaped-target');
    const escapeLink = join(cloneRoot, 'escape');
    mkdirSync(cloneRoot, { recursive: true });
    mkdirSync(escapedTarget);
    symlinkSync(escapedTarget, escapeLink);
    process.env.OCTOCODE_HOME = customHome;

    const r = validateToolPath({ path: escapeLink }, 'LOCAL_VIEW_STRUCTURE');

    expect(r.isValid).toBe(false);
  });
});
