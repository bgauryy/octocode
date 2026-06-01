/**
 * Default entries per page for GitHub repo structure pagination.
 * Kept in sync with the schema default in scheme/remoteSchemaOverlay.ts so
 * typical monorepo `packages/` dirs (~40 folders) return in a single page
 * instead of forcing a second `entryPageNumber=2` call.
 */
export const GITHUB_STRUCTURE_DEFAULTS = {
  ENTRIES_PER_PAGE: 100,
  MAX_ENTRIES_PER_PAGE: 200,
} as const;
