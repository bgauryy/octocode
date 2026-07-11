// Thin barrel: the implementation lives under ./ripgrepResultBuilder/ (split to
// satisfy the max-lines lint rule). Re-exported here so no consumer's import
// path needs to change.
export * from './ripgrepResultBuilder/buildResult.js';
export * from './ripgrepResultBuilder/searchNext.js';
