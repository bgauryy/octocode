import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { mkdtempSync, mkdirSync, realpathSync, rmSync } from 'node:fs';
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

  beforeEach(() => {
    outside = realpathSync(mkdtempSync(join(tmpdir(), 'octocoderc-allowed-')));
    mkdirSync(join(outside, 'proj'));
    state.allowedPaths = [];
    state.workspaceRoot = undefined;
    // Restore the home-inclusive default so no roots leak between tests.
    resetPathValidator();
  });

  afterEach(() => {
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
});
