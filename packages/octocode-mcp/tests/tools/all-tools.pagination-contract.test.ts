import { describe, it, expect } from 'vitest';
import { formatDirectToolSchemaText } from '@octocodeai/octocode-tools-core';
import { ALL_TOOLS } from '../../src/tools/toolConfig.js';

const LOSS_LANGUAGE: RegExp[] = [
  /may be truncated/i,
  /silently (?:dropped|truncated)/i,
  /first \d+ [^."]*only/i,
];

const TOOL_PAGINATION_KNOBS: Record<string, string[]> = {
  ghSearchCode: ['page'],
  ghGetFileContent: ['startLine', 'endLine'],
  ghViewRepoStructure: ['page'],
  ghSearchRepos: ['page'],
  ghHistoryResearch: ['page'],
  npmSearch: ['page'],
  ghCloneRepo: ['owner', 'repo'],
  localSearchCode: ['page'],
  localViewStructure: ['page'],
  localFindFiles: ['page'],
  localGetFileContent: ['startLine', 'endLine'],
  localBinaryInspect: ['path', 'mode'],
  lspGetSemantics: ['uri', 'lineHint', 'type'],
  oqlSearch: ['page', 'itemsPerPage'],
};

describe('all-tools pagination contract', () => {
  it('covers every tool in the live catalog', () => {
    expect(Object.keys(TOOL_PAGINATION_KNOBS).sort()).toEqual(
      ALL_TOOLS.map(tool => tool.name).sort()
    );
  });

  describe.each(Object.entries(TOOL_PAGINATION_KNOBS))(
    '%s',
    (toolName, knobs) => {
      const schemaText = formatDirectToolSchemaText(toolName);

      it(`declares its pagination knob(s): ${knobs.join(', ')}`, () => {
        for (const knob of knobs) {
          expect(schemaText, `missing knob "${knob}"`).toContain(`"${knob}"`);
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
