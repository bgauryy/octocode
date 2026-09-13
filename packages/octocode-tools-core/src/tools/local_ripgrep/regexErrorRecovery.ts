import type { RipgrepQuery } from '@octocodeai/octocode-core/schema';
import { TOOL_NAMES } from '../toolMetadata/names.js';

/** Add query repairs only for native matcher compilation errors, never I/O errors. */
export function regexErrorRecovery(
  error: unknown,
  query: RipgrepQuery
): Record<string, unknown> {
  if (query.regex === 'fixed') return {};
  const message = error instanceof Error ? error.message : String(error);
  const rustError = message.startsWith('regex parse error:');
  const pcreError = message.startsWith('PCRE2: error compiling pattern');
  if (!rustError && !pcreError) return {};

  // The printed pattern is untrusted evidence. Classify the final compiler
  // diagnostic, not words that happen to occur inside the user's pattern.
  const reason = message.slice(message.lastIndexOf('\nerror: ') + 8);
  const needsPcre =
    rustError &&
    query.regex !== 'perl' &&
    (reason === 'backreferences are not supported' ||
      reason ===
        'look-around, including look-ahead and look-behind, is not supported');
  const hint = needsPcre
    ? 'Use regex:"pcre2" for lookaround/backreferences, or rewrite the expression for regex:"rust".'
    : 'Use regex:"literal" for exact text, or escape metacharacters/fix searchText to keep regex matching.';
  const { snapshot: _snapshot, ...restartQuery } = query;

  return {
    errorCode: needsPcre ? 'unsupportedRegex' : 'invalidRegex',
    hints: [hint],
    next: {
      repair: {
        tool: TOOL_NAMES.LOCAL_RIPGREP,
        query: {
          ...restartQuery,
          regex: needsPcre ? 'perl' : 'fixed',
          page: 1,
          matchPage: 1,
        },
        why: needsPcre
          ? 'Start a new search using PCRE2 for this unsupported Rust regex feature.'
          : 'Start a new search treating searchText as literal text, if that was intended.',
      },
    },
  };
}
