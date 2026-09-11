import {
  DIRECT_TOOL_SPECIFICATIONS,
  type DirectToolSpecification,
} from '@octocodeai/octocode-core/schema';
import { LSP_SEARCH_TOOL_NAME } from '@octocodeai/octocode-core/schema';
import {
  GITHUB_SEARCH_TOOL_NAME,
  GITHUB_GET_HISTORY_ITEM_TOOL_NAME,
  GITHUB_SEARCH_HISTORY_TOOL_NAME,
  AST_SEARCH_TOOL_NAME,
  LOCAL_SEARCH_TOOL_NAME,
  STATIC_TOOL_NAMES,
} from '@octocodeai/octocode-core/schema';
import {
  type ToolConfig,
  type ToolDirectExecutionConfig,
} from './toolCatalogFactory.js';

interface ToolCatalog {
  GITHUB_SEARCH: ToolConfig;
  GITHUB_FETCH_CONTENT: ToolConfig;
  GITHUB_SEARCH_HISTORY: ToolConfig;
  GITHUB_GET_HISTORY_ITEM: ToolConfig;
  PACKAGE_SEARCH: ToolConfig;
  GITHUB_CLONE_REPO: ToolConfig;
  LOCAL_SEARCH: ToolConfig;
  AST_SEARCH: ToolConfig;
  LOCAL_FETCH_CONTENT: ToolConfig;
  LSP_SEARCH: ToolConfig;
  ALL_TOOLS: ToolConfig[];
}

const REMOTE_DIRECT = {
  security: 'remote',
  requiresServerRuntime: true,
  requiresProviders: true,
} as const;

type RuntimeToolAttachment = Omit<
  ToolConfig,
  'name' | 'title' | 'description' | 'direct'
> & {
  direct: Omit<ToolDirectExecutionConfig, 'schema' | 'inputSchema'>;
};

const RUNTIME_ATTACHMENT_BY_NAME: Readonly<
  Record<string, RuntimeToolAttachment>
> = {
  [GITHUB_SEARCH_TOOL_NAME]: {
    isDefault: true,
    isLocal: false,
    type: 'search',
    direct: {
      executionFn: async input =>
        (await import('./github_search/execution.js')).executeGitHubSearch(
          input
        ),
      ...REMOTE_DIRECT,
    },
  },
  [STATIC_TOOL_NAMES.GITHUB_FETCH_CONTENT]: {
    isDefault: true,
    isLocal: false,
    type: 'content',
    direct: {
      executionFn: async input =>
        (
          await import('./github_fetch_content/execution.js')
        ).fetchMultipleGitHubFileContents(input),
      ...REMOTE_DIRECT,
    },
  },
  [GITHUB_SEARCH_HISTORY_TOOL_NAME]: {
    isDefault: true,
    isLocal: false,
    type: 'history',
    direct: {
      executionFn: async input =>
        (
          await import('./github_search_pull_requests/historyExecutions.js')
        ).searchMultipleGitHubHistory(input),
      ...REMOTE_DIRECT,
    },
  },
  [GITHUB_GET_HISTORY_ITEM_TOOL_NAME]: {
    isDefault: true,
    isLocal: false,
    type: 'history',
    direct: {
      executionFn: async input =>
        (
          await import('./github_search_pull_requests/historyExecutions.js')
        ).getMultipleGitHubHistoryItems(input),
      ...REMOTE_DIRECT,
    },
  },
  [STATIC_TOOL_NAMES.PACKAGE_SEARCH]: {
    isDefault: true,
    isLocal: false,
    type: 'search',
    direct: {
      executionFn: async input =>
        (await import('./package_search/execution.js')).searchPackages(input),
      security: 'remote',
      requiresServerRuntime: true,
    },
  },
  [STATIC_TOOL_NAMES.GITHUB_CLONE_REPO]: {
    isDefault: true,
    isLocal: true,
    isClone: true,
    type: 'content',
    direct: {
      executionFn: async input =>
        (await import('./github_clone_repo/execution.js')).executeCloneRepo(
          input
        ),
      timeoutMs: 150_000,
      ...REMOTE_DIRECT,
    },
  },
  [LOCAL_SEARCH_TOOL_NAME]: {
    isDefault: true,
    isLocal: true,
    type: 'search',
    direct: {
      executionFn: async input =>
        (await import('./local_search/execution.js')).executeLocalSearch(input),
      security: 'basic',
    },
  },
  [AST_SEARCH_TOOL_NAME]: {
    isDefault: true,
    isLocal: true,
    type: 'search',
    direct: {
      executionFn: async input =>
        (await import('./ast_search/execution.js')).executeAstSearch(input),
      security: 'basic',
    },
  },
  [STATIC_TOOL_NAMES.LOCAL_FETCH_CONTENT]: {
    isDefault: true,
    isLocal: true,
    type: 'content',
    direct: {
      executionFn: async input =>
        (
          await import('./local_fetch_content/execution.js')
        ).executeFetchContent(input),
      security: 'basic',
    },
  },
  [LSP_SEARCH_TOOL_NAME]: {
    isDefault: true,
    isLocal: true,
    type: 'content',
    direct: {
      executionFn: async input =>
        (await import('./lsp/semantic_content/execution.js')).executeLspSearch(
          input
        ),
      security: 'basic',
    },
  },
};

function attachRuntimeConfiguration(
  specification: DirectToolSpecification
): ToolConfig {
  const runtime = RUNTIME_ATTACHMENT_BY_NAME[specification.name];
  if (!runtime) {
    throw new Error(`Missing runtime configuration for ${specification.name}`);
  }

  return {
    name: specification.name,
    title: specification.title,
    description: specification.description,
    ...runtime,
    direct: {
      schema: specification.schema,
      inputSchema: specification.inputSchema,
      ...runtime.direct,
    },
  };
}

function createToolCatalog(): ToolCatalog {
  const toolsByName = new Map(
    DIRECT_TOOL_SPECIFICATIONS.map(specification => {
      const tool = attachRuntimeConfiguration(specification);
      return [tool.name, tool] as const;
    })
  );
  const getTool = (name: string): ToolConfig => {
    const tool = toolsByName.get(name);
    if (!tool) {
      throw new Error(`Missing direct-tool specification for ${name}`);
    }
    return tool;
  };

  const GITHUB_SEARCH = getTool(GITHUB_SEARCH_TOOL_NAME);
  const GITHUB_FETCH_CONTENT = getTool(STATIC_TOOL_NAMES.GITHUB_FETCH_CONTENT);
  const GITHUB_SEARCH_HISTORY = getTool(GITHUB_SEARCH_HISTORY_TOOL_NAME);
  const GITHUB_GET_HISTORY_ITEM = getTool(GITHUB_GET_HISTORY_ITEM_TOOL_NAME);
  const PACKAGE_SEARCH = getTool(STATIC_TOOL_NAMES.PACKAGE_SEARCH);
  const GITHUB_CLONE_REPO = getTool(STATIC_TOOL_NAMES.GITHUB_CLONE_REPO);
  const LOCAL_SEARCH = getTool(LOCAL_SEARCH_TOOL_NAME);
  const AST_SEARCH = getTool(AST_SEARCH_TOOL_NAME);
  const LOCAL_FETCH_CONTENT = getTool(STATIC_TOOL_NAMES.LOCAL_FETCH_CONTENT);
  const LSP_SEARCH = getTool(LSP_SEARCH_TOOL_NAME);
  const ALL_TOOLS = DIRECT_TOOL_SPECIFICATIONS.map(specification =>
    getTool(specification.name)
  );

  return {
    GITHUB_SEARCH,
    GITHUB_FETCH_CONTENT,
    GITHUB_SEARCH_HISTORY,
    GITHUB_GET_HISTORY_ITEM,
    PACKAGE_SEARCH,
    GITHUB_CLONE_REPO,
    LOCAL_SEARCH,
    AST_SEARCH,
    LOCAL_FETCH_CONTENT,
    LSP_SEARCH,
    ALL_TOOLS,
  };
}

const DEFAULT_TOOL_CATALOG = createToolCatalog();

export const GITHUB_SEARCH = DEFAULT_TOOL_CATALOG.GITHUB_SEARCH;
export const GITHUB_FETCH_CONTENT = DEFAULT_TOOL_CATALOG.GITHUB_FETCH_CONTENT;
export const GITHUB_SEARCH_HISTORY = DEFAULT_TOOL_CATALOG.GITHUB_SEARCH_HISTORY;
export const GITHUB_GET_HISTORY_ITEM =
  DEFAULT_TOOL_CATALOG.GITHUB_GET_HISTORY_ITEM;
export const PACKAGE_SEARCH = DEFAULT_TOOL_CATALOG.PACKAGE_SEARCH;
export const GITHUB_CLONE_REPO = DEFAULT_TOOL_CATALOG.GITHUB_CLONE_REPO;
export const LOCAL_SEARCH = DEFAULT_TOOL_CATALOG.LOCAL_SEARCH;
export const AST_SEARCH = DEFAULT_TOOL_CATALOG.AST_SEARCH;
export const LOCAL_FETCH_CONTENT = DEFAULT_TOOL_CATALOG.LOCAL_FETCH_CONTENT;
export const LSP_SEARCH = DEFAULT_TOOL_CATALOG.LSP_SEARCH;
export const ALL_TOOLS = DEFAULT_TOOL_CATALOG.ALL_TOOLS;
