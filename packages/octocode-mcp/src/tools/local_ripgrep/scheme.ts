import { z } from 'zod';
import { RipgrepQuerySchema as UpstreamRipgrepQuerySchema } from '@octocodeai/octocode-core/schemas';
import {
  clampedInt,
  contextLinesField,
  createRelaxedBulkQuerySchema,
  optionalMetaFields,
  relaxedPageNumberField,
  withCoreSchemaDescriptions,
  WithLocalOverlay,
} from '../../scheme/localSchemaOverlay.js';
import { STATIC_TOOL_NAMES } from '../toolNames.js';

const RIPGREP_HIDDEN_FIELDS = {
  type: true,
  count: true,
  countMatches: true,
  matchesPerPage: true,
  filesPerPage: true,
  filePageNumber: true,
  smartCase: true,
  beforeContext: true,
  afterContext: true,
  binaryFiles: true,
  encoding: true,
  includeStats: true,
  noMessages: true,
  lineRegexp: true,
  passthru: true,
  debug: true,
  showFileLastModified: true,
  noUnicode: true,
  threads: true,
  mmap: true,
  followSymlinks: true,
} as const;

const matchContentLengthField = clampedInt(1, 100_000).optional().default(500);

const RipgrepQueryBaseSchema = withCoreSchemaDescriptions(
  STATIC_TOOL_NAMES.LOCAL_RIPGREP,
  UpstreamRipgrepQuerySchema.omit(RIPGREP_HIDDEN_FIELDS).extend({
    ...optionalMetaFields,
    langType: UpstreamRipgrepQuerySchema.shape.type,
    countLinesPerFile: UpstreamRipgrepQuerySchema.shape.count.optional(),
    countMatchesPerFile:
      UpstreamRipgrepQuerySchema.shape.countMatches.optional(),
    matchContentLength: matchContentLengthField,
    sort: z
      .enum(['path', 'modified', 'accessed', 'created'])
      .optional()
      .default('path'),
    // No zod default here: applyWorkflowMode (mode="detailed") only expands
    // context when contextLines is undefined. The default (2) is applied
    // AFTER workflow-mode resolution in searchContentRipgrep.ts.
    contextLines: contextLinesField,
    maxFiles: clampedInt(1, 100_000).optional(),
    maxMatchesPerFile: clampedInt(1, 100_000).optional(),
    matchPage: relaxedPageNumberField.default(1),
    page: relaxedPageNumberField.default(1),
  })
);

export const LocalRipgrepQuerySchema = RipgrepQueryBaseSchema.superRefine(
  (data, ctx) => {
    const d = data as {
      filesOnly?: boolean;
      filesWithoutMatch?: boolean;
      fixedString?: boolean;
      perlRegex?: boolean;
      caseSensitive?: boolean;
      caseInsensitive?: boolean;
      multiline?: boolean;
      multilineDotall?: boolean;
    };
    if (d.filesOnly === true && d.filesWithoutMatch === true) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        message:
          '`filesOnly` and `filesWithoutMatch` are mutually exclusive. Choose ONE: filesOnly=true for paths with matches, OR filesWithoutMatch=true for paths without matches.',
        path: ['filesWithoutMatch'],
      });
    }
    if (d.fixedString === true && d.perlRegex === true) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        message:
          '`fixedString` and `perlRegex` are mutually exclusive. fixedString treats the pattern as a literal string; perlRegex treats it as a Perl-compatible regex. Choose ONE.',
        path: ['perlRegex'],
      });
    }
    if (d.caseSensitive === true && d.caseInsensitive === true) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        message:
          '`caseSensitive` and `caseInsensitive` are mutually exclusive. Choose ONE.',
        path: ['caseInsensitive'],
      });
    }
    if (
      (d as { countLinesPerFile?: boolean }).countLinesPerFile === true &&
      (d as { countMatchesPerFile?: boolean }).countMatchesPerFile === true
    ) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        message:
          '`countLinesPerFile` and `countMatchesPerFile` are mutually exclusive. Choose ONE: countLinesPerFile for matching-line counts, OR countMatchesPerFile for total match counts.',
        path: ['countMatchesPerFile'],
      });
    }
    if (d.multilineDotall === true && d.multiline !== true) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        message:
          '`multilineDotall` requires `multiline=true`. Set multiline=true to enable cross-line matching first.',
        path: ['multilineDotall'],
      });
    }
  }
);

export type RipgrepQuery = WithLocalOverlay<
  z.infer<typeof UpstreamRipgrepQuerySchema>
>;

export const LocalRipgrepBulkQuerySchema = createRelaxedBulkQuerySchema(
  STATIC_TOOL_NAMES.LOCAL_RIPGREP,
  RipgrepQueryBaseSchema,
  { maxQueries: 5 }
);
