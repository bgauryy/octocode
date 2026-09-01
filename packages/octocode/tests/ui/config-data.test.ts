import { describe, expect, it } from 'vitest';
import { DIRECT_TOOL_DISCOVERY_DEFINITIONS } from '@octocodeai/octocode-tools-core/schema';
import {
  ALL_CONFIG_OPTIONS,
  getAllTools,
} from '../../src/ui/config/config-data.js';

describe('interactive configuration catalog', () => {
  it('stays complete with the canonical engine-free public tool catalog', () => {
    expect(getAllTools().map(tool => tool.id)).toEqual(
      DIRECT_TOOL_DISCOVERY_DEFINITIONS.map(tool => tool.name)
    );
    expect(getAllTools()).toHaveLength(12);
    expect(getAllTools().map(tool => tool.id)).toEqual(
      expect.arrayContaining(['ghSearchHistory', 'ghGetHistoryItem'])
    );
    expect(getAllTools().map(tool => tool.id)).not.toEqual(
      expect.arrayContaining([
        'ghSearchPullRequests',
        'ghSearchIssues',
        'ghSearchCommits',
      ])
    );
  });

  it.each([
    'github.code',
    'github.repositories',
    'github.tree',
    'local.text',
    'local.files',
    'local.tree',
  ])('does not expose removed tool %s', toolName => {
    expect(getAllTools().some(tool => tool.id === toolName)).toBe(false);
  });

  it('classifies npm separately and describes only npm package search', () => {
    expect(getAllTools().find(tool => tool.id === 'npmSearch')).toMatchObject({
      category: 'package',
      description: expect.stringMatching(/npm/i),
    });
    expect(
      getAllTools().find(tool => tool.id === 'npmSearch')?.description
    ).not.toMatch(/python/i);
  });

  it('shows the canonical enabled-by-default local setting', () => {
    expect(
      ALL_CONFIG_OPTIONS.find(option => option.envVar === 'ENABLE_LOCAL')
        ?.defaultValue
    ).toBe('true');
  });

  it.each(['ENABLE_RELEASES', 'ENABLE_DISCUSSIONS'])(
    'shows %s as opt-in',
    envVar => {
      expect(
        ALL_CONFIG_OPTIONS.find(option => option.envVar === envVar)
          ?.defaultValue
      ).toBe('false');
    }
  );
});
