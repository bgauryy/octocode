import { z } from 'zod';
import { completeMetadata } from '@octocodeai/octocode-core';
import { LOCAL_MAX_FILES_PER_PAGE } from '../../config.js';
import {
  clampedInt,
  createRelaxedBulkQuerySchema,
  LOCAL_OVERLAY_MAX_LIMIT,
  relaxedPageNumberField,
} from '../../scheme/localSchemaOverlay.js';
import { STATIC_TOOL_NAMES } from '../toolNames.js';

const QUERY_DESCRIPTIONS = {
  ...completeMetadata.baseSchema,
  ...completeMetadata.tools[STATIC_TOOL_NAMES.LOCAL_FIND_FILES]?.schema,
} as Record<string, string>;

const LOCAL_FIND_SORT_FIELDS = ['modified', 'name', 'path', 'size'] as const;
const LOCAL_FIND_ENTRY_TYPES = ['f', 'd'] as const;

const FindFilesQueryShape = z.object({
  id: z.string().optional().describe(QUERY_DESCRIPTIONS.id!),
  mainResearchGoal: z
    .string()
    .optional()
    .describe(QUERY_DESCRIPTIONS.mainResearchGoal!),
  researchGoal: z
    .string()
    .optional()
    .describe(QUERY_DESCRIPTIONS.researchGoal!),
  reasoning: z.string().optional().describe(QUERY_DESCRIPTIONS.reasoning!),
  path: z.string().describe(QUERY_DESCRIPTIONS.path!),
  maxDepth: clampedInt(0, 100)
    .optional()
    .describe(QUERY_DESCRIPTIONS.maxDepth!),
  minDepth: clampedInt(0, 100)
    .optional()
    .describe(QUERY_DESCRIPTIONS.minDepth!),
  name: z.string().optional().describe(QUERY_DESCRIPTIONS.name!),
  names: z.array(z.string()).optional().describe(QUERY_DESCRIPTIONS.names!),
  pathPattern: z.string().optional().describe(QUERY_DESCRIPTIONS.pathPattern!),
  regex: z.string().optional().describe(QUERY_DESCRIPTIONS.regex!),
  regexType: z.string().optional().describe(QUERY_DESCRIPTIONS.regexType!),
  empty: z.boolean().optional().describe(QUERY_DESCRIPTIONS.empty!),
  modifiedWithin: z
    .string()
    .optional()
    .describe(QUERY_DESCRIPTIONS.modifiedWithin!),
  modifiedBefore: z
    .string()
    .optional()
    .describe(QUERY_DESCRIPTIONS.modifiedBefore!),
  accessedWithin: z
    .string()
    .optional()
    .describe(QUERY_DESCRIPTIONS.accessedWithin!),
  sizeGreater: z.string().optional().describe(QUERY_DESCRIPTIONS.sizeGreater!),
  sizeLess: z.string().optional().describe(QUERY_DESCRIPTIONS.sizeLess!),
  permissions: z.string().optional().describe(QUERY_DESCRIPTIONS.permissions!),
  executable: z.boolean().optional().describe(QUERY_DESCRIPTIONS.executable!),
  readable: z.boolean().optional().describe(QUERY_DESCRIPTIONS.readable!),
  writable: z.boolean().optional().describe(QUERY_DESCRIPTIONS.writable!),
  excludeDir: z
    .array(z.string())
    .optional()
    .describe(QUERY_DESCRIPTIONS.excludeDir!),
  limit: clampedInt(1, LOCAL_OVERLAY_MAX_LIMIT)
    .optional()
    .describe(QUERY_DESCRIPTIONS.limit!),
  details: z.boolean().optional().describe(QUERY_DESCRIPTIONS.details!),
  showFileLastModified: z
    .boolean()
    .optional()
    .describe(QUERY_DESCRIPTIONS.showFileLastModified!),
  sortBy: z
    .enum(LOCAL_FIND_SORT_FIELDS)
    .optional()
    .describe(QUERY_DESCRIPTIONS.sortBy!),
  entryType: z
    .enum(LOCAL_FIND_ENTRY_TYPES)
    .optional()
    .describe(QUERY_DESCRIPTIONS.entryType!),
  page: relaxedPageNumberField.default(1).describe(QUERY_DESCRIPTIONS.page!),
  itemsPerPage: clampedInt(1, LOCAL_MAX_FILES_PER_PAGE)
    .optional()
    .describe(QUERY_DESCRIPTIONS.itemsPerPage!),
});

export const LocalFindFilesQuerySchema = FindFilesQueryShape;
export type FindFilesQuery = z.infer<typeof FindFilesQueryShape>;

export const LocalFindFilesBulkQuerySchema = createRelaxedBulkQuerySchema(
  STATIC_TOOL_NAMES.LOCAL_FIND_FILES,
  FindFilesQueryShape,
  { maxQueries: 5 }
);
