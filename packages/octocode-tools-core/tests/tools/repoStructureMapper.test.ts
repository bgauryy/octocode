import { describe, expect, it } from 'vitest';

import { mapRepoStructureProviderResult } from '../../src/tools/providerMappers.js';
import type { RepoStructureResult } from '../../src/providers/providerResults.js';

type Structure = RepoStructureResult['structure'];

function makeResult(overrides: Partial<RepoStructureResult>): RepoStructureResult {
  return {
    projectPath: 'owner/repo',
    branch: 'main',
    path: '/',
    structure: {},
    summary: { totalFiles: 0, totalFolders: 0, truncated: false },
    ...overrides,
  };
}

describe('mapRepoStructureProviderResult – fileSizes keying (basename collision)', () => {
  it('keys sizes by full relative path so same-named files in different dirs do not collide', () => {
    const structure: Structure = {
      'src/a': { files: ['index.ts'], folders: [] },
      'src/b': { files: ['index.ts'], folders: [] },
    };
    const data = makeResult({
      structure,
      fileSizeMap: {
        'src/a': { 'index.ts': 111 },
        'src/b': { 'index.ts': 222 },
      },
      summary: { totalFiles: 2, totalFolders: 0, truncated: false },
    });

    const result = mapRepoStructureProviderResult(
      data,
      {} as never,
      structure,
      'main'
    );

    const fileSizes = result.fileSizes as Record<string, number>;
    expect(fileSizes['src/a/index.ts']).toBe(111);
    expect(fileSizes['src/b/index.ts']).toBe(222);
    // The pre-fix behavior collapsed both onto a single 'index.ts' key.
    expect(fileSizes['index.ts']).toBeUndefined();
  });

  it('keeps root-directory files under their bare name', () => {
    const structure: Structure = {
      '.': { files: ['README.md', 'package.json'], folders: ['src'] },
    };
    const data = makeResult({
      structure,
      fileSizeMap: { '.': { 'README.md': 2048, 'package.json': 1024 } },
      summary: { totalFiles: 2, totalFolders: 1, truncated: false },
    });

    const result = mapRepoStructureProviderResult(
      data,
      {} as never,
      structure,
      'main'
    );

    const fileSizes = result.fileSizes as Record<string, number>;
    expect(fileSizes['README.md']).toBe(2048);
    expect(fileSizes['package.json']).toBe(1024);
  });
});

describe('mapRepoStructureProviderResult – summary reflects filtered structure', () => {
  it('recomputes totalFiles/totalFolders from the filtered structure, not the provider summary', () => {
    // Provider counted 5 files / 2 folders before filtering out ignored
    // entries; the filtered structure only carries 2 files / 1 folder.
    const filtered: Structure = {
      '.': { files: ['index.ts'], folders: ['src'] },
      src: { files: ['app.ts'], folders: [] },
    };
    const data = makeResult({
      structure: filtered,
      summary: { totalFiles: 5, totalFolders: 2, truncated: false },
    });

    const result = mapRepoStructureProviderResult(
      data,
      {} as never,
      filtered,
      'main'
    );

    expect(result.summary).toEqual({ totalFiles: 2, totalFolders: 1 });
  });
});
