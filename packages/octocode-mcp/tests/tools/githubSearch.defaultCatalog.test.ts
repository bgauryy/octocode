import { describe, expect, it } from 'vitest';

import { ALL_TOOLS } from '../../src/tools/toolConfig.js';
import { DIRECT_TOOL_DISCOVERY_DEFINITIONS } from '@octocodeai/octocode-tools-core';

const LEGACY_GITHUB_DISCOVERY_TOOLS = [
  'github.code',
  'github.repositories',
  'github.tree',
] as const;

describe('MCP unified ghSearch default catalog', () => {
  it('keeps ghSearch default and removes the legacy discovery aliases', () => {
    expect(ALL_TOOLS.find(tool => tool.name === 'ghSearch')).toMatchObject({
      isDefault: true,
      isLocal: false,
    });
    for (const legacyName of LEGACY_GITHUB_DISCOVERY_TOOLS) {
      expect(ALL_TOOLS.some(tool => tool.name === legacyName)).toBe(false);
      expect(
        DIRECT_TOOL_DISCOVERY_DEFINITIONS.some(tool => tool.name === legacyName)
      ).toBe(false);
    }
  });

  it('keeps the two gated GitHub tools discoverable with explicit env gates', () => {
    expect(
      DIRECT_TOOL_DISCOVERY_DEFINITIONS.filter(tool => tool.disabled).map(
        tool => [tool.name, tool.disabled?.envVar]
      )
    ).toEqual([
      ['ghListReleases', 'ENABLE_RELEASES'],
      ['ghSearchDiscussions', 'ENABLE_DISCUSSIONS'],
    ]);
  });
});
