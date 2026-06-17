import { z } from 'zod';
import { FileContentQuerySchema as CoreFileContentQuerySchema } from '@octocodeai/octocode-core/schemas';
import { MAX_CHAR_LENGTH } from '../../config.js';
import {
  clampedInt,
  contextLinesField,
  createRelaxedBulkQuerySchema,
  lineNumberField,
} from '../../scheme/fields.js';

const minifyField = z
  .enum(['none', 'standard', 'symbols'])
  .optional()
  .default('standard');
import {
  createQueryShapeSchema,
  describeQuerySchema,
} from '../../scheme/coreSchemas.js';
import { responseEnvelopeFields } from '../../scheme/responseEnvelope.js';

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

const queryOverrides = {
  startLine: lineNumberField,
  endLine: lineNumberField,
  contextLines: contextLinesField,
  charOffset: clampedInt(0, 100_000_000).optional(),
  charLength: clampedInt(1, MAX_CHAR_LENGTH).optional(),
  minify: minifyField,
} as const;

export const FileContentQueryBaseLocalSchema = createQueryShapeSchema(
  CoreFileContentQuerySchema,
  queryOverrides
);

export const FileContentQueryLocalSchema = describeQuerySchema(
  CoreFileContentQuerySchema,
  queryOverrides
);

export const FileContentBulkQueryLocalSchema = createRelaxedBulkQuerySchema(
  FileContentQueryBaseLocalSchema
);

export const GitHubFetchContentOutputLocalSchema = z.object({
  base: z.string().optional(),
  shared: z
    .record(z.string(), z.union([z.string(), z.number(), z.boolean()]))
    .optional(),
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
