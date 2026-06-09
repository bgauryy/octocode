import { z } from 'zod';
import { FindFilesQuerySchema as UpstreamFindFilesQuerySchema } from '@octocodeai/octocode-core/schemas';
import {
  clampedInt,
  createRelaxedBulkQuerySchema,
  describeField,
  LOCAL_OVERLAY_MAX_LIMIT,
  optionalMetaFields,
  relaxedPageNumberField,
  STRUCTURE_PAGE_SIZE,
  withCoreSchemaDescriptions,
  WithLocalOverlay,
} from '../../scheme/localSchemaOverlay.js';
import { STATIC_TOOL_NAMES } from '../toolNames.js';

const fsDepthField = clampedInt(0, 100).optional();

const limitField = clampedInt(1, LOCAL_OVERLAY_MAX_LIMIT)
  .optional()
  .describe(
    `Hard PRE-pagination cap: the maximum entries discovered before paging — ` +
      `distinct from the fixed page size (${STRUCTURE_PAGE_SIZE} items/page for navigation tools). ` +
      `Max ${LOCAL_OVERLAY_MAX_LIMIT}.`
  );

export const LocalFindFilesQuerySchema = withCoreSchemaDescriptions(
  STATIC_TOOL_NAMES.LOCAL_FIND_FILES,
  UpstreamFindFilesQuerySchema.omit({
    filesPerPage: true,
    filePageNumber: true,
    type: true,
  }).extend({
    ...optionalMetaFields,
    path: describeField(
      UpstreamFindFilesQuerySchema.shape.path,
      "Directory root for metadata search. Relative paths resolve against the server's working directory; absolute paths must be within an allowed root (home directory or ALLOWED_PATHS)."
    ),
    names: describeField(
      UpstreamFindFilesQuerySchema.shape.names,
      'Primary name filter — one or more filename globs OR-combined (e.g. ["*.ts", "*.tsx"]). Preferred over the single-value name/iname shortcuts.'
    ),
    name: describeField(
      UpstreamFindFilesQuerySchema.shape.name,
      'Single case-sensitive filename glob (e.g. "*.ts"). Use names (array) when matching multiple patterns.'
    ),
    iname: describeField(
      UpstreamFindFilesQuerySchema.shape.iname,
      '[Deprecated: use names instead] Single case-insensitive filename glob — useful for README/readme or mixed-case filenames.'
    ),
    pathPattern: describeField(
      UpstreamFindFilesQuerySchema.shape.pathPattern,
      'Glob matched against the full path, useful for monorepo package roots or nested directory slices.'
    ),
    regex: describeField(
      UpstreamFindFilesQuerySchema.shape.regex,
      'Match entries whose name matches this regular expression. Prefer the names array for simple glob patterns.'
    ),
    regexType: describeField(
      UpstreamFindFilesQuerySchema.shape.regexType,
      'Regex dialect for `regex` (e.g. "posix-extended"). Omit for the default.'
    ),
    empty: describeField(
      UpstreamFindFilesQuerySchema.shape.empty,
      'Match only empty files or directories.'
    ),
    modifiedBefore: describeField(
      UpstreamFindFilesQuerySchema.shape.modifiedBefore,
      'Modified before a time window (e.g. "7d", "2h").'
    ),
    accessedWithin: describeField(
      UpstreamFindFilesQuerySchema.shape.accessedWithin,
      'Accessed within a time window (e.g. "7d", "2h").'
    ),
    permissions: describeField(
      UpstreamFindFilesQuerySchema.shape.permissions,
      'Match by octal permission bits (e.g. "644", "755").'
    ),
    executable: describeField(
      UpstreamFindFilesQuerySchema.shape.executable,
      'Match only executable files.'
    ),
    readable: describeField(
      UpstreamFindFilesQuerySchema.shape.readable,
      'Match only readable files.'
    ),
    writable: describeField(
      UpstreamFindFilesQuerySchema.shape.writable,
      'Match only writable files.'
    ),
    excludeDir: describeField(
      UpstreamFindFilesQuerySchema.shape.excludeDir,
      'Directory names to skip.'
    ),
    details: describeField(
      UpstreamFindFilesQuerySchema.shape.details,
      'Include file size, permissions, and dates.'
    ),
    showFileLastModified: describeField(
      UpstreamFindFilesQuerySchema.shape.showFileLastModified,
      'Include last-modified timestamps.'
    ),
    entryType: describeField(
      UpstreamFindFilesQuerySchema.shape.type,
      'Filesystem entry type: "f" for files, "d" for directories.'
    ),
    minDepth: fsDepthField,
    maxDepth: fsDepthField,
    page: relaxedPageNumberField
      .default(1)
      .describe(
        `Result page (1-based). Each page returns up to ${STRUCTURE_PAGE_SIZE} files. Use page=2, page=3, … to walk through results.`
      ),
    itemsPerPage: clampedInt(1, 50)
      .optional()
      .describe('Files per page for metadata result pagination.'),
    limit: limitField,
  })
);

export type FindFilesQuery = WithLocalOverlay<
  z.infer<typeof UpstreamFindFilesQuerySchema>
>;

export const LocalFindFilesBulkQuerySchema = createRelaxedBulkQuerySchema(
  STATIC_TOOL_NAMES.LOCAL_FIND_FILES,
  LocalFindFilesQuerySchema,
  { maxQueries: 5 }
);
