import { ADVANCED_GRAPH } from './advanced-graph.mjs';
import { ADVANCED_LOCAL } from './advanced-local.mjs';
import { ADVANCED_REMOTE } from './advanced-remote.mjs';
import { ADVANCED_SEARCH_CONTINUATION } from './advanced-search-continuation.mjs';
import { ADVANCED_CLASSIFICATION } from './advanced-classification.mjs';
import { ADVANCED_CURSOR_REPLAY } from './advanced-cursor-replay.mjs';
import { ADVANCED_CLI_REPLAY } from './advanced-cli-replay.mjs';
import { ADVANCED_PERMUTATIONS } from './advanced-permutations.mjs';
import { ADVANCED_WORKFLOWS } from './advanced-workflows.mjs';
import { advancedStructureWindow } from './advanced-structure-window.mjs';
import { ADVANCED_GITHUB_WORKFLOWS } from './advanced-github-workflows.mjs';

// Inline classification augments the same search/read tools as their domain suite.
export const ADVANCED = {};
for (const group of [ADVANCED_GRAPH, ADVANCED_LOCAL, ADVANCED_REMOTE, ADVANCED_SEARCH_CONTINUATION, ADVANCED_CLASSIFICATION, ADVANCED_CURSOR_REPLAY, ADVANCED_CLI_REPLAY, ADVANCED_PERMUTATIONS, ADVANCED_WORKFLOWS, ADVANCED_GITHUB_WORKFLOWS]) {
  for (const [tool, callback] of Object.entries(group)) (ADVANCED[tool] ??= []).push(callback);
}
ADVANCED.structureSearch.push(advancedStructureWindow);
