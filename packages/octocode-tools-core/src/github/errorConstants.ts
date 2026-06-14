export const ERROR_CODES = {
  AUTH_REQUIRED: 'AUTH_REQUIRED',
  RATE_LIMIT_PRIMARY: 'RATE_LIMIT_PRIMARY',
  RATE_LIMIT_SECONDARY: 'RATE_LIMIT_SECONDARY',
  FORBIDDEN_PERMISSIONS: 'FORBIDDEN_PERMISSIONS',
  NOT_FOUND: 'NOT_FOUND',
  INVALID_REQUEST: 'INVALID_REQUEST',
  SERVER_UNAVAILABLE: 'SERVER_UNAVAILABLE',
  NETWORK_CONNECTION_FAILED: 'NETWORK_CONNECTION_FAILED',
  REQUEST_TIMEOUT: 'REQUEST_TIMEOUT',
  UNKNOWN: 'UNKNOWN',
} as const;

export type ErrorCode = (typeof ERROR_CODES)[keyof typeof ERROR_CODES];

export const ERROR_MESSAGES = {
  [ERROR_CODES.AUTH_REQUIRED]: {
    message: 'GitHub authentication required',
    suggestion:
      "TELL THE USER: Refresh your GitHub token! Run 'gh auth login' OR 'gh auth refresh' OR set a new GITHUB_TOKEN/GH_TOKEN environment variable",
    explanation:
      'API request requires authentication. GitHub APIs have different rate limits for authenticated (5000/hour) vs unauthenticated (60/hour) requests.',
  },

  [ERROR_CODES.RATE_LIMIT_PRIMARY]: {
    message: 'GitHub API rate limit exceeded',
    messageWithTime: (resetTime: Date, seconds: number) =>
      `GitHub API rate limit exceeded. Resets at ${resetTime.toISOString()} (in ${seconds} seconds)`,
    messageWithoutTime:
      'GitHub API rate limit exceeded. Reset time unavailable - check GitHub status or try again later',
    suggestion:
      'Set GITHUB_TOKEN for higher rate limits (5000/hour vs 60/hour)',
    explanation:
      'Primary rate limit tracks total API calls per hour. Authenticated users get 5000 requests/hour, unauthenticated get 60 requests/hour.',
  },

  [ERROR_CODES.RATE_LIMIT_SECONDARY]: {
    message: (retryAfter: number) =>
      `GitHub secondary rate limit triggered. Retry after ${retryAfter} seconds`,
    suggestion: 'Reduce request frequency to avoid abuse detection',
    explanation:
      'Secondary rate limits prevent API abuse by limiting request frequency. Triggered by making too many requests too quickly, regardless of remaining quota.',
    fallbackRetryAfter: 60,
  },

  [ERROR_CODES.FORBIDDEN_PERMISSIONS]: {
    message: 'Access forbidden - insufficient permissions',
    suggestion: 'Check repository permissions or authentication',
    suggestionWithScopes: (missing: string[]) =>
      `Missing required scopes: ${missing.join(', ')}. Run: gh auth refresh -s ${missing.join(' -s ')}`,
    fallbackSuggestion:
      'Token may not have sufficient permissions for this operation',
    explanation:
      'GitHub tokens require specific OAuth scopes for different operations. Common scopes: repo (full repository access), read:org (organization access), gist (gist access).',
  },

  [ERROR_CODES.NOT_FOUND]: {
    message: 'Repository, resource, or path not found',
    explanation:
      'Resource not found or not accessible. Could be: incorrect path, deleted resource, private repository without access, wrong branch name.',
  },

  [ERROR_CODES.INVALID_REQUEST]: {
    message: 'Invalid search query or request parameters',
    suggestion: 'Check search syntax and parameter values',
    explanation:
      'Request was well-formed but contains invalid parameters. Common causes: invalid search syntax, parameters out of range, invalid filter combinations.',
  },

  [ERROR_CODES.SERVER_UNAVAILABLE]: {
    message: 'GitHub API temporarily unavailable',
    suggestion: 'Retry the request after a short delay',
    explanation:
      'GitHub servers are temporarily unavailable. Usually resolves quickly. Check https://www.githubstatus.com for service status.',
  },

  [ERROR_CODES.NETWORK_CONNECTION_FAILED]: {
    message: 'Network connection failed',
    suggestion: 'Check internet connection and GitHub API status',
    explanation:
      'Cannot establish connection to GitHub API. Check internet connectivity, DNS settings, and firewall/proxy configuration.',
  },

  [ERROR_CODES.REQUEST_TIMEOUT]: {
    message: 'Request timeout',
    suggestion: 'Retry the request or check network connectivity',
    explanation:
      'Request exceeded timeout limit. Could be slow network, large response size, or GitHub server delay.',
  },

  [ERROR_CODES.UNKNOWN]: {
    message: 'Unknown error occurred',
    explanation:
      'An unexpected error occurred that does not match known error patterns.',
  },
} as const;

export const STATUS_TO_ERROR_CODE: Record<number, ErrorCode> = {
  401: ERROR_CODES.AUTH_REQUIRED,
  403: ERROR_CODES.FORBIDDEN_PERMISSIONS,
  404: ERROR_CODES.NOT_FOUND,
  422: ERROR_CODES.INVALID_REQUEST,
  502: ERROR_CODES.SERVER_UNAVAILABLE,
  503: ERROR_CODES.SERVER_UNAVAILABLE,
  504: ERROR_CODES.SERVER_UNAVAILABLE,
};

export const NETWORK_ERROR_PATTERNS = {
  CONNECTION_FAILED: ['ENOTFOUND', 'ECONNREFUSED'],
  TIMEOUT: ['timeout'],
} as const;

export const RATE_LIMIT_PATTERNS = {
  SECONDARY: /\bsecondary rate\b/i,
  GRAPHQL_TYPE: 'RATE_LIMITED',
} as const;

export const RATE_LIMIT_CONFIG = {
  RESET_BUFFER_SECONDS: 1,

  SECONDARY_FALLBACK_SECONDS: 60,
} as const;
