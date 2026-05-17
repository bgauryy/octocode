import { z } from 'zod/v4';
import {
  RipgrepQuerySchema as UpstreamRipgrepQuerySchema,
  FindFilesQuerySchema as UpstreamFindFilesQuerySchema,
  ViewStructureQuerySchema as UpstreamViewStructureQuerySchema,
  FetchContentQuerySchema as UpstreamFetchContentQuerySchema,
  createBulkQuerySchema,
} from '@octocodeai/octocode-core';
import { STATIC_TOOL_NAMES } from '../tools/toolNames.js';

export const LOCAL_OVERLAY_MAX_MATCH_CONTENT_LENGTH = 50_000;

export const LOCAL_OVERLAY_MAX_CHAR_LENGTH = 50_000;

const matchContentLengthField = z
  .number()
  .int()
  .min(1)
  .max(LOCAL_OVERLAY_MAX_MATCH_CONTENT_LENGTH)
  .optional()
  .default(200)
  .describe(
    'Maximum characters per individual match snippet. Default 200, max 50000. ' +
      'Raise this when matches sit on very long lines (minified code, JSON blobs, generated SQL). ' +
      'Total output size is still bounded by charLength / responseCharLength budgets — ' +
      'prefer paginating via filePageNumber/matchesPerPage over truncating a single match.'
  );

const localCharLengthField = z
  .number()
  .int()
  .min(1)
  .max(LOCAL_OVERLAY_MAX_CHAR_LENGTH)
  .optional()
  .describe(
    'Character budget for output pagination of this query. Unified at 50000 across local tools. ' +
      'Pair with charOffset for explicit pagination instead of truncating responses.'
  );

export const VERBOSITY_VALUES = ['compact', 'verbose', 'ultra'] as const;
export type Verbosity = (typeof VERBOSITY_VALUES)[number];

export const verbosityField = z
  .enum(VERBOSITY_VALUES)
  .optional()
  .describe(
    'Choose response size. Less tokens per call leaves more budget for follow-up. ' +
      "'compact' is the default and returns actionable detail. " +
      "'ultra' returns lossy counts/summaries for cheap broad probes. " +
      "'verbose' currently equals compact; skip it unless tool docs say otherwise. " +
      "Drill-back: re-call with 'compact' for paths, lines, snippets, or entries."
  );

export function createVerbosityField(
  toolDetail: string,
  ultraDetail: string,
  drillBack: string
) {
  return z
    .enum(VERBOSITY_VALUES)
    .optional()
    .describe(
      `Choose response size. compact (default): ${toolDetail}; use for normal work and follow-up line hints. ` +
        `ultra: ${ultraDetail}; use first for broad/large probes when counts or top locations are enough. ` +
        'verbose: currently same as compact; skip it unless future docs say it adds detail. ' +
        `Drill-back from ultra: ${drillBack}.`
    );
}

const ripgrepVerbosityField = createVerbosityField(
  'files[] with path:line matches, snippets, match counts, search engine, and pagination',
  'match/file counts plus the top path:line; files[] and match snippets are dropped',
  're-call with verbosity:"compact" or scope path/include to the top path'
);

const findFilesVerbosityField = createVerbosityField(
  'files[] with paths, type, size, permissions, timestamps, and pagination',
  'file/dir counts plus the newest path; files[] is dropped',
  're-call with verbosity:"compact" or narrow name/type/time filters'
);

const fetchContentVerbosityField = createVerbosityField(
  'content for the requested file/slice plus line ranges, matchRanges, partial flag, and pagination',
  'line/token estimates and ranges with content set to empty',
  're-call with verbosity:"compact", matchString, or a startLine/endLine range'
);

const viewStructureVerbosityField = createVerbosityField(
  'entries[] with names, types, size/modified metadata, summary, and pagination',
  'entry/file/dir counts and summary; entries[] is dropped',
  're-call with verbosity:"compact" and entryPageNumber/entriesPerPage'
);

export const RipgrepQuerySchema = UpstreamRipgrepQuerySchema.extend({
  matchContentLength: matchContentLengthField,
  verbosity: ripgrepVerbosityField,
});

export type RipgrepQuery = z.infer<typeof RipgrepQuerySchema>;

export const BulkRipgrepQuerySchema = createBulkQuerySchema(
  STATIC_TOOL_NAMES.LOCAL_RIPGREP,
  RipgrepQuerySchema,
  { maxQueries: 5 }
);

export const FindFilesQuerySchema = UpstreamFindFilesQuerySchema.extend({
  charLength: localCharLengthField,
  verbosity: findFilesVerbosityField,
});

export type FindFilesQuery = z.infer<typeof FindFilesQuerySchema>;

export const BulkFindFilesSchema = createBulkQuerySchema(
  STATIC_TOOL_NAMES.LOCAL_FIND_FILES,
  FindFilesQuerySchema,
  { maxQueries: 5 }
);

export const FetchContentQuerySchema = UpstreamFetchContentQuerySchema.extend({
  verbosity: fetchContentVerbosityField,
});

export type FetchContentQuery = z.infer<typeof FetchContentQuerySchema>;

export const BulkFetchContentQuerySchema = createBulkQuerySchema(
  STATIC_TOOL_NAMES.LOCAL_FETCH_CONTENT,
  FetchContentQuerySchema,
  { maxQueries: 5 }
);

export const ViewStructureQuerySchema = UpstreamViewStructureQuerySchema.extend(
  {
    charLength: localCharLengthField,
    verbosity: viewStructureVerbosityField,
  }
);

export type ViewStructureQuery = z.infer<typeof ViewStructureQuerySchema>;

export const BulkViewStructureSchema = createBulkQuerySchema(
  STATIC_TOOL_NAMES.LOCAL_VIEW_STRUCTURE,
  ViewStructureQuerySchema,
  { maxQueries: 5 }
);
