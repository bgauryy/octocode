import { describe, it, expect, beforeAll, beforeEach, vi } from 'vitest';
import { initializeToolMetadata } from '../../../src/tools/toolMetadata/state.js';
import {
  TOOL_HINTS,
  getToolHintsSync,
  getGenericErrorHintsSync,
} from '../../../src/tools/toolMetadata/hints.js';

const LOCAL_TOOL = 'localSearchCode';
const REMOTE_TOOL = 'githubSearchCode';
const UNKNOWN_TOOL = 'thisToolDoesNotExist';

describe('hints.ts with global mock metadata', () => {
  beforeAll(async () => {
    await initializeToolMetadata();
  });

  describe('TOOL_HINTS proxy get trap', () => {
    it('returns baseHints for the "base" key', () => {
      expect(TOOL_HINTS.base).toEqual({
        hasResults: ['Base hint for hasResults'],
        empty: ['Base hint for empty'],
      });
    });

    it('returns the tool hints object for a real tool', () => {
      const hints = TOOL_HINTS[REMOTE_TOOL];
      expect(hints?.empty).toEqual([
        'Test hint for empty 1',
        'Test hint for empty 2',
      ]);
    });

    it('returns EMPTY_HINTS fallback for an unknown prop', () => {
      const hints = TOOL_HINTS[UNKNOWN_TOOL];
      expect(hints).toEqual({ empty: [] });
    });
  });

  describe('TOOL_HINTS proxy ownKeys trap', () => {
    it('includes "base" plus every tool name', () => {
      const keys = Object.keys(TOOL_HINTS);
      expect(keys).toContain('base');
      expect(keys).toContain(LOCAL_TOOL);
      expect(keys).toContain(REMOTE_TOOL);
      expect(keys.length).toBeGreaterThan(1);
    });
  });

  describe('TOOL_HINTS proxy getOwnPropertyDescriptor trap', () => {
    it('describes the "base" key', () => {
      const desc = Object.getOwnPropertyDescriptor(TOOL_HINTS, 'base');
      expect(desc).toMatchObject({ enumerable: true, configurable: true });
      expect(desc?.value).toEqual({
        hasResults: ['Base hint for hasResults'],
        empty: ['Base hint for empty'],
      });
    });

    it('describes a real tool key with its hints', () => {
      const desc = Object.getOwnPropertyDescriptor(TOOL_HINTS, REMOTE_TOOL);
      expect(desc).toMatchObject({ enumerable: true, configurable: true });
      expect((desc?.value as { empty: string[] }).empty).toEqual([
        'Test hint for empty 1',
        'Test hint for empty 2',
      ]);
    });

    it('returns undefined for an unknown key', () => {
      const desc = Object.getOwnPropertyDescriptor(TOOL_HINTS, UNKNOWN_TOOL);
      expect(desc).toBeUndefined();
    });
  });

  describe('getToolHintsSync', () => {
    it('returns base + tool hints for a remote tool (no base filtering)', () => {
      const hints = getToolHintsSync(REMOTE_TOOL, 'empty');
      expect(hints).toEqual([
        'Base hint for empty',
        'Test hint for empty 1',
        'Test hint for empty 2',
      ]);
    });

    it('returns filtered base + tool hints for a local tool', () => {
      const hints = getToolHintsSync(LOCAL_TOOL, 'empty');
      expect(hints).toEqual([
        'Base hint for empty',
        'Test hint for empty 1',
        'Test hint for empty 2',
      ]);
    });

    it('returns [] for an unknown tool', () => {
      expect(getToolHintsSync(UNKNOWN_TOOL, 'empty')).toEqual([]);
    });
  });

  describe('getGenericErrorHintsSync', () => {
    it('returns the metadata genericErrorHints array', () => {
      expect(getGenericErrorHintsSync()).toEqual([
        'Generic error hint 1',
        'Generic error hint 2',
        'Generic error hint 3',
        'Generic error hint 4',
        'Generic error hint 5',
      ]);
    });
  });
});

describe('hints.ts with isolated re-mocked metadata', () => {
  beforeEach(() => {
    vi.resetModules();
  });

  it('filters out local-irrelevant base hints for a local tool', async () => {
    const isolatedMetadata = {
      toolNames: { LOCAL_RIPGREP: 'localSearchCode' },
      tools: {
        localSearchCode: {
          name: 'localSearchCode',
          description: 'd',
          schema: {},
          hints: { hasResults: [], empty: ['Tool-specific empty hint'] },
        },
      },
      baseHints: {
        hasResults: [],
        empty: [
          "Provide 'owner', 'repo', 'branch', 'path' to narrow the search",
          "Always set 'mainResearchGoal' for traceability",
          'A genuinely local-relevant base hint',
        ],
      },
      genericErrorHints: ['err'],
    };

    vi.doMock('@octocodeai/octocode-core', () => ({
      completeMetadata: isolatedMetadata,
      octocodeConfig: isolatedMetadata,
    }));

    const { initializeToolMetadata: init, _resetMetadataState } =
      await import('../../../src/tools/toolMetadata/state.js');
    const { getToolHintsSync: getHints } =
      await import('../../../src/tools/toolMetadata/hints.js');

    _resetMetadataState();
    await init();

    const hints = getHints('localSearchCode', 'empty');
    expect(hints).toEqual([
      'A genuinely local-relevant base hint',
      'Tool-specific empty hint',
    ]);
  });

  it('handles tools/baseHints missing optional fields (nullish fallbacks)', async () => {
    const sparseMetadata = {
      toolNames: { GITHUB_SEARCH_CODE: 'githubSearchCode' },
      tools: {
        githubSearchCode: {
          name: 'githubSearchCode',
          description: 'd',
          schema: {},
        },
        githubSearchRepositories: {
          name: 'githubSearchRepositories',
          description: 'd',
          schema: {},
          hints: { hasResults: ['hr'] },
        },
      },
      genericErrorHints: [],
    };

    vi.doMock('@octocodeai/octocode-core', () => ({
      completeMetadata: sparseMetadata,
      octocodeConfig: sparseMetadata,
    }));

    const { initializeToolMetadata: init, _resetMetadataState } =
      await import('../../../src/tools/toolMetadata/state.js');
    const { TOOL_HINTS: HINTS, getToolHintsSync: getHints } =
      await import('../../../src/tools/toolMetadata/hints.js');

    _resetMetadataState();
    await init();

    expect(
      Object.getOwnPropertyDescriptor(HINTS, 'githubSearchCode')?.value
    ).toEqual({ empty: [] });

    expect(getHints('githubSearchRepositories', 'empty')).toEqual([]);
  });

  it('falls back to completeMetadata when metadata state is null', async () => {
    const fallbackMetadata = {
      toolNames: { GITHUB_SEARCH_CODE: 'githubSearchCode' },
      tools: {
        githubSearchCode: {
          name: 'githubSearchCode',
          description: 'd',
          schema: {},
          hints: { hasResults: [], empty: ['fallback tool hint'] },
        },
      },
      baseHints: { hasResults: [], empty: ['fallback base hint'] },
      genericErrorHints: ['fallback error hint'],
    };

    vi.doMock('@octocodeai/octocode-core', () => ({
      completeMetadata: fallbackMetadata,
      octocodeConfig: fallbackMetadata,
    }));

    const { _resetMetadataState } =
      await import('../../../src/tools/toolMetadata/state.js');
    const {
      TOOL_HINTS: HINTS,
      getToolHintsSync: getHints,
      getGenericErrorHintsSync: getErrHints,
    } = await import('../../../src/tools/toolMetadata/hints.js');

    _resetMetadataState();

    expect(HINTS.base).toEqual({
      hasResults: [],
      empty: ['fallback base hint'],
    });
    expect(Object.keys(HINTS)).toEqual(['base', 'githubSearchCode']);
    expect(
      Object.getOwnPropertyDescriptor(HINTS, 'githubSearchCode')?.value
    ).toEqual({ hasResults: [], empty: ['fallback tool hint'] });

    expect(getHints('githubSearchCode', 'empty')).toEqual([
      'fallback base hint',
      'fallback tool hint',
    ]);
    expect(getErrHints()).toEqual(['fallback error hint']);
  });
});
