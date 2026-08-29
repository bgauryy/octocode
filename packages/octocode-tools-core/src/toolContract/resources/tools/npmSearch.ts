import { z } from 'zod';

import type { ToolSpec } from '../../types.js';
import { buildObject, defineTool, metaFields, pageNumber } from './_toolkit.js';

export const npmSearch: ToolSpec = defineTool({
  name: 'npmSearch',
  type: 'NPM',
  shortDescription: 'Look up npm packages and find their source repositories.',
  instructions: `Resolve a package to its source repository; skip if owner/repo is known. Exact name → one rich result; keywords → paged candidates. Follow repository into GitHub tools.`,
  schema: {
    keywords:
      'Registry keyword query (string; an array of terms is accepted and joined with spaces).',
    packageName: 'Exact package or keyword query; include scope.',
    page: 'Keyword-result page only.',
  },
});

export const NpmPackageQuerySchema = buildObject(npmSearch.schema, {
  ...metaFields,
  packageName: z.string(),
  // Accepted at runtime alongside packageName — a registry keyword query;
  // an array of terms is joined with spaces before searching.
  keywords: z.union([z.string(), z.array(z.string())]).optional(),
  page: pageNumber(),
});
