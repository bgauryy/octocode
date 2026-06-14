/**
 * Central configuration for all tool-layer defaults, limits, and page sizes.
 *
 * Every magic number that controls agent-visible pagination, result counts, or
 * API caps lives here. Import from this file — never inline literals.
 */

// ---------------------------------------------------------------------------
// GitHub API search — code, repos, pull-requests
// ---------------------------------------------------------------------------

/** Default results per page when the agent omits `limit`. GitHub allows up to 100. */
export const GITHUB_SEARCH_DEFAULT_LIMIT = 30;

/** Maximum results per page the GitHub search API accepts. */
export const GITHUB_SEARCH_MAX_LIMIT = 100;

/** Maximum pages surfaced from GitHub code-search (API hard cap). */
export const GITHUB_SEARCH_MAX_PAGES = 10;

// ---------------------------------------------------------------------------
// GitHub repo structure
// ---------------------------------------------------------------------------

/** Default entries per page for repo tree listings. */
export const GITHUB_STRUCTURE_DEFAULT_ENTRIES_PER_PAGE = 100;

/** Maximum entries per page for repo tree listings. */
export const GITHUB_STRUCTURE_MAX_ENTRIES_PER_PAGE = 200;

// ---------------------------------------------------------------------------
// PR content sub-items (changed files, comments, commits)
// ---------------------------------------------------------------------------

/** Default items per page for PR changed-files, comments, and commit lists. */
export const PR_CONTENT_DEFAULT_ITEMS_PER_PAGE = 20;

/** Maximum items per page for PR content sub-item lists. */
export const PR_CONTENT_MAX_ITEMS_PER_PAGE = 100;

// ---------------------------------------------------------------------------
// Local file tools
// ---------------------------------------------------------------------------

/** Default files per page for localFindFiles and localViewStructure results. */
export const LOCAL_DEFAULT_FILES_PER_PAGE = 20;

/** Maximum files per page for local listing tools. */
export const LOCAL_MAX_FILES_PER_PAGE = 50;

// ---------------------------------------------------------------------------
// Schema field bounds (used by scheme/fields.ts and individual tool schemes)
// ---------------------------------------------------------------------------

/** Maximum value for `limit` fields on local listing tools (pre-pagination cap). */
export const LOCAL_MAX_LIMIT = 10_000;

/** Maximum recursion depth for directory/repo tree tools. */
export const LOCAL_MAX_DEPTH = 20;

/** Maximum page number accepted by any tool. */
export const MAX_PAGE_NUMBER = 1_000;

/** Maximum context lines returned around a code match. */
export const MAX_CONTEXT_LINES = 100;

/** Maximum char window for a single paginated response. */
export const MAX_CHAR_LENGTH = 50_000;

/**
 * Default page size for githubGetFileContent pagination.
 * Intentionally smaller than the global output limit — GitHub files are
 * fetched and cached once; all continuation pages are served from cache
 * with no re-fetch overhead. Keeping this tight avoids large token dumps
 * when the agent only needs a targeted slice.
 */
export const GITHUB_FILE_CONTENT_DEFAULT_CHAR_LENGTH = 1_000;

/** Maximum match content length (chars) for ripgrep snippets. */
export const MAX_MATCH_CONTENT_LENGTH = 100_000;
