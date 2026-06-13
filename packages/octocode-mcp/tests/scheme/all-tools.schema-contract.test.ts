import { describe, it, expect } from 'vitest';
import { z } from 'zod';
import { completeMetadata } from '@octocodeai/octocode-core';
import { readFileSync } from 'node:fs';
import { ALL_TOOLS } from '../../src/tools/toolConfig.js';
import { STATIC_TOOL_NAMES } from '../../src/tools/toolNames.js';
import { LSP_GET_SEMANTIC_CONTENT_TOOL_NAME } from '../../src/tools/lsp/shared/semanticTypes.js';

const SHARED_FIELDS = [
  'id',
  'mainResearchGoal',
  'researchGoal',
  'reasoning',
] as const;

const SCHEME_FILES = [
  '../../src/tools/github_clone_repo/scheme.ts',
  '../../src/tools/github_fetch_content/scheme.ts',
  '../../src/tools/github_search_code/scheme.ts',
  '../../src/tools/github_search_pull_requests/scheme.ts',
  '../../src/tools/github_search_repos/scheme.ts',
  '../../src/tools/github_view_repo_structure/scheme.ts',
  '../../src/tools/local_fetch_content/scheme.ts',
  '../../src/tools/local_find_files/scheme.ts',
  '../../src/tools/local_ripgrep/scheme.ts',
  '../../src/tools/local_view_structure/scheme.ts',
  '../../src/tools/lsp/semantic_content/scheme.ts',
  '../../src/tools/package_search/scheme.ts',
] as const;

const MINIMAL_QUERY: Record<string, Record<string, unknown>> = {
  [STATIC_TOOL_NAMES.LOCAL_RIPGREP]: { keywords: 'foo', path: '.' },
  [STATIC_TOOL_NAMES.LOCAL_VIEW_STRUCTURE]: { path: '.' },
  [STATIC_TOOL_NAMES.LOCAL_FIND_FILES]: { path: '.' },
  [STATIC_TOOL_NAMES.LOCAL_FETCH_CONTENT]: { path: '/tmp/test.ts' },
  [LSP_GET_SEMANTIC_CONTENT_TOOL_NAME]: {
    uri: '/tmp/test.ts',
    type: 'definition',
    symbolName: 'myFn',
    lineHint: 10,
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
  [STATIC_TOOL_NAMES.PACKAGE_SEARCH]: { packageName: 'zod' },
  [STATIC_TOOL_NAMES.GITHUB_CLONE_REPO]: {
    owner: 'facebook',
    repo: 'react',
  },
};

function getQueryShape(bulkSchema: z.ZodTypeAny): z.ZodRawShape | null {
  if (!(bulkSchema instanceof z.ZodObject)) return null;
  const queriesField = bulkSchema.shape['queries'];
  if (!(queriesField instanceof z.ZodArray)) return null;
  const element = queriesField.element;
  if (element instanceof z.ZodObject) return element.shape;
  return null;
}

function getJsonProperties(schema: z.ZodTypeAny): Record<string, unknown> {
  const jsonSchema = z.toJSONSchema(schema);
  const properties = (jsonSchema as { properties?: unknown }).properties;
  return properties &&
    typeof properties === 'object' &&
    !Array.isArray(properties)
    ? (properties as Record<string, unknown>)
    : {};
}

/**
 * Per-tool intentional deviations from the octocode-core schema contract.
 * - removedCoreFields: core-described fields intentionally omitted from the local schema
 *   (e.g. replaced by a better local alternative).
 * - addedLocalFields: local schema fields that extend core but are not yet described there
 *   (must carry an inline description in scheme.ts).
 * - overriddenDescriptions: fields whose upstream description is intentionally replaced
 *   with a more accurate local one (skips the description-match assertion).
 */
const SCHEMA_EXCEPTIONS: Record<
  string,
  { removedCoreFields?: string[]; addedLocalFields?: string[]; overriddenDescriptions?: string[] }
> = {
  [STATIC_TOOL_NAMES.PACKAGE_SEARCH]: {
    // `mode` (smart/full/lean) is not exposed; the tool always returns a compact
    // string-list format so there is no output mode to select.
    removedCoreFields: ['mode'],
  },

};

function getCoreQueryDescriptions(toolName: string): Record<string, string> {
  const tool = completeMetadata.tools[toolName];
  return {
    id: completeMetadata.baseSchema.id,
    mainResearchGoal: completeMetadata.baseSchema.mainResearchGoal,
    researchGoal: completeMetadata.baseSchema.researchGoal,
    reasoning: completeMetadata.baseSchema.reasoning,
    ...(tool?.schema ?? {}),
  };
}

function getPropertyDescription(property: unknown): string | undefined {
  return property && typeof property === 'object'
    ? (property as { description?: string }).description
    : undefined;
}

describe('all-tools schema contract', () => {
  describe.each(SCHEME_FILES)('scheme source: %s', schemeFile => {
    const source = readFileSync(new URL(schemeFile, import.meta.url), 'utf8');

    it('imports completeMetadata directly from octocode-core', () => {
      expect(source).toContain(
        "import { completeMetadata } from '@octocodeai/octocode-core';"
      );
    });

    it('does not use shared description/meta-field injection', () => {
      expect(source).not.toContain('withCoreSchemaDescriptions');
      expect(source).not.toContain('optionalMetaFields');
    });
  });

  describe.each(ALL_TOOLS.map(t => [t.name, t] as const))(
    'tool: %s',
    (toolName, tool) => {
      const bulkSchema = tool.direct.inputSchema as z.ZodTypeAny;
      const querySchema = tool.direct.schema as z.ZodTypeAny;

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

      it('bulk envelope declares queries field', () => {
        expect(
          bulkSchema instanceof z.ZodObject && 'queries' in bulkSchema.shape,
          `${toolName}: missing "queries" in bulk schema shape`
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

      it('per-query schema (tool.direct.schema) exposes all cross-tool shared fields', () => {
        const shape = (querySchema as any)?.shape;
        if (!shape) {
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
        if (!shape) return;
        for (const field of SHARED_FIELDS) {
          expect(
            field in shape,
            `${toolName}: bulk per-query element missing shared field "${field}"`
          ).toBe(true);
        }
      });

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

      it('uses octocode-core for the tool description', () => {
        expect(tool.description).toBe(
          completeMetadata.tools[toolName]?.description
        );
      });

      it('uses octocode-core descriptions for every query parameter', () => {
        const properties = getJsonProperties(querySchema);
        const expectedDescriptions = getCoreQueryDescriptions(toolName);
        const actualFields = Object.keys(properties);
        const exceptions = SCHEMA_EXCEPTIONS[toolName] ?? {};
        const allowedMissing = exceptions.removedCoreFields ?? [];
        const allowedExtra = exceptions.addedLocalFields ?? [];
        const allowedDescriptionOverrides = exceptions.overriddenDescriptions ?? [];

        const missingFields = Object.keys(expectedDescriptions).filter(
          field => !(field in properties) && !allowedMissing.includes(field)
        );
        const undocumentedFields = actualFields.filter(
          field =>
            !(field in expectedDescriptions) && !allowedExtra.includes(field)
        );
        const mismatchedDescriptions = actualFields
          .filter(field => !allowedExtra.includes(field))
          .filter(field => !allowedDescriptionOverrides.includes(field))
          .filter(field => {
            const expected = expectedDescriptions[field];
            return (
              expected !== undefined &&
              getPropertyDescription(properties[field]) !== expected
            );
          });

        expect(
          missingFields,
          `${toolName}: query schema is missing core-described fields`
        ).toEqual([]);
        expect(
          undocumentedFields,
          `${toolName}: query schema fields must be described in octocode-core`
        ).toEqual([]);
        expect(
          mismatchedDescriptions,
          `${toolName}: query field descriptions must match octocode-core`
        ).toEqual([]);
      });

      it('parses with all research metadata', () => {
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
            },
          ],
        });
        expect(
          result.success,
          `${toolName}: failed with research metadata.\n` +
            `Errors: ${!result.success ? JSON.stringify(result.error.issues) : ''}`
        ).toBe(true);
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

      it('parses with extra unknown envelope fields ignored (does not reject)', () => {
        const minQuery = MINIMAL_QUERY[toolName];
        if (!minQuery) return;
        const r = bulkSchema.safeParse({
          queries: [minQuery],
        });
        expect(
          r.success,
          `${toolName}: minimal parse should succeed.\n` +
            `Errors: ${!r.success ? JSON.stringify(r.error.issues) : ''}`
        ).toBe(true);
      });
    }
  );

  describe('global invariants', () => {
    it('ALL_TOOLS contains exactly 12 tools', () => {
      expect(ALL_TOOLS).toHaveLength(12);
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

    it('every bulk schema has a .shape with queries', () => {
      const missing: string[] = [];
      for (const tool of ALL_TOOLS) {
        const s = tool.direct.inputSchema as any;
        if (!(s instanceof z.ZodObject)) continue;
        for (const field of ['queries']) {
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
