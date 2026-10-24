import { advancedSearch } from './advanced-local/search.mjs';
import { advancedStructure } from './advanced-local/structure.mjs';
import { advancedAst } from './advanced-local/ast.mjs';
import { advancedRead } from './advanced-local/read.mjs';
import { advancedLsp } from './advanced-local/lsp.mjs';

export const ADVANCED_LOCAL = {
  localSearch: advancedSearch,
  structureSearch: advancedStructure,
  astSearch: advancedAst,
  localFetch: advancedRead,
  lspSearch: advancedLsp,
};
