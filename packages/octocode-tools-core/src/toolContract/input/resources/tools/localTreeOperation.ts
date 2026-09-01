import { z } from 'zod';

import {
  buildObject,
  intRange,
  MAX_LOCAL_DEPTH,
  MAX_LOCAL_ITEMS_PER_PAGE,
  MAX_LOCAL_LIMIT,
  metaFields,
  pageNumber,
  StringArray,
} from './_toolkit.js';

export const localTreeOperationDescriptions = {
  pattern: 'Name glob or substring; use the files operation for regex.',
  entryType: '"f" files only, "d" dirs only; omit for both.',
  detail: '"basic" (default), "modified" (+mtime), or "full" (size/perms).',
  hidden: 'Include dot-files/dirs.',
  recursive:
    'true enables the default recursive depth; omit with maxDepth for immediate children.',
  maxDepth:
    'Recursion depth. Omitted: immediate children, or 5 if recursive:true; maxDepth alone enables recursion, so recursive:true is not required. 0 uses those defaults.',
  extensions: 'Allowlist without dots.',
  excludeDir: 'Directories to prune; [] includes generated/vendor directories.',
  sortBy: '"name" (default), "size", "time", or "extension".',
  reverse: 'Reverse the sort order.',
  limit: 'Discovery cap before pagination.',
};

export const ViewStructureQuerySchema = buildObject(
  localTreeOperationDescriptions,
  {
    ...metaFields,
    path: z.string(),
    detail: z.enum(['basic', 'modified', 'full']).default('basic'),
    hidden: z.boolean().optional(),
    sortBy: z.enum(['name', 'size', 'time', 'extension']).default('name'),
    reverse: z.boolean().optional(),
    pattern: z.string().optional(),
    entryType: z.enum(['f', 'd']).optional(),
    recursive: z.boolean().optional(),
    extensions: StringArray,
    excludeDir: StringArray,
    maxDepth: intRange(0, MAX_LOCAL_DEPTH).optional(),
    limit: intRange(1, MAX_LOCAL_LIMIT).optional(),
    page: pageNumber(),
    itemsPerPage: intRange(1, MAX_LOCAL_ITEMS_PER_PAGE).optional(),
  }
);
