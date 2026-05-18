/**
 * Local Tools — Schema & Pagination Audit
 *
 * Tracks the schema contract for every local tool that octocode-mcp registers
 * with MCP. Split into two parts:
 *
 *  1. Pinning tests against the upstream schemas in `@octocodeai/octocode-core`
 *     — these document the current per-tool caps and pagination field names so
 *     a regression in the upstream package is detected at build time.
 *
 *  2. Behavioural tests against the **local overlay** at
 *     `../../src/scheme/localSchemaOverlay.ts`, which is what the tool
 *     `register.ts` files actually hand to the MCP SDK. The overlay relaxes
 *     the user-hostile caps that were causing live `-32602` errors.
 */
import { describe, expect, it } from 'vitest';
import {
  RipgrepQuerySchema as UpstreamRipgrepQuerySchema,
  FetchContentQuerySchema as UpstreamFetchContentQuerySchema,
  FindFilesQuerySchema as UpstreamFindFilesQuerySchema,
  ViewStructureQuerySchema as UpstreamViewStructureQuerySchema,
  LSPFindReferencesQuerySchema,
} from '@octocodeai/octocode-core';

import {
  RipgrepQuerySchema,
  BulkRipgrepQuerySchema,
  FindFilesQuerySchema,
  BulkFindFilesSchema,
  ViewStructureQuerySchema,
  BulkViewStructureSchema,
  FetchContentQuerySchema,
  BulkFetchContentQuerySchema,
  LOCAL_OVERLAY_MAX_MATCH_CONTENT_LENGTH,
  LOCAL_OVERLAY_MAX_CHAR_LENGTH,
  VERBOSITY_VALUES,
  verbosityField,
} from '../../src/scheme/localSchemaOverlay.js';

import {
  LSPGotoDefinitionQuerySchema,
  BulkLSPGotoDefinitionQuerySchema,
  LSPFindReferencesQuerySchema as OverlayLSPFindReferencesQuerySchema,
  BulkLSPFindReferencesQuerySchema,
  LSPCallHierarchyQuerySchema,
  BulkLSPCallHierarchyQuerySchema,
} from '../../src/scheme/lspSchemaOverlay.js';

const baseRipgrep = {
  id: 'rip_test',
  researchGoal: 'audit',
  reasoning: 'audit',
  pattern: 'x',
  path: '/src',
};

const baseFindFiles = {
  id: 'find_test',
  researchGoal: 'audit',
  reasoning: 'audit',
  path: '/src',
};

const baseViewStructure = {
  id: 'view_test',
  researchGoal: 'audit',
  reasoning: 'audit',
  path: '/src',
};

const baseFetchContent = {
  id: 'fetch_test',
  researchGoal: 'audit',
  reasoning: 'audit',
  path: '/src/file.ts',
};

const baseLSPRefs = {
  id: 'refs_test',
  researchGoal: 'audit',
  reasoning: 'audit',
  uri: '/src/file.ts',
  symbolName: 'foo',
  lineHint: 1,
};

describe('Local Tools — Upstream schema pinning', () => {
  describe('1. matchContentLength cap (the user-reported bug)', () => {
    it('upstream caps matchContentLength at 800 (root cause of -32602 errors)', () => {
      const result = UpstreamRipgrepQuerySchema.safeParse({
        ...baseRipgrep,
        matchContentLength: 801,
      });
      expect(result.success).toBe(false);
      expect(JSON.stringify(result.error?.issues ?? [])).toContain('800');
    });

    it('upstream default for matchContentLength is 200', () => {
      const result = UpstreamRipgrepQuerySchema.parse(baseRipgrep);
      expect(result.matchContentLength).toBe(200);
    });
  });

  describe('2. charLength caps (drift across local tools)', () => {
    it('ripgrep charLength is uncapped on input shape (max 50000)', () => {
      const ok = UpstreamRipgrepQuerySchema.safeParse({
        ...baseRipgrep,
        charLength: 50000,
      });
      expect(ok.success).toBe(true);
    });

    it('localGetFileContent charLength upstream cap is 10000', () => {
      const result = UpstreamFetchContentQuerySchema.safeParse({
        ...baseFetchContent,
        charLength: 10001,
      });
      expect(result.success).toBe(false);
    });

    it('localFindFiles charLength upstream cap is 10000', () => {
      const result = UpstreamFindFilesQuerySchema.safeParse({
        ...baseFindFiles,
        charLength: 10001,
      });
      expect(result.success).toBe(false);
    });

    it('localViewStructure charLength upstream cap is 10000', () => {
      const result = UpstreamViewStructureQuerySchema.safeParse({
        ...baseViewStructure,
        charLength: 10001,
      });
      expect(result.success).toBe(false);
    });
  });

  describe('3. Pagination field-name drift', () => {
    it('localSearchCode (ripgrep) uses filePageNumber, not page', () => {
      const parsed = UpstreamRipgrepQuerySchema.parse(baseRipgrep);
      expect(parsed.filePageNumber).toBe(1);
      expect('page' in parsed).toBe(false);
    });

    it('localFindFiles uses filePageNumber, not page', () => {
      const parsed = UpstreamFindFilesQuerySchema.parse(baseFindFiles);
      expect(parsed.filePageNumber).toBe(1);
      expect('page' in parsed).toBe(false);
    });

    it('localViewStructure uses entryPageNumber, not page or filePageNumber', () => {
      const parsed = UpstreamViewStructureQuerySchema.parse(baseViewStructure);
      expect(parsed.entryPageNumber).toBe(1);
      expect('page' in parsed).toBe(false);
      expect('filePageNumber' in parsed).toBe(false);
    });

    it('LSP tools use page (the inconsistency target)', () => {
      const parsed = LSPFindReferencesQuerySchema.parse(baseLSPRefs);
      expect(parsed.page).toBe(1);
    });
  });
});

describe('Local Tools — Overlay schemas (what MCP actually receives)', () => {
  describe('Overlay caps', () => {
    it('overlay raises matchContentLength cap to >=2000 (fixes -32602 bug)', () => {
      expect(LOCAL_OVERLAY_MAX_MATCH_CONTENT_LENGTH).toBeGreaterThanOrEqual(
        2000
      );

      const result = RipgrepQuerySchema.safeParse({
        ...baseRipgrep,
        matchContentLength: 2000,
      });
      expect(result.success).toBe(true);
      if (result.success) {
        expect(result.data.matchContentLength).toBe(2000);
      }
    });

    it('overlay still rejects matchContentLength above its own ceiling', () => {
      const result = RipgrepQuerySchema.safeParse({
        ...baseRipgrep,
        matchContentLength: LOCAL_OVERLAY_MAX_MATCH_CONTENT_LENGTH + 1,
      });
      expect(result.success).toBe(false);
    });

    it('overlay preserves the matchContentLength default of 200', () => {
      const result = RipgrepQuerySchema.parse(baseRipgrep);
      expect(result.matchContentLength).toBe(200);
    });

    it('overlay raises localFindFiles charLength cap to >=10000', () => {
      expect(LOCAL_OVERLAY_MAX_CHAR_LENGTH).toBeGreaterThanOrEqual(10000);

      const result = FindFilesQuerySchema.safeParse({
        ...baseFindFiles,
        charLength: LOCAL_OVERLAY_MAX_CHAR_LENGTH,
      });
      expect(result.success).toBe(true);
    });

    it('overlay raises localViewStructure charLength cap to >=10000', () => {
      const result = ViewStructureQuerySchema.safeParse({
        ...baseViewStructure,
        charLength: LOCAL_OVERLAY_MAX_CHAR_LENGTH,
      });
      expect(result.success).toBe(true);
    });

    it('overlay unifies the charLength ceiling across local tools', () => {
      const acceptedByEveryone = LOCAL_OVERLAY_MAX_CHAR_LENGTH;
      expect(
        RipgrepQuerySchema.safeParse({
          ...baseRipgrep,
          charLength: acceptedByEveryone,
        }).success
      ).toBe(true);
      expect(
        FindFilesQuerySchema.safeParse({
          ...baseFindFiles,
          charLength: acceptedByEveryone,
        }).success
      ).toBe(true);
      expect(
        ViewStructureQuerySchema.safeParse({
          ...baseViewStructure,
          charLength: acceptedByEveryone,
        }).success
      ).toBe(true);
    });
  });

  describe('Overlay pagination fields', () => {
    it('overlay keeps the original page-number fields working unchanged', () => {
      expect(
        RipgrepQuerySchema.parse({ ...baseRipgrep, filePageNumber: 2 })
          .filePageNumber
      ).toBe(2);
      expect(
        FindFilesQuerySchema.parse({ ...baseFindFiles, filePageNumber: 2 })
          .filePageNumber
      ).toBe(2);
      expect(
        ViewStructureQuerySchema.parse({
          ...baseViewStructure,
          entryPageNumber: 2,
        }).entryPageNumber
      ).toBe(2);
    });
  });

  describe('Overlay bulk schemas (the schema MCP actually validates against)', () => {
    it('BulkRipgrepQuerySchema accepts matchContentLength above the upstream cap', () => {
      const result = BulkRipgrepQuerySchema.safeParse({
        queries: [{ ...baseRipgrep, matchContentLength: 2000 }],
      });
      expect(result.success).toBe(true);
    });

    it('BulkFindFilesSchema accepts overlay-level charLength', () => {
      const result = BulkFindFilesSchema.safeParse({
        queries: [
          { ...baseFindFiles, charLength: LOCAL_OVERLAY_MAX_CHAR_LENGTH },
        ],
      });
      expect(result.success).toBe(true);
    });

    it('BulkViewStructureSchema accepts entryPageNumber', () => {
      const result = BulkViewStructureSchema.safeParse({
        queries: [{ ...baseViewStructure, entryPageNumber: 2 }],
      });
      expect(result.success).toBe(true);
    });

    it('bulk schemas strip unknown top-level keys instead of rejecting them', () => {
      const result = BulkRipgrepQuerySchema.safeParse({
        queries: [baseRipgrep],
        unexpected: true,
      });
      expect(result.success).toBe(true);
    });

    it('bulk schemas keep duplicate-id detection from upstream', () => {
      const result = BulkRipgrepQuerySchema.safeParse({
        queries: [
          { ...baseRipgrep, id: 'dup' },
          { ...baseRipgrep, id: 'dup' },
        ],
      });
      expect(result.success).toBe(false);
      expect(JSON.stringify(result.error?.issues ?? [])).toContain('Duplicate');
    });
  });
});

/**
 * RFC `.octocode/rfc/rtk-token-techniques/RFC.md` §4.7.9 — schema descriptions
 * are the agent's training material for "less tokens, more quality research".
 * These tests pin both the field surface and the description's four-part
 * anatomy (intent line, cost framing, per-value semantics, drill-back).
 */
describe('Local Tools — Verbosity field (RFC §4.7.9 training material)', () => {
  it('exposes the three canonical verbosity values', () => {
    expect(VERBOSITY_VALUES).toEqual(['compact', 'verbose', 'ultra']);
  });

  /**
   * Every local + LSP tool MUST accept `verbosity` on its overlay schema.
   * RFC §3.1: omitted ⇒ byte-identical to current behaviour; the field
   * is the agent's training material for cost-aware usage.
   */
  const verbositySchemas: Array<{
    name: string;
    schema: { safeParse: (v: unknown) => { success: boolean } };
    base: Record<string, unknown>;
    bulk: { safeParse: (v: unknown) => { success: boolean } };
  }> = [
    {
      name: 'localSearchCode (ripgrep)',
      schema: RipgrepQuerySchema,
      base: baseRipgrep,
      bulk: BulkRipgrepQuerySchema,
    },
    {
      name: 'localFindFiles',
      schema: FindFilesQuerySchema,
      base: baseFindFiles,
      bulk: BulkFindFilesSchema,
    },
    {
      name: 'localViewStructure',
      schema: ViewStructureQuerySchema,
      base: baseViewStructure,
      bulk: BulkViewStructureSchema,
    },
    {
      name: 'localGetFileContent',
      schema: FetchContentQuerySchema,
      base: baseFetchContent,
      bulk: BulkFetchContentQuerySchema,
    },
    {
      name: 'lspGotoDefinition',
      schema: LSPGotoDefinitionQuerySchema,
      base: baseLSPRefs,
      bulk: BulkLSPGotoDefinitionQuerySchema,
    },
    {
      name: 'lspFindReferences (overlay)',
      schema: OverlayLSPFindReferencesQuerySchema,
      base: baseLSPRefs,
      bulk: BulkLSPFindReferencesQuerySchema,
    },
    {
      name: 'lspCallHierarchy',
      schema: LSPCallHierarchyQuerySchema,
      base: { ...baseLSPRefs, direction: 'incoming' as const },
      bulk: BulkLSPCallHierarchyQuerySchema,
    },
  ];

  describe.each(verbositySchemas)(
    'verbosity field — $name',
    ({ schema, base, bulk }) => {
      it('verbosity is optional (omitted ⇒ undefined ⇒ default/compact)', () => {
        const result = schema.safeParse(base);
        expect(result.success).toBe(true);
      });

      it.each(VERBOSITY_VALUES)('accepts verbosity = %s', value => {
        const result = schema.safeParse({ ...base, verbosity: value });
        expect(result.success).toBe(true);
      });

      it('rejects unknown verbosity values', () => {
        const result = schema.safeParse({ ...base, verbosity: 'minimal' });
        expect(result.success).toBe(false);
      });

      it('bulk schema propagates verbosity into queries', () => {
        const result = bulk.safeParse({
          queries: [{ ...base, verbosity: 'ultra' }],
        });
        expect(result.success).toBe(true);
      });
    }
  );

  describe('description string is agent training material (four-part anatomy)', () => {
    const description = verbosityField.description ?? '';

    it('cost framing — description mentions "less tokens" / "fewer tokens" / "cheaper"', () => {
      const hasCostFraming = /less tokens|fewer tokens|cheaper/i.test(
        description
      );
      expect(hasCostFraming).toBe(true);
    });

    it('drill-back — description names a recovery path ("Drill-back" / "re-call" / "re-query")', () => {
      const hasDrillBack = /drill-back|re-call|re-query/i.test(description);
      expect(hasDrillBack).toBe(true);
    });

    it('per-value semantics — description teaches each of the three values', () => {
      expect(description).toMatch(/['"]ultra['"]/);
      expect(description).toMatch(/['"]compact['"]/);
      expect(description).toMatch(/['"]verbose['"]/);
    });

    it('default invariant — description says "compact" is the default', () => {
      expect(description.toLowerCase()).toMatch(
        /['"]compact['"][^'"]*default|default[^'"]*['"]compact['"]/i
      );
    });

    it('length budget — description fits the 600-char ceiling for MCP clients', () => {
      expect(description.length).toBeLessThanOrEqual(600);
      expect(description.length).toBeGreaterThan(0);
    });
  });
});

describe('Local Tools — Per-tool rating table (smoke pinning)', () => {
  it('every tool with item pagination exposes a per-page size and *PageNumber', () => {
    const ripgrep = RipgrepQuerySchema.parse(baseRipgrep);
    expect(ripgrep.filesPerPage).toBeDefined();
    expect(ripgrep.filePageNumber).toBeDefined();
    expect(ripgrep.matchesPerPage).toBeDefined();

    const find = FindFilesQuerySchema.parse(baseFindFiles);
    expect(find.filesPerPage).toBeDefined();
    expect(find.filePageNumber).toBeDefined();

    const view = ViewStructureQuerySchema.parse(baseViewStructure);
    expect(view.entriesPerPage).toBeDefined();
    expect(view.entryPageNumber).toBeDefined();
  });
});
