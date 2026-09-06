import { z } from 'zod';
import { PUBLIC_TOOL_DESCRIPTIONS } from '../../../descriptions.js';

import type { ToolSpec } from '../../types/index.js';
import {
  buildObject,
  defineTool,
  metaFields,
  optionalPageNumber,
} from './_toolkit.js';

export const npmSearch: ToolSpec = defineTool({
  name: 'npmSearch',
  type: 'NPM',
  shortDescription: 'Look up npm packages and find their source repositories.',
  instructions: PUBLIC_TOOL_DESCRIPTIONS.npmSearch,
  schema: {
    keywords:
      'Discovery terms joined with spaces. Mutually exclusive with packageName.',
    packageName: 'Exact package name; include scope for scoped packages.',
    page: 'Keyword-result page only.',
    registry:
      'Optional registry URL overriding npm defaults and scope mappings. Credentials come from npm configuration, never tool arguments.',
  },
});

export const NpmPackageQuerySchema = buildObject(npmSearch.schema, {
  ...metaFields,
  packageName: z.string().trim().min(1).optional(),
  keywords: z.array(z.string().trim().min(1)).min(1).optional(),
  page: optionalPageNumber(),
  registry: z
    .string()
    .url()
    .refine(value => {
      const url = new URL(value);
      return (
        ['http:', 'https:'].includes(url.protocol) &&
        !url.username &&
        !url.password &&
        !url.search &&
        !url.hash
      );
    }, 'Use an HTTP(S) registry URL without credentials, query, or fragment.')
    .optional(),
}).superRefine((query, ctx) => {
  if (query.packageName === undefined && query.keywords === undefined) {
    ctx.addIssue({
      code: 'custom',
      path: ['packageName'],
      message: 'Set packageName or keywords.',
    });
  }
  if (query.packageName !== undefined && query.keywords !== undefined) {
    ctx.addIssue({
      code: 'custom',
      path: ['keywords'],
      message: 'Set packageName or keywords, not both.',
    });
  }
  if (query.packageName !== undefined && query.page !== undefined) {
    ctx.addIssue({
      code: 'custom',
      path: ['page'],
      message:
        'page applies only to keyword discovery; omit it for an exact package lookup.',
    });
  }
});
