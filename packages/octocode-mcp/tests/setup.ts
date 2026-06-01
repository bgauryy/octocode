import { beforeEach, afterEach, afterAll, vi } from 'vitest';
import { initializeToolMetadata } from '../src/tools/toolMetadata/state.js';
import { resetCircuitBreaker } from '../src/utils/http/circuitBreaker.js';
import {
  consumeExpectedStderrWarning,
  resetExpectedStderrWarnings,
  shouldSuppressUnexpectedWarningFailure,
} from './warningPolicy.js';

// Increase max listeners to avoid warnings in test environments
// Tests may legitimately register many listeners due to module isolation
process.setMaxListeners(50);

// Session telemetry and other code paths call global fetch; tests configure via vi.mocked(fetch)
vi.stubGlobal(
  'fetch',
  vi.fn(() => Promise.resolve(new Response('', { status: 200 })))
);

// Global mock for octocode-shared to prevent filesystem access during tests
// This is needed because some tests use vi.resetModules() which can break file-level mocks

// Generate a mock UUID v4 format (36 characters: xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx)
const generateMockUUID = () => {
  const hex = '0123456789abcdef';
  let uuid = '';
  for (let i = 0; i < 36; i++) {
    if (i === 8 || i === 13 || i === 18 || i === 23) {
      uuid += '-';
    } else if (i === 14) {
      uuid += '4'; // Version 4
    } else if (i === 19) {
      uuid += hex[(Math.random() * 4) | 8]; // Variant
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

// Default config mock for getConfigSync
// Note: This mock matches the actual ResolvedConfig structure after dead code cleanup
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

// Env var parsing helpers (mirrors octocode-shared resolverSections.ts behavior)
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

// Helper to build config dynamically (mirrors shared module's resolver behavior)
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
  // Global config mock - re-evaluates ENABLE_LOCAL on each call
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

// Export for tests that need to access the mock state
export { sessionMockState };

// Minimal mock content for tests - metadata is fetched from API in production
// Schema requires: instructions, prompts, toolNames, baseSchema, tools, baseHints, genericErrorHints
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

// githubGetFileContent needs specific schema fields for validation messages
// Schema is a flat object with string values
const githubFetchContentSchema = {
  name: 'githubGetFileContent',
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

// Dynamic hints mock for local tools
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
  singleRepo: ['Searching single repo: use githubGetFileContent for details'],
  multiRepo: ['Searching multiple repos: narrow with owner/repo'],
  pathEmpty: ['Path search empty - try match="file" instead'],
  crossRepoEmpty: ['Cross-repo search empty - specify owner/repo'],
  fileTooLarge: ['File too large - use matchString or line range'],
};

// LSP tool schema with descriptions
const lspGotoDefinitionSchema = {
  name: 'lspGotoDefinition',
  description: 'Navigate to symbol definition using Language Server Protocol',
  schema: {
    uri: 'File URI',
    symbolName: 'Symbol name to find',
    lineHint: 'Line number hint',
    orderHint: 'Order hint for multiple occurrences',
    contextLines: 'Lines of context to include',
  },
  hints: {
    ...mockToolHints,
    dynamic: {
      multipleDefinitions: ['Multiple definitions found - check all locations'],
      externalPackage: ['Definition in external package - use packageSearch'],
      fallbackMode: ['Using text-based fallback (LSP unavailable)'],
      symbolNotFound: ['Symbol not found - verify name and lineHint'],
      fileNotFound: ['File not found - check path'],
      timeout: ['LSP timeout - try again or use localSearchCode'],
    },
  },
};

const lspFindReferencesSchema = {
  name: 'lspFindReferences',
  description: 'Find all references to a symbol using Language Server Protocol',
  schema: {
    uri: 'File URI',
    symbolName: 'Symbol name to find',
    lineHint: 'Line number hint',
    orderHint: 'Order hint',
    includeDeclaration: 'Include declaration in results',
    contextLines: 'Lines of context',
    referencesPerPage: 'References per page',
    page: 'Page number',
  },
  hints: {
    ...mockToolHints,
    dynamic: {
      manyReferences: ['Many references - use pagination'],
      multipleFiles: ['References span multiple files'],
      pagination: ['More results available - increment page'],
      fallbackMode: ['Using text-based fallback (LSP unavailable)'],
      symbolNotFound: ['Symbol not found - verify name and lineHint'],
      timeout: ['LSP timeout - try localSearchCode instead'],
    },
  },
};

const lspCallHierarchySchema = {
  name: 'lspCallHierarchy',
  description: 'Explore function call hierarchy using Language Server Protocol',
  schema: {
    uri: 'File URI',
    symbolName: 'Symbol name',
    lineHint: 'Line number hint',
    orderHint: 'Order hint',
    direction: 'incoming or outgoing',
    depth: 'Call chain depth',
    contextLines: 'Lines of context',
    callsPerPage: 'Calls per page',
    page: 'Page number',
  },
  hints: {
    ...mockToolHints,
    dynamic: {
      incomingResults: ['Found callers - trace the call chain'],
      outgoingResults: ['Found callees - explore dependencies'],
      deepChain: ['Deep call chain - use depth=1 for performance'],
      pagination: ['More calls available - increment page'],
      fallbackMode: ['Using pattern-based fallback (LSP unavailable)'],
      noCallers: ['No callers found - function may be entry point'],
      noCallees: ['No callees found - function is leaf node'],
      notAFunction: ['Symbol is not a function - use lspFindReferences'],
      timeout: ['LSP timeout - reduce depth or use localSearchCode'],
    },
  },
};

// Enhanced local tool schemas with dynamic hints
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

// Enhanced GitHub tool schemas with dynamic hints
const githubSearchCodeSchema = {
  name: 'githubSearchCode',
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
    GITHUB_FETCH_CONTENT: 'githubGetFileContent',
    GITHUB_SEARCH_CODE: 'githubSearchCode',
    GITHUB_SEARCH_PULL_REQUESTS: 'githubSearchPullRequests',
    GITHUB_SEARCH_REPOSITORIES: 'githubSearchRepositories',
    GITHUB_VIEW_REPO_STRUCTURE: 'githubViewRepoStructure',
    PACKAGE_SEARCH: 'packageSearch',
    GITHUB_CLONE_REPO: 'githubCloneRepo',
    LOCAL_RIPGREP: 'localSearchCode',
    LOCAL_FETCH_CONTENT: 'localGetFileContent',
    LOCAL_FIND_FILES: 'localFindFiles',
    LOCAL_VIEW_STRUCTURE: 'localViewStructure',
    LSP_GOTO_DEFINITION: 'lspGotoDefinition',
    LSP_FIND_REFERENCES: 'lspFindReferences',
    LSP_CALL_HIERARCHY: 'lspCallHierarchy',
  },
  baseSchema: {
    mainResearchGoal: 'Main research goal description',
    researchGoal: 'Research goal description',
    reasoning: 'Reasoning description',
    bulkQuery: (toolName: string) =>
      `Research queries for ${toolName} (1-3 queries per call for optimal resource management). Review schema before use for optimal results`,
  },
  tools: {
    githubGetFileContent: githubFetchContentSchema,
    githubSearchCode: githubSearchCodeSchema,
    githubSearchPullRequests: mockToolSchema,
    githubSearchRepositories: mockToolSchema,
    githubViewRepoStructure: mockToolSchema,
    packageSearch: mockToolSchema,
    githubCloneRepo: {
      name: 'githubCloneRepo',
      description: 'Clone GitHub repository to local filesystem',
      schema: {
        owner: 'Repository owner (user or org)',
        repo: 'Repository name',
        branch: 'Branch/tag/SHA to clone',
        sparse_path: 'Fetch only this subdirectory (sparse checkout)',
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
    lspGotoDefinition: lspGotoDefinitionSchema,
    lspFindReferences: lspFindReferencesSchema,
    lspCallHierarchy: lspCallHierarchySchema,
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

// Mock @octocodeai/octocode-core for metadata loading (no HTTP fetch).
// vi.mock factories are hoisted and run before mockContent is initialized,
// so we use a mutable holder set via vi.hoisted + a getter for lazy access.
const _coreMock = vi.hoisted(() => ({ ref: null as unknown }));
vi.mock('@octocodeai/octocode-core', async importOriginal => {
  const actual =
    await importOriginal<typeof import('@octocodeai/octocode-core')>();

  // The installed core@1.0.2 only exports octocodeConfig + completeMetadata,
  // but src/ imports many additional schemas/validators/constants that exist
  // in newer builds of the host repo. We stub them here with minimal Zod
  // shapes so the test imports resolve. Tests that need richer behavior
  // re-mock specific symbols locally.
  const { z } = await import('zod/v4');
  // Stubs need `.shape.charOffset`, `.shape.queries`, etc. accessed by the
  // overlay code in src/scheme/. Include the keys that are actually read.
  const passthrough = () =>
    z
      .object({
        charOffset: z.number().optional().default(0),
      })
      .passthrough();
  const identityValidator = <T>(v: T) => v;
  const stubBulkSchema = () =>
    z.object({ queries: z.array(z.unknown()) }).passthrough();

  const schemaStubs = {
    // Local tools
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
    // GitHub tools
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
    // Package search
    NpmPackageQuerySchema: passthrough(),
    PackageSearchBulkQuerySchema: stubBulkSchema(),
    PackageSearchOutputSchema: passthrough(),
    // LSP
    LSPGotoDefinitionQuerySchema: passthrough(),
    BulkLSPGotoDefinitionSchema: stubBulkSchema(),
    BulkLSPGotoDefinitionQuerySchema: stubBulkSchema(),
    LspGotoDefinitionOutputSchema: passthrough(),
    LSPFindReferencesQuerySchema: passthrough(),
    BulkLSPFindReferencesSchema: stubBulkSchema(),
    BulkLSPFindReferencesQuerySchema: stubBulkSchema(),
    LspFindReferencesOutputSchema: passthrough(),
    LSPCallHierarchyQuerySchema: passthrough(),
    BulkLSPCallHierarchySchema: stubBulkSchema(),
    BulkLSPCallHierarchyQuerySchema: stubBulkSchema(),
    LspCallHierarchyOutputSchema: passthrough(),
    // Base / shared
    BaseQuerySchema: passthrough(),
    BaseQuerySchemaLocal: passthrough(),
    ErrorDataSchema: passthrough(),
    BulkFetchContentSchema: stubBulkSchema(),
    BulkViewStructureSchema: stubBulkSchema(),
    BulkFindFilesSchema: stubBulkSchema(),
    // Tool name constants (used by registration)
    GITHUB_FETCH_CONTENT: 'githubGetFileContent',
    GITHUB_SEARCH_CODE: 'githubSearchCode',
    GITHUB_SEARCH_PULL_REQUESTS: 'githubSearchPullRequests',
    GITHUB_SEARCH_REPOSITORIES: 'githubSearchRepositories',
    GITHUB_VIEW_REPO_STRUCTURE: 'githubViewRepoStructure',
    GITHUB_CLONE_REPO: 'githubCloneRepo',
    PACKAGE_SEARCH: 'packageSearch',
    LOCAL_RIPGREP: 'localSearchCode',
    LOCAL_FETCH_CONTENT: 'localGetFileContent',
    LOCAL_FIND_FILES: 'localFindFiles',
    LOCAL_VIEW_STRUCTURE: 'localViewStructure',
    LSP_GOTO_DEFINITION: 'lspGotoDefinition',
    LSP_FIND_REFERENCES: 'lspFindReferences',
    LSP_CALL_HIERARCHY: 'lspCallHierarchy',
    // Validators (identity — runtime checks delegated to overlay schemas)
    validateRipgrepQuery: identityValidator,
    validateFindFilesQuery: identityValidator,
    validateViewStructureQuery: identityValidator,
    validateFetchContentQuery: identityValidator,
    applyWorkflowMode: identityValidator,
    createBulkQuerySchema: stubBulkSchema,
    // Description constants
    LOCAL_RIPGREP_DESCRIPTION: 'localSearchCode',
    LOCAL_FIND_FILES_DESCRIPTION: 'localFindFiles',
    LOCAL_VIEW_STRUCTURE_DESCRIPTION: 'localViewStructure',
    LOCAL_FETCH_CONTENT_DESCRIPTION: 'localGetFileContent',
    LSP_GOTO_DEFINITION_DESCRIPTION: 'lspGotoDefinition',
    LSP_FIND_REFERENCES_DESCRIPTION: 'lspFindReferences',
    LSP_CALL_HIERARCHY_DESCRIPTION: 'lspCallHierarchy',
    GITHUB_CLONE_REPO_DESCRIPTION: 'githubCloneRepo',
    // Additional schemas
    PackageSearchQuerySchema: passthrough(),
    LocalSearchCodeDataSchema: passthrough(),
    LocalFindFilesDataSchema: passthrough(),
    LocalViewStructureDataSchema: passthrough(),
    LocalGetFileContentDataSchema: passthrough(),
    LspGotoDefinitionDataSchema: passthrough(),
    LspFindReferencesDataSchema: passthrough(),
    LspCallHierarchyDataSchema: passthrough(),
    GitHubSearchCodeDataSchema: passthrough(),
    GitHubGetFileContentDataSchema: passthrough(),
    GitHubSearchPullRequestsDataSchema: passthrough(),
    GitHubSearchRepositoriesDataSchema: passthrough(),
    GitHubViewRepoStructureDataSchema: passthrough(),
    GitHubCloneRepoDataSchema: passthrough(),
    PackageSearchDataSchema: passthrough(),
  };

  return {
    ...actual,
    ...schemaStubs,
    get octocodeConfig() {
      return _coreMock.ref;
    },
    get completeMetadata() {
      return _coreMock.ref;
    },
  };
});

// Mock child_process for exec utilities - MUST be done before any exec imports
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

// Wire mock content into the @octocodeai/octocode-core mock before initialization
_coreMock.ref = mockContent;

// Initialize tool metadata for all tests - using top-level await to ensure it runs before test file imports
await initializeToolMetadata();

// Mock console methods to avoid noise during tests
beforeEach(() => {
  // Reset session mock state with a new UUID
  sessionMockState.sessionId = generateMockUUID();
  sessionMockState.deleted = false;
  capturedWarnings = [];
  resetExpectedStderrWarnings();
  // #T13: clear circuit-breaker state so a host that tripped in one test
  // doesn't fail-fast unrelated tests sharing the same host.
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

  // Only mock if not in debug mode
  if (!process.env.VITEST_DEBUG && !enforceWarningFreeTests) {
    vi.spyOn(console, 'log').mockImplementation(() => {});
    vi.spyOn(console, 'warn').mockImplementation(() => {});
    vi.spyOn(console, 'error').mockImplementation(() => {});
  }
});

afterEach(() => {
  // Clean up session mock state with a new UUID for next test
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

// Global test environment setup
process.env.NODE_ENV = 'test';
process.env.VITEST_TEST_MODE = '1';
// Set a default GitHub token to prevent "No GitHub token available" warnings during tests
// Tests that need to verify "no token" behavior should explicitly delete this
process.env.GITHUB_TOKEN = 'test-token-for-vitest';

// Suppress expected unhandled errors from process.exit() mocking in index tests
// These are expected behavior when testing process termination scenarios
const originalUnhandledRejection = process.listeners('unhandledRejection');
const originalUncaughtException = process.listeners('uncaughtException');

process.removeAllListeners('unhandledRejection');
process.removeAllListeners('uncaughtException');

process.on('unhandledRejection', (reason, promise) => {
  // Only suppress errors that are from our process.exit mocking
  if (
    reason instanceof Error &&
    reason.message.includes('process.exit called with code')
  ) {
    // This is expected from our index.test.ts process.exit mocking - ignore it
    return;
  }

  // Suppress expected unhandled rejections from promiseUtils test mocks
  if (
    reason instanceof Error &&
    (reason.message.includes('always fails') ||
      reason.message.includes('non-retryable error') ||
      reason.message.includes('retryable error'))
  ) {
    // These are expected from promiseUtils.test.ts retry testing - ignore them
    return;
  }

  // For any other unhandled rejections, call the original handlers
  originalUnhandledRejection.forEach(handler => {
    if (typeof handler === 'function') {
      handler(reason, promise);
    }
  });
});

process.on('uncaughtException', error => {
  // Only suppress errors that are from our process.exit mocking
  if (error.message.includes('process.exit called with code')) {
    // This is expected from our index.test.ts process.exit mocking - ignore it
    return;
  }

  // For any other uncaught exceptions, call the original handlers
  originalUncaughtException.forEach(handler => {
    if (typeof handler === 'function') {
      handler(error, 'uncaughtException');
    }
  });
});
