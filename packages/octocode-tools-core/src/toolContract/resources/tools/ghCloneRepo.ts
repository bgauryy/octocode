import { z } from 'zod';

import type { ToolSpec } from '../../types.js';
import { buildObject, defineTool, metaFields } from './_toolkit.js';

export const ghCloneRepo: ToolSpec = defineTool({
  name: 'ghCloneRepo',
  type: 'Github',
  shortDescription:
    'Clone a GitHub repo or subtree locally for repeated reads, search, or LSP.',
  instructions: `Clone only when remote work needs repeated reads, local AST/regex, or LSP — not for a single read (ghGetFileContent). Pick a bounded sparsePath via ghViewRepoStructure/ghSearchCode first.
owner+repo identify the source; branch selects ref; sparsePath limits checkout; forceRefresh bypasses the cached clone. Follow result.localPath into localSearchCode/localGetFileContent/localViewStructure/lspGetSemantics.`,
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
