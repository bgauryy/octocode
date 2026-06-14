import { z } from 'zod';
import { completeMetadata } from '@octocodeai/octocode-core';
import { MAX_MATCH_CONTENT_LENGTH, MAX_PAGE_NUMBER } from '../../config.js';
import {
  clampedInt,
  contextLinesField,
  createRelaxedBulkQuerySchema,
  relaxedPageNumberField,
} from '../../scheme/fields.js';
import { STATIC_TOOL_NAMES } from '../toolNames.js';

const QUERY_DESCRIPTIONS = {
  ...completeMetadata.baseSchema,
  ...completeMetadata.tools[STATIC_TOOL_NAMES.LOCAL_RIPGREP]?.schema,
} as Record<string, string>;

const LOCAL_SEARCH_MODES = ['paginated', 'discovery', 'detailed'] as const;

const RipgrepQueryShape = z.object({
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
  keywords: z.string().describe(QUERY_DESCRIPTIONS.keywords!),
  path: z.string().describe(QUERY_DESCRIPTIONS.path!),
  mode: z
    .enum(LOCAL_SEARCH_MODES)
    .optional()
    .describe(QUERY_DESCRIPTIONS.mode!),
  fixedString: z.boolean().optional().describe(QUERY_DESCRIPTIONS.fixedString!),
  perlRegex: z.boolean().optional().describe(QUERY_DESCRIPTIONS.perlRegex!),
  caseInsensitive: z
    .boolean()
    .optional()
    .describe(QUERY_DESCRIPTIONS.caseInsensitive!),
  caseSensitive: z
    .boolean()
    .optional()
    .describe(QUERY_DESCRIPTIONS.caseSensitive!),
  wholeWord: z.boolean().optional().describe(QUERY_DESCRIPTIONS.wholeWord!),
  invertMatch: z.boolean().optional().describe(QUERY_DESCRIPTIONS.invertMatch!),
  include: z.array(z.string()).optional().describe(QUERY_DESCRIPTIONS.include!),
  exclude: z.array(z.string()).optional().describe(QUERY_DESCRIPTIONS.exclude!),
  excludeDir: z
    .array(z.string())
    .optional()
    .describe(QUERY_DESCRIPTIONS.excludeDir!),
  noIgnore: z.boolean().optional().describe(QUERY_DESCRIPTIONS.noIgnore!),
  hidden: z.boolean().optional().describe(QUERY_DESCRIPTIONS.hidden!),
  filesOnly: z.boolean().optional().describe(QUERY_DESCRIPTIONS.filesOnly!),
  filesWithoutMatch: z
    .boolean()
    .optional()
    .describe(QUERY_DESCRIPTIONS.filesWithoutMatch!),
  contextLines: contextLinesField.describe(QUERY_DESCRIPTIONS.contextLines!),
  matchContentLength: clampedInt(1, MAX_MATCH_CONTENT_LENGTH)
    .optional()
    .default(500)
    .describe(QUERY_DESCRIPTIONS.matchContentLength!),
  maxMatchesPerFile: clampedInt(1, MAX_MATCH_CONTENT_LENGTH)
    .optional()
    .describe(QUERY_DESCRIPTIONS.maxMatchesPerFile!),
  maxFiles: clampedInt(1, MAX_MATCH_CONTENT_LENGTH)
    .optional()
    .describe(QUERY_DESCRIPTIONS.maxFiles!),
  multiline: z.boolean().optional().describe(QUERY_DESCRIPTIONS.multiline!),
  multilineDotall: z
    .boolean()
    .optional()
    .describe(QUERY_DESCRIPTIONS.multilineDotall!),
  sort: z
    .enum(['path', 'modified', 'accessed', 'created'])
    .optional()
    .default('path')
    .describe(QUERY_DESCRIPTIONS.sort!),
  sortReverse: z.boolean().optional().describe(QUERY_DESCRIPTIONS.sortReverse!),
  langType: z.string().optional().describe(QUERY_DESCRIPTIONS.langType!),
  countLinesPerFile: z
    .boolean()
    .optional()
    .describe(QUERY_DESCRIPTIONS.countLinesPerFile!),
  countMatchesPerFile: z
    .boolean()
    .optional()
    .describe(QUERY_DESCRIPTIONS.countMatchesPerFile!),
  matchPage: relaxedPageNumberField
    .optional()
    .describe(QUERY_DESCRIPTIONS.matchPage!),
  itemsPerPage: clampedInt(1, MAX_PAGE_NUMBER)
    .optional()
    .describe(QUERY_DESCRIPTIONS.itemsPerPage!),
  page: relaxedPageNumberField.default(1).describe(QUERY_DESCRIPTIONS.page!),
});

export const LocalRipgrepQuerySchema = RipgrepQueryShape.superRefine(
  (data, ctx) => {
    const d = data as {
      filesOnly?: boolean;
      filesWithoutMatch?: boolean;
      fixedString?: boolean;
      perlRegex?: boolean;
      countLinesPerFile?: boolean;
      countMatchesPerFile?: boolean;
    };
    if (d.filesOnly && d.filesWithoutMatch)
      ctx.addIssue({
        code: 'custom',
        message: 'filesOnly and filesWithoutMatch are mutually exclusive.',
        path: ['filesWithoutMatch'],
      });
    if (d.fixedString && d.perlRegex)
      ctx.addIssue({
        code: 'custom',
        message: 'fixedString and perlRegex are mutually exclusive.',
        path: ['perlRegex'],
      });
    if (d.countLinesPerFile && d.countMatchesPerFile)
      ctx.addIssue({
        code: 'custom',
        message:
          'countLinesPerFile and countMatchesPerFile are mutually exclusive.',
        path: ['countMatchesPerFile'],
      });
  }
);

export type RipgrepQuery = z.infer<typeof RipgrepQueryShape>;

// Bulk uses the base shape (no mutex superRefine) so one invalid query
// does not reject the whole batch. Per-query mutex checks run at execution.
export const LocalRipgrepBulkQuerySchema = createRelaxedBulkQuerySchema(
  RipgrepQueryShape,
  { maxQueries: 5 }
);
