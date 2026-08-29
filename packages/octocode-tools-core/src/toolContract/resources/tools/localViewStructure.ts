import { z } from 'zod';

import type { ToolSpec } from '../../types.js';
import {
  buildObject,
  defineTool,
  intRange,
  MAX_LOCAL_DEPTH,
  MAX_LOCAL_ITEMS_PER_PAGE,
  MAX_LOCAL_LIMIT,
  metaFields,
  pageNumber,
  StringArray,
} from './_toolkit.js';

export const localViewStructure: ToolSpec = defineTool({
  name: 'localViewStructure',
  type: 'Local',
  shortDescription:
    'Browse a local directory tree — cheapest first orientation step; no content loaded.',
  instructions: `Cheapest local orientation before reading. Metadata/name filters → localFindFiles; files containing text → localSearchCode(mode:"discovery").
path is the root; pattern/extensions/entryType filter names; recursive enables maxDepth; detail switches output shape; page advances only on hasMore. Follow paths into localGetFileContent/search/LSP.`,
  schema: {
    pattern: 'Name glob/substr; use localFindFiles for regex.',
    entryType: '"f" files only, "d" dirs only; omit for both.',
    detail: '"basic" (default), "modified" (+mtime), or "full" (size/perms).',
    hidden: 'Include dot-files/dirs.',
    recursive: 'Enables maxDepth.',
    maxDepth:
      "Maximum recursion depth: 1 = the target directory's immediate children, 2 = children + grandchildren, and so on (upper bound lives in the schema). Setting maxDepth on its own enables recursion to that depth — recursive:true is not required. Effective depth when OMITTED: 1 (immediate children only) if recursive is unset, or 5 if recursive:true. maxDepth:0 is treated as unset and falls back to those defaults.",
    extensions: 'Allowlist without dots.',
    sortBy: '"name" (default), "size", "time", or "extension".',
    reverse: 'Reverse the sort order.',
    limit: 'Discovery cap before pagination.',
  },
});

export const ViewStructureQuerySchema = buildObject(localViewStructure.schema, {
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
  maxDepth: intRange(0, MAX_LOCAL_DEPTH).optional(),
  limit: intRange(1, MAX_LOCAL_LIMIT).optional(),
  page: pageNumber(),
  itemsPerPage: intRange(1, MAX_LOCAL_ITEMS_PER_PAGE).optional(),
});
