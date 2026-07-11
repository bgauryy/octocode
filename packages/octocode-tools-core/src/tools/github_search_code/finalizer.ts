// Barrel: the implementation lives under ./finalizer/ (split to satisfy the
// max-lines:400 ESLint rule). Re-exported here so existing imports of
// './finalizer.js' keep working unchanged.
export { applyExactMatchRanking } from './finalizer/ranking.js';
export { buildGhSearchCodeFinalizer } from './finalizer/build.js';
