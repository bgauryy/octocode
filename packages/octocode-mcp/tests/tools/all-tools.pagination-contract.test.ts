/**
 * Per-tool PAGINATION CONTRACT + loss-language sanity, for every tool.
 *
 * Deliberately NON-overlapping with the neighbouring suites:
 *   - catalog registration / bulk-schema existence → directToolCatalog.test.ts
 *   - envelope numeric bounds (responseChar*) per tool → scheme/bulk_envelope_bounds.test.ts
 *   - pagination-cursor generator uniformity → all-tools.pagination.test.ts
 *
 * What ONLY this suite asserts, uniformly across all 14 tools:
 *   1. each tool declares a usable per-query pagination knob (the agent can
 *      always page to more — no dead-end result sets), and
 *   2. its schema carries no silent-loss language (we paginate, never truncate).
 */
import { describe, it, expect } from 'vitest';
import { formatDirectToolSchemaText } from '../../src/tools/directToolCatalog.js';

/** Schema phrases that would imply silent loss (contract drift). */
const LOSS_LANGUAGE: RegExp[] = [
  /may be truncated/i,
  /silently (?:dropped|truncated)/i,
  /first \d+ [^."]*only/i,
];

/**
 * The per-query pagination knob(s) each tool exposes as schema properties.
 * (charOffset is the response-level cursor; per-query tools expose at least one
 * of these so the agent can always reach the rest of a large result.)
 */
const TOOL_PAGINATION_KNOBS: Record<string, string[]> = {
  githubSearchCode: ['charLength', 'page'],
  githubGetFileContent: ['charOffset', 'charLength'],
  githubViewRepoStructure: ['page', 'itemsPerPage'],
  githubSearchRepositories: ['page'],
  githubSearchPullRequests: ['charOffset', 'charLength', 'page'],
  packageSearch: ['itemsPerPage'],
  githubCloneRepo: ['charOffset', 'charLength'],
  localSearchCode: ['page', 'itemsPerPage', 'matchesPerFile'],
  localViewStructure: ['page', 'itemsPerPage'],
  localFindFiles: ['page', 'itemsPerPage'],
  localGetFileContent: ['charOffset', 'charLength'],
  lspGotoDefinition: ['charOffset', 'charLength'],
  lspFindReferences: ['itemsPerPage', 'page'],
  lspCallHierarchy: ['itemsPerPage', 'page'],
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

      it('declares verbosity (token cost lever)', () => {
        expect(schemaText).toMatch(/"verbosity"/);
      });

      it('schema is free of silent-loss language (paginates, never truncates)', () => {
        for (const re of LOSS_LANGUAGE) {
          expect(schemaText, `loss-language matched ${re}`).not.toMatch(re);
        }
      });
    }
  );
});
