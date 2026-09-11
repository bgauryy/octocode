import { describe, expect, it } from 'vitest';
import { DIRECT_TOOL_DISCOVERY_DEFINITIONS } from '@octocodeai/octocode-core/schema';
import {
  ALL_CONFIG_OPTIONS,
  getAllTools,
} from '../../src/ui/config/config-data.js';

describe('interactive configuration catalog', () => {
  it('stays complete with the canonical engine-free public tool catalog', () => {
    expect(getAllTools().map(tool => tool.id)).toEqual(
      DIRECT_TOOL_DISCOVERY_DEFINITIONS.map(tool => tool.name)
    );
    expect(getAllTools()).toHaveLength(11);
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

  it('derives tool titles and descriptions from the canonical catalog', () => {
    expect(getAllTools()).toEqual(
      DIRECT_TOOL_DISCOVERY_DEFINITIONS.map(tool => ({
        id: tool.name,
        name: tool.title,
        description: tool.description,
        category:
          tool.name === 'artifactSearch'
            ? 'package'
            : tool.name.startsWith('gh')
              ? 'github'
              : 'local',
      }))
    );
  });

  it.each([
    'github.code',
    'github.repositories',
    'github.tree',
    'local.text',
    'localAnalyzeGraph',
    'lspGetSemantics',
  ])('does not expose removed tool %s', toolName => {
    expect(getAllTools().some(tool => tool.id === toolName)).toBe(false);
  });

  it('classifies artifact discovery as package search', () => {
    expect(
      getAllTools().find(tool => tool.id === 'artifactSearch')
    ).toMatchObject({
      category: 'package',
      description: expect.stringMatching(/packages|registry/i),
    });
    expect(
      getAllTools().find(tool => tool.id === 'artifactSearch')?.description
    ).toMatch(/ecosystem|type|PyPI/i);
  });

  it('shows the canonical enabled-by-default local setting', () => {
    expect(
      ALL_CONFIG_OPTIONS.find(option => option.envVar === 'ENABLE_LOCAL')
        ?.defaultValue
    ).toBe('true');
  });
});
