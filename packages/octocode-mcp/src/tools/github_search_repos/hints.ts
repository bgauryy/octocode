/**
 * Dynamic hints for githubSearchRepositories tool
 * @module tools/github_search_repos/hints
 */

import type { HintContext, ToolHintGenerators } from '../../types/metadata.js';

export const hints: ToolHintGenerators = {
  hasResults: (_ctx: HintContext = {}) => [
    // Context-aware hints - static hints cover generic cases
    // Metadata dynamic hints (topicsHasResults, etc.) are loaded separately via extraHints
  ],
  empty: (_ctx: HintContext = {}) => [
    // Language vs topic guidance — this is the #1 source of missed repos
    'Language filter tip: use language="TypeScript" (primary language filter) instead of or ' +
      'in addition to topicsToSearch:["typescript"]. ' +
      'Topic tags are self-reported and sparse — many TypeScript repos do not have the "typescript" topic. ' +
      'language:X is reliable because GitHub auto-detects it from file extensions.',
    'Drop rarest filter first: if using stars + created + language + topics, drop topics first, ' +
      'then created, then widen stars range.',
  ],
  error: (_ctx: HintContext = {}) => [],
};
