import { beforeEach, afterEach, afterAll, vi } from 'vitest';
import { resetCircuitBreaker } from '../../octocode-tools-core/src/utils/http/circuitBreaker.js';
import {
  consumeExpectedStderrWarning,
  resetExpectedStderrWarnings,
  shouldSuppressUnexpectedWarningFailure,
} from './warningPolicy.js';

process.setMaxListeners(50);

vi.stubGlobal(
  'fetch',
  vi.fn(() => Promise.resolve(new Response('', { status: 200 })))
);

const generateMockUUID = () => {
  const hex = '0123456789abcdef';
  let uuid = '';
  for (let i = 0; i < 36; i++) {
    if (i === 8 || i === 13 || i === 18 || i === 23) {
      uuid += '-';
    } else if (i === 14) {
      uuid += '4';
    } else if (i === 19) {
      uuid += hex[(Math.random() * 4) | 8];
    } else {
      uuid += hex[(Math.random() * 16) | 0];
    }
  }
  return uuid;
};

const sessionMockState = {
  sessionId: generateMockUUID(),
  deleted: false,
};

const mockDefaultConfig = {
  version: 1,
  github: {
    apiUrl: 'https://api.github.com',
  },
  local: {
    enabled: true,
    enableClone: false,
    allowedPaths: [],
  },
  tools: {
    enabled: null,
    enableAdditional: null,
    disabled: null,
  },
  network: {
    timeout: 30000,
    maxRetries: 3,
  },
  telemetry: {
    logging: true,
  },
  lsp: {
    configPath: undefined,
  },
  output: {
    format: 'yaml',
    pagination: {
      defaultCharLength: 8000,
    },
  },
  source: 'defaults',
  configPath: undefined,
};

function mockParseBooleanEnv(value: string | undefined): boolean | undefined {
  if (value === undefined || value === null) return undefined;
  const trimmed = value.trim().toLowerCase();
  if (trimmed === '') return undefined;
  if (trimmed === 'true' || trimmed === '1') return true;
  if (trimmed === 'false' || trimmed === '0') return false;
  return undefined;
}

function mockParseLoggingEnv(value: string | undefined): boolean | undefined {
  if (value === undefined || value === null) return undefined;
  const trimmed = value.trim().toLowerCase();
  if (trimmed === '') return undefined;
  if (trimmed === 'false' || trimmed === '0') return false;
  return true;
}

function mockParseIntEnv(value: string | undefined): number | undefined {
  if (value === undefined || value === null) return undefined;
  const trimmed = value.trim();
  if (trimmed === '') return undefined;
  const parsed = parseInt(trimmed, 10);
  if (isNaN(parsed)) return undefined;
  return parsed;
}

function mockParseStringArrayEnv(
  value: string | undefined
): string[] | undefined {
  if (value === undefined || value === null) return undefined;
  const trimmed = value.trim();
  if (trimmed === '') return undefined;
  return trimmed
    .split(',')
    .map(s => s.trim())
    .filter(s => s.length > 0);
}

const buildMockConfig = () => {
  const envEnableLocal = mockParseBooleanEnv(process.env.ENABLE_LOCAL);
  const envEnableClone = mockParseBooleanEnv(process.env.ENABLE_CLONE);
  const envLogging = mockParseLoggingEnv(process.env.LOG);
  const envTimeout = mockParseIntEnv(process.env.REQUEST_TIMEOUT);
  const envMaxRetries = mockParseIntEnv(process.env.MAX_RETRIES);
  const envApiUrl = process.env.GITHUB_API_URL?.trim();
  const envToolsToRun = mockParseStringArrayEnv(process.env.TOOLS_TO_RUN);
  const envEnableTools = mockParseStringArrayEnv(process.env.ENABLE_TOOLS);
  const envDisableTools = mockParseStringArrayEnv(process.env.DISABLE_TOOLS);

  let timeout = envTimeout ?? mockDefaultConfig.network.timeout;
  timeout = Math.max(5000, Math.min(300000, timeout));

  let maxRetries = envMaxRetries ?? mockDefaultConfig.network.maxRetries;
  maxRetries = Math.max(0, Math.min(10, maxRetries));

  return {
    ...mockDefaultConfig,
    github: {
      apiUrl: envApiUrl || mockDefaultConfig.github.apiUrl,
    },
    local: {
      ...mockDefaultConfig.local,
      enabled: envEnableLocal ?? mockDefaultConfig.local.enabled,
      enableClone: envEnableClone ?? mockDefaultConfig.local.enableClone,
      allowedPaths:
        mockParseStringArrayEnv(process.env.ALLOWED_PATHS) ??
        mockDefaultConfig.local.allowedPaths,
    },
    tools: {
      enabled: envToolsToRun ?? mockDefaultConfig.tools.enabled,
      enableAdditional:
        envEnableTools ?? mockDefaultConfig.tools.enableAdditional,
      disabled: envDisableTools ?? mockDefaultConfig.tools.disabled,
    },
    network: {
      timeout,
      maxRetries,
    },
    telemetry: {
      logging: envLogging ?? mockDefaultConfig.telemetry.logging,
    },
  };
};

vi.mock('octocode-shared', () => ({
  getConfigSync: vi.fn(() => buildMockConfig()),
  getConfig: vi.fn(async () => buildMockConfig()),
  _resetSessionState: vi.fn(() => {
    sessionMockState.sessionId = generateMockUUID();
    sessionMockState.deleted = false;
  }),
  getOrCreateSession: vi.fn(() => {
    if (sessionMockState.deleted) {
      sessionMockState.sessionId = generateMockUUID();
      sessionMockState.deleted = false;
    }
    return {
      version: 1,
      sessionId: sessionMockState.sessionId,
      createdAt: '2024-01-01T00:00:00.000Z',
      lastActiveAt: '2024-01-01T00:00:00.000Z',
      stats: { toolCalls: 0, errors: 0, rateLimits: 0 },
    };
  }),
  incrementToolCalls: vi.fn(() => ({ success: true })),
  incrementErrors: vi.fn(() => ({ success: true })),
  incrementRateLimits: vi.fn(() => ({ success: true })),
  updateSessionStats: vi.fn(() => ({ success: true })),
  incrementRateLimitByProvider: vi.fn(() => ({ success: true })),
  incrementToolCharSavings: vi.fn(() => ({ success: true })),
  incrementGitHubCacheHits: vi.fn(() => ({ success: true })),
  incrementGitHubCacheRateLimits: vi.fn(() => ({ success: true })),
  incrementPackageRegistryFailures: vi.fn(() => ({ success: true })),
  deleteSession: vi.fn(() => {
    sessionMockState.deleted = true;
    return true;
  }),
  ensureOctocodeDir: vi.fn(),
  OCTOCODE_DIR: '/mock/.octocode',
  getOctocodeDir: vi.fn(() => '/mock/.octocode'),
  getOctocodeToken: vi.fn().mockResolvedValue(null),
  getToken: vi.fn().mockResolvedValue(null),
  getTokenFromEnv: vi.fn(() => {
    const envVars = ['OCTOCODE_TOKEN', 'GH_TOKEN', 'GITHUB_TOKEN'];
    for (const v of envVars) {
      if (process.env[v]) return process.env[v];
    }
    return null;
  }),
  getEnvTokenSource: vi.fn(() => {
    if (process.env.OCTOCODE_TOKEN) return 'env:OCTOCODE_TOKEN';
    if (process.env.GH_TOKEN) return 'env:GH_TOKEN';
    if (process.env.GITHUB_TOKEN) return 'env:GITHUB_TOKEN';
    return null;
  }),
  resolveTokenFull: vi.fn(async () => {
    const envVars: Array<[string, string]> = [
      ['OCTOCODE_TOKEN', 'env:OCTOCODE_TOKEN'],
      ['GH_TOKEN', 'env:GH_TOKEN'],
      ['GITHUB_TOKEN', 'env:GITHUB_TOKEN'],
    ];
    for (const [envVar, source] of envVars) {
      const token = process.env[envVar];
      if (token) return { token, source, wasRefreshed: false };
    }
    return null;
  }),
  invalidateConfigCache: vi.fn(),
  _resetConfigCache: vi.fn(),
  getDirectorySizeBytes: vi.fn(() => 0),
  formatBytes: vi.fn((b: number) => `${b} B`),
}));

export { sessionMockState };

const mockToolHints = {
  hasResults: ['Test hint for hasResults 1', 'Test hint for hasResults 2'],
  empty: ['Test hint for empty 1', 'Test hint for empty 2'],
};

const mockToolSchema = {
  name: 'mockTool',
  description: 'Mock tool for testing',
  schema: {},
  hints: mockToolHints,
};

const githubFetchContentSchema = {
  name: 'ghGetFileContent',
  description: 'Read file content from GitHub',
  schema: {
    owner: 'GitHub owner',
    repo: 'Repository name',
    branch: 'Branch name',
    path: 'File path',
    startLine: 'Start line number',
    endLine: 'End line number',
    fullContent: 'Fetch full content',
    matchString: 'Match string',
    parameterConflict:
      'parameterConflict: Cannot use fullContent with other range parameters',
    lineRangeMismatch:
      'lineRangeMismatch: startLine and endLine must be used together',
  },
  hints: mockToolHints,
};

const mockDynamicHints = {
  parallelTip: ['Use parallel queries for faster results'],
  multipleFiles: ['Multiple files found - use localGetFileContent to read'],
  grepFallback: ['Using grep fallback (ripgrep unavailable)'],
  grepFallbackEmpty: ['Try with ripgrep for better results'],
  nodeModulesSearch: [
    'Consider searching in packages/ instead of node_modules',
  ],
  largeResult: ['Large result set - narrow your search'],
  largeFile: ['Use matchString or charLength for large files'],
  patternTooBroad: ['Pattern matched too many results - narrow it'],
  parallelize: ['Parallelize: multiple directories found'],
  largeDirectory: ['Use entriesPerPage to paginate large directories'],
  batchParallel: ['Use parallel queries for batch operations'],
  manyResults: ['Many results found - consider filtering'],
  configFiles: ['Config files found - check for project settings'],
  singleRepo: ['Searching single repo: use ghGetFileContent for details'],
  multiRepo: ['Searching multiple repos: narrow with owner/repo'],
  pathEmpty: ['Path search empty - try match="file" instead'],
  crossRepoEmpty: ['Cross-repo search empty - specify owner/repo'],
  fileTooLarge: ['File too large - use matchString or line range'],
};

const lspGetSemanticsSchema = {
  name: 'lspGetSemantics',
  description: 'Get semantic code intelligence using Language Server Protocol',
  schema: {
    uri: 'File URI',
    type: 'Semantic content type',
    symbolName: 'Symbol name to find',
    lineHint: 'Line number hint',
    orderHint: 'Order hint for multiple occurrences',
    itemsPerPage: 'Semantic items per page',
    contextLines: 'Lines of context to include',
  },
  hints: {
    ...mockToolHints,
    dynamic: {
      semanticContent: ['Semantic content returned'],
      symbolNotFound: ['Symbol not found - verify name and lineHint'],
      fileNotFound: ['File not found - check path'],
      timeout: ['LSP timeout - try again or use localSearchCode'],
    },
  },
};

const localRipgrepSchema = {
  name: 'localSearchCode',
  description: 'Search code with ripgrep',
  schema: {},
  hints: {
    ...mockToolHints,
    dynamic: mockDynamicHints,
  },
};

const localFetchContentSchema = {
  name: 'localGetFileContent',
  description: 'Read local file content',
  schema: {},
  hints: {
    ...mockToolHints,
    dynamic: mockDynamicHints,
  },
};

const localViewStructureSchema = {
  name: 'localViewStructure',
  description: 'Browse local directory structure',
  schema: {},
  hints: {
    ...mockToolHints,
    dynamic: mockDynamicHints,
  },
};

const localFindFilesSchema = {
  name: 'localFindFiles',
  description: 'Find files by metadata',
  schema: {},
  hints: {
    ...mockToolHints,
    dynamic: mockDynamicHints,
  },
};

const ghSearchCodeSchema = {
  name: 'ghSearchCode',
  description: 'Search code across GitHub',
  schema: {},
  hints: {
    ...mockToolHints,
    dynamic: mockDynamicHints,
  },
};

const mockContent = {
  instructions: 'Test instructions',
  prompts: {},
  toolNames: {
    GITHUB_FETCH_CONTENT: 'ghGetFileContent',
    GITHUB_SEARCH_CODE: 'ghSearchCode',
    GITHUB_SEARCH_PULL_REQUESTS: 'ghHistoryResearch',
    GITHUB_SEARCH_REPOSITORIES: 'ghSearchRepos',
    GITHUB_VIEW_REPO_STRUCTURE: 'ghViewRepoStructure',
    PACKAGE_SEARCH: 'npmSearch',
    GITHUB_CLONE_REPO: 'ghCloneRepo',
    LOCAL_RIPGREP: 'localSearchCode',
    LOCAL_FETCH_CONTENT: 'localGetFileContent',
    LOCAL_FIND_FILES: 'localFindFiles',
    LOCAL_VIEW_STRUCTURE: 'localViewStructure',
    LSP_GET_SEMANTIC_CONTENT: 'lspGetSemantics',
    GITHUB_HISTORY: 'ghHistorySearch',
    LOCAL_BINARY_INSPECT: 'localBinaryInspect',
  },
  baseSchema: {
    id: 'Stable query identifier.',
    mainResearchGoal: 'Main research goal description',
    researchGoal: 'Research goal description',
    reasoning: 'Reasoning description',
  },
  tools: {
    ghGetFileContent: githubFetchContentSchema,
    ghSearchCode: ghSearchCodeSchema,
    ghHistoryResearch: mockToolSchema,
    ghSearchRepos: mockToolSchema,
    ghViewRepoStructure: mockToolSchema,
    npmSearch: mockToolSchema,
    ghCloneRepo: {
      name: 'ghCloneRepo',
      description: 'Clone GitHub repository to local filesystem',
      schema: {
        owner: 'Repository owner (user or org)',
        repo: 'Repository name',
        branch: 'Branch/tag/SHA to clone',
        sparsePath: 'Fetch only this subdirectory (sparse checkout)',
        forceRefresh: 'Bypass cache and force a fresh clone',
        charOffset: 'Character offset for output pagination',
        charLength: 'Character budget for output pagination',
      },
      hints: mockToolHints,
    },
    localSearchCode: localRipgrepSchema,
    localGetFileContent: localFetchContentSchema,
    localFindFiles: localFindFilesSchema,
    localViewStructure: localViewStructureSchema,
    lspGetSemantics: lspGetSemanticsSchema,
    ghHistorySearch: mockToolSchema,
    localBinaryInspect: mockToolSchema,
  },
  baseHints: {
    hasResults: ['Base hint for hasResults'],
    empty: ['Base hint for empty'],
  },
  genericErrorHints: [
    'Generic error hint 1',
    'Generic error hint 2',
    'Generic error hint 3',
    'Generic error hint 4',
    'Generic error hint 5',
  ],
  bulkOperations: {
    instructions: {
      base: 'Bulk response with {count} results: {counts}',
      hasResults: 'Review hasResultsStatusHints for guidance',
      empty: 'Review emptyStatusHints for no-results scenarios',
      error: 'Review errorStatusHints for error recovery',
    },
  },
};

const _coreMock = vi.hoisted(() => ({ ref: null as unknown }));
vi.mock('@octocodeai/octocode-core', async importOriginal => {
  const actual =
    await importOriginal<typeof import('@octocodeai/octocode-core')>();

  const { z } = await import('zod');
  const passthrough = () =>
    z.looseObject({
      charOffset: z.number().optional().default(0),
    });
  const identityValidator = <T>(v: T) => v;
  const stubBulkSchema = () => z.looseObject({ queries: z.array(z.unknown()) });

  const schemaStubs = {
    RipgrepQuerySchema: passthrough(),
    BulkRipgrepQuerySchema: stubBulkSchema(),
    FindFilesQuerySchema: passthrough(),
    BulkFindFilesQuerySchema: stubBulkSchema(),
    ViewStructureQuerySchema: passthrough(),
    BulkViewStructureQuerySchema: stubBulkSchema(),
    FetchContentQuerySchema: passthrough(),
    FetchContentBulkQuerySchema: stubBulkSchema(),
    LocalSearchCodeOutputSchema: passthrough(),
    LocalFindFilesOutputSchema: passthrough(),
    LocalViewStructureOutputSchema: passthrough(),
    LocalGetFileContentOutputSchema: passthrough(),
    FileContentQuerySchema: passthrough(),
    FileContentBulkQuerySchema: stubBulkSchema(),
    GitHubCodeSearchQuerySchema: passthrough(),
    GitHubCodeSearchBulkQuerySchema: stubBulkSchema(),
    GitHubCodeSearchOutputSchema: passthrough(),
    GitHubReposSearchQuerySchema: passthrough(),
    GitHubReposSearchSingleQuerySchema: passthrough(),
    GitHubReposSearchBulkQuerySchema: stubBulkSchema(),
    GitHubSearchRepositoriesOutputSchema: passthrough(),
    GitHubPullRequestSearchQuerySchema: passthrough(),
    GitHubPullRequestSearchBulkQuerySchema: stubBulkSchema(),
    GitHubSearchPullRequestsOutputSchema: passthrough(),
    GitHubViewRepoStructureQuerySchema: passthrough(),
    GitHubViewRepoStructureBulkQuerySchema: stubBulkSchema(),
    GitHubViewRepoStructureOutputSchema: passthrough(),
    BulkCloneRepoSchema: stubBulkSchema(),
    GitHubCloneRepoOutputSchema: passthrough(),
    NpmPackageQuerySchema: passthrough(),
    NpmSearchBulkQuerySchema: stubBulkSchema(),
    NpmSearchOutputSchema: passthrough(),
    BaseQuerySchema: passthrough(),
    BaseQuerySchemaLocal: passthrough(),
    ErrorDataSchema: passthrough(),
    BulkFetchContentSchema: stubBulkSchema(),
    BulkViewStructureSchema: stubBulkSchema(),
    BulkFindFilesSchema: stubBulkSchema(),
    GITHUB_FETCH_CONTENT: 'ghGetFileContent',
    GITHUB_SEARCH_CODE: 'ghSearchCode',
    GITHUB_SEARCH_PULL_REQUESTS: 'ghHistoryResearch',
    GITHUB_SEARCH_REPOSITORIES: 'ghSearchRepos',
    GITHUB_VIEW_REPO_STRUCTURE: 'ghViewRepoStructure',
    GITHUB_CLONE_REPO: 'ghCloneRepo',
    PACKAGE_SEARCH: 'npmSearch',
    LOCAL_RIPGREP: 'localSearchCode',
    LOCAL_FETCH_CONTENT: 'localGetFileContent',
    LOCAL_FIND_FILES: 'localFindFiles',
    LOCAL_VIEW_STRUCTURE: 'localViewStructure',
    LSP_GET_SEMANTIC_CONTENT: 'lspGetSemantics',
    validateRipgrepQuery: identityValidator,
    validateFindFilesQuery: identityValidator,
    validateViewStructureQuery: identityValidator,
    validateFetchContentQuery: identityValidator,
    applyWorkflowMode: identityValidator,
    createBulkQuerySchema: stubBulkSchema,
    NpmSearchQuerySchema: passthrough(),
    LocalSearchCodeDataSchema: passthrough(),
    LocalFindFilesDataSchema: passthrough(),
    LocalViewStructureDataSchema: passthrough(),
    LocalGetFileContentDataSchema: passthrough(),
    GitHubSearchCodeDataSchema: passthrough(),
    GitHubGetFileContentDataSchema: passthrough(),
    GitHubSearchPullRequestsDataSchema: passthrough(),
    GitHubSearchRepositoriesDataSchema: passthrough(),
    GitHubViewRepoStructureDataSchema: passthrough(),
    GitHubCloneRepoDataSchema: passthrough(),
    NpmSearchDataSchema: passthrough(),
  };

  return {
    ...actual,
    ...schemaStubs,
    get octocodeConfig() {
      return _coreMock.ref;
    },
    get completeMetadata() {
      return {
        ...(_coreMock.ref as Record<string, unknown>),
        baseSchema: actual.completeMetadata.baseSchema,
        tools: actual.completeMetadata.tools,
      };
    },
  };
});

vi.mock('child_process', () => ({
  spawn: vi.fn(),
}));

const enforceWarningFreeTests =
  process.env.OCTOCODE_ENFORCE_WARNING_FREE_TESTS === '1';

interface CapturedWarning {
  source: 'console.warn' | 'process.emitWarning' | 'process.stderr.write';
  message: string;
}

let capturedWarnings: CapturedWarning[] = [];

function formatWarningMessage(value: unknown): string {
  if (value instanceof Error) {
    return value.message;
  }

  if (typeof value === 'string') {
    return value;
  }

  return String(value);
}

function captureProcessWarning(warning: string | Error): void {
  capturedWarnings.push({
    source: 'process.emitWarning',
    message: formatWarningMessage(warning),
  });
}

function captureStderrWrite(chunk: string | Uint8Array): boolean {
  const message =
    typeof chunk === 'string' ? chunk : Buffer.from(chunk).toString('utf8');

  if (consumeExpectedStderrWarning(message)) {
    return true;
  }

  capturedWarnings.push({
    source: 'process.stderr.write',
    message: message.trimEnd(),
  });

  return true;
}

_coreMock.ref = mockContent;

beforeEach(() => {
  sessionMockState.sessionId = generateMockUUID();
  sessionMockState.deleted = false;
  capturedWarnings = [];
  resetExpectedStderrWarnings();
  resetCircuitBreaker();

  if (enforceWarningFreeTests) {
    vi.spyOn(console, 'warn').mockImplementation((...args: unknown[]) => {
      capturedWarnings.push({
        source: 'console.warn',
        message: args.map(formatWarningMessage).join(' '),
      });
    });
    vi.spyOn(process, 'emitWarning').mockImplementation(
      captureProcessWarning as typeof process.emitWarning
    );
    vi.spyOn(process.stderr, 'write').mockImplementation(
      captureStderrWrite as typeof process.stderr.write
    );
  }

  if (!process.env.VITEST_DEBUG && !enforceWarningFreeTests) {
    vi.spyOn(console, 'log').mockImplementation(() => {});
    vi.spyOn(console, 'warn').mockImplementation(() => {});
    vi.spyOn(console, 'error').mockImplementation(() => {});
  }
});

afterEach(() => {
  sessionMockState.sessionId = generateMockUUID();
  sessionMockState.deleted = false;

  if (
    enforceWarningFreeTests &&
    capturedWarnings.length > 0 &&
    !shouldSuppressUnexpectedWarningFailure()
  ) {
    const warnings = capturedWarnings
      .map(warning => `${warning.source}: ${warning.message}`)
      .join('\n');

    throw new Error(
      `Unexpected warning emitted during contract test.\n${warnings}`
    );
  }
});

afterAll(() => {
  vi.restoreAllMocks();
});

process.env.NODE_ENV = 'test';
process.env.VITEST_TEST_MODE = '1';
process.env.GITHUB_TOKEN = 'test-token-for-vitest';

const originalUnhandledRejection = process.listeners('unhandledRejection');
const originalUncaughtException = process.listeners('uncaughtException');

process.removeAllListeners('unhandledRejection');
process.removeAllListeners('uncaughtException');

process.on('unhandledRejection', (reason, promise) => {
  if (
    reason instanceof Error &&
    reason.message.includes('process.exit called with code')
  ) {
    return;
  }

  if (
    reason instanceof Error &&
    (reason.message.includes('always fails') ||
      reason.message.includes('non-retryable error') ||
      reason.message.includes('retryable error'))
  ) {
    return;
  }

  originalUnhandledRejection.forEach(handler => {
    if (typeof handler === 'function') {
      handler(reason, promise);
    }
  });
});

process.on('uncaughtException', error => {
  if (error.message.includes('process.exit called with code')) {
    return;
  }

  originalUncaughtException.forEach(handler => {
    if (typeof handler === 'function') {
      handler(error, 'uncaughtException');
    }
  });
});
