// Local schema for ghSearchDiscussions. Field set / enums / defaults / prose
// come from octocode-core (SearchDiscussionsQuerySchema); the runtime only
// relaxes numeric validation (clamp instead of reject), matching the other
// GitHub tools. One source of truth; no duplicated prose.
import { SearchDiscussionsQuerySchema as CoreSearchDiscussionsQuerySchema } from '@octocodeai/octocode-core/schemas';
import {
  PR_CONTENT_DEFAULT_ITEMS_PER_PAGE,
  PR_CONTENT_MAX_ITEMS_PER_PAGE,
} from '../../config.js';
import {
  clampedInt,
  createRelaxedBulkQuerySchema,
} from '../../scheme/fields.js';
import {
  createQueryShapeSchema,
  describeQuerySchema,
} from '../../scheme/coreSchemas.js';

const discussionsOverrides = {
  itemsPerPage: clampedInt(1, PR_CONTENT_MAX_ITEMS_PER_PAGE)
    .optional()
    .default(PR_CONTENT_DEFAULT_ITEMS_PER_PAGE),
  // `limit` accepted as an alias for `itemsPerPage` (flow consistency); execution
  // prefers it when explicitly provided.
  limit: clampedInt(1, PR_CONTENT_MAX_ITEMS_PER_PAGE).optional(),
} as const;

export const SearchDiscussionsLocalSchema = describeQuerySchema(
  CoreSearchDiscussionsQuerySchema,
  discussionsOverrides
);

export const SearchDiscussionsBulkLocalSchema = createRelaxedBulkQuerySchema(
  createQueryShapeSchema(CoreSearchDiscussionsQuerySchema, discussionsOverrides)
);
