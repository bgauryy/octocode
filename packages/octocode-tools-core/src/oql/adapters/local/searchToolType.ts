/**
 * `LocalSearchToolQuery` on its own, so this is the only sibling module that
 * needs a runtime import of the ripgrep-backed content-search runner purely
 * to derive its input type via `Parameters<typeof …>`.
 */
import { searchContentRipgrep } from '../../../tools/local_ripgrep/searchContentRipgrep.js';

// Type the compiled tool query against the runner's real input contract
// instead of `Record<string, unknown>`, so field construction is checked.
export type LocalSearchToolQuery = Parameters<typeof searchContentRipgrep>[0];
