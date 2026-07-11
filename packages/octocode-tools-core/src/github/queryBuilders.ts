// Barrel: query-building logic lives under ./queryBuilders/*.
// Split to satisfy the max-lines:400 ESLint rule; behavior is unchanged.
export { getOwnerQualifier } from './queryBuilders/base.js';
export {
  buildCodeSearchQuery,
  buildRepoSearchQuery,
} from './queryBuilders/codeAndRepo.js';
export {
  buildPullRequestSearchQuery,
  shouldUseSearchForPRs,
} from './queryBuilders/pullRequests.js';
export {
  buildIssueSearchQuery,
  shouldUseSearchForIssues,
  type IssueSearchParams,
} from './queryBuilders/issues.js';
