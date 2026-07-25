import { describe, expect, it } from 'vitest';

import { buildFindFilesNextMap } from '../../../src/tools/local_find_files/findFilesNext.js';
import type { LocalFindFilesEntry } from '@octocodeai/octocode-core/types';

const file = (path: string): LocalFindFilesEntry => ({ path, type: 'file' });
const dir = (path: string): LocalFindFilesEntry => ({ path, type: 'directory' });

describe('buildFindFilesNextMap', () => {
  it('points fetch at the first file with its absolute path, meta-free', () => {
    const map = buildFindFilesNextMap([
      dir('/repo/src'),
      file('/repo/src/a.ts'),
      file('/repo/src/b.ts'),
    ]);

    expect(map?.fetch?.tool).toBe('localGetFileContent');
    expect(map?.fetch?.query).toEqual({
      path: '/repo/src/a.ts',
      minify: 'standard',
    });
    expect(map?.fetch?.confidence).toBe('exact');
    // No auto-filled per-call metadata leaks into a from-scratch hint query.
    for (const k of ['id', 'researchGoal', 'mainResearchGoal', 'reasoning']) {
      expect(map?.fetch?.query).not.toHaveProperty(k);
    }
    expect(map).not.toHaveProperty('viewStructure');
  });

  it('falls back to a viewStructure hint when the page is all directories', () => {
    const map = buildFindFilesNextMap([dir('/repo/a'), dir('/repo/b')]);

    expect(map?.viewStructure?.tool).toBe('localViewStructure');
    expect(map?.viewStructure?.query).toEqual({ path: '/repo/a' });
    expect(map).not.toHaveProperty('fetch');
  });

  it('returns undefined when there is nothing to point at', () => {
    expect(buildFindFilesNextMap([])).toBeUndefined();
  });
});
