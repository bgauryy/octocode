import { z } from 'zod';
import { ViewStructureQuerySchema as UpstreamViewStructureQuerySchema } from '@octocodeai/octocode-core/schemas';
import {
  clampedInt,
  createRelaxedBulkQuerySchema,
  depthField,
  LOCAL_OVERLAY_MAX_LIMIT,
  optionalMetaFields,
  relaxedPageNumberField,
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

const limitField = clampedInt(1, LOCAL_OVERLAY_MAX_LIMIT).optional();

export const LocalViewStructureQuerySchema = withCoreSchemaDescriptions(
  STATIC_TOOL_NAMES.LOCAL_VIEW_STRUCTURE,
  UpstreamViewStructureQuerySchema.omit(VIEW_STRUCTURE_HIDDEN_FIELDS).extend({
    ...optionalMetaFields,
    page: relaxedPageNumberField.default(1),
    itemsPerPage: clampedInt(1, 50).optional(),
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
