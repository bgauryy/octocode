import { afterEach, describe, expect, it, vi } from 'vitest';
import {
  mkdirSync,
  mkdtempSync,
  rmSync,
  utimesSync,
  writeFileSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import {
  findStaleSourceInput,
  maybeWarnAboutStaleBuild,
  resetStaleBuildWarningForTests,
} from '../../src/cli/stale-build.js';

const roots: string[] = [];

function fixture(): { root: string; built: string; changed: string } {
  const root = mkdtempSync(join(tmpdir(), 'octocode-stale-build-'));
  roots.push(root);
  const built = join(root, 'out', 'chunks', 'cli.js');
  const entry = join(root, 'src', 'cli', 'index.ts');
  const changed = join(root, 'src', 'cli', 'main-help.ts');
  mkdirSync(join(root, 'out', 'chunks'), { recursive: true });
  mkdirSync(join(root, 'src', 'cli'), { recursive: true });
  writeFileSync(built, 'built');
  writeFileSync(entry, 'entry');
  writeFileSync(changed, 'changed');
  const old = new Date('2026-01-01T00:00:00.000Z');
  const fresh = new Date('2026-01-01T00:00:05.000Z');
  utimesSync(built, old, old);
  utimesSync(entry, old, old);
  utimesSync(changed, fresh, fresh);
  return { root, built, changed };
}

afterEach(() => {
  resetStaleBuildWarningForTests();
  while (roots.length > 0)
    rmSync(roots.pop()!, { recursive: true, force: true });
});

describe('stale build detection', () => {
  it('detects changes anywhere under src, not only cli/index.ts', () => {
    const { built } = fixture();
    expect(findStaleSourceInput(built)).toBe('src/cli/main-help.ts');
  });

  it('does not warn while executing source directly', () => {
    const { root } = fixture();
    expect(
      findStaleSourceInput(join(root, 'src', 'cli', 'index.ts'))
    ).toBeUndefined();
  });

  it('warns at most once and names the changed input', () => {
    const { built } = fixture();
    const warn = vi.fn();
    maybeWarnAboutStaleBuild({ currentFile: built, env: {}, warn });
    maybeWarnAboutStaleBuild({ currentFile: built, env: {}, warn });
    expect(warn).toHaveBeenCalledTimes(1);
    expect(warn).toHaveBeenCalledWith(
      expect.stringContaining('src/cli/main-help.ts')
    );
  });
});
