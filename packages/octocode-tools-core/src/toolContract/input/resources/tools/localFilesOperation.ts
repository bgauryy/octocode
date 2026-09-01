import { z } from 'zod';

import {
  buildObject,
  intRange,
  MAX_FIND_DEPTH,
  MAX_LOCAL_ITEMS_PER_PAGE,
  MAX_LOCAL_LIMIT,
  metaFields,
  pageNumber,
  StringArray,
} from './_toolkit.js';

export const localFilesOperationDescriptions = {
  maxDepth: 'Maximum levels below path; 0 is direct entries.',
  minDepth: 'Requires <= maxDepth when both set.',
  names: 'Basename globs; ORed.',
  pathPattern: 'Full-path glob.',
  regex: 'Basename Rust regex.',
  empty: 'Match only empty files/dirs.',
  time: 'Time filters using d/h/w/m durations.',
  'time.modifiedWithin': 'Modified within this duration, e.g. 2d or 6h.',
  'time.modifiedBefore': 'Modified before this duration ago, e.g. 30d.',
  'time.accessedWithin': 'Accessed within this duration, e.g. 1w.',
  size: 'Size range, e.g. 100k or 1m.',
  'size.greater': 'Minimum file size, e.g. 100k or 1m.',
  'size.less': 'Maximum file size, e.g. 500b or 2m.',
  extensions: 'Extensions without dots, e.g. ["ts","tsx"].',
  excludeDir:
    'Directory names; generated/vendor dirs are pruned by default. Pass [] to prune nothing.',
  entryType: '"f" files, "d" dirs.',
  permissions: 'Exact permission filter.',
  access: '"executable", "readable", or "writable" filter.',
  detail: '"basic" (default), "modified" (+mtime), or "full" (all metadata).',
  sortBy: '"modified" (default), "name", "path", or "size".',
  limit: 'Post-sort result cap; pagination stays within it.',
};

const findProse = localFilesOperationDescriptions;

export const FindFilesQuerySchema = buildObject(findProse, {
  ...metaFields,
  path: z.string(),
  maxDepth: intRange(0, MAX_FIND_DEPTH).optional(),
  minDepth: intRange(0, MAX_FIND_DEPTH).optional(),
  names: StringArray,
  pathPattern: z.string().optional(),
  regex: z.string().optional(),
  empty: z.boolean().optional(),
  time: buildObject(
    findProse,
    {
      modifiedWithin: z.string().optional(),
      modifiedBefore: z.string().optional(),
      accessedWithin: z.string().optional(),
    },
    'time'
  ).optional(),
  size: buildObject(
    findProse,
    {
      greater: z.string().optional(),
      less: z.string().optional(),
    },
    'size'
  ).optional(),
  permissions: z.string().optional(),
  access: z.enum(['executable', 'readable', 'writable']).optional(),
  extensions: StringArray,
  excludeDir: StringArray,
  limit: intRange(1, MAX_LOCAL_LIMIT).optional(),
  detail: z.enum(['basic', 'modified', 'full']).default('basic'),
  sortBy: z.enum(['modified', 'name', 'path', 'size']).default('modified'),
  entryType: z.enum(['f', 'd']).optional(),
  page: pageNumber(),
  itemsPerPage: intRange(1, MAX_LOCAL_ITEMS_PER_PAGE).optional(),
}).superRefine((query, ctx) => {
  if (
    query.minDepth !== undefined &&
    query.maxDepth !== undefined &&
    query.minDepth > query.maxDepth
  ) {
    ctx.addIssue({
      code: 'custom',
      message: 'minDepth must be less than or equal to maxDepth.',
      path: ['minDepth'],
    });
  }
});
