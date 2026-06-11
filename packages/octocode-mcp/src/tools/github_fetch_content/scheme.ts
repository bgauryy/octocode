import { z } from 'zod';
import { FileContentQuerySchema as UpstreamFileContentQuerySchema } from '@octocodeai/octocode-core/schemas';
import {
  clampedInt,
  contextLinesField,
  createRelaxedBulkQuerySchema,
  lineNumberField,
  minifyFieldWithSymbols,
  optionalMetaFields,
  withCoreSchemaDescriptions,
} from '../../scheme/localSchemaOverlay.js';
import { validateFileContentExtractionMode } from '../../scheme/fileContentModeValidation.js';
import {
  EvidenceSchema,
  responseEnvelopeFields,
} from '../../scheme/responseEnvelope.js';
import { STATIC_TOOL_NAMES } from '../toolNames.js';

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
  resolvedBranch: z.string().optional(),
  pagination: PaginationInfoSchema.optional(),
  isPartial: z.boolean().optional(),
  startLine: z.number().optional(),
  endLine: z.number().optional(),
  // Slice ranges when matchString hits multiple separated spots in the file.
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
    .array(
      z.object({
        path: z.string(),
        size: z.number(),
        type: z.string(),
      })
    )
    .optional(),
  cached: z.boolean().optional(),
  resolvedBranch: z.string().optional(),
});

export const FileContentQueryBaseLocalSchema = withCoreSchemaDescriptions(
  STATIC_TOOL_NAMES.GITHUB_FETCH_CONTENT,
  UpstreamFileContentQuerySchema.omit({
    matchStringContextLines: true,
  } as const).extend({
    ...optionalMetaFields,
    type: z.enum(['file', 'directory']).optional(),
    startLine: lineNumberField,
    endLine: lineNumberField,
    contextLines: contextLinesField,
    matchString: z.string().optional(),
    matchStringIsRegex: z.boolean().optional(),
    matchStringCaseSensitive: z.boolean().optional(),
    charOffset: clampedInt(0, 100_000_000).optional(),
    charLength: clampedInt(1, 50_000).optional(),
    minify: minifyFieldWithSymbols,
  })
);

export const FileContentQueryLocalSchema =
  FileContentQueryBaseLocalSchema.superRefine(
    validateFileContentExtractionMode
  );

export const FileContentBulkQueryLocalSchema = createRelaxedBulkQuerySchema(
  STATIC_TOOL_NAMES.GITHUB_FETCH_CONTENT,
  FileContentQueryBaseLocalSchema
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
