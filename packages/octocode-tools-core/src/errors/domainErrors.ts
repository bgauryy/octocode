export const CONFIG_ERRORS = {
  NOT_INITIALIZED: {
    code: 'CONFIG_NOT_INITIALIZED',
    message:
      'Configuration not initialized. Call initialize() and await its completion before calling getServerConfig().',
  },
} as const;

export const VALIDATION_ERRORS = {
  PROMISES_NOT_ARRAY: {
    code: 'VALIDATION_PROMISES_NOT_ARRAY',
    message: 'promises must be an array',
  },
  TIMEOUT_NOT_POSITIVE: {
    code: 'VALIDATION_TIMEOUT_NOT_POSITIVE',
    message: 'timeout must be positive',
  },
  CONCURRENCY_NOT_POSITIVE: {
    code: 'VALIDATION_CONCURRENCY_NOT_POSITIVE',
    message: 'concurrency must be positive',
  },
} as const;

export const FETCH_ERRORS = {
  FETCH_NOT_AVAILABLE: {
    code: 'FETCH_NOT_AVAILABLE',
    message: 'Global fetch is not available in this environment.',
  },
  FETCH_FAILED_AFTER_RETRIES: {
    code: 'FETCH_FAILED_AFTER_RETRIES',
    message: (attempts: number, errorMessage: string) =>
      `Failed to fetch after ${attempts} attempts: ${errorMessage}`,
  },
  FETCH_HTTP_ERROR: {
    code: 'FETCH_HTTP_ERROR',
    message: (status: number, statusText: string) =>
      `Failed to fetch (${status} ${statusText})`,
  },
} as const;

export const TOOL_METADATA_ERRORS = {
  INVALID_FORMAT: {
    code: 'TOOL_METADATA_INVALID_FORMAT',
    message: 'Invalid tool metadata format from remote source.',
  },
  INVALID_API_RESPONSE: {
    code: 'TOOL_METADATA_INVALID_API_RESPONSE',
    message: 'Invalid API response structure',
  },
} as const;

export const FILE_OPERATION_ERRORS = {
  PATH_IS_DIRECTORY: {
    code: 'FILE_PATH_IS_DIRECTORY',
    message: (toolName: string) =>
      `Path is a directory. Use ${toolName} to list directory contents`,
  },
  FILE_TOO_LARGE: {
    code: 'FILE_TOO_LARGE',
    message: (fileSizeKB: number, maxSizeKB: number, toolName: string) =>
      `File too large (${fileSizeKB}KB > ${maxSizeKB}KB). Use ${toolName} to search within the file or use startLine/endLine parameters to get specific sections`,
  },
  FILE_EMPTY: {
    code: 'FILE_EMPTY',
    message: 'File is empty - no content to display',
  },
  BINARY_FILE: {
    code: 'FILE_BINARY',
    message:
      'Binary file detected. Cannot display as text - download directly from GitHub',
  },
  DECODE_FAILED: {
    code: 'FILE_DECODE_FAILED',
    message:
      'Failed to decode file. Encoding may not be supported (expected UTF-8)',
  },
  UNSUPPORTED_TYPE: {
    code: 'FILE_UNSUPPORTED_TYPE',
    message: (type: string) => `Unsupported file type: ${type}`,
  },
} as const;

export const REPOSITORY_ERRORS = {
  NOT_FOUND: {
    code: 'REPO_NOT_FOUND',
    message: (owner: string, repo: string, error: string) =>
      `Repository "${owner}/${repo}" not found or not accessible: ${error}`,
  },
  PATH_NOT_FOUND: {
    code: 'REPO_PATH_NOT_FOUND',
    message: (path: string, owner: string, repo: string, branch: string) =>
      `Path "${path}" not found in repository "${owner}/${repo}" on branch "${branch}"`,
  },
  PATH_NOT_FOUND_ANY_BRANCH: {
    code: 'REPO_PATH_NOT_FOUND_ANY_BRANCH',
    message: (path: string, owner: string, repo: string) =>
      `Path "${path}" not found in repository "${owner}/${repo}" on any common branch`,
  },
  ACCESS_FAILED: {
    code: 'REPO_ACCESS_FAILED',
    message: (owner: string, repo: string, error: string) =>
      `Failed to access repository "${owner}/${repo}": ${error}`,
  },
  STRUCTURE_EXPLORATION_FAILED: {
    code: 'REPO_STRUCTURE_EXPLORATION_FAILED',
    message: 'Failed to explore repository structure',
  },
} as const;

export const SEARCH_ERRORS = {
  QUERY_EMPTY: {
    code: 'SEARCH_QUERY_EMPTY',
    message: 'Search query cannot be empty',
  },
  NO_VALID_PARAMETERS: {
    code: 'SEARCH_NO_VALID_PARAMETERS',
    message: 'No valid search parameters provided',
  },
  PR_REQUIRED_PARAMS: {
    code: 'SEARCH_PR_REQUIRED_PARAMS',
    message: 'Owner, repo, and prNumber are required parameters',
  },
  PR_SINGLE_VALUES: {
    code: 'SEARCH_PR_SINGLE_VALUES',
    message: 'Owner and repo must be single values',
  },
  PULL_REQUEST_SEARCH_FAILED: {
    code: 'SEARCH_PR_SEARCH_FAILED',
    message: (error: string) => `Pull request search failed: ${error}`,
  },
  PULL_REQUEST_LIST_FAILED: {
    code: 'SEARCH_PR_LIST_FAILED',
    message: (error: string) => `Pull request list failed: ${error}`,
  },
  PULL_REQUEST_FETCH_FAILED: {
    code: 'SEARCH_PR_FETCH_FAILED',
    message: (prNumber: number, error: string) =>
      `Failed to fetch pull request #${prNumber}: ${error}`,
  },
} as const;

export const STARTUP_ERRORS = {
  NO_TOOLS_REGISTERED: {
    code: 'STARTUP_NO_TOOLS_REGISTERED',
    message: 'No tools were successfully registered',
  },
  UNCAUGHT_EXCEPTION: {
    code: 'STARTUP_UNCAUGHT_EXCEPTION',
    message: (error: string) => `Uncaught exception: ${error}`,
  },
  UNHANDLED_REJECTION: {
    code: 'STARTUP_UNHANDLED_REJECTION',
    message: (reason: string) => `Unhandled rejection: ${reason}`,
  },
  STARTUP_FAILED: {
    code: 'STARTUP_FAILED',
    message: (error: string) => `Startup failed: ${error}`,
  },
} as const;

export const PROMISE_ERRORS = {
  TIMEOUT: {
    code: 'PROMISE_TIMEOUT',
    message: (index: number, timeout: number) =>
      `Promise ${index} timed out after ${timeout}ms`,
  },
  NOT_A_FUNCTION: {
    code: 'PROMISE_NOT_A_FUNCTION',
    message: (index: number) =>
      `Promise function at index ${index} is not a function`,
  },
  FUNCTION_UNDEFINED: {
    code: 'PROMISE_FUNCTION_UNDEFINED',
    message: 'Promise function is undefined',
  },
} as const;

export const TOOL_ERRORS = {
  EXECUTION_FAILED: {
    code: 'TOOL_EXECUTION_FAILED',
    message: (toolName: string, error: string) =>
      `Tool ${toolName} execution failed: ${error}`,
  },
  SECURITY_VALIDATION_FAILED: {
    code: 'TOOL_SECURITY_VALIDATION_FAILED',
    message: (toolName: string, error: string) =>
      `Security validation failed for ${toolName}: ${error}`,
  },
} as const;

export const ALL_ERROR_CODES = {
  ...CONFIG_ERRORS,
  ...VALIDATION_ERRORS,
  ...FETCH_ERRORS,
  ...TOOL_METADATA_ERRORS,
  ...FILE_OPERATION_ERRORS,
  ...REPOSITORY_ERRORS,
  ...SEARCH_ERRORS,
  ...STARTUP_ERRORS,
  ...PROMISE_ERRORS,
  ...TOOL_ERRORS,
} as const;
