import { z } from 'zod';

import type { ToolSpec } from '../../types/index.js';
import { buildObject, defineTool, metaFields } from './_toolkit.js';

export const ghCloneRepo: ToolSpec = defineTool({
  name: 'ghCloneRepo',
  type: 'Github',
  shortDescription:
    'Clone a GitHub repo or subtree locally for repeated reads, search, or LSP.',
  instructions: `Best for repeated reads, local AST/regex, or LSP; use ghGetFileContent for one read. Discover a bounded sparsePath first. branch selects the ref and forceRefresh bypasses the cached clone. Pass results[].data.location.localPath to local tools.`,
  schema: {
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
