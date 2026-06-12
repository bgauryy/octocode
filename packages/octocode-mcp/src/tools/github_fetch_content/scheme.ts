import { z } from 'zod';
import { completeMetadata } from '@octocodeai/octocode-core';
import { MAX_CHAR_LENGTH } from '../../config.js';
import {
  clampedInt,
  contextLinesField,
  createRelaxedBulkQuerySchema,
  lineNumberField,
  minifyFieldWithSymbols,
} from '../../scheme/localSchemaOverlay.js';
import { validateFileContentExtractionMode } from '../../scheme/fileContentModeValidation.js';
import {
  EvidenceSchema,
  responseEnvelopeFields,
} from '../../scheme/responseEnvelope.js';
import { STATIC_TOOL_NAMES } from '../toolNames.js';

const QUERY_DESCRIPTIONS = {
  ...completeMetadata.baseSchema,
  ...completeMetadata.tools[STATIC_TOOL_NAMES.GITHUB_FETCH_CONTENT]?.schema,
} as Record<string, string>;

const PaginationInfoSchema = z.object({
  currentPage: z.number(),
  totalPages: z.number(),
  hasMore: z.boolean(),
  charOffset: z.number().optional(),
  charLength: z.number().optional(),
  totalChars: z.number().optional(),
  nextCharOffset: z.number().optional(),
  filesPerPage: z.number().optional(),
  totalFiles: z.number().optional(),
  entriesPerPage: z.number().optional(),
  totalEntries: z.number().optional(),
  matchesPerPage: z.number().optional(),
  totalMatches: z.number().optional(),
});

const GitHubFetchFileEntrySchema = z.object({
  path: z.string(),
  content: z.string(),
  contentView: z.enum(['none', 'standard', 'symbols']).optional(),
  isSkeleton: z.boolean().optional(),
  totalLines: z.number().optional(),
  sourceChars: z.number().optional(),
  sourceBytes: z.number().optional(),
  resolvedBranch: z.string().optional(),
  pagination: PaginationInfoSchema.optional(),
  isPartial: z.boolean().optional(),
  startLine: z.number().optional(),
  endLine: z.number().optional(),
  matchRanges: z
    .array(z.object({ start: z.number(), end: z.number() }))
    .optional(),
  lastModified: z.string().optional(),
  lastModifiedBy: z.string().optional(),
  warnings: z.array(z.string()).optional(),
  matchNotFound: z.boolean().optional(),
  searchedFor: z.string().optional(),
});

const GitHubFetchDirectoryEntrySchema = z.object({
  path: z.string(),
  localPath: z.string(),
  fileCount: z.number(),
  totalSize: z.number(),
  files: z
    .array(z.object({ path: z.string(), size: z.number(), type: z.string() }))
    .optional(),
  cached: z.boolean().optional(),
  resolvedBranch: z.string().optional(),
});

const FileContentQueryShape = z.object({
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
  owner: z.string().describe(QUERY_DESCRIPTIONS.owner!),
  repo: z.string().describe(QUERY_DESCRIPTIONS.repo!),
  branch: z.string().optional().describe(QUERY_DESCRIPTIONS.branch!),
  path: z.string().describe(QUERY_DESCRIPTIONS.path!),
  startLine: lineNumberField.describe(QUERY_DESCRIPTIONS.startLine!),
  endLine: lineNumberField.describe(QUERY_DESCRIPTIONS.endLine!),
  fullContent: z.boolean().optional().describe(QUERY_DESCRIPTIONS.fullContent!),
  matchString: z.string().optional().describe(QUERY_DESCRIPTIONS.matchString!),
  matchStringIsRegex: z
    .boolean()
    .optional()
    .describe(QUERY_DESCRIPTIONS.matchStringIsRegex!),
  matchStringCaseSensitive: z
    .boolean()
    .optional()
    .describe(QUERY_DESCRIPTIONS.matchStringCaseSensitive!),
  forceRefresh: z
    .boolean()
    .optional()
    .describe(QUERY_DESCRIPTIONS.forceRefresh!),
  type: z
    .enum(['file', 'directory'])
    .optional()
    .describe(QUERY_DESCRIPTIONS.type!),
  contextLines: contextLinesField.describe(QUERY_DESCRIPTIONS.contextLines!),
  charOffset: clampedInt(0, 100_000_000)
    .optional()
    .describe(QUERY_DESCRIPTIONS.charOffset!),
  charLength: clampedInt(1, MAX_CHAR_LENGTH)
    .optional()
    .describe(QUERY_DESCRIPTIONS.charLength!),
  minify: minifyFieldWithSymbols.describe(QUERY_DESCRIPTIONS.minify!),
});

export const FileContentQueryBaseLocalSchema = FileContentQueryShape;

export const FileContentQueryLocalSchema = FileContentQueryShape.superRefine(
  validateFileContentExtractionMode
);

export const FileContentBulkQueryLocalSchema = createRelaxedBulkQuerySchema(
  STATIC_TOOL_NAMES.GITHUB_FETCH_CONTENT,
  FileContentQueryShape
);

export const GitHubFetchContentOutputLocalSchema = z.object({
  base: z.string().optional(),
  shared: z
    .record(z.string(), z.union([z.string(), z.number(), z.boolean()]))
    .optional(),
  evidence: EvidenceSchema,
  responsePagination: responseEnvelopeFields.responsePagination,
  results: z.array(
    z.object({
      id: z.string(),
      owner: z.string(),
      repo: z.string(),
      files: z.array(GitHubFetchFileEntrySchema).optional(),
      directories: z.array(GitHubFetchDirectoryEntrySchema).optional(),
    })
  ),
  hints: z.array(z.string()).optional(),
  errors: z
    .array(
      z.object({
        id: z.string(),
        owner: z.string().optional(),
        repo: z.string().optional(),
        path: z.string().optional(),
        error: z.string(),
        hints: z.array(z.string()).optional(),
      })
    )
    .optional(),
});

export type GitHubFetchContentOutputLocal = z.infer<
  typeof GitHubFetchContentOutputLocalSchema
>;
