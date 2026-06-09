import { z } from 'zod';
import { ViewStructureQuerySchema as UpstreamViewStructureQuerySchema } from '@octocodeai/octocode-core/schemas';
import {
  clampedInt,
  createRelaxedBulkQuerySchema,
  depthField,
  describeField,
  LOCAL_OVERLAY_MAX_LIMIT,
  optionalMetaFields,
  relaxedPageNumberField,
  STRUCTURE_PAGE_SIZE,
  withCoreSchemaDescriptions,
  WithLocalOverlay,
} from '../../scheme/localSchemaOverlay.js';
import { STATIC_TOOL_NAMES } from '../toolNames.js';

const VIEW_STRUCTURE_HIDDEN_FIELDS = {
  extension: true,
  recursive: true,
  entriesPerPage: true,
  entryPageNumber: true,
} as const;

const limitField = clampedInt(1, LOCAL_OVERLAY_MAX_LIMIT)
  .optional()
  .describe(
    `Hard PRE-pagination cap: the maximum entries discovered before paging — ` +
      `distinct from the fixed page size (${STRUCTURE_PAGE_SIZE} for navigation tools). ` +
      `Max ${LOCAL_OVERLAY_MAX_LIMIT}.`
  );

export const LocalViewStructureQuerySchema = withCoreSchemaDescriptions(
  STATIC_TOOL_NAMES.LOCAL_VIEW_STRUCTURE,
  UpstreamViewStructureQuerySchema.omit(VIEW_STRUCTURE_HIDDEN_FIELDS).extend({
    ...optionalMetaFields,
    path: describeField(
      UpstreamViewStructureQuerySchema.shape.path,
      "Directory to browse. Relative paths resolve against the server's working directory; absolute paths must be within an allowed root (home directory or ALLOWED_PATHS). Start at the repo root with depth=1."
    ),
    page: relaxedPageNumberField
      .default(1)
      .describe(
        `Result page (1-based). Each page returns up to ${STRUCTURE_PAGE_SIZE} directory entries. Use page=2, page=3, … to walk through large directories.`
      ),
    itemsPerPage: clampedInt(1, 50)
      .optional()
      .describe('Directory entries per page for structure pagination.'),
    limit: limitField,
    depth: depthField,
  })
);

export type ViewStructureQuery = WithLocalOverlay<
  z.infer<typeof UpstreamViewStructureQuerySchema>
>;

export const LocalViewStructureBulkQuerySchema = createRelaxedBulkQuerySchema(
  STATIC_TOOL_NAMES.LOCAL_VIEW_STRUCTURE,
  LocalViewStructureQuerySchema,
  { maxQueries: 5 }
);
