import { z } from 'zod';
import { completeMetadata } from '@octocodeai/octocode-core';
import {
  LOCAL_MAX_DEPTH,
  LOCAL_MAX_FILES_PER_PAGE,
  LOCAL_MAX_LIMIT,
} from '../../config.js';
import {
  clampedInt,
  createRelaxedBulkQuerySchema,
  relaxedPageNumberField,
} from '../../scheme/fields.js';
import { STATIC_TOOL_NAMES } from '../toolNames.js';

const QUERY_DESCRIPTIONS = {
  ...completeMetadata.baseSchema,
  ...completeMetadata.tools[STATIC_TOOL_NAMES.LOCAL_VIEW_STRUCTURE]?.schema,
} as Record<string, string>;

const LOCAL_VIEW_SORT_FIELDS = ['name', 'size', 'time', 'extension'] as const;

const ViewStructureQueryShape = z.object({
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
  details: z.boolean().optional().describe(QUERY_DESCRIPTIONS.details!),
  hidden: z.boolean().optional().describe(QUERY_DESCRIPTIONS.hidden!),
  // humanReadable removed: in recursive/walk mode formatFileSize always runs
  // (flag is silently ignored); in ls+details mode it double-formats the
  // already-formatted ls -h string, reducing precision (24.3KB → 24.0KB).
  // The default behaviour (no -h flag → raw bytes → formatFileSize) is best.
  sortBy: z
    .enum(LOCAL_VIEW_SORT_FIELDS)
    .optional()
    .describe(QUERY_DESCRIPTIONS.sortBy!),
  reverse: z.boolean().optional().describe(QUERY_DESCRIPTIONS.reverse!),
  pattern: z.string().optional().describe(QUERY_DESCRIPTIONS.pattern!),
  directoriesOnly: z
    .boolean()
    .optional()
    .describe(QUERY_DESCRIPTIONS.directoriesOnly!),
  filesOnly: z.boolean().optional().describe(QUERY_DESCRIPTIONS.filesOnly!),
  recursive: z.boolean().optional().describe(QUERY_DESCRIPTIONS.recursive!),
  extensions: z
    .array(z.string())
    .optional()
    .describe(QUERY_DESCRIPTIONS.extensions!),
  depth: clampedInt(0, LOCAL_MAX_DEPTH)
    .optional()
    .describe(QUERY_DESCRIPTIONS.depth!),
  limit: clampedInt(1, LOCAL_MAX_LIMIT)
    .optional()
    .describe(QUERY_DESCRIPTIONS.limit!),
  showFileLastModified: z
    .boolean()
    .optional()
    .describe(QUERY_DESCRIPTIONS.showFileLastModified!),
  page: relaxedPageNumberField.default(1).describe(QUERY_DESCRIPTIONS.page!),
  itemsPerPage: clampedInt(1, LOCAL_MAX_FILES_PER_PAGE)
    .optional()
    .describe(QUERY_DESCRIPTIONS.itemsPerPage!),
});

export const LocalViewStructureQuerySchema = ViewStructureQueryShape;
export type ViewStructureQuery = z.infer<typeof ViewStructureQueryShape>;

export const LocalViewStructureBulkQuerySchema = createRelaxedBulkQuerySchema(
  ViewStructureQueryShape,
  { maxQueries: 5 }
);
