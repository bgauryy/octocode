import { z } from 'zod';
import { FindFilesQuerySchema as UpstreamFindFilesQuerySchema } from '@octocodeai/octocode-core/schemas';
import {
  clampedInt,
  createRelaxedBulkQuerySchema,
  LOCAL_OVERLAY_MAX_LIMIT,
  optionalMetaFields,
  relaxedPageNumberField,
  withCoreSchemaDescriptions,
  WithLocalOverlay,
} from '../../scheme/localSchemaOverlay.js';
import { STATIC_TOOL_NAMES } from '../toolNames.js';

const fsDepthField = clampedInt(0, 100).optional();

const limitField = clampedInt(1, LOCAL_OVERLAY_MAX_LIMIT).optional();

export const LocalFindFilesQuerySchema = withCoreSchemaDescriptions(
  STATIC_TOOL_NAMES.LOCAL_FIND_FILES,
  UpstreamFindFilesQuerySchema.omit({
    filesPerPage: true,
    filePageNumber: true,
    type: true,
  }).extend({
    ...optionalMetaFields,
    entryType: UpstreamFindFilesQuerySchema.shape.type,
    minDepth: fsDepthField,
    maxDepth: fsDepthField,
    page: relaxedPageNumberField.default(1),
    itemsPerPage: clampedInt(1, 50).optional(),
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
