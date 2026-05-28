/**
 * Dynamic hints for githubSearchRepositories tool
 * @module tools/github_search_repos/hints
 */

import type { ToolHintGenerators } from '../../types/metadata.js';

export const hints: ToolHintGenerators = {
  empty: () => [],
  error: () => [],
};
