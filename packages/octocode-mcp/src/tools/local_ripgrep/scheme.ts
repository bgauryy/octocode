import { z } from 'zod';
import { RipgrepQuerySchema as UpstreamRipgrepQuerySchema } from '@octocodeai/octocode-core/schemas';
import {
  clampedInt,
  contextLinesField,
  createRelaxedBulkQuerySchema,
  DEFAULT_PAGE_SIZE,
  describeField,
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

const matchContentLengthField = clampedInt(1, 100_000)
  .optional()
  .default(200)
  .describe(
    'Maximum characters per individual match snippet. Default 200, max 100000. ' +
      'Raise this when matches sit on very long lines (minified code, JSON blobs, generated SQL).'
  );

const RipgrepQueryBaseSchema = withCoreSchemaDescriptions(
  STATIC_TOOL_NAMES.LOCAL_RIPGREP,
  UpstreamRipgrepQuerySchema.omit(RIPGREP_HIDDEN_FIELDS).extend({
    ...optionalMetaFields,
    pattern: describeField(
      UpstreamRipgrepQuerySchema.shape.pattern,
      'Text or regex pattern to search for. Use fixedString=true for literal text and perlRegex=true only when regex features are required.'
    ),
    path: describeField(
      UpstreamRipgrepQuerySchema.shape.path,
      "File or directory to search. Relative paths resolve against the server's working directory; absolute paths must be within an allowed root (home directory or ALLOWED_PATHS)."
    ),
    mode: describeField(
      UpstreamRipgrepQuerySchema.shape.mode,
      'Result shape: "paginated"/default for normal reading, "discovery" for cheap presence checks, "detailed" for expanded snippets.'
    ),
    langType: describeField(
      UpstreamRipgrepQuerySchema.shape.type,
      'Ripgrep language/type filter (ts, js, py, go, …) — restricts the search to files of that language.'
    ),
    countLinesPerFile: UpstreamRipgrepQuerySchema.shape.count
      .optional()
      .describe(
        'Return the number of matching lines per file instead of match content (one number per file). Mutually exclusive with countMatchesPerFile.'
      ),
    countMatchesPerFile: UpstreamRipgrepQuerySchema.shape.countMatches
      .optional()
      .describe(
        'Return the total match occurrence count per file (counts multiple matches on the same line). Mutually exclusive with countLinesPerFile.'
      ),
    matchContentLength: matchContentLengthField,
    invertMatch: UpstreamRipgrepQuerySchema.shape.invertMatch.describe(
      'Return lines/files NOT matching the pattern (-v). ' +
        'Combine with filesOnly to list files that lack a pattern entirely.'
    ),
    caseInsensitive: UpstreamRipgrepQuerySchema.shape.caseInsensitive.describe(
      'Force case-insensitive matching (-i). Overrides smartCase. ' +
        'Mutually exclusive with caseSensitive.'
    ),
    multiline: UpstreamRipgrepQuerySchema.shape.multiline.describe(
      'Enable cross-line matching (-U). Pattern can span multiple lines. ' +
        'Pair with perlRegex for named captures; pair with multilineDotall to let . match newlines.'
    ),
    multilineDotall: UpstreamRipgrepQuerySchema.shape.multilineDotall.describe(
      'Make . match newlines in multiline mode (--multiline-dotall). ' +
        'Requires multiline=true.'
    ),
    sort: z
      .enum(['path', 'modified', 'accessed', 'created'])
      .optional()
      .default('path')
      .describe(
        'Sort results by: path (default, deterministic), modified (most recently changed first), ' +
          'accessed, or created.'
      ),
    sortReverse: UpstreamRipgrepQuerySchema.shape.sortReverse.describe(
      'Reverse sort direction. Pair with sort (e.g. sort=modified + sortReverse=true for oldest first).'
    ),
    contextLines: contextLinesField.default(2),
    maxFiles: clampedInt(1, 100_000).optional(),
    maxMatchesPerFile: clampedInt(1, 100_000).optional(),
    matchPage: relaxedPageNumberField
      .default(1)
      .describe(
        'Per-file match page (1-based). Use with maxMatchesPerFile to continue matches inside files that report pagination.hasMore=true.'
      ),
    page: relaxedPageNumberField
      .default(1)
      .describe(
        `Result page (1-based). Each page returns up to ${DEFAULT_PAGE_SIZE} files. Use page=2, page=3, … to walk through results.`
      ),
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
