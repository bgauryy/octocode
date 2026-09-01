import { describe, expect, it } from 'vitest';

import { ALL_TOOLS } from '../../src/tools/toolConfig.js';
import { DIRECT_TOOL_DISCOVERY_DEFINITIONS } from '@octocodeai/octocode-tools-core';
import { isToolEnabled } from '../../src/tools/toolFilters.js';

const LEGACY_LOCAL_DISCOVERY_TOOLS = [
  'local.text',
  'local.files',
  'local.tree',
] as const;

describe('MCP unified localSearch default catalog', () => {
  it('keeps four local defaults and removes the legacy discovery aliases', () => {
    const localResearchNames = ALL_TOOLS.filter(
      tool => tool.isLocal && !tool.isClone && tool.isDefault
    )
      .map(tool => tool.name)
      .sort();

    expect(localResearchNames).toEqual(
      [
        'localSearch',
        'localGetFileContent',
        'localAnalyzeGraph',
        'lspGetSemantics',
      ].sort()
    );
    for (const legacyName of LEGACY_LOCAL_DISCOVERY_TOOLS) {
      expect(ALL_TOOLS.some(tool => tool.name === legacyName)).toBe(false);
      expect(
        DIRECT_TOOL_DISCOVERY_DEFINITIONS.some(tool => tool.name === legacyName)
      ).toBe(false);
    }
  });

  it('keeps clone default-classified but separately gated', () => {
    const cloneTools = ALL_TOOLS.filter(tool => tool.isClone);
    expect(cloneTools).toHaveLength(1);
    expect(cloneTools[0]).toMatchObject({
      name: 'ghCloneRepo',
      isDefault: true,
      isLocal: true,
      isClone: true,
    });

    const withoutClone = ALL_TOOLS.filter(tool =>
      isToolEnabled(tool, {
        localEnabled: true,
        cloneEnabled: false,
        filterConfig: { toolsToRun: [], disableTools: [] },
      })
    ).map(tool => tool.name);
    expect(withoutClone).not.toContain('ghCloneRepo');
    expect(withoutClone).toEqual(expect.arrayContaining(['localSearch']));

    const withClone = ALL_TOOLS.filter(tool =>
      isToolEnabled(tool, {
        localEnabled: true,
        cloneEnabled: true,
        filterConfig: { toolsToRun: [], disableTools: [] },
      })
    ).map(tool => tool.name);
    expect(withClone).toContain('ghCloneRepo');
  });
});
