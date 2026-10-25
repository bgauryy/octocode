import { ADVANCED_LOCAL } from './advanced-local.mjs';
import { ADVANCED_REMOTE } from './advanced-remote.mjs';
import { ADVANCED_SEARCH_CONTINUATION } from './advanced-search-continuation.mjs';
import { ADVANCED_CLASSIFICATION } from './advanced-classification.mjs';
import { ADVANCED_CURSOR_REPLAY } from './advanced-cursor-replay.mjs';
import { ADVANCED_CLI_REPLAY } from './advanced-cli-replay.mjs';

// Inline classification augments the same search/read tools as their domain suite.
export const ADVANCED = {};
for (const group of [ADVANCED_LOCAL, ADVANCED_REMOTE, ADVANCED_SEARCH_CONTINUATION, ADVANCED_CLASSIFICATION, ADVANCED_CURSOR_REPLAY, ADVANCED_CLI_REPLAY]) {
  for (const [tool, callback] of Object.entries(group)) (ADVANCED[tool] ??= []).push(callback);
}
