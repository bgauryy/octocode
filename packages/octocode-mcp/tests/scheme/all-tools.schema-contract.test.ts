/**
 * ALL-TOOLS SCHEMA CONTRACT — Zod v4 compliance + MCP descriptor correctness
 *
 * Single loop over all 14 tools. Asserts the five invariants that must hold
 * after the Zod v4 migration (no zod/v4 compat shim, no z.preprocess wrapper
 * at bulk level):
 *
 *   1. STRUCTURE  — bulk schema is a ZodObject (not ZodPipe).
 *                   A ZodPipe at the top level makes normalizeObjectSchema in
 *                   the MCP SDK return EMPTY_OBJECT_JSON_SCHEMA → agents see
 *                   `{ properties: {} }` in tools/list and cannot discover fields.
 *
 *   2. ENVELOPE   — bulk schema has `queries`, `responseCharOffset`,
 *                   `responseCharLength` (the shared bulk envelope contract).
 *
 *   3. SHARED FIELDS — every per-query schema exposes the cross-tool fields:
 *                   id, mainResearchGoal, researchGoal, reasoning, verbose,
 *                   verbosity.
 *
 *   4. PARSE OK   — a minimal valid input for each tool parses without error.
 *
 *   5. PARSE FAIL — invalid inputs (empty queries, duplicate ids, wrong types)
 *                   are rejected with structured errors.
 *
 * Schema access path: tool.direct.inputSchema (bulk) / tool.direct.schema (per-query).
 */

import { describe, it, expect } from 'vitest';
import { z } from 'zod';
import { ALL_TOOLS } from '../../src/tools/toolConfig.js';
import { STATIC_TOOL_NAMES } from '../../src/tools/toolNames.js';

// ---------------------------------------------------------------------------
// Minimal valid per-query inputs — one per tool, keyed by canonical tool name.
// Only required fields are supplied; optional fields are intentionally absent
// to confirm defaults work and the schema doesn't demand extra fields.
// ---------------------------------------------------------------------------
const MINIMAL_QUERY: Record<string, Record<string, unknown>> = {
  [STATIC_TOOL_NAMES.LOCAL_RIPGREP]: { pattern: 'foo', path: '.' },
  [STATIC_TOOL_NAMES.LOCAL_VIEW_STRUCTURE]: { path: '.' },
  [STATIC_TOOL_NAMES.LOCAL_FIND_FILES]: { path: '.' },
  [STATIC_TOOL_NAMES.LOCAL_FETCH_CONTENT]: { path: '/tmp/test.ts' },
  [STATIC_TOOL_NAMES.LSP_GOTO_DEFINITION]: {
    uri: '/tmp/test.ts',
    symbolName: 'myFn',
    lineHint: 10,
  },
  [STATIC_TOOL_NAMES.LSP_FIND_REFERENCES]: {
    uri: '/tmp/test.ts',
    symbolName: 'myFn',
    lineHint: 10,
  },
  [STATIC_TOOL_NAMES.LSP_CALL_HIERARCHY]: {
    uri: '/tmp/test.ts',
    symbolName: 'myFn',
    lineHint: 10,
    direction: 'incoming',
  },
  [STATIC_TOOL_NAMES.GITHUB_SEARCH_CODE]: {
    keywordsToSearch: ['useState'],
    owner: 'facebook',
  },
  [STATIC_TOOL_NAMES.GITHUB_FETCH_CONTENT]: {
    owner: 'facebook',
    repo: 'react',
    path: 'README.md',
  },
  [STATIC_TOOL_NAMES.GITHUB_VIEW_REPO_STRUCTURE]: {
    owner: 'facebook',
    repo: 'react',
  },
  [STATIC_TOOL_NAMES.GITHUB_SEARCH_REPOSITORIES]: {
    keywordsToSearch: ['react'],
  },
  [STATIC_TOOL_NAMES.GITHUB_SEARCH_PULL_REQUESTS]: {
    owner: 'facebook',
    repo: 'react',
  },
  [STATIC_TOOL_NAMES.PACKAGE_SEARCH]: { name: 'zod' },
  [STATIC_TOOL_NAMES.GITHUB_CLONE_REPO]: {
    owner: 'facebook',
    repo: 'react',
  },
};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/**
 * Returns the shape of the per-query element schema inside a bulk schema.
 * Bulk schema: z.object({ queries: z.array(querySchema), ... })
 * We extract querySchema from the array's element.
 */
function getQueryShape(bulkSchema: z.ZodTypeAny): z.ZodRawShape | null {
  if (!(bulkSchema instanceof z.ZodObject)) return null;
  const queriesField = bulkSchema.shape['queries'];
  if (!(queriesField instanceof z.ZodArray)) return null;
  const element = queriesField.element;
  if (element instanceof z.ZodObject) return element.shape;
  return null;
}

// ---------------------------------------------------------------------------
// Contract loop — iterates over ALL 14 tools
// ---------------------------------------------------------------------------

describe('all-tools schema contract', () => {
  describe.each(ALL_TOOLS.map(t => [t.name, t] as const))(
    'tool: %s',
    (toolName, tool) => {
      // Schemas live on tool.direct (ToolDirectExecutionConfig)
      const bulkSchema = tool.direct.inputSchema as z.ZodTypeAny;
      const querySchema = tool.direct.schema as z.ZodTypeAny;

      // -----------------------------------------------------------------------
      // 1. STRUCTURE — bulk schema must be ZodObject, NOT ZodPipe
      // -----------------------------------------------------------------------
      it('bulk inputSchema is a ZodObject (not ZodPipe) — MCP descriptor contract', () => {
        expect(
          bulkSchema instanceof z.ZodObject,
          `${toolName}: bulk inputSchema must be ZodObject.\n` +
            `Got def.type: ${(bulkSchema as any)?._zod?.def?.type ?? (bulkSchema as any)?._def?.typeName ?? typeof bulkSchema}.\n` +
            `A ZodPipe breaks tools/list schema discovery (agents see empty properties: {}).`
        ).toBe(true);
      });

      it('bulk schema def.type is "object"', () => {
        const defType = (bulkSchema as any)._zod?.def?.type;
        expect(
          defType,
          `${toolName}: _zod.def.type must be "object", got "${defType}"`
        ).toBe('object');
      });

      it('bulk schema has .shape (ZodObject property)', () => {
        expect(
          'shape' in bulkSchema,
          `${toolName}: missing .shape on bulk schema`
        ).toBe(true);
      });

      // -----------------------------------------------------------------------
      // 2. ENVELOPE — queries, responseCharOffset, responseCharLength
      // -----------------------------------------------------------------------
      it('bulk envelope declares queries field', () => {
        expect(
          bulkSchema instanceof z.ZodObject && 'queries' in bulkSchema.shape,
          `${toolName}: missing "queries" in bulk schema shape`
        ).toBe(true);
      });

      it('bulk envelope declares responseCharOffset + responseCharLength', () => {
        if (!(bulkSchema instanceof z.ZodObject)) return;
        expect(
          'responseCharOffset' in bulkSchema.shape,
          `${toolName}: missing "responseCharOffset"`
        ).toBe(true);
        expect(
          'responseCharLength' in bulkSchema.shape,
          `${toolName}: missing "responseCharLength"`
        ).toBe(true);
      });

      it('queries is a ZodArray with min=1', () => {
        if (!(bulkSchema instanceof z.ZodObject)) return;
        const queriesField = bulkSchema.shape['queries'];
        expect(
          queriesField instanceof z.ZodArray,
          `${toolName}: queries must be ZodArray`
        ).toBe(true);
        if (!(queriesField instanceof z.ZodArray)) return;
        const checks = (queriesField as any)._zod?.def?.checks ?? [];
        // Zod v4: check kind stored in c._zod.def.check
        const hasMin = checks.some(
          (c: unknown) =>
            (c as any)?._zod?.def?.check === 'min_length' ||
            (c as any)?._zod?.def?.check === 'min' ||
            (c as any)?.kind === 'min_length' ||
            (c as any)?.kind === 'min'
        );
        expect(
          hasMin,
          `${toolName}: queries array must have a min(1) constraint`
        ).toBe(true);
      });

      // -----------------------------------------------------------------------
      // 3. SHARED FIELDS — per-query schema must expose cross-tool fields
      // -----------------------------------------------------------------------
      const SHARED_FIELDS = [
        'id',
        'mainResearchGoal',
        'researchGoal',
        'reasoning',
        'verbose',
        'verbosity',
      ] as const;

      it('per-query schema (tool.direct.schema) exposes all cross-tool shared fields', () => {
        const shape = (querySchema as any)?.shape;
        if (!shape) {
          // querySchema isn't a ZodObject with .shape — skip gracefully
          return;
        }
        for (const field of SHARED_FIELDS) {
          expect(
            field in shape,
            `${toolName}: per-query schema missing shared field "${field}"`
          ).toBe(true);
        }
      });

      it('bulk per-query element also exposes shared fields', () => {
        const shape = getQueryShape(bulkSchema);
        if (!shape) return; // cannot extract — skip
        for (const field of SHARED_FIELDS) {
          expect(
            field in shape,
            `${toolName}: bulk per-query element missing shared field "${field}"`
          ).toBe(true);
        }
      });

      // -----------------------------------------------------------------------
      // 4. PARSE OK — minimal valid inputs succeed
      // -----------------------------------------------------------------------
      it('parses minimal valid input without error', () => {
        const minQuery = MINIMAL_QUERY[toolName];
        expect(
          minQuery,
          `${toolName}: add a MINIMAL_QUERY entry for this tool`
        ).toBeDefined();
        if (!minQuery) return;

        const result = bulkSchema.safeParse({ queries: [minQuery] });
        expect(
          result.success,
          `${toolName}: minimal input failed.\n` +
            `Input: ${JSON.stringify({ queries: [minQuery] })}\n` +
            `Errors: ${!result.success ? JSON.stringify(result.error.issues) : ''}`
        ).toBe(true);
      });

      it('parses with all research metadata + verbose=true', () => {
        const minQuery = MINIMAL_QUERY[toolName];
        if (!minQuery) return;
        const result = bulkSchema.safeParse({
          queries: [
            {
              ...minQuery,
              id: 'q1',
              mainResearchGoal: 'contract test',
              researchGoal: 'schema validation',
              reasoning: 'zod v4 audit',
              verbose: true,
            },
          ],
        });
        expect(
          result.success,
          `${toolName}: failed with research metadata + verbose.\n` +
            `Errors: ${!result.success ? JSON.stringify(result.error.issues) : ''}`
        ).toBe(true);
      });

      it('accepts all three verbosity values (basic / compact / concise)', () => {
        const minQuery = MINIMAL_QUERY[toolName];
        if (!minQuery) return;
        for (const verbosity of ['basic', 'compact', 'concise'] as const) {
          const r = bulkSchema.safeParse({
            queries: [{ ...minQuery, verbosity }],
          });
          expect(
            r.success,
            `${toolName}: rejected verbosity="${verbosity}".\n` +
              `Errors: ${!r.success ? JSON.stringify(r.error.issues) : ''}`
          ).toBe(true);
        }
      });

      it('parses 3 parallel queries (bulk batching)', () => {
        const minQuery = MINIMAL_QUERY[toolName];
        if (!minQuery) return;
        const r = bulkSchema.safeParse({
          queries: [
            { ...minQuery, id: 'q1' },
            { ...minQuery, id: 'q2' },
            { ...minQuery, id: 'q3' },
          ],
        });
        expect(
          r.success,
          `${toolName}: 3-query batch failed.\n` +
            `Errors: ${!r.success ? JSON.stringify(r.error.issues) : ''}`
        ).toBe(true);
      });

      // -----------------------------------------------------------------------
      // 5. PARSE FAIL — invalid inputs are rejected
      // -----------------------------------------------------------------------
      it('rejects empty queries array', () => {
        const r = bulkSchema.safeParse({ queries: [] });
        expect(r.success).toBe(false);
      });

      it('rejects missing queries entirely', () => {
        const r = bulkSchema.safeParse({});
        expect(r.success).toBe(false);
      });

      it('rejects queries of wrong type (string)', () => {
        const r = bulkSchema.safeParse({ queries: 'not-an-array' });
        expect(r.success).toBe(false);
      });

      it('rejects duplicate query ids with a structured Zod error', () => {
        const minQuery = MINIMAL_QUERY[toolName];
        if (!minQuery) return;
        const r = bulkSchema.safeParse({
          queries: [
            { ...minQuery, id: 'dup' },
            { ...minQuery, id: 'dup' },
          ],
        });
        expect(r.success).toBe(false);
        if (!r.success) {
          const hasDup = r.error.issues.some(i =>
            i.message.includes('Duplicate query id')
          );
          expect(
            hasDup,
            `${toolName}: expected "Duplicate query id" error.\n` +
              `Got: ${JSON.stringify(r.error.issues)}`
          ).toBe(true);
        }
      });

      it('clamps out-of-range responseCharOffset / responseCharLength (does not reject)', () => {
        const minQuery = MINIMAL_QUERY[toolName];
        if (!minQuery) return;
        const r = bulkSchema.safeParse({
          queries: [minQuery],
          responseCharOffset: 999_999_999_999,
          responseCharLength: 999_999_999_999,
        });
        expect(
          r.success,
          `${toolName}: out-of-range envelope fields should clamp, not reject.\n` +
            `Errors: ${!r.success ? JSON.stringify(r.error.issues) : ''}`
        ).toBe(true);
        if (r.success) {
          const MAX_OFFSET = 10_000_000;
          const MAX_LENGTH = 100_000;
          expect(r.data.responseCharOffset).toBeLessThanOrEqual(MAX_OFFSET);
          expect(r.data.responseCharLength).toBeLessThanOrEqual(MAX_LENGTH);
        }
      });
    }
  );

  // -------------------------------------------------------------------------
  // Global invariants — asserted once across the full catalog
  // -------------------------------------------------------------------------
  describe('global invariants', () => {
    it('ALL_TOOLS contains exactly 14 tools', () => {
      expect(ALL_TOOLS).toHaveLength(14);
    });

    it('every tool has a MINIMAL_QUERY entry in this test', () => {
      const missing = ALL_TOOLS.filter(t => !MINIMAL_QUERY[t.name]).map(
        t => t.name
      );
      expect(
        missing,
        `Missing MINIMAL_QUERY entries: ${missing.join(', ')}`
      ).toHaveLength(0);
    });

    it('no bulk schema has ZodPipe at top level (_zod.def.type !== "pipe")', () => {
      const pipes = ALL_TOOLS.filter(t => {
        const s = t.direct.inputSchema as any;
        return s?._zod?.def?.type === 'pipe';
      }).map(t => t.name);

      expect(
        pipes,
        `Tools with ZodPipe bulk schema (breaks tools/list): ${pipes.join(', ')}\n` +
          `Fix: z.preprocess() or .transform() wrapping the bulk schema must be removed.`
      ).toHaveLength(0);
    });

    it('every bulk schema is a ZodObject instance', () => {
      const nonObjects = ALL_TOOLS.filter(
        t => !(t.direct.inputSchema instanceof z.ZodObject)
      ).map(t => t.name);

      expect(
        nonObjects,
        `Non-ZodObject bulk schemas: ${nonObjects.join(', ')}`
      ).toHaveLength(0);
    });

    it('every bulk schema has a .shape with queries, responseCharOffset, responseCharLength', () => {
      const missing: string[] = [];
      for (const tool of ALL_TOOLS) {
        const s = tool.direct.inputSchema as any;
        if (!(s instanceof z.ZodObject)) continue;
        for (const field of [
          'queries',
          'responseCharOffset',
          'responseCharLength',
        ]) {
          if (!(field in s.shape)) {
            missing.push(`${tool.name}.${field}`);
          }
        }
      }
      expect(
        missing,
        `Missing envelope fields: ${missing.join(', ')}`
      ).toHaveLength(0);
    });
  });
});
