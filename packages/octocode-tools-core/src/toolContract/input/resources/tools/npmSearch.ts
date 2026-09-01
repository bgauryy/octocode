import { z } from 'zod';

import type { ToolSpec } from '../../types/index.js';
import { buildObject, defineTool, metaFields, pageNumber } from './_toolkit.js';

export const npmSearch: ToolSpec = defineTool({
  name: 'npmSearch',
  type: 'NPM',
  shortDescription: 'Look up npm packages and find their source repositories.',
  instructions: `Resolve npm packages to source repositories. Set exactly one of packageName for exact lookup or keywords for discovery. Follow the repository into GitHub tools.`,
  schema: {
    keywords:
      'Discovery terms joined with spaces. Mutually exclusive with packageName.',
    packageName: 'Exact package name; include scope for scoped packages.',
    page: 'Keyword-result page only.',
  },
});

export const NpmPackageQuerySchema = buildObject(npmSearch.schema, {
  ...metaFields,
  packageName: z.string().optional(),
  keywords: z.array(z.string()).optional(),
  page: pageNumber(),
}).superRefine((query, ctx) => {
  const keywords = query.keywords?.join(' ').trim();
  if (!query.packageName?.trim() && !keywords) {
    ctx.addIssue({
      code: 'custom',
      path: ['packageName'],
      message: 'Set packageName or keywords.',
    });
  }
  if (query.packageName?.trim() && keywords) {
    ctx.addIssue({
      code: 'custom',
      path: ['keywords'],
      message: 'Set packageName or keywords, not both.',
    });
  }
});
