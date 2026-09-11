import type { ICodeHostProvider } from '../types.js';
import type { GitHubProvider } from './GitHubProvider.js';

/**
 * GitHub-only history capabilities kept outside the provider-neutral contract.
 * This prevents the foundational provider types from depending on GitHub API
 * implementations while retaining exact method signatures at the tool edge.
 */
export type GitHubHistoryProvider = Pick<
  GitHubProvider,
  | 'fetchIssues'
  | 'searchCommits'
  | 'fetchHistory'
  | 'compareRefs'
  | 'fetchCommit'
>;

export function requireGitHubHistoryProvider<
  TMethod extends keyof GitHubHistoryProvider,
>(
  provider: ICodeHostProvider,
  methodNames: readonly TMethod[]
): Pick<GitHubHistoryProvider, TMethod> {
  const candidate = provider as ICodeHostProvider &
    Partial<GitHubHistoryProvider>;
  if (
    provider.type !== 'github' ||
    methodNames.some(name => typeof candidate[name] !== 'function')
  ) {
    throw new Error(
      `GitHub history operations are unavailable for provider '${provider.type}'.`
    );
  }
  return candidate as Pick<GitHubHistoryProvider, TMethod>;
}
