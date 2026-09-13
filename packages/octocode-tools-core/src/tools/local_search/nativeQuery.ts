import type { LocalTextResultView } from '@octocodeai/octocode-core/schema';
import { LOCAL_DEFAULT_FILES_PER_PAGE } from '@octocodeai/octocode-core/schema';

const REGEX_TO_NATIVE = {
  literal: 'fixed',
  rust: 'smart',
  pcre2: 'perl',
} as const;

export function toLegacyTextQuery(
  query: Record<string, unknown>,
  resultView: LocalTextResultView
): Record<string, unknown> {
  const { pageSize, reverse, regex, resultView: _resultView, ...input } = query;
  const mode =
    resultView === 'paginated' ||
    resultView === 'discovery' ||
    resultView === 'detailed'
      ? resultView
      : 'paginated';
  return {
    ...input,
    ...(pageSize !== undefined
      ? { itemsPerPage: pageSize }
      : { itemsPerPage: LOCAL_DEFAULT_FILES_PER_PAGE }),
    ...(reverse !== undefined ? { sortReverse: reverse } : {}),
    ...(regex !== undefined
      ? { regex: REGEX_TO_NATIVE[regex as keyof typeof REGEX_TO_NATIVE] }
      : {}),
    mode,
    ...(mode === 'paginated' &&
    !['paginated', 'discovery', 'detailed'].includes(resultView)
      ? { output: resultView }
      : {}),
  };
}
