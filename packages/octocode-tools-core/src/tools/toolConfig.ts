import {
  DIRECT_TOOL_SPECIFICATIONS,
  isDirectToolSpecificationEnabled,
  type DirectToolSpecification,
} from './directToolCatalog/toolSpecifications.js';
import { executeCloneRepo } from './github_clone_repo/execution.js';
import { fetchMultipleGitHubFileContents } from './github_fetch_content/execution.js';
import { executeGitHubSearch } from './github_search/execution.js';
import { listMultipleGitHubReleases } from './github_search_pull_requests/splitExecutions.js';
import {
  getMultipleGitHubHistoryItems,
  searchMultipleGitHubHistory,
} from './github_search_pull_requests/historyExecutions.js';
import { searchMultipleGitHubDiscussions } from './github_search_discussions/execution.js';
import { searchPackages } from './package_search/execution.js';
import { executeFetchContent } from './local_fetch_content/execution.js';
import { executeAnalyzeGraph } from './local_analyze_graph/execution.js';
import { executeLocalSearch } from './local_search/execution.js';
import { executeLspGetSemantics } from './lsp/semantic_content/execution.js';
import { LSP_GET_SEMANTICS_TOOL_NAME } from './lsp/shared/semanticTypes.js';
import {
  GITHUB_SEARCH_TOOL_NAME,
  GITHUB_GET_HISTORY_ITEM_TOOL_NAME,
  GITHUB_SEARCH_HISTORY_TOOL_NAME,
  LOCAL_ANALYZE_GRAPH_TOOL_NAME,
  LOCAL_SEARCH_TOOL_NAME,
  STATIC_TOOL_NAMES,
} from './toolNames.js';
import {
  type ToolConfig,
  type ToolDirectExecutionConfig,
  type ToolDirectSecurity,
} from './toolCatalogFactory.js';

export type { ToolConfig, ToolDirectExecutionConfig, ToolDirectSecurity };
export type { ToolInvocationCallback } from '../types/toolResults.js';

interface ToolCatalog {
  GITHUB_SEARCH: ToolConfig;
  GITHUB_FETCH_CONTENT: ToolConfig;
  GITHUB_SEARCH_HISTORY: ToolConfig;
  GITHUB_GET_HISTORY_ITEM: ToolConfig;
  GITHUB_RELEASES: ToolConfig;
  GITHUB_DISCUSSIONS: ToolConfig;
  PACKAGE_SEARCH: ToolConfig;
  GITHUB_CLONE_REPO: ToolConfig;
  LOCAL_SEARCH: ToolConfig;
  LOCAL_ANALYZE_GRAPH: ToolConfig;
  LOCAL_FETCH_CONTENT: ToolConfig;
  LSP_GET_SEMANTIC_CONTENT: ToolConfig;
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
      executionFn: executeGitHubSearch,
      ...REMOTE_DIRECT,
    },
  },
  [STATIC_TOOL_NAMES.GITHUB_FETCH_CONTENT]: {
    isDefault: true,
    isLocal: false,
    type: 'content',
    direct: {
      executionFn: fetchMultipleGitHubFileContents,
      ...REMOTE_DIRECT,
    },
  },
  [GITHUB_SEARCH_HISTORY_TOOL_NAME]: {
    isDefault: true,
    isLocal: false,
    type: 'history',
    direct: {
      executionFn: searchMultipleGitHubHistory,
      ...REMOTE_DIRECT,
    },
  },
  [GITHUB_GET_HISTORY_ITEM_TOOL_NAME]: {
    isDefault: true,
    isLocal: false,
    type: 'history',
    direct: {
      executionFn: getMultipleGitHubHistoryItems,
      ...REMOTE_DIRECT,
    },
  },
  [STATIC_TOOL_NAMES.GITHUB_RELEASES]: {
    isDefault: true,
    isLocal: false,
    type: 'history',
    direct: {
      executionFn: listMultipleGitHubReleases,
      ...REMOTE_DIRECT,
    },
  },
  [STATIC_TOOL_NAMES.GITHUB_DISCUSSIONS]: {
    isDefault: true,
    isLocal: false,
    type: 'history',
    direct: {
      executionFn: searchMultipleGitHubDiscussions,
      ...REMOTE_DIRECT,
    },
  },
  [STATIC_TOOL_NAMES.PACKAGE_SEARCH]: {
    isDefault: true,
    isLocal: false,
    type: 'search',
    direct: {
      executionFn: searchPackages,
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
      executionFn: executeCloneRepo,
      timeoutMs: 150_000,
      ...REMOTE_DIRECT,
    },
  },
  [LOCAL_SEARCH_TOOL_NAME]: {
    isDefault: true,
    isLocal: true,
    type: 'search',
    direct: {
      executionFn: executeLocalSearch,
      security: 'basic',
    },
  },
  [LOCAL_ANALYZE_GRAPH_TOOL_NAME]: {
    isDefault: true,
    isLocal: true,
    type: 'search',
    direct: {
      executionFn: executeAnalyzeGraph,
      security: 'basic',
    },
  },
  [STATIC_TOOL_NAMES.LOCAL_FETCH_CONTENT]: {
    isDefault: true,
    isLocal: true,
    type: 'content',
    direct: {
      executionFn: executeFetchContent,
      security: 'basic',
    },
  },
  [LSP_GET_SEMANTICS_TOOL_NAME]: {
    isDefault: true,
    isLocal: true,
    type: 'content',
    direct: {
      executionFn: executeLspGetSemantics,
      security: 'basic',
      requiresServerRuntime: true,
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
  const GITHUB_RELEASES = getTool(STATIC_TOOL_NAMES.GITHUB_RELEASES);
  const GITHUB_DISCUSSIONS = getTool(STATIC_TOOL_NAMES.GITHUB_DISCUSSIONS);
  const PACKAGE_SEARCH = getTool(STATIC_TOOL_NAMES.PACKAGE_SEARCH);
  const GITHUB_CLONE_REPO = getTool(STATIC_TOOL_NAMES.GITHUB_CLONE_REPO);
  const LOCAL_SEARCH = getTool(LOCAL_SEARCH_TOOL_NAME);
  const LOCAL_ANALYZE_GRAPH = getTool(LOCAL_ANALYZE_GRAPH_TOOL_NAME);
  const LOCAL_FETCH_CONTENT = getTool(STATIC_TOOL_NAMES.LOCAL_FETCH_CONTENT);
  const LSP_GET_SEMANTIC_CONTENT = getTool(LSP_GET_SEMANTICS_TOOL_NAME);
  const ALL_TOOLS = DIRECT_TOOL_SPECIFICATIONS.filter(
    isDirectToolSpecificationEnabled
  ).map(specification => getTool(specification.name));

  return {
    GITHUB_SEARCH,
    GITHUB_FETCH_CONTENT,
    GITHUB_SEARCH_HISTORY,
    GITHUB_GET_HISTORY_ITEM,
    GITHUB_RELEASES,
    GITHUB_DISCUSSIONS,
    PACKAGE_SEARCH,
    GITHUB_CLONE_REPO,
    LOCAL_SEARCH,
    LOCAL_ANALYZE_GRAPH,
    LOCAL_FETCH_CONTENT,
    LSP_GET_SEMANTIC_CONTENT,
    ALL_TOOLS,
  };
}

const DEFAULT_TOOL_CATALOG = createToolCatalog();

export const GITHUB_SEARCH = DEFAULT_TOOL_CATALOG.GITHUB_SEARCH;
export const GITHUB_FETCH_CONTENT = DEFAULT_TOOL_CATALOG.GITHUB_FETCH_CONTENT;
export const GITHUB_SEARCH_HISTORY = DEFAULT_TOOL_CATALOG.GITHUB_SEARCH_HISTORY;
export const GITHUB_GET_HISTORY_ITEM =
  DEFAULT_TOOL_CATALOG.GITHUB_GET_HISTORY_ITEM;
export const GITHUB_RELEASES = DEFAULT_TOOL_CATALOG.GITHUB_RELEASES;
export const GITHUB_DISCUSSIONS = DEFAULT_TOOL_CATALOG.GITHUB_DISCUSSIONS;
export const PACKAGE_SEARCH = DEFAULT_TOOL_CATALOG.PACKAGE_SEARCH;
export const GITHUB_CLONE_REPO = DEFAULT_TOOL_CATALOG.GITHUB_CLONE_REPO;
export const LOCAL_SEARCH = DEFAULT_TOOL_CATALOG.LOCAL_SEARCH;
export const LOCAL_ANALYZE_GRAPH = DEFAULT_TOOL_CATALOG.LOCAL_ANALYZE_GRAPH;
export const LOCAL_FETCH_CONTENT = DEFAULT_TOOL_CATALOG.LOCAL_FETCH_CONTENT;
export const LSP_GET_SEMANTIC_CONTENT =
  DEFAULT_TOOL_CATALOG.LSP_GET_SEMANTIC_CONTENT;
export const ALL_TOOLS = DEFAULT_TOOL_CATALOG.ALL_TOOLS;
