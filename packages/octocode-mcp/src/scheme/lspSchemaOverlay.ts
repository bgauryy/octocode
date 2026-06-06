import { z } from 'zod';
import {
  LSPGotoDefinitionQuerySchema as UpstreamGotoDefinitionQuerySchema,
  LSPFindReferencesQuerySchema as UpstreamFindReferencesQuerySchema,
  LSPCallHierarchyQuerySchema as UpstreamCallHierarchyQuerySchema,
} from '@octocodeai/octocode-core/schemas';
import { STATIC_TOOL_NAMES } from '../tools/toolNames.js';
import {
  createRelaxedBulkQuerySchema,
  createVerbosityFields,
  contextLinesField,
  optionalMetaFields,
  relaxedPageNumberField,
  depthField,
  requiredLineHintField,
  orderHintField,
  DEFAULT_PAGE_SIZE,
  withCoreSchemaDescriptions,
} from './localSchemaOverlay.js';

function withFilePathAlias<
  T extends z.ZodObject<
    z.ZodRawShape & { uri: z.ZodTypeAny; filePath: z.ZodTypeAny }
  >,
>(schema: T) {
  const withValidation = schema.superRefine((q, ctx) => {
    if (!q.uri && !q.filePath) {
      ctx.addIssue({
        code: 'custom',
        path: ['uri'],
        message: 'Either uri or filePath is required',
      });
    }
  });
  return z.preprocess((q: unknown) => {
    if (q && typeof q === 'object' && 'filePath' in q && !('uri' in q)) {
      const { filePath, ...rest } = q as {
        filePath: string;
        [k: string]: unknown;
      };
      return { ...rest, uri: filePath };
    }
    return q;
  }, withValidation);
}

export const LSPGotoDefinitionQuerySchema = withFilePathAlias(
  withCoreSchemaDescriptions(
    STATIC_TOOL_NAMES.LSP_GOTO_DEFINITION,
    UpstreamGotoDefinitionQuerySchema.extend({
      ...optionalMetaFields,
      uri: z.string().optional(),
      filePath: z
        .string()
        .optional()
        .describe('Alias for uri — pass either, not both'),
      lineHint: requiredLineHintField,
      orderHint: orderHintField,
      ...createVerbosityFields(),
      contextLines: contextLinesField,
    })
  )
);

export const BulkLSPGotoDefinitionQuerySchema = createRelaxedBulkQuerySchema(
  STATIC_TOOL_NAMES.LSP_GOTO_DEFINITION,
  LSPGotoDefinitionQuerySchema,
  { maxQueries: 5 }
);

export const LSPFindReferencesQuerySchema = withFilePathAlias(
  withCoreSchemaDescriptions(
    STATIC_TOOL_NAMES.LSP_FIND_REFERENCES,
    UpstreamFindReferencesQuerySchema.omit({
      referencesPerPage: true,
    }).extend({
      ...optionalMetaFields,
      uri: z.string().optional(),
      filePath: z
        .string()
        .optional()
        .describe('Alias for uri — pass either, not both'),
      lineHint: requiredLineHintField,
      orderHint: orderHintField,
      ...createVerbosityFields(),
      contextLines: contextLinesField,
      page: relaxedPageNumberField
        .default(1)
        .describe(
          `Result page (1-based). Each page returns up to ${DEFAULT_PAGE_SIZE} references.`
        ),
      groupByFile: z.boolean().optional(),
    })
  )
);

export const BulkLSPFindReferencesQuerySchema = createRelaxedBulkQuerySchema(
  STATIC_TOOL_NAMES.LSP_FIND_REFERENCES,
  LSPFindReferencesQuerySchema,
  { maxQueries: 5 }
);

export const LSPCallHierarchyQuerySchema = withFilePathAlias(
  withCoreSchemaDescriptions(
    STATIC_TOOL_NAMES.LSP_CALL_HIERARCHY,
    UpstreamCallHierarchyQuerySchema.omit({
      callsPerPage: true,
    }).extend({
      ...optionalMetaFields,
      uri: z.string().optional(),
      filePath: z
        .string()
        .optional()
        .describe('Alias for uri — pass either, not both'),
      lineHint: requiredLineHintField,
      orderHint: orderHintField,
      ...createVerbosityFields(),
      contextLines: contextLinesField,
      page: relaxedPageNumberField
        .default(1)
        .describe(
          `Result page (1-based). Each page returns up to ${DEFAULT_PAGE_SIZE} calls.`
        ),
      depth: depthField,
      direction: z
        .enum(['incoming', 'outgoing'])
        .default('incoming')
        .describe('incoming callers or outgoing callees. Default: "incoming".'),
    })
  )
);

export const BulkLSPCallHierarchyQuerySchema = createRelaxedBulkQuerySchema(
  STATIC_TOOL_NAMES.LSP_CALL_HIERARCHY,
  LSPCallHierarchyQuerySchema,
  { maxQueries: 5 }
);
