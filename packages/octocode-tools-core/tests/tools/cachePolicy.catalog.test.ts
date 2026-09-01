import { afterAll, describe, expect, it, vi } from 'vitest';

const previousFlags = vi.hoisted(() => {
  const values = {
    releases: process.env.ENABLE_RELEASES,
    discussions: process.env.ENABLE_DISCUSSIONS,
  };
  process.env.ENABLE_RELEASES = 'true';
  process.env.ENABLE_DISCUSSIONS = 'true';
  return values;
});

import { ALL_TOOLS } from '../../src/tools/toolConfig.js';

afterAll(() => {
  if (previousFlags.releases === undefined) delete process.env.ENABLE_RELEASES;
  else process.env.ENABLE_RELEASES = previousFlags.releases;
  if (previousFlags.discussions === undefined)
    delete process.env.ENABLE_DISCUSSIONS;
  else process.env.ENABLE_DISCUSSIONS = previousFlags.discussions;
});

const CACHE_WORKFLOWS = {
  sharedResponse: [
    'ghSearch',
    'ghSearchHistory',
    'ghGetHistoryItem',
    'ghListReleases',
    'ghSearchDiscussions',
    'npmSearch',
  ],
  conditionalMaterialization: ['ghGetFileContent'],
  clone: ['ghCloneRepo'],
  liveWorkspace: [
    'localSearch',
    'localGetFileContent',
    'localAnalyzeGraph',
    'lspGetSemantics',
  ],
} as const;

describe('public tool cache-workflow policy', () => {
  it('classifies every tool exactly once', () => {
    const classified = Object.values(CACHE_WORKFLOWS).flat();
    const catalogNames = ALL_TOOLS.map(tool => tool.name);

    expect(new Set(classified).size).toBe(classified.length);
    expect([...classified].sort()).toEqual([...catalogNames].sort());
  });

  it('keeps cache policy aligned with runtime routing metadata', () => {
    const tools = new Map(ALL_TOOLS.map(tool => [tool.name, tool]));

    for (const name of CACHE_WORKFLOWS.liveWorkspace) {
      const tool = tools.get(name);
      expect(tool, name).toBeDefined();
      expect(tool?.isLocal, name).toBe(true);
      expect(tool?.isClone, name).not.toBe(true);
    }

    for (const name of [
      ...CACHE_WORKFLOWS.sharedResponse,
      ...CACHE_WORKFLOWS.conditionalMaterialization,
    ]) {
      const tool = tools.get(name);
      expect(tool, name).toBeDefined();
      expect(tool?.isLocal, name).toBe(false);
      if (name === 'npmSearch') {
        expect(tool?.direct.requiresProviders, name).not.toBe(true);
      } else {
        expect(tool?.direct.requiresProviders, name).toBe(true);
      }
      expect(tool?.isClone, name).not.toBe(true);
    }

    const clone = tools.get(CACHE_WORKFLOWS.clone[0]);
    expect(clone).toBeDefined();
    expect(clone?.isLocal).toBe(true);
    expect(clone?.isClone).toBe(true);
    expect(clone?.direct.requiresProviders).toBe(true);
  });
});
