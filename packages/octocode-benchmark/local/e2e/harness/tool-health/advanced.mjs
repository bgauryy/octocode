import { ADVANCED_LOCAL } from './advanced-local.mjs';
import { ADVANCED_REMOTE } from './advanced-remote.mjs';
import { ADVANCED_CLASSIFICATION } from './advanced-classification.mjs';

// Inline classification augments the same search/read tools as their domain suite.
export const ADVANCED = {};
for (const group of [ADVANCED_LOCAL, ADVANCED_REMOTE, ADVANCED_CLASSIFICATION]) {
  for (const [tool, callback] of Object.entries(group)) (ADVANCED[tool] ??= []).push(callback);
}
