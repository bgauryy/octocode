import { describe, it, expect } from 'vitest';
import {
  DIRECT_TOOL_DISCOVERY_DEFINITIONS,
  formatDirectToolOutputSchemaText,
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
  ghSearchCode: { controls: ['page'] },
  ghGetFileContent: { controls: ['charOffset', 'charLength'] },
  ghViewRepoStructure: { controls: ['page', 'itemsPerPage'] },
  ghSearchRepos: { controls: ['page'] },
  ghSearchPullRequests: { controls: ['page', 'charOffset', 'charLength'] },
  ghSearchIssues: { controls: ['page', 'charOffset', 'charLength'] },
  ghSearchCommits: { controls: ['page', 'charOffset', 'charLength'] },
  ghListReleases: { controls: ['page', 'itemsPerPage'] },
  ghSearchDiscussions: { controls: ['after', 'itemsPerPage'] },
  npmSearch: { controls: ['page'] },
  ghCloneRepo: {
    controls: [],
    exemption: 'bounded clone/materialization operation',
  },
  localSearchCode: { controls: ['page', 'itemsPerPage'] },
  localViewStructure: { controls: ['page', 'itemsPerPage'] },
  localFindFiles: { controls: ['page', 'itemsPerPage'] },
  localAnalyzeGraph: { controls: ['page', 'itemsPerPage'] },
  localGetFileContent: { controls: ['charOffset', 'charLength'] },
  lspGetSemantics: { controls: ['page', 'itemsPerPage'] },
};

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
      const outputSchemaText = formatDirectToolOutputSchemaText(toolName);

      it('declares real pagination controls or a bounded-operation exemption', () => {
        expect(
          contract.controls.length > 0 || contract.exemption,
          'missing pagination controls or exemption reason'
        ).toBeTruthy();
        for (const knob of contract.controls) {
          expect(schemaText, `missing knob "${knob}"`).toContain(`"${knob}"`);
        }
      });

      it('declares pagination and machine-ready continuation output', () => {
        expect(outputSchemaText).toContain('"pagination"');
        expect(outputSchemaText).toContain('"next"');
      });

      it('schema is free of silent-loss language (paginates, never truncates)', () => {
        for (const re of LOSS_LANGUAGE) {
          expect(schemaText, `loss-language matched ${re}`).not.toMatch(re);
        }
      });
    }
  );
});
