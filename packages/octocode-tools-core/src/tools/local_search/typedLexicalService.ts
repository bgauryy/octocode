import { searchContentRipgrep } from '../local_ripgrep/searchContentRipgrep.js';
import {
  LocalRipgrepQuerySchema,
  type LocalTextResultView,
  type LocalSearchQuery,
  type RipgrepQuery,
} from '@octocodeai/octocode-core/schema';
import type { LocalSearchCodeToolResult } from '@octocodeai/octocode-core/extra-types';
import { toLegacyTextQuery } from './nativeQuery.js';

export type TypedLexicalSearchQuery = {
  path: string;
  searchText: string;
  resultView?: LocalTextResultView;
} & Record<string, unknown>;

/**
 * Private typed boundary for every caller that needs lexical repository search.
 *
 * The ripgrep runner remains the execution owner while this adapter keeps its
 * legacy field names out of public-tool and semantic warmup callers.
 */
export async function runTypedLexicalSearch(
  query: LocalSearchQuery | TypedLexicalSearchQuery
): Promise<LocalSearchCodeToolResult> {
  const { resultView = 'paginated', ...input } = query;
  return searchContentRipgrep(
    LocalRipgrepQuerySchema.parse(
      toLegacyTextQuery(input, resultView as LocalTextResultView)
    ) as RipgrepQuery
  );
}
