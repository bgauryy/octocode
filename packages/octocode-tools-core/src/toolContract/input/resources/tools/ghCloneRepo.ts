import { z } from 'zod';
import { PUBLIC_TOOL_DESCRIPTIONS } from '../../../descriptions.js';

import type { ToolSpec } from '../../types/index.js';
import { buildObject, defineTool, metaFields } from './_toolkit.js';

export const ghCloneRepo: ToolSpec = defineTool({
  name: 'ghCloneRepo',
  type: 'Github',
  shortDescription:
    'Clone a GitHub repo or subtree locally for repeated reads, search, or LSP.',
  instructions: PUBLIC_TOOL_DESCRIPTIONS.ghCloneRepo,
  schema: {
    branch:
      'Branch, tag, or full 40-character commit SHA to check out; defaults to the repository default branch.',
    sparsePath: 'Bound checkout to a repo-relative file/dir.',
    forceRefresh: 'Bypass cached clone.',
  },
});

export const CloneRepoQuerySchema = buildObject(ghCloneRepo.schema, {
  ...metaFields,
  owner: z.string(),
  repo: z.string(),
  branch: z.string().optional(),
  sparsePath: z.string().optional(),
  forceRefresh: z.boolean().optional(),
});
