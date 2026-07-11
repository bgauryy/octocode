import { z } from 'zod';
import { FileContentQuerySchema as CoreFileContentQuerySchema } from '@octocodeai/octocode-core/schemas';
import { MAX_CHAR_LENGTH } from '../../config.js';
import {
  clampedInt,
  contextLinesField,
  createRelaxedBulkQuerySchema,
  lineNumberField,
} from '../../scheme/fields.js';
import {
  createQueryShapeSchema,
  describeQuerySchema,
} from '../../scheme/coreSchemas.js';
import { responseEnvelopeFields } from '../../scheme/responseEnvelope.js';
import {
  CharPaginationSchema,
  ItemPaginationSchema,
  ToolContinuationSchema,
} from '../../scheme/pagination.js';

const minifyField = z
  .enum(['none', 'standard', 'symbols'])
  .optional()
  .default('standard');

// Parity with local_fetch_content: compose shared char/item pagination +
// ToolContinuation (continueChars) instead of inline duplicates.
const GitHubFetchFilePaginationSchema = z.union([
  CharPaginationSchema.extend({
    nextPage: z.number().optional(),
  }),
  ItemPaginationSchema.extend({
    charOffset: z.number().optional(),
    charLength: z.number().optional(),
    totalChars: z.number().optional(),
    nextCharOffset: z.number().optional(),
  }),
]);

const GitHubFetchFileNextSchema = z.object({
  continueChars: ToolContinuationSchema.extend({
    tool: z.literal('ghGetFileContent'),
  }).optional(),
});

const GitHubFetchFileEntrySchema = z.object({
  path: z.string(),
  content: z.string(),
  localPath: z.string().optional(),
  repoRoot: z.string().optional(),
  // isSkeleton was dropped — always equal to contentView==='symbols', so it
  // carried no information a consumer couldn't already derive from contentView.
  contentView: z.enum(['none', 'standard', 'symbols']).optional(),
  totalLines: z.number().optional(),
  sourceChars: z.number().optional(),
  sourceBytes: z.number().optional(),
  resolvedBranch: z.string().optional(),
  // Runtime also emits `modified` (ISO mtime); keep lastModified for older shapes.
  modified: z.string().optional(),
  fileSize: z.number().optional(),
  pagination: GitHubFetchFilePaginationSchema.optional(),
  next: GitHubFetchFileNextSchema.optional(),
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
  cached: z.boolean().optional(),
});

const GitHubFetchDirectoryEntrySchema = z.object({
  path: z.string(),
  localPath: z.string(),
  repoRoot: z.string().optional(),
  fileCount: z.number(),
  totalSize: z.number(),
  complete: z.boolean().optional(),
  directoryEntryCount: z.number().optional(),
  eligibleFileCount: z.number().optional(),
  savedFileCount: z.number().optional(),
  skipped: z
    .object({
      nonFile: z.number(),
      missingDownloadUrl: z.number(),
      oversized: z.number(),
      binary: z.number(),
      fileLimit: z.number(),
      fetchFailed: z.number(),
      totalSizeLimit: z.number(),
      pathTraversal: z.number(),
    })
    .optional(),
  limits: z
    .object({
      maxDirectoryFiles: z.number(),
      maxTotalSize: z.number(),
      maxFileSize: z.number(),
    })
    .optional(),
  warnings: z.array(z.string()).optional(),
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
      data: z
        .object({
          owner: z.string(),
          repo: z.string(),
          files: z.array(GitHubFetchFileEntrySchema).optional(),
          directories: z.array(GitHubFetchDirectoryEntrySchema).optional(),
        })
        .optional(),
    })
  ),
  errors: z
    .array(
      z.object({
        id: z.string(),
        owner: z.string().optional(),
        repo: z.string().optional(),
        path: z.string().optional(),
        error: z.string(),
      })
    )
    .optional(),
});

export type GitHubFetchContentOutputLocal = z.infer<
  typeof GitHubFetchContentOutputLocalSchema
>;
