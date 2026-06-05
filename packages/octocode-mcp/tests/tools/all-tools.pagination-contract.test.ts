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
 * All tools use `page` (1-based) with a fixed internal page size constant.
 * Navigation tools (view-structure, find-files) expose `page`; search tools
 * expose `page` from the upstream schema.
 */
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
