import { withCircuitBreaker } from './circuitBreaker.js';
import { withRetry, RETRY_CONFIGS } from './retry.js';
import { withTimeout } from './asyncTimeout.js';


const DEFAULT_TOOL_TIMEOUT_MS = 30000;


const TIMEOUT_CONFIGS = {
  github: 60000,
  local: 30000,
  lsp: 30000,
  package: 30000,
} as const;


const TOOL_CIRCUIT_MAP: Record<string, string> = {
  githubSearchCode: 'github:search',
  githubSearchRepositories: 'github:search',
  githubSearchPullRequests: 'github:pulls',
  githubGetFileContent: 'github:content',
  githubViewRepoStructure: 'github:content',

  lspGotoDefinition: 'lsp:navigation',
  lspFindReferences: 'lsp:navigation',
  lspCallHierarchy: 'lsp:hierarchy',

  localSearchCode: 'local',
  localGetFileContent: 'local',
  localFindFiles: 'local',
  localViewStructure: 'local',

  packageSearch: 'package',
};


const RESILIENCE_CONFIGS = {
  github: {
    retry: RETRY_CONFIGS.github,
  },
  local: {
    retry: RETRY_CONFIGS.local,
  },
  lsp: {
    retry: RETRY_CONFIGS.lsp,
  },
  package: {
    retry: RETRY_CONFIGS.package,
  },
} as const;

type ResilienceCategory = keyof typeof RESILIENCE_CONFIGS;

async function withResilience<T>(
  category: ResilienceCategory,
  operation: () => Promise<T>,
  context?: { tool: string }
): Promise<T> {
  const config = RESILIENCE_CONFIGS[category];
  const timeoutMs = TIMEOUT_CONFIGS[category] || DEFAULT_TOOL_TIMEOUT_MS;
  const toolName = context?.tool || category;

  const circuitName = TOOL_CIRCUIT_MAP[toolName] || category;

  return withTimeout(
    () => withCircuitBreaker(circuitName, async () => {
      return withRetry(operation, config.retry, context);
    }),
    timeoutMs,
    `${toolName}:timeout`
  );
}


export async function withGitHubResilience<T>(
  operation: () => Promise<T>,
  toolName: string
): Promise<T> {
  return withResilience('github', operation, { tool: toolName });
}


export async function withLspResilience<T>(
  operation: () => Promise<T>,
  toolName: string
): Promise<T> {
  return withResilience('lsp', operation, { tool: toolName });
}


export async function withLocalResilience<T>(
  operation: () => Promise<T>,
  toolName: string
): Promise<T> {
  return withResilience('local', operation, { tool: toolName });
}


export async function withPackageResilience<T>(
  operation: () => Promise<T>,
  toolName: string
): Promise<T> {
  return withResilience('package', operation, { tool: toolName });
}
