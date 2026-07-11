// Barrel/orchestrator for PR content fetching & transformation.
// Implementation lives in ./prContentFetcher/* (split to satisfy the
// max-lines:400 ESLint rule); this file re-exports the public surface so
// existing imports (e.g. './prContentFetcher.js') keep working unchanged.

export { shouldEnrichPullRequestFromSearch } from './prContentFetcher/flags.js';
export {
  transformPullRequestItemFromSearch,
  transformPullRequestItemFromREST,
} from './prContentFetcher/transform.js';
