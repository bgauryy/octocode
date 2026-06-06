import { describe, it, expect } from 'vitest';
import { formatDirectToolSchemaText } from '../../src/tools/directToolCatalog.js';

const LOSS_LANGUAGE: RegExp[] = [
  /may be truncated/i,
  /silently (?:dropped|truncated)/i,
  /first \d+ [^."]*only/i,
];

const TOOL_PAGINATION_KNOBS: Record<string, string[]> = {
  githubSearchCode: ['page'],
  githubGetFileContent: ['startLine', 'endLine'],
  githubViewRepoStructure: ['page'],
  githubSearchRepositories: ['page'],
  githubSearchPullRequests: ['page'],
  packageSearch: ['page'],
  githubCloneRepo: ['owner', 'repo'],
  localSearchCode: ['page'],
  localViewStructure: ['page'],
  localFindFiles: ['page'],
  localGetFileContent: ['startLine', 'endLine'],
  lspGotoDefinition: ['uri', 'lineHint'],
  lspFindReferences: ['page'],
  lspCallHierarchy: ['page'],
};

describe('all-tools pagination contract', () => {
  describe.each(Object.entries(TOOL_PAGINATION_KNOBS))(
    '%s',
    (toolName, knobs) => {
      const schemaText = formatDirectToolSchemaText(toolName);

      it(`declares its pagination knob(s): ${knobs.join(', ')}`, () => {
        for (const knob of knobs) {
          expect(schemaText, `missing knob "${knob}"`).toContain(`"${knob}"`);
        }
      });

      it('declares the boolean verbose detail control', () => {
        expect(schemaText).toMatch(/"verbose"/);
      });

      it('schema is free of silent-loss language (paginates, never truncates)', () => {
        for (const re of LOSS_LANGUAGE) {
          expect(schemaText, `loss-language matched ${re}`).not.toMatch(re);
        }
      });
    }
  );
});
