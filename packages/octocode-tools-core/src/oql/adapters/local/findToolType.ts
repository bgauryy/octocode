/**
 * `LocalFindToolQuery` on its own, so this is the only sibling module that
 * needs a runtime import of the file-finder runner purely to derive its
 * input type via `Parameters<typeof …>`.
 */
import { findFiles } from '../../../tools/local_find_files/findFiles.js';

// Type the compiled tool query against the runner's real input contract
// instead of `Record<string, unknown>`, so field construction is checked.
// The runner reads `page`/`itemsPerPage` at runtime (see its executor) but
// omits them from its declared input type — name them here rather than
// hiding the gap behind a cast.
export type LocalFindToolQuery = Parameters<typeof findFiles>[0] & {
  page?: number;
  itemsPerPage?: number;
};
