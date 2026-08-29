import { z } from 'zod';

import type { ToolSpec } from '../../types.js';
import {
  buildObject,
  defineTool,
  intRange,
  MAX_FIND_DEPTH,
  MAX_LOCAL_ITEMS_PER_PAGE,
  MAX_LOCAL_LIMIT,
  metaFields,
  pageNumber,
  StringArray,
} from './_toolkit.js';

export const localFindFiles: ToolSpec = defineTool({
  name: 'localFindFiles',
  type: 'Local',
  shortDescription:
    'Find local files and directories by name, size, time, or permissions.',
  instructions: `Local path discovery by name/type/metadata — not contents (localSearchCode) or tree shape (localViewStructure). All filters AND. Prunes common generated/vendor dirs by default (node_modules, .git, dist, build, out, coverage, target, .next, .cache) — excludeDir:[] prunes nothing, or set excludeDir explicitly. Follow paths into read/search/LSP.`,
  schema: {
    maxDepth:
      "Recurse at most this many levels below path (0 = path's own entries).",
    minDepth: 'Requires <= maxDepth when both set.',
    names: 'Basename globs; ORed.',
    pathPattern: 'Full-path glob.',
    regex: 'Basename Rust regex.',
    empty: 'Match only empty files/dirs.',
    time: '{ modifiedWithin?, modifiedBefore?, accessedWithin? } d/h/w/m windows.',
    size: '{ greater?, less? } e.g. 100k, 1m, 500b.',
    extensions:
      'Extensions to include, e.g. ["ts","tsx"] (no dot); cheaper and clearer than a names glob for this.',
    excludeDir:
      'Directory names to prune from the walk. Common generated/vendor dirs are pruned by default (node_modules, .git, dist, build, out, coverage, target, .next, .cache); pass [] to prune nothing, or pass an explicit list to choose pruned dirs.',
    entryType: '"f" files, "d" dirs.',
    permissions: 'Exact permission filter.',
    access: '"executable", "readable", or "writable" filter.',
    detail: '"basic" (default), "modified" (+mtime), or "full" (all metadata).',
    sortBy: '"modified" (default), "name", "path", or "size".',
    limit:
      'Discovery cap applied after sort, before pagination — total results are capped here; itemsPerPage/page page within that cap.',
  },
});

const findProse = localFindFiles.schema;

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
