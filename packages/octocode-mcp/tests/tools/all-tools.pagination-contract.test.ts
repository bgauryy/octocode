import { describe, it, expect } from 'vitest';
import {
  DIRECT_TOOL_DISCOVERY_DEFINITIONS,
  formatDirectToolSchemaText,
} from '@octocodeai/octocode-tools-core';

const LOSS_LANGUAGE: RegExp[] = [
  /may be truncated/i,
  /silently (?:dropped|truncated)/i,
  /first \d+ [^."]*only/i,
];

const TOOL_PAGINATION_CONTRACT: Record<
  string,
  { controls: string[]; exemption?: string }
> = {
  ghSearch: { controls: ['page', 'pageSize'] },
  ghGetFileContent: { controls: ['charOffset', 'charLength'] },
  ghSearchHistory: { controls: ['page', 'pageSize'] },
  ghGetHistoryItem: {
    controls: [
      'filePage',
      'commentPage',
      'commitPage',
      'pageSize',
      'charOffset',
      'commentBodyOffset',
      'charLength',
    ],
  },
  ghListReleases: { controls: ['page', 'pageSize'] },
  ghSearchDiscussions: { controls: ['after', 'pageSize'] },
  npmSearch: { controls: ['page', 'pageSize'] },
  ghCloneRepo: {
    controls: [],
    exemption: 'bounded clone/materialization operation',
  },
  localSearch: { controls: ['page', 'pageSize'] },
  localAnalyzeGraph: { controls: ['page', 'pageSize'] },
  localGetFileContent: { controls: ['charOffset', 'charLength'] },
  lspGetSemantics: { controls: ['page', 'pageSize'] },
};

const TOTAL_CAP_TOOLS = new Set(['localSearch', 'localAnalyzeGraph']);

describe('all-tools pagination contract', () => {
  it('covers every tool in the live catalog', () => {
    expect(Object.keys(TOOL_PAGINATION_CONTRACT).sort()).toEqual(
      DIRECT_TOOL_DISCOVERY_DEFINITIONS.map(tool => tool.name).sort()
    );
  });

  describe.each(Object.entries(TOOL_PAGINATION_CONTRACT))(
    '%s',
    (toolName, contract) => {
      const schemaText = formatDirectToolSchemaText(toolName);

      it('declares real pagination controls or a bounded-operation exemption', () => {
        expect(
          contract.controls.length > 0 || contract.exemption,
          'missing pagination controls or exemption reason'
        ).toBeTruthy();
        for (const knob of contract.controls) {
          expect(schemaText, `missing knob "${knob}"`).toContain(`"${knob}"`);
        }
      });

      it('uses limit only as a supported pre-pagination total cap', () => {
        if (TOTAL_CAP_TOOLS.has(toolName)) {
          expect(schemaText).toContain('"limit"');
        } else {
          expect(schemaText).not.toContain('"limit"');
        }
      });

      it('schema is free of silent-loss language (paginates, never truncates)', () => {
        for (const re of LOSS_LANGUAGE) {
          expect(schemaText, `loss-language matched ${re}`).not.toMatch(re);
        }
      });
    }
  );
});
