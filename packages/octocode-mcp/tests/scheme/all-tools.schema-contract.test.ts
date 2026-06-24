import { describe, it, expect } from 'vitest';
import { z } from 'zod';
import { existsSync, readdirSync } from 'node:fs';
import { ALL_TOOLS } from '../../src/tools/toolConfig.js';
import {
  OQL_SEARCH_TOOL_NAME,
  STATIC_TOOL_NAMES,
} from '../../../octocode-tools-core/src/tools/toolNames.js';
import { LSP_GET_SEMANTIC_CONTENT_TOOL_NAME } from '../../../octocode-tools-core/src/tools/lsp/shared/semanticTypes.js';
const SHARED_FIELDS = [
  'id',
  'mainResearchGoal',
  'researchGoal',
  'reasoning',
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
    keywords: ['useState'],
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
    keywords: ['react'],
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
  [STATIC_TOOL_NAMES.LOCAL_BINARY_INSPECT]: {
    path: '/tmp/test.bin',
    mode: 'inspect',
  },
  [OQL_SEARCH_TOOL_NAME]: {
    target: 'code',
    from: { kind: 'local', path: '.' },
    where: { kind: 'text', value: 'foo' },
  },
};

function getQueryShape(bulkSchema: z.ZodTypeAny): z.ZodRawShape | null {
  if (!(bulkSchema instanceof z.ZodObject)) return null;
  const queriesField = unwrapOptionalSchema(bulkSchema.shape['queries']);
  if (!(queriesField instanceof z.ZodArray)) return null;
  const element = queriesField.element;
  if (element instanceof z.ZodObject) return element.shape;
  return null;
}

function unwrapOptionalSchema(schema: z.ZodTypeAny): z.ZodTypeAny {
  let current = schema;
  while (current instanceof z.ZodOptional || current instanceof z.ZodDefault) {
    current = current.unwrap();
  }
  return current;
}

describe('all-tools schema contract', () => {
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
        const queriesField = unwrapOptionalSchema(bulkSchema.shape['queries']);
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
        if (!shape) return;
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
        if (toolName === OQL_SEARCH_TOOL_NAME) {
          expect(bulkSchema.safeParse(MINIMAL_QUERY[toolName]).success).toBe(
            true
          );
          return;
        }
        const r = bulkSchema.safeParse({});
        expect(r.success).toBe(false);
      });

      it('rejects queries of wrong type (string)', () => {
        const r = bulkSchema.safeParse({ queries: 'not-an-array' });
        expect(r.success).toBe(false);
      });

      it('rejects duplicate query ids with a structured Zod error', () => {
        if (toolName === OQL_SEARCH_TOOL_NAME) {
          return;
        }
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
          unknownEnvelopeField: 'ignored',
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

    it('keeps each tool schema surface in its scheme.ts file', () => {
      const toolsRoot = new URL(
        '../../../octocode-tools-core/src/tools/',
        import.meta.url
      );
      const files = readdirSync(toolsRoot, { recursive: true }).map(String);
      const schemeFiles = files.filter(file => file.endsWith('scheme.ts'));
      const splitSchemaFiles = files.filter(
        file =>
          /schema\.ts$/i.test(file) &&
          !file.endsWith('scheme.ts') &&
          !file.startsWith('toolMetadata/')
      );

      expect(schemeFiles).toHaveLength(
        ALL_TOOLS.filter(tool => tool.name !== OQL_SEARCH_TOOL_NAME).length
      );
      expect(
        existsSync(
          new URL(
            '../../../octocode-tools-core/src/oql/schema.ts',
            import.meta.url
          )
        )
      ).toBe(true);
      expect(splitSchemaFiles).toEqual([]);
    });
  });
});
