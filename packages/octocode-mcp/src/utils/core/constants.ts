/**
 * Default configuration values
 */
export const DEFAULTS = {
  COMMAND_TIMEOUT: 30000, // 30 seconds in milliseconds
  MAX_OUTPUT_SIZE: 10 * 1024 * 1024, // 10MB
  MAX_RESULTS: 100, // Default limit for result sets
  CONTEXT_LINES: 5, // Default context lines for ripgrep (0 = only matching lines)
  MAX_OUTPUT_CHARS: 2000, // Maximum output characters (~500 tokens) - use charLength for pagination
} as const;

/**
 * Centralized resource limits for memory, tokens, pagination, and execution
 * All magic numbers and limits are consolidated here for consistency and maintainability
 */
export const RESOURCE_LIMITS = {
  /** Maximum tokens allowed in MCP response (25K token limit) */
  MCP_MAX_TOKENS: 25000,
  /** Average characters per token (used for estimation) */
  CHARS_PER_TOKEN: 4,

  /** Default character length for pagination (10K chars ~2.5K tokens) */
  DEFAULT_CHAR_LENGTH: 10000,
  /** Recommended character length for safe pagination */
  RECOMMENDED_CHAR_LENGTH: 10000,

  /** Maximum characters for fetch_content operations (50K chars ~12.5K tokens) */
  MAX_FETCH_CONTENT_CHARS: 50000,
  /** Maximum output size in bytes (10MB safety limit) */
  MAX_OUTPUT_SIZE_BYTES: 10 * 1024 * 1024,
  /** File size threshold for requiring pagination (100KB) */
  LARGE_FILE_THRESHOLD_KB: 100,

  /** Number of entries before pagination should be used */
  MAX_ENTRIES_BEFORE_PAGINATION: 100,
  /** Default maximum number of files to process */
  MAX_FILES_DEFAULT: 1000,
  /** Default entries per page for view_structure */
  DEFAULT_ENTRIES_PER_PAGE: 20,
  /** Maximum entries per page for view_structure */
  MAX_ENTRIES_PER_PAGE: 20,
  /** Maximum list items to return in detailed mode (with size/permissions) */
  MAX_LIST_ITEMS_DETAILED: 100,
  /** Maximum list items to return in simple mode (paths only) */
  MAX_LIST_ITEMS_SIMPLE: 200,
  /** Maximum archive entries to return per page */
  MAX_ARCHIVE_ENTRIES_PER_PAGE: 100,
  /** Maximum directory entries to return per page */
  MAX_DIR_ENTRIES_PER_PAGE: 100,
  /** Maximum directory entries to display in tree view */
  MAX_DIR_ENTRIES_TREE: 1000,

  /** Default command execution timeout in milliseconds (30 seconds) */
  DEFAULT_EXEC_TIMEOUT_MS: 30000,

  /** Default maximum matches to return per file */
  DEFAULT_MAX_MATCHES_PER_FILE: 3,
  /** Default context lines around matches */
  DEFAULT_CONTEXT_LINES: 5,
  /** Default length of match content value in characters */
  DEFAULT_MATCH_CONTENT_LENGTH: 200,
  /** Maximum length of match content value in characters */
  MAX_MATCH_CONTENT_LENGTH: 800,
  /** Default matches per page for per-file pagination */
  DEFAULT_MATCHES_PER_PAGE: 10,
  /** Maximum matches per page */
  MAX_MATCHES_PER_PAGE: 100,
  /** Default files per page for file-level pagination */
  DEFAULT_FILES_PER_PAGE: 10,
  /** Maximum files per page */
  MAX_FILES_PER_PAGE: 20,

  /** Maximum directory size in MB before suggesting chunking workflow (100MB) */
  MAX_RIPGREP_DIRECTORY_SIZE_MB: 100,
  /** Maximum file count before suggesting chunking workflow */
  MAX_FILE_COUNT_FOR_SEARCH: 1000,
  /** Estimated average file size in bytes for directory size estimation (50KB) */
  ESTIMATED_AVG_FILE_SIZE_BYTES: 50 * 1024,
  /**
   * Post-flight (ripgrep stdout length) threshold for emitting the
   * "narrow your search" hint. Picked at 1.5MB so we stay below
   * MAX_LOCAL_TOOL_OUTPUT_BYTES (10MB) while still nudging agents to
   * paginate before the response gets unwieldy.
   */
  LARGE_RESULT_BYTES_HINT: 1.5 * 1024 * 1024,

  /** Maximum bytes to process in binary operations */
  BINARY_MAX_BYTES: 10 * 1024,
  /** Default number of hex dump lines to display */
  BINARY_DEFAULT_HEX_LINES: 20,
  /** Default minimum string length for string extraction */
  BINARY_DEFAULT_MIN_STRING_LENGTH: 6,

  /** Maximum files to list from archive */
  MAX_ARCHIVE_FILES: 1000,
  /** Default maximum files to list from archive */
  DEFAULT_ARCHIVE_MAX_FILES: 200,

  /** Global memory limit for all operations (100MB) */
  GLOBAL_MEMORY_LIMIT_BYTES: 100 * 1024 * 1024,
  /** Per-operation memory limit (10MB) */
  PER_OPERATION_MEMORY_LIMIT_BYTES: 10 * 1024 * 1024,
  /** Timeout for stale memory reservations (5 minutes) */
  MEMORY_RESERVATION_TIMEOUT_MS: 5 * 60 * 1000,

  /** Token threshold for critical warnings (>50K tokens) */
  TOKEN_CRITICAL_THRESHOLD: 50000,
  /** Token threshold for high warnings (>25K tokens) */
  TOKEN_HIGH_THRESHOLD: 25000,
  /** Token threshold for moderate warnings (>10K tokens) */
  TOKEN_MODERATE_THRESHOLD: 10000,
  /** Token threshold for notice (>2.5K tokens) */
  TOKEN_NOTICE_THRESHOLD: 2500,
} as const;
