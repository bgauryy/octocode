import { z } from 'zod';
import { FindFilesQuerySchema as CoreFindFilesQuerySchema } from '@octocodeai/octocode-core/schemas';
import { LOCAL_MAX_FILES_PER_PAGE, LOCAL_MAX_LIMIT } from '../../config.js';
import {
  clampedInt,
  createRelaxedBulkQuerySchema,
  relaxedPageNumberField,
} from '../../scheme/fields.js';
import {
  createQueryShapeSchema,
  describeQuerySchema,
} from '../../scheme/coreSchemas.js';

const queryOverrides = {
  maxDepth: clampedInt(0, 100).optional(),
  minDepth: clampedInt(0, 100).optional(),
  limit: clampedInt(1, LOCAL_MAX_LIMIT).optional(),
  page: relaxedPageNumberField.default(1),
  itemsPerPage: clampedInt(1, LOCAL_MAX_FILES_PER_PAGE).optional(),
} as const;

const FindFilesQueryShape = createQueryShapeSchema(
  CoreFindFilesQuerySchema,
  queryOverrides
);

export const LocalFindFilesQuerySchema = describeQuerySchema(
  CoreFindFilesQuerySchema,
  queryOverrides
);
export type FindFilesQuery = z.infer<typeof LocalFindFilesQuerySchema>;

export const LocalFindFilesBulkQuerySchema = createRelaxedBulkQuerySchema(
  FindFilesQueryShape,
  { maxQueries: 5 }
);
