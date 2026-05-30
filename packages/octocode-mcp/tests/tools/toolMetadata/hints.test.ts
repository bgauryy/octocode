/**
 * Coverage tests for src/tools/toolMetadata/hints.ts
 *
 * Two test groups:
 * 1. "with global mock metadata" — uses the rich `mockContent` wired in
 *    tests/setup.ts (already initialized via initializeToolMetadata()).
 *    Exercises the proxy traps, getToolHintsSync (local/remote/unknown),
 *    getGenericErrorHintsSync, and getDynamicHints.
 * 2. "with isolated re-mocked metadata" — re-mocks @octocodeai/octocode-core
 *    locally so baseHints contain the strings that isLocalRelevantBaseHint
 *    filters out, and so the `getMetadataOrNull() ?? completeMetadata`
 *    fallback (null state) branches are covered.
 */
import { describe, it, expect, beforeAll, beforeEach, vi } from 'vitest';
import { initializeToolMetadata } from '../../../src/tools/toolMetadata/state.js';
import {
  TOOL_HINTS,
  getToolHintsSync,
  getGenericErrorHintsSync,
  getDynamicHints,
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
      expect(hints.empty).toEqual([
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
      // base + 14 tools
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
      // Global mock base hint ("Base hint for empty") does not match either
      // filter string, so it survives isLocalRelevantBaseHint (both ifs false).
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

  describe('getDynamicHints', () => {
    it('returns the dynamic hint array for a tool that has one', () => {
      expect(getDynamicHints(LOCAL_TOOL, 'parallelTip')).toEqual([
        'Use parallel queries for faster results',
      ]);
    });

    it('returns [] for a tool without that dynamic hint type', () => {
      // packageSearch uses mockToolSchema which has no `dynamic` block.
      expect(getDynamicHints('packageSearch', 'parallelTip')).toEqual([]);
    });

    it('returns [] for a known tool with an unknown dynamic hint key', () => {
      expect(getDynamicHints(LOCAL_TOOL, 'noSuchDynamicKey')).toEqual([]);
    });

    it('returns [] for an unknown tool', () => {
      expect(getDynamicHints(UNKNOWN_TOOL, 'parallelTip')).toEqual([]);
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
    // Both filtered hints removed; only the relevant base hint + tool hint kept.
    expect(hints).toEqual([
      'A genuinely local-relevant base hint',
      'Tool-specific empty hint',
    ]);
  });

  it('handles tools/baseHints missing optional fields (nullish fallbacks)', async () => {
    // A tool present in `tools` but missing `.hints`, and metadata with no
    // `baseHints` block and tool hints missing the requested resultType key.
    const sparseMetadata = {
      toolNames: { GITHUB_SEARCH_CODE: 'githubSearchCode' },
      tools: {
        // present so getOwnPropertyDescriptor enters the truthy branch, but no
        // `.hints` -> exercises `?? EMPTY_HINTS` (line 40).
        githubSearchCode: {
          name: 'githubSearchCode',
          description: 'd',
          schema: {},
        },
        // has hints object but no `empty` key -> exercises `?? []` (line 59).
        githubSearchRepositories: {
          name: 'githubSearchRepositories',
          description: 'd',
          schema: {},
          hints: { hasResults: ['hr'] },
        },
      },
      // no baseHints field at all -> exercises `metadata.baseHints?.[...] ?? []`
      // (line 54).
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

    // line 40: tool present but no .hints -> EMPTY_HINTS
    expect(
      Object.getOwnPropertyDescriptor(HINTS, 'githubSearchCode')?.value
    ).toEqual({ empty: [] });

    // line 54 (no baseHints) + line 59 (hints has no `empty` key)
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
      getDynamicHints: getDyn,
    } = await import('../../../src/tools/toolMetadata/hints.js');

    // Force getMetadataOrNull() to return null so `?? completeMetadata` runs.
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
    expect(getDyn('githubSearchCode', 'whatever')).toEqual([]);
  });
});
