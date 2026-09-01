import { describe, expect, it } from 'vitest';

import { buildViewStructureNextMap } from '../../../src/tools/local_view_structure/viewStructureNext.js';
import type { DirectoryEntry } from '../../../src/tools/local_view_structure/structureFilters.js';

const file = (path: string): DirectoryEntry => ({
  name: path.split('/').pop() ?? path,
  path,
  type: 'file',
});
const dir = (path: string): DirectoryEntry => ({
  name: path.split('/').pop() ?? path,
  path,
  type: 'directory',
});

describe('buildViewStructureNextMap', () => {
  it('emits fetch (first file) and viewDeeper (first dir), meta-free', () => {
    const map = buildViewStructureNextMap([
      file('/repo/src/index.ts'),
      dir('/repo/src/utils'),
    ]);

    expect(map?.fetch?.tool).toBe('localGetFileContent');
    expect(map?.fetch?.query).toEqual({
      path: '/repo/src/index.ts',
      minify: 'standard',
    });
    expect(map?.viewDeeper?.tool).toBe('local.tree');
    expect(map?.viewDeeper?.query).toEqual({ path: '/repo/src/utils' });
    for (const k of ['id', 'researchGoal', 'mainResearchGoal', 'reasoning']) {
      expect(map?.fetch?.query).not.toHaveProperty(k);
      expect(map?.viewDeeper?.query).not.toHaveProperty(k);
    }
  });

  it('emits only fetch when there are no subdirectories', () => {
    const map = buildViewStructureNextMap([file('/repo/a.ts')]);
    expect(map?.fetch).toBeDefined();
    expect(map).not.toHaveProperty('viewDeeper');
  });

  it('emits only viewDeeper when there are no files', () => {
    const map = buildViewStructureNextMap([dir('/repo/pkgs')]);
    expect(map?.viewDeeper).toBeDefined();
    expect(map).not.toHaveProperty('fetch');
  });

  it('returns undefined for an empty listing', () => {
    expect(buildViewStructureNextMap([])).toBeUndefined();
  });
});
